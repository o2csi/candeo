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
//! - [`resolver`] decides, and is pure: which occurrences are under way on a
//!   device at an instant.
//! - The scheduler here applies: a thread that asks the resolver every second, on
//!   the second, and whenever something that changes the answer happens, then
//!   starts or ends interruptions through the engine.
//!
//! Rules live in Rust, not in the window: they must hold with the window closed.
//!
//! # Runs
//!
//! Occurrences of one rule that start before the previous one ended — every
//! second for a second, every minute for a minute — are **one run**: the effect
//! is not restarted at each, and Resume dismisses the run, not one second of it.
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
use resolver::{Context, Interruption, Rule};

/// How late an occurrence may start after the last one ended and still continue
/// its run: one tick and a half. The scheduler wakes every second, and a tick
/// that runs late must not read two touching occurrences as separate.
const CHAIN_MS: i64 = 1500;

/// Milliseconds in a day: how long a dismissal is remembered after its run.
const DAY_MS: i64 = 86_400_000;

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
    /// When it ends, in epoch milliseconds; absent while occurrences keep
    /// following one another, since no one knows when that stops.
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
    /// Runs someone ended — Resume, or an effect applied by hand — as the end of
    /// the run, per device and rule: every run under way on the device then, not
    /// only the one on screen. An occurrence continuing the run stays dismissed
    /// and pushes that end further; one after a gap starts over.
    dismissed: HashMap<(DeviceRef, String), i64>,
    /// Rules tried by hand, and when.
    tried: HashMap<String, i64>,
    /// What the scheduler put on each device.
    running: HashMap<DeviceRef, Running>,
    /// Occurrences that did not start — an effect deleted since the rule was
    /// written, a device gone mid-write — so that one is reported once, not once
    /// a second until it ends.
    failed: HashSet<(DeviceRef, String, i64)>,
}

