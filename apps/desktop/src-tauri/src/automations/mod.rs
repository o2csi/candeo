//! Automations: rules that interrupt the effect applied on a device for a while,
//! then give it back (#106, `docs/design/inputs-and-automations.md` §3).
//!
//! A device still runs exactly one effect. The one someone applied, remembered
//! in `activeEffects`, is its resting state; a rule replaces it for the length
//! of an occurrence and never rewrites that record, so when the rule ends the
//! device goes back to what someone chose.
//!
//! # Two halves
//!
//! - [`resolver`] decides, and is pure: what should interrupt a device at an
//!   instant, and until when.
//! - The scheduler here applies: a thread that asks the resolver every second,
//!   on the second, and whenever something that changes the answer happens,
//!   then starts or ends interruptions through the engine.
//!
//! Rules live in Rust, not in the window: they must hold with the window closed.
//!
//! # Locks
//!
//! [`Automations`] holds its tables for as long as it takes to read or change
//! them, and **never** across an engine call, an HID write or a disk access —
//! the rule [`AppState`] states for its own tables. The scheduler decides under
//! the lock and acts after releasing it.

pub mod resolver;

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::Duration;

use candeo_protocol::Effect;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::runtime::{DeviceEngineStatus, EngineStatus};
use crate::{AppState, CmdResult, DeviceRef, Failure};
use resolver::{Context, Interruption, Moment, Rule};

/// What the window and the tray show of an interruption.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InterruptionStatus {
    /// The rule's id.
    pub rule: String,
    /// The rule's name, as someone wrote it; empty when they did not.
    pub name: String,
    /// The effect it shows.
    pub effect: String,
    /// When it ends, in epoch milliseconds; absent for a rule that never stops.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<i64>,
}

/// The scheduler's state, managed by Tauri beside [`AppState`].
#[derive(Default)]
pub struct Automations {
    tables: Mutex<Tables>,
    /// Wakes the scheduler before its next second. `None` until it started.
    wake: Mutex<Option<Sender<()>>>,
}

#[derive(Default)]
struct Tables {
    /// When each enabled rule was first seen enabled, epoch milliseconds: the
    /// origin of a schedule not aligned on the clock. In memory only — after a
    /// restart such a rule counts from the restart, which is when it was, as far
    /// as this process knows, switched on.
    anchors: HashMap<String, i64>,
    /// Occurrences someone ended. See [`resolver::Context::dismissed`].
    dismissed: HashSet<(DeviceRef, String, i64)>,
    /// Rules tried by hand, and when.
    tried: HashMap<String, i64>,
    /// What the scheduler put on each device.
    running: HashMap<DeviceRef, Running>,
    /// Occurrences that did not start — an effect deleted since the rule was
    /// written, a device gone mid-write — so that one is reported once, not
    /// once a second until it ends.
    failed: HashSet<(DeviceRef, String, i64)>,
}

struct Running {
    interruption: Interruption,
    name: String,
    resting: Resting,
}

/// What a device goes back to once no rule interrupts it.
///
/// Read at the **first** interruption and carried over when one rule hands the
/// device to another: by then the device shows the first rule's effect, which
/// is nobody's resting state.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Resting {
    /// A host loop was running: the applied effect, restarted from
    /// `activeEffects` with its saved settings. Its `time` starts again from
    /// zero (§3.4): keeping the interrupted loop alive underneath would be two
    /// loops on one device.
    Applied,
    /// No host loop was running, so the firmware drove the lighting: its effect,
    /// read back before interrupting, since nothing else remembers it. `None`
    /// when it could not be read, and the device then goes dark rather than
    /// staying frozen on the rule's last frame.
    Firmware(Option<Effect>),
}

/// What a tick asks of a device.
#[derive(Debug, PartialEq)]
enum Change {
    /// Show this — the first interruption, or a switch to another rule.
    Interrupt(Interruption),
    /// No rule applies any more: back to the resting state.
    GiveBack,
}