struct Running {
    interruption: Interruption,
    name: String,
    resting: Resting,
    /// A later occurrence continued this run: its end is no longer known.
    continued: bool,
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
        let now = chrono::Local::now().timestamp_millis();
        match wake.recv_timeout(until_next_second(now)) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// The wait until just past the next second, so that a tick reads the second it
/// is about — the hour, not 59 minutes 59 and a bit.
fn until_next_second(epoch_ms: i64) -> Duration {
    let into = epoch_ms.rem_euclid(1000) as u64;
    Duration::from_millis(1000 - into + 5)
}

/// One decision: what each open device should show, then acting on it.
fn tick(app: &AppHandle) {
    let (rules, paused) = match read_rules(app) {
        Ok(read) => read,
        Err(e) => {
            tracing::debug!("automations skip a second, settings not read: {e}");
            return;
        }
    };
    let now = chrono::Local::now();
    // Read once, for every device: idleness is the session's, not a keyboard's.
    let idle = crate::idle::idle_ms();
    let (Some(automations), Some(state)) =
        (app.try_state::<Automations>(), app.try_state::<AppState>())
    else {
        return;
    };
    let open = state.open_devices();

    let plan: Vec<(DeviceRef, Change)> = {
        let mut guard = automations.tables.lock().unwrap();
        let tables = &mut *guard;
        tables.forget_old(&rules, now.timestamp_millis());
        // A device that closed keeps nothing. Reopened, it resumes its applied
        // effect, and the next tick interrupts it again if a rule still applies.
        tables.running.retain(|device, _| open.contains(device));

        let mut plan = Vec::new();
        for &device in &open {
            let under_way = resolver::active(
                &Context {
                    now,
                    rules: &rules,
                    paused,
                    tried: &tables.tried,
                    idle,
                },
                device,
            )
            .into_iter()
            .filter(|i| !tables.failed.contains(&(device, i.rule.clone(), i.since)))
            .collect();
            let answer = choose(under_way, device, &mut tables.dismissed);
            if let Some(change) = decide(tables.running.get_mut(&device), answer) {
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

/// The rules that can run, and whether automations are paused.
fn read_rules(app: &AppHandle) -> CmdResult<(Vec<Rule>, bool)> {
    let settings = crate::storage::store(app)?.read_settings()?;
    let rules = settings
        .rules()
        .into_iter()
        .filter_map(Result::ok)
        .collect();
    Ok((rules, settings.preferences.automations_paused))
}

/// Whether an occurrence starting at `since` continues a run that ended at
/// `until`.
fn continues(since: i64, until: i64) -> bool {
    since <= until + CHAIN_MS
}

/// The occurrence that should run on a device: the first under way whose run
/// nobody dismissed.
///
/// A dismissed run follows its rule while occurrences keep touching: each pushes
/// the dismissal's end further, so Resume on "every second for a second" holds.
/// After a gap the rule comes back — the hour, the next hour.
fn choose(
    under_way: Vec<Interruption>,
    device: DeviceRef,
    dismissed: &mut HashMap<(DeviceRef, String), i64>,
) -> Option<Interruption> {
    for occurrence in under_way {
        let key = (device, occurrence.rule.clone());
        if let Some(until) = dismissed.get_mut(&key) {
            if continues(occurrence.since, *until) {
                *until = (*until).max(occurrence.until);
                continue;
            }
            dismissed.remove(&key);
        }
        return Some(occurrence);
    }
    None
}

/// What changes on a device, given what runs there and what should.
///
/// An occurrence of the running rule that continues its run changes nothing but
/// the run's end: that is what keeps "every second for a second" from restarting
/// its effect every second.
fn decide(current: Option<&mut Running>, answer: Option<Interruption>) -> Option<Change> {
    match (current, answer) {
        (None, None) => None,
        (Some(running), Some(answer))
            if running.interruption.rule == answer.rule
                && continues(answer.since, running.interruption.until) =>
        {
            if answer.since > running.interruption.since {
                running.continued = true;
            }
            running.interruption.until = running.interruption.until.max(answer.until);
            None
        }
        (_, Some(answer)) => Some(Change::Interrupt(answer)),
        (Some(_), None) => Some(Change::GiveBack),
    }
}

impl Tables {
    /// Tries whose occurrence is over — or whose rule is gone — are dropped, and
    /// so are dismissals and failures older than a day.
    fn forget_old(&mut self, rules: &[Rule], now: i64) {
        self.tried.retain(|id, since| {
            rules
                .iter()
                .find(|r| &r.id == id)
                .is_some_and(|r| now < *since + i64::from(r.lasts.seconds) * 1000)
        });
        self.dismissed.retain(|_, until| now - *until < DAY_MS);
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
    let carried = {
        let tables = automations.tables.lock().unwrap();
        // Someone acted on the device between the decision and now: their
        // gesture wins.
        let dismissed = tables
            .dismissed
            .get(&(device, interruption.rule.clone()))
            .is_some_and(|until| continues(interruption.since, *until));
        if dismissed {
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
                    continued: false,
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
            tables
                .failed
                .insert((device, interruption.rule, interruption.since));
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
        Resting::Applied => match crate::storage::store(app).and_then(|s| s.read_settings()) {
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
        },
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

/// The occurrences under way on `device` now, for a gesture to dismiss. Empty
/// when the rules could not be read: the gesture then ends the one on screen.
fn under_way_now(
    tables: &Tables,
    device: DeviceRef,
    read: CmdResult<(Vec<Rule>, bool)>,
) -> Vec<Interruption> {
    match read {
        Ok((rules, paused)) => resolver::active(
            &Context {
                now: chrono::Local::now(),
                rules: &rules,
                paused,
                tried: &tables.tried,
                idle: crate::idle::idle_ms(),
            },
            device,
        ),
        Err(e) => {
            tracing::debug!(
                device = %device,
                "only the rule on screen is dismissed, settings not read: {e}"
            );
            Vec::new()
        }
    }
}

/// Dismisses every run under way on `device`, and returns whether a rule was on
/// screen there.
///
/// Every run, not the one on screen: a lower rule under way — the night under
/// the hourly clock — would otherwise take the device back at the next tick, a
/// second after someone acted on it.
fn dismiss_runs(tables: &mut Tables, device: DeviceRef, under_way: Vec<Interruption>) -> bool {
    let on_screen = tables
        .running
        .get(&device)
        .map(|r| (r.interruption.rule.clone(), r.interruption.until));
    let was_on_screen = on_screen.is_some();
    let ends = under_way.into_iter().map(|o| (o.rule, o.until));
    for (rule, until) in ends.chain(on_screen) {
        let end = tables.dismissed.entry((device, rule)).or_insert(until);
        *end = (*end).max(until);
    }
    was_on_screen
}

/// Ends the interruption on `device` because someone acted on it — applied an
/// effect, stopped it, turned it off. It gives nothing back: what they did takes
/// its place. The rules come back after their runs.
pub(crate) fn dismiss(app: &AppHandle, device: DeviceRef) {
    let Some(automations) = app.try_state::<Automations>() else {
        return;
    };
    // Read before taking the lock, which is never held across a disk access.
    let read = read_rules(app);
    let mut tables = automations.tables.lock().unwrap();
    let under_way = under_way_now(&tables, device, read);
    if dismiss_runs(&mut tables, device, under_way) {
        if let Some(running) = tables.running.remove(&device) {
            tracing::info!(
                device = %device,
                rule = %running.interruption.rule,
                "an interruption ends, someone acted on the device"
            );
        }
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
            until: (!self.continued && !self.interruption.open).then_some(self.interruption.until),
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
/// state. The rules under way come back after their runs.
#[tauri::command]
pub fn resume_device(app: AppHandle, device: DeviceRef) {
    if let Some(automations) = app.try_state::<Automations>() {
        let read = read_rules(&app);
        let mut tables = automations.tables.lock().unwrap();
        let under_way = under_way_now(&tables, device, read);
        dismiss_runs(&mut tables, device, under_way);
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
/// not is a mistake to show rather than a rule to write and ignore — an
/// expression typed in the advanced field that is not cron, most of all. A broken
/// rule already in the file and sent back **unchanged** passes: it stays as
/// written, and does nothing, rather than blocking every other edit.
#[tauri::command]
pub fn set_rules(app: AppHandle, rules: Vec<serde_json::Value>) -> CmdResult<()> {
    let store = crate::storage::store(&app)?;
    let mut settings = store.read_settings()?;
    let already: HashSet<String> = settings.rules.iter().map(ToString::to_string).collect();

    for (raw, parsed) in rules.iter().zip(resolver::parse(&rules)) {
        if let Err(error) = parsed {
            if already.contains(&raw.to_string()) {
                continue;
            }
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
        tables
            .tried
            .insert(id, chrono::Local::now().timestamp_millis());
    }
    wake(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEYBOARD: DeviceRef = DeviceRef {
        vid: 0x1532,
        pid: 0x0292,
    };

    fn occurrence(rule: &str, since: i64, until: i64) -> Interruption {
        Interruption {
            rule: rule.into(),
            effect: "shipped:Clock".into(),
            params: serde_json::Map::new(),
            since,
            until,
            open: false,
        }
    }

    fn running(rule: &str, since: i64, until: i64) -> Running {
        Running {
            interruption: occurrence(rule, since, until),
            name: String::new(),
            resting: Resting::Applied,
            continued: false,
        }
    }

    /// Nothing, then a rule: interrupt. Another rule: interrupt again. No rule:
    /// give back.
    #[test]
    fn a_tick_changes_a_device_when_the_rule_does() {
        assert_eq!(decide(None, None), None);
        assert_eq!(
            decide(None, Some(occurrence("hourly", 0, 10_000))),
            Some(Change::Interrupt(occurrence("hourly", 0, 10_000)))
        );
        assert!(matches!(
            decide(
                Some(&mut running("hourly", 0, 10_000)),
                Some(occurrence("night", 0, 20_000))
            ),
            Some(Change::Interrupt(_))
        ));
        assert_eq!(
            decide(Some(&mut running("hourly", 0, 10_000)), None),
            Some(Change::GiveBack)
        );
    }

    /// Every second for a second: each occurrence continues the run, nothing
    /// restarts, and the run's end is no longer announced.
    #[test]
    fn touching_occurrences_are_one_run() {
        let mut current = running("steady", 0, 1_000);
        assert_eq!(
            decide(Some(&mut current), Some(occurrence("steady", 1_000, 2_000))),
            None
        );
        assert_eq!(current.interruption.until, 2_000);
        assert!(current.continued);
        assert_eq!(current.status().until, None);

        // An hour later is a new run, and starts again.
        assert!(matches!(
            decide(
                Some(&mut running("hourly", 0, 10_000)),
                Some(occurrence("hourly", 3_600_000, 3_610_000))
            ),
            Some(Change::Interrupt(_))
        ));
    }

    /// Nobody at the computer: each look finds the same stretch, known a second
    /// ahead. It is one run, never restarted, and the card announces no end.
    #[test]
    fn an_idle_stretch_is_one_run_with_no_end_announced() {
        let idle = |until| Interruption {
            open: true,
            ..occurrence("away", 0, until)
        };
        let mut current = Running {
            interruption: idle(1_000),
            ..running("away", 0, 1_000)
        };
        assert_eq!(current.status().until, None, "open from the start");
        assert_eq!(decide(Some(&mut current), Some(idle(2_000))), None);
        assert_eq!(current.interruption.until, 2_000);
    }

    /// Resume on a run of back-to-back occurrences holds while they keep coming,
    /// and gives way to a rule that starts meanwhile.
    #[test]
    fn a_dismissed_run_stays_dismissed_while_it_continues() {
        let mut dismissed = HashMap::from([((KEYBOARD, "steady".to_string()), 1_000)]);

        let answer = choose(
            vec![
                occurrence("steady", 1_000, 2_000),
                occurrence("rain", 1_000, 30_000),
            ],
            KEYBOARD,
            &mut dismissed,
        );
        assert_eq!(answer.map(|a| a.rule), Some("rain".into()));
        assert_eq!(
            dismissed[&(KEYBOARD, "steady".to_string())],
            2_000,
            "pushed on"
        );

        // After a gap, the rule comes back, and its dismissal is gone.
        let answer = choose(
            vec![occurrence("steady", 60_000, 61_000)],
            KEYBOARD,
            &mut dismissed,
        );
        assert_eq!(answer.map(|a| a.rule), Some("steady".into()));
        assert!(dismissed.is_empty());
    }

    /// Resume on the hourly clock dismisses this hour, not the next.
    #[test]
    fn a_dismissed_hour_comes_back_the_next_hour() {
        let mut dismissed = HashMap::from([((KEYBOARD, "hourly".to_string()), 10_000)]);
        assert!(choose(
            vec![occurrence("hourly", 0, 10_000)],
            KEYBOARD,
            &mut dismissed
        )
        .is_none());
        assert!(choose(
            vec![occurrence("hourly", 3_600_000, 3_610_000)],
            KEYBOARD,
            &mut dismissed
        )
        .is_some());
    }

    /// Acting on the keyboard while the hourly clock hides the night ends both:
    /// the night does not take the keyboard back a second later. The next hour
    /// still comes, and so does the next night.
    #[test]
    fn a_gesture_ends_every_rule_under_way() {
        const HOUR: i64 = 3_600_000;
        let hourly = |hour: i64| occurrence("hourly", hour * HOUR, hour * HOUR + 10_000);
        let night = |day: i64| occurrence("night", day * DAY_MS, day * DAY_MS + 9 * HOUR);
        let mut tables = Tables::default();
        tables
            .running
            .insert(KEYBOARD, running("hourly", HOUR, HOUR + 10_000));

        assert!(dismiss_runs(
            &mut tables,
            KEYBOARD,
            vec![hourly(1), night(0)]
        ));
        let dismissed = &mut tables.dismissed;
        assert!(choose(vec![hourly(1), night(0)], KEYBOARD, dismissed).is_none());
        assert!(
            choose(vec![night(0)], KEYBOARD, dismissed).is_none(),
            "the night stays dismissed once the hour ends"
        );
        assert_eq!(
            choose(vec![hourly(2), night(0)], KEYBOARD, dismissed).map(|a| a.rule),
            Some("hourly".into()),
            "the next hour"
        );
        assert_eq!(
            choose(vec![night(1)], KEYBOARD, dismissed).map(|a| a.rule),
            Some("night".into()),
            "the next night"
        );
    }

    /// A hidden run of back-to-back occurrences is dismissed as a run: the
    /// occurrence after the gesture continues it and stays dismissed.
    #[test]
    fn a_gesture_ends_a_hidden_run_as_a_run() {
        let mut tables = Tables::default();
        tables
            .running
            .insert(KEYBOARD, running("hourly", 0, 10_000));
        dismiss_runs(
            &mut tables,
            KEYBOARD,
            vec![
                occurrence("hourly", 0, 10_000),
                occurrence("steady", 5_000, 6_000),
            ],
        );
        assert!(choose(
            vec![occurrence("steady", 6_000, 7_000)],
            KEYBOARD,
            &mut tables.dismissed
        )
        .is_none());
    }

    /// Past the second means just past it: a tick never lands before the second
    /// it is about.
    #[test]
    fn the_scheduler_wakes_just_past_the_next_second() {
        assert_eq!(until_next_second(10_000), Duration::from_millis(1005));
        assert_eq!(until_next_second(10_999), Duration::from_millis(6));
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