/// Starts the scheduler: a thread of its own, for as long as the process runs,
/// like the hotplug watch.
pub(crate) fn start(app: &AppHandle) {
    let (sender, receiver) = mpsc::channel();
    let Some(automations) = app.try_state::<Automations>() else {
        tracing::error!("automations not managed, no rule will apply");
        return;
    };
    *automations.wake.lock().unwrap() = Some(sender);

    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("candeo-automations".into())
        .spawn(move || run(&app, &receiver));
    if let Err(e) = spawned {
        tracing::error!("automations not started, no rule will apply: {e}");
    }
}

/// Asks the scheduler to decide now rather than at the next second: a rule
/// edited, the pause toggled, a rule tried.
pub(crate) fn wake(app: &AppHandle) {
    if let Some(automations) = app.try_state::<Automations>() {
        if let Some(sender) = automations.wake.lock().unwrap().as_ref() {
            let _ = sender.send(());
        }
    }
}

fn run(app: &AppHandle, wake: &Receiver<()>) {
    tracing::info!("automations started");
    loop {
        tick(app);
        match wake.recv_timeout(until_next_second(now())) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// The wait until just past the next second, so that a tick reads the second it
/// is about — the hour, not 59 minutes 59 and a bit.
fn until_next_second(now: Moment) -> Duration {
    let into = now.epoch_ms.rem_euclid(1000) as u64;
    Duration::from_millis(1000 - into + 5)
}

/// The local wall clock, as the resolver reads it.
fn now() -> Moment {
    use chrono::Timelike;
    let local = chrono::Local::now();
    Moment {
        epoch_ms: local.timestamp_millis(),
        day_ms: i64::from(local.num_seconds_from_midnight()) * 1000
            + i64::from(local.timestamp_subsec_millis().min(999)),
    }
}

/// One decision: what each open device should show, then acting on it.
fn tick(app: &AppHandle) {
    let settings = match crate::storage::store(app).and_then(|s| s.read_settings()) {
        Ok(settings) => settings,
        Err(e) => {
            tracing::debug!("automations skip a second, settings not read: {e}");
            return;
        }
    };
    let rules: Vec<Rule> = settings
        .rules()
        .into_iter()
        .filter_map(Result::ok)
        .collect();
    let paused = settings.preferences.automations_paused;
    let moment = now();
    let (Some(automations), Some(state)) =
        (app.try_state::<Automations>(), app.try_state::<AppState>())
    else {
        return;
    };
    let open = state.open_devices();

    let plan: Vec<(DeviceRef, Change)> = {
        let mut tables = automations.tables.lock().unwrap();
        tables.keep_anchors(&rules, moment.epoch_ms);
        tables.forget_old_tries(&rules, moment.epoch_ms);
        // A device that closed keeps nothing. Reopened, it resumes its applied
        // effect, and the next tick interrupts it again if a rule still applies.
        tables.running.retain(|device, _| open.contains(device));

        let mut plan = Vec::new();
        for &device in &open {
            let answer = resolver::resolve(
                &Context {
                    now: moment,
                    rules: &rules,
                    paused,
                    anchors: &tables.anchors,
                    dismissed: &tables.dismissed,
                    tried: &tables.tried,
                },
                device,
            )
            .filter(|a| !tables.failed.contains(&(device, a.rule.clone(), a.since)));
            let current = tables.running.get_mut(&device);
            if let Some(change) = decide(current, answer) {
                plan.push((device, change));
            }
        }
        plan
    };

    if plan.is_empty() {
        return;
    }
    for (device, change) in plan {
        match change {
            Change::Interrupt(interruption) => interrupt(app, device, interruption, &rules),
            Change::GiveBack => give_back(app, device),
        }
    }
    crate::tray::refresh(app);
    crate::tray::notify_state_changed(app);
}

/// What changes on a device, given what runs there and what the resolver says.
///
/// The same occurrence of the same rule changes nothing — that is what keeps
/// "every second for a second" from restarting its effect every second. Only its
/// end moves, when someone edited the rule's duration meanwhile.
fn decide(current: Option<&mut Running>, answer: Option<Interruption>) -> Option<Change> {
    match (current, answer) {
        (None, None) => None,
        (Some(running), Some(answer))
            if running.interruption.rule == answer.rule
                && running.interruption.since == answer.since =>
        {
            running.interruption.until = answer.until;
            None
        }
        (_, Some(answer)) => Some(Change::Interrupt(answer)),
        (Some(_), None) => Some(Change::GiveBack),
    }
}

impl Tables {
    /// Anchors the rules that just became enabled, and forgets those that no
    /// longer are: switched off then on again, a rule counts afresh.
    fn keep_anchors(&mut self, rules: &[Rule], now: i64) {
        self.anchors
            .retain(|id, _| rules.iter().any(|r| &r.id == id && r.enabled));
        for rule in rules.iter().filter(|r| r.enabled) {
            self.anchors.entry(rule.id.clone()).or_insert(now);
        }
    }

    /// Tries whose occurrence is over — or whose rule is gone — are dropped, and
    /// so is what was dismissed or failed more than a day ago: an occurrence is
    /// never longer than a day, since a window is.
    fn forget_old_tries(&mut self, rules: &[Rule], now: i64) {
        self.tried.retain(|id, since| {
            rules
                .iter()
                .find(|r| &r.id == id)
                .is_some_and(|r| now < *since + i64::from(r.lasts.seconds) * 1000)
        });
        const DAY_MS: i64 = 86_400_000;
        self.dismissed.retain(|(_, _, since)| now - since < DAY_MS);
        self.failed.retain(|(_, _, since)| now - since < DAY_MS);
    }
}

/// The firmware effect a rule's `hardware:` id names, or `None` for an effect
/// from the library.
///
/// The Wave's direction and speed are the only ones surveyed, the same the
/// gallery uses (`src/composables/useEffects.ts`): offering others would be
/// inventing a scale.
fn hardware(id: &str) -> Option<Effect> {
    match id {
        "hardware:off" => Some(Effect::Off),
        "hardware:spectrumCycle" => Some(Effect::SpectrumCycle),
        "hardware:wave" => Some(Effect::Wave {
            direction: 0x02,
            speed: 0x28,
        }),
        _ => None,
    }
}

/// Puts a rule's effect on a device.
fn interrupt(app: &AppHandle, device: DeviceRef, interruption: Interruption, rules: &[Rule]) {
    let (Some(automations), Some(state)) =
        (app.try_state::<Automations>(), app.try_state::<AppState>())
    else {
        return;
    };
    let occurrence = (device, interruption.rule.clone(), interruption.since);
    let carried = {
        let tables = automations.tables.lock().unwrap();
        // Someone acted on the device between the decision and now: their
        // gesture wins.
        if tables.dismissed.contains(&occurrence) {
            return;
        }
        tables.running.get(&device).map(|r| r.resting)
    };
    let resting = carried.unwrap_or_else(|| resting_state(&state, device));

    let started = match hardware(&interruption.effect) {
        Some(effect) => {
            // The loop stops first, and is awaited: its next frame would light
            // up again what the firmware was just told to do.
            state.engine.stop(device);
            crate::with_keyboard(&state, device, |kb| Ok(kb.set_effect(effect)?))
        }
        None => crate::runtime::run_with(
            app,
            device,
            &interruption.effect,
            Some(&interruption.params),
        ),
    };

    let name = rules
        .iter()
        .find(|r| r.id == interruption.rule)
        .map(|r| r.name.clone())
        .unwrap_or_default();
    let mut tables = automations.tables.lock().unwrap();
    match started {
        Ok(()) => {
            tracing::info!(
                device = %device,
                rule = %interruption.rule,
                effect = %interruption.effect,
                "a rule interrupts the device"
            );
            tables.running.insert(
                device,
                Running {
                    interruption,
                    name,
                    resting,
                },
            );
        }
        Err(e) => {
            tracing::warn!(
                device = %device,
                rule = %interruption.rule,
                effect = %interruption.effect,
                "a rule could not interrupt the device: {e}"
            );
            tables.failed.insert(occurrence);
        }
    }
}

/// What the device runs now, before a first interruption: see [`Resting`].
fn resting_state(state: &AppState, device: DeviceRef) -> Resting {
    let host_loop = state
        .engine
        .device_status()
        .into_iter()
        .any(|s| s.device == device && s.status.running);
    if host_loop {
        return Resting::Applied;
    }
    let read = crate::with_keyboard(state, device, |kb| {
        kb.current_effect().map_err(Failure::unexpected)
    });
    match read {
        // Host-controlled with no loop is a frozen frame, not a resting state.
        Ok(effect) => Resting::Firmware(effect.filter(|e| *e != Effect::Custom)),
        Err(e) => {
            tracing::info!(device = %device, "firmware effect not read before interrupting: {e}");
            Resting::Firmware(None)
        }
    }
}

/// Ends the interruption on a device and gives it its resting state back.
fn give_back(app: &AppHandle, device: DeviceRef) {
    let (Some(automations), Some(state)) =
        (app.try_state::<Automations>(), app.try_state::<AppState>())
    else {
        return;
    };
    let Some(running) = automations.tables.lock().unwrap().running.remove(&device) else {
        return;
    };

    let restored = match running.resting {
        Resting::Applied => {
            let settings = crate::storage::store(app).and_then(|s| s.read_settings());
            match settings {
                Ok(settings) => match settings.active_effect(device.vid, device.pid) {
                    Some(effect) => crate::runtime::run_with(
                        app,
                        device,
                        effect,
                        settings.effect_params(device.vid, device.pid, effect),
                    ),
                    // Stopped meanwhile from elsewhere: nothing to go back to.
                    None => {
                        state.engine.stop(device);
                        Ok(())
                    }
                },
                Err(e) => Err(e),
            }
        }
        Resting::Firmware(effect) => {
            state.engine.stop(device);
            crate::with_keyboard(&state, device, |kb| {
                Ok(kb.set_effect(effect.unwrap_or(Effect::Off))?)
            })
        }
    };

    match restored {
        Ok(()) => tracing::info!(
            device = %device,
            rule = %running.interruption.rule,
            "the rule gives the device back"
        ),
        Err(e) => {
            // The rule's effect must not keep running as if it still applied.
            state.engine.stop(device);
            tracing::warn!(
                device = %device,
                rule = %running.interruption.rule,
                "the device was not given back its effect: {e}"
            );
        }
    }
}

/// Ends the interruption on `device` because someone acted on it — applied an
/// effect, stopped it, turned it off. It gives nothing back: what they did takes
/// its place. The rule comes back at its next occurrence.
pub(crate) fn dismiss(app: &AppHandle, device: DeviceRef) {
    let Some(automations) = app.try_state::<Automations>() else {
        return;
    };
    let mut tables = automations.tables.lock().unwrap();
    if let Some(running) = tables.running.remove(&device) {
        tracing::info!(
            device = %device,
            rule = %running.interruption.rule,
            "an interruption ends, someone acted on the device"
        );
        tables.dismissed.insert((
            device,
            running.interruption.rule,
            running.interruption.since,
        ));
    }
}

/// Whether a rule interrupts `device` right now.
pub(crate) fn interrupts(app: &AppHandle, device: DeviceRef) -> bool {
    app.try_state::<Automations>()
        .is_some_and(|a| a.tables.lock().unwrap().running.contains_key(&device))
}

/// The interruption under way on `device`, as the window and the tray show it.
pub(crate) fn status(app: &AppHandle, device: DeviceRef) -> Option<InterruptionStatus> {
    let automations = app.try_state::<Automations>()?;
    let tables = automations.tables.lock().unwrap();
    tables.running.get(&device).map(Running::status)
}

impl Running {
    fn status(&self) -> InterruptionStatus {
        InterruptionStatus {
            rule: self.interruption.rule.clone(),
            name: self.name.clone(),
            effect: self.interruption.effect.clone(),
            until: self.interruption.until,
        }
    }
}

/// Adds the interruptions to an engine report. A device a firmware effect
/// interrupts may have no loop, hence no entry yet: it gets one, so the window
/// can still say what happens to it.
pub(crate) fn annotate(app: &AppHandle, devices: &mut Vec<DeviceEngineStatus>) {
    let Some(automations) = app.try_state::<Automations>() else {
        return;
    };
    let tables = automations.tables.lock().unwrap();
    for (device, running) in &tables.running {
        match devices.iter_mut().find(|d| d.device == *device) {
            Some(entry) => entry.status.interruption = Some(running.status()),
            None => devices.push(DeviceEngineStatus {
                device: *device,
                status: EngineStatus {
                    interruption: Some(running.status()),
                    ..EngineStatus::default()
                },
            }),
        }
    }
}

// ---------------------------------------------------------------- commands

/// Resume: ends the interruption on a device now and gives it back its resting
/// state. The rule comes back at its next occurrence.
#[tauri::command]
pub fn resume_device(app: AppHandle, device: DeviceRef) {
    if let Some(automations) = app.try_state::<Automations>() {
        let mut tables = automations.tables.lock().unwrap();
        let occurrence = tables
            .running
            .get(&device)
            .map(|r| (device, r.interruption.rule.clone(), r.interruption.since));
        if let Some(occurrence) = occurrence {
            tables.dismissed.insert(occurrence);
        }
    }
    give_back(&app, device);
    crate::tray::refresh(&app);
    crate::tray::notify_state_changed(&app);
}

/// Pause automations, or turn them back on. Paused, every interruption under way
/// ends at the next tick — now, since the scheduler is woken.
#[tauri::command]
pub fn set_automations_paused(app: AppHandle, paused: bool) -> CmdResult<()> {
    let store = crate::storage::store(&app)?;
    let mut settings = store.read_settings()?;
    if settings.preferences.automations_paused != paused {
        settings.preferences.automations_paused = paused;
        store.write_settings(&settings)?;
        tracing::info!(paused, "automations paused or resumed");
    }
    wake(&app);
    Ok(())
}

/// Replaces the rules, in the order given, which is their priority.
///
/// Each is checked first: the interface builds complete rules, and one it could
/// not is a mistake to show rather than a rule to write and ignore.
#[tauri::command]
pub fn set_rules(app: AppHandle, rules: Vec<serde_json::Value>) -> CmdResult<()> {
    for (raw, parsed) in rules.iter().zip(resolver::parse(&rules)) {
        if let Err(error) = parsed {
            let name = raw
                .get("name")
                .and_then(|n| n.as_str())
                .filter(|n| !n.is_empty())
                .or_else(|| raw.get("id").and_then(|i| i.as_str()))
                .unwrap_or_default();
            return Err(Failure::new("ruleInvalid")
                .with("name", name)
                .with("error", error));
        }
    }
    let store = crate::storage::store(&app)?;
    let mut settings = store.read_settings()?;
    settings.rules = rules;
    store.write_settings(&settings)?;
    wake(&app);
    Ok(())
}

/// Try: runs a rule once, now, for its duration — switched on or not, paused or
/// not. What the rule does is seen before waiting for the hour.
#[tauri::command]
pub fn try_rule(app: AppHandle, id: String) -> CmdResult<()> {
    let settings = crate::storage::store(&app)?.read_settings()?;
    let known = settings
        .rules()
        .into_iter()
        .any(|r| r.is_ok_and(|r| r.id == id));
    if !known {
        return Err(Failure::new("ruleNotFound").with("name", &id));
    }
    if let Some(automations) = app.try_state::<Automations>() {
        let mut tables = automations.tables.lock().unwrap();
        tables.tried.insert(id, now().epoch_ms);
    }
    wake(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interruption(rule: &str, since: i64, until: Option<i64>) -> Interruption {
        Interruption {
            rule: rule.into(),
            effect: "shipped:Clock".into(),
            params: serde_json::Map::new(),
            since,
            until,
        }
    }

    fn running(rule: &str, since: i64) -> Running {
        Running {
            interruption: interruption(rule, since, Some(since + 10_000)),
            name: String::new(),
            resting: Resting::Applied,
        }
    }

    /// Nothing, then a rule: interrupt. The same occurrence: nothing, even when
    /// its end moved. Another occurrence or rule: interrupt again. No rule: give
    /// back.
    #[test]
    fn a_tick_changes_a_device_only_when_the_answer_does() {
        assert_eq!(decide(None, None), None);
        assert_eq!(
            decide(None, Some(interruption("hourly", 0, Some(10_000)))),
            Some(Change::Interrupt(interruption("hourly", 0, Some(10_000))))
        );

        let mut current = running("hourly", 0);
        assert_eq!(
            decide(
                Some(&mut current),
                Some(interruption("hourly", 0, Some(20_000)))
            ),
            None
        );
        assert_eq!(current.interruption.until, Some(20_000), "the end moved");

        assert!(matches!(
            decide(
                Some(&mut running("hourly", 0)),
                Some(interruption("night", 0, None))
            ),
            Some(Change::Interrupt(_))
        ));
        assert_eq!(
            decide(Some(&mut running("hourly", 0)), None),
            Some(Change::GiveBack)
        );
    }

    /// A rule switched on is anchored once; switched off, it loses its anchor,
    /// and counts afresh when switched on again.
    #[test]
    fn anchors_follow_rules_switching_on_and_off() {
        let raw = serde_json::json!({
            "id": "every7", "enabled": true, "devices": [{ "vid": 1, "pid": 2 }],
            "when": { "kind": "schedule", "every": 7 }, "show": { "effect": "shipped:Clock" }
        });
        let mut rule: Rule = serde_json::from_value(raw).unwrap();
        let mut tables = Tables::default();

        tables.keep_anchors(std::slice::from_ref(&rule), 1_000);
        tables.keep_anchors(std::slice::from_ref(&rule), 5_000);
        assert_eq!(tables.anchors.get("every7"), Some(&1_000));

        rule.enabled = false;
        tables.keep_anchors(std::slice::from_ref(&rule), 6_000);
        assert!(tables.anchors.is_empty());

        rule.enabled = true;
        tables.keep_anchors(std::slice::from_ref(&rule), 9_000);
        assert_eq!(tables.anchors.get("every7"), Some(&9_000));
    }

    /// Past the second means just past it: a tick never lands before the second
    /// it is about.
    #[test]
    fn the_scheduler_wakes_just_past_the_next_second() {
        let at = |epoch_ms| Moment {
            epoch_ms,
            day_ms: 0,
        };
        assert_eq!(until_next_second(at(10_000)), Duration::from_millis(1005));
        assert_eq!(until_next_second(at(10_999)), Duration::from_millis(6));
    }

    #[test]
    fn hardware_ids_name_firmware_effects() {
        assert_eq!(hardware("hardware:off"), Some(Effect::Off));
        assert_eq!(
            hardware("hardware:spectrumCycle"),
            Some(Effect::SpectrumCycle)
        );
        assert!(matches!(
            hardware("hardware:wave"),
            Some(Effect::Wave { .. })
        ));
        assert_eq!(hardware("shipped:Clock"), None);
    }
}
