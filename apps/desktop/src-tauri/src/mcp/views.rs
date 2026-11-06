//! What the tools and resources return (`docs/design/mcp-server.md` §4.2, §6),
//! built from the window's own types by pure functions.
//!
//! What MCP leaves out (§3.6) is left out here, by construction: no type below
//! has a field for a signal's value, a serial number, a path or an applied
//! effect's frames, and the error of an effect reading keys or signals is
//! withheld. The tests build these from made-up data, with no Tauri and no
//! device.
//!
//! The schemas the tools declare are derived from these types: what a field
//! means is said once, in its documentation, which schemars copies into the
//! schema the model reads.

use std::collections::BTreeMap;

use chrono::TimeZone;
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::automations::resolver::{self, Trigger};
use crate::runtime::{DeviceEngineStatus, EngineStatus, RegionEngineStatus};
use crate::signals::store::SignalView;
use crate::storage::{DeviceState, EffectEntry, EffectKind, EffectState, Settings};
use crate::{DeviceInfo, DeviceRef, LayoutInfo, RegionInfo};

// ---------------------------------------------------------------- devices

/// Every device Candeo knows, plugged in or not.
#[derive(Serialize, JsonSchema, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DevicesView {
    pub devices: Vec<DeviceView>,
    /// Automations are paused: no rule interrupts any device until they resume.
    pub automations_paused: bool,
}

/// What the person decided for a device.
#[derive(Serialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DecisionView {
    /// Listed, not opened: nobody decided anything yet.
    Detected,
    /// Controlled: Candeo opens it whenever it is plugged in.
    Adopted,
    /// Left alone: Candeo never opens it.
    Ignored,
}

impl From<DeviceState> for DecisionView {
    fn from(state: DeviceState) -> Self {
        match state {
            DeviceState::Detected => Self::Detected,
            DeviceState::Adopted => Self::Adopted,
            DeviceState::Ignored => Self::Ignored,
        }
    }
}

#[derive(Serialize, JsonSchema, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeviceView {
    /// What every other tool names this device by.
    pub device: DeviceRef,
    pub name: String,
    /// Plugged in now.
    pub present: bool,
    pub state: DecisionView,
    /// Open now: an effect applied to it reaches it.
    pub open: bool,
    /// Its definition was never verified on the device.
    pub unverified: bool,
    /// The firmware read when it was opened.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware: Option<String>,
    /// `keys`, which are pressed and sit in rows, or `zones`, lit and never typed on.
    pub lights: String,
    pub light_count: usize,
    /// What an effect may require of it: `matrix`, `geometry`.
    pub capabilities: Vec<String>,
    /// Split into regions, each running its own effect.
    pub split: bool,
    /// The regions its definition declares, split or not.
    pub regions: Vec<RegionView>,
    /// What runs on the whole device; absent while nothing runs there, and while
    /// it is split.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs: Option<RunView>,
    /// What runs on each region, while it is split.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub region_runs: Vec<RegionRunView>,
    /// The rule showing its effect instead of the applied one, for now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interruption: Option<InterruptionView>,
    /// Frames actually reach the device.
    pub reaching_keyboard: bool,
    /// Why it could not be opened, as an error code (`errors.<code>`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_error: Option<String>,
    /// What its inspection on opening found worth knowing, in English.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Serialize, JsonSchema, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RegionView {
    /// What settings and tools name it by: `keyboard`, `logo`.
    pub id: String,
    /// As its definition writes it: a text, or one per language.
    pub name: Value,
    pub lights: String,
    pub light_count: usize,
    pub capabilities: Vec<String>,
}

impl From<&RegionInfo> for RegionView {
    fn from(region: &RegionInfo) -> Self {
        Self {
            id: region.id.to_owned(),
            name: serde_json::to_value(&region.name).unwrap_or(Value::Null),
            lights: region.lights.to_owned(),
            light_count: region.positions.len(),
            capabilities: strings(&region.capabilities),
        }
    }
}

/// An effect on a device or a region.
#[derive(Serialize, JsonSchema, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunView {
    /// The effect's id, `shipped:Rain` or `user:Rain`, or a firmware effect,
    /// `hardware:wave`.
    pub effect: String,
    /// False once it stopped on its own, after an error.
    pub running: bool,
    /// The settings kept for it here: what departs from its declared defaults.
    pub params: Map<String, Value>,
    /// Settings that follow a signal, the sound or the computer instead of their
    /// value: `{"colour": "signal:status"}`.
    pub bindings: BTreeMap<String, String>,
    /// Why it stopped, as its code said it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// It failed, and its text is not handed out: an effect reading key presses
    /// or signals writes what it likes into it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub error_withheld: bool,
}

#[derive(Serialize, JsonSchema, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RegionRunView {
    pub region: String,
    /// Absent while nothing runs there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs: Option<RunView>,
}

#[derive(Serialize, JsonSchema, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InterruptionView {
    /// The rule's id.
    pub rule: String,
    /// Its name, as someone wrote it; empty when they did not.
    pub name: String,
    /// The effect it shows.
    pub effect: String,
    /// When it ends, in milliseconds since the epoch; absent while occurrences
    /// keep following one another.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<i64>,
}

/// A run's error, unless its effect reads what must not leave the window.
fn error_of(error: Option<&String>, private: bool) -> (Option<String>, bool) {
    match error {
        Some(_) if private => (None, true),
        Some(error) => (Some(error.clone()), false),
        None => (None, false),
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_owned()).collect()
}

/// The devices, what runs on them and the pause, from what the window reads:
/// `list_devices`, `engine_status` and `settings.json`.
pub(crate) fn devices_view(
    devices: &[DeviceInfo],
    engine: &[DeviceEngineStatus],
    settings: &Settings,
) -> DevicesView {
    let rules: Vec<resolver::Rule> = settings.rules().into_iter().flatten().collect();
    let views = devices
        .iter()
        .map(|info| {
            let device = DeviceRef {
                vid: info.vid,
                pid: info.pid,
            };
            let status = engine
                .iter()
                .find(|s| s.device == device)
                .map(|s| &s.status);
            let interruption = status.and_then(|s| s.interruption.as_ref());
            // A rule shows its effect with its own settings, not the device's.
            let rule = interruption.and_then(|i| rules.iter().find(|r| r.id == i.rule));
            let (runs, region_runs) = match status {
                Some(EngineStatus {
                    regions: Some(regions),
                    ..
                }) => (
                    None,
                    regions
                        .iter()
                        .map(|r| region_run(r, device, settings))
                        .collect(),
                ),
                Some(status) => (whole_run(status, device, settings, rule), Vec::new()),
                None => (None, Vec::new()),
            };
            DeviceView {
                device,
                name: info.name.clone(),
                present: info.present,
                state: info.state.into(),
                open: info.open,
                unverified: info.unverified,
                firmware: info.firmware.clone(),
                lights: info.lights.to_owned(),
                light_count: info.light_count,
                capabilities: strings(&info.capabilities),
                split: info.split,
                regions: info.regions.iter().map(RegionView::from).collect(),
                runs,
                region_runs,
                interruption: interruption.map(|i| InterruptionView {
                    rule: i.rule.clone(),
                    name: i.name.clone(),
                    effect: i.effect.clone(),
                    until: i.until,
                }),
                reaching_keyboard: status.is_some_and(|s| s.reaching_keyboard),
                open_error: info.error.as_ref().map(|f| f.code.to_owned()),
                warnings: info.warnings.clone(),
            }
        })
        .collect();
    DevicesView {
        devices: views,
        automations_paused: settings.preferences.automations_paused,
    }
}

fn whole_run(
    status: &EngineStatus,
    device: DeviceRef,
    settings: &Settings,
    rule: Option<&resolver::Rule>,
) -> Option<RunView> {
    let effect = status.effect_id.clone()?;
    let (params, bindings) = match rule.filter(|r| r.show.effect == effect) {
        Some(rule) => (rule.show.params.clone(), rule.show.bindings.clone()),
        None => (
            settings
                .effect_params(device.vid, device.pid, &effect)
                .cloned()
                .unwrap_or_default(),
            settings.effect_bindings(device.vid, device.pid, &effect),
        ),
    };
    let (error, error_withheld) = error_of(status.error.as_ref(), status.reads_private);
    Some(RunView {
        effect,
        running: status.running,
        params,
        bindings,
        error,
        error_withheld,
    })
}

fn region_run(
    region: &RegionEngineStatus,
    device: DeviceRef,
    settings: &Settings,
) -> RegionRunView {
    let runs = region.effect_id.clone().map(|effect| {
        let kept = settings.split(device.vid, device.pid).and_then(|split| {
            split
                .effect_params
                .iter()
                .find(|p| p.region == region.region && p.effect == effect)
        });
        let (error, error_withheld) = error_of(region.error.as_ref(), region.reads_private);
        RunView {
            params: kept.map(|k| k.values.clone()).unwrap_or_default(),
            bindings: kept.map(|k| k.bindings.clone()).unwrap_or_default(),
            effect,
            running: region.running,
            error,
            error_withheld,
        }
    });
    RegionRunView {
        region: region.region.clone(),
        runs,
    }
}

/// What runs everywhere, the `candeo://state` resource: [`DevicesView`] without
/// what describes the devices themselves.
#[derive(Serialize, JsonSchema, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StateView {
    pub devices: Vec<DeviceStateView>,
    pub automations_paused: bool,
}

#[derive(Serialize, JsonSchema, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeviceStateView {
    pub device: DeviceRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs: Option<RunView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub region_runs: Vec<RegionRunView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interruption: Option<InterruptionView>,
    pub reaching_keyboard: bool,
}

pub(crate) fn state_view(devices: DevicesView) -> StateView {
    StateView {
        devices: devices
            .devices
            .into_iter()
            .map(|d| DeviceStateView {
                device: d.device,
                runs: d.runs,
                region_runs: d.region_runs,
                interruption: d.interruption,
                reaching_keyboard: d.reaching_keyboard,
            })
            .collect(),
        automations_paused: devices.automations_paused,
    }
}

// ---------------------------------------------------------------- layouts

/// A device's lights and their geometry, as the simulator draws them.
#[derive(Serialize, JsonSchema, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LayoutView {
    pub name: String,
    /// The matrix's size: `row` and `col` of each light fall within it.
    pub rows: u8,
    pub cols: u8,
    /// Positions in a frame, the matrix's holes included.
    pub frame_len: usize,
    pub lights: String,
    pub capabilities: Vec<String>,
    /// The lights, in index order. An effect receives exactly these as
    /// `layout.keys`.
    pub keys: Vec<KeyView>,
    pub regions: Vec<RegionView>,
    /// Parts that light nothing, drawn under the lights: empty for a keyboard.
    pub outline: Vec<OutlineView>,
}

/// One light. `x`, `y`, `w` and `h` are its rectangle in keyboard pitch units
/// (1 u is one letter key), origin at the top left; `row` and `col` its place
/// in the matrix, which says nothing of its size.
#[derive(Serialize, JsonSchema, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KeyView {
    pub index: u16,
    pub row: u8,
    pub col: u8,
    /// The key's name in the computer's keyboard layout.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// `disc`, `archUp` or `archDown`; absent for a rectangle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
}

#[derive(Serialize, JsonSchema, Debug, PartialEq)]
pub(crate) struct OutlineView {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Corner radius.
    pub r: f32,
}

impl From<LayoutInfo> for LayoutView {
    fn from(l: LayoutInfo) -> Self {
        Self {
            name: l.name,
            rows: l.rows,
            cols: l.cols,
            frame_len: l.frame_len,
            lights: l.lights.to_owned(),
            capabilities: strings(&l.capabilities),
            keys: l
                .keys
                .into_iter()
                .map(|k| KeyView {
                    index: k.index,
                    row: k.row,
                    col: k.col,
                    label: k.label,
                    x: k.x,
                    y: k.y,
                    w: k.w,
                    h: k.h,
                    shape: k.shape.map(str::to_owned),
                })
                .collect(),
            regions: l.regions.iter().map(RegionView::from).collect(),
            outline: l
                .outline
                .into_iter()
                .map(|o| OutlineView {
                    x: o.x,
                    y: o.y,
                    w: o.w,
                    h: o.h,
                    r: o.r,
                })
                .collect(),
        }
    }
}

// ---------------------------------------------------------------- effects

#[derive(Serialize, JsonSchema, Debug, PartialEq)]
pub(crate) struct EffectsView {
    pub effects: Vec<EffectView>,
}

#[derive(Serialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum KindView {
    /// Shipped with Candeo.
    Builtin,
    /// Written, duplicated or dropped in by the person.
    User,
}

#[derive(Serialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum EffectStateView {
    /// Compiled: it can be previewed and applied.
    Ready,
    /// Not compiled since it last changed: the window compiles it.
    Stale,
    /// It does not load: `error` says why.
    Broken,
}

#[derive(Serialize, JsonSchema, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EffectView {
    /// What tools name it by: `shipped:Rain`, `user:Rain`.
    pub id: String,
    /// Its file's name.
    pub name: String,
    pub kind: KindView,
    pub state: EffectStateView,
    /// Why it does not load, in English.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// What it does: a text, or one per language.
    pub description: Value,
    /// Its settings, as it declares them: each a `ParamSpec` of the effects API.
    pub params: Map<String, Value>,
    pub api_version: u32,
    /// What it reads besides time and its settings: `keys`, `clock`, `signals`,
    /// `audio`, `system`.
    pub inputs: Vec<String>,
    /// The lights it makes sense on: `keys`, `zones` or `all`.
    pub kinds: Vec<String>,
    /// What it needs of a device: `matrix`, `geometry`.
    pub requires: Vec<String>,
    /// The game it is for, `cs2`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    /// A few colours it shows, `#rrggbb`, sampled by running it.
    pub swatch: Vec<String>,
}

impl From<EffectEntry> for EffectView {
    fn from(e: EffectEntry) -> Self {
        let m = e.manifest;
        let inputs = [
            (m.reads_keys, "keys"),
            (m.reads_clock, "clock"),
            (m.reads_signals, "signals"),
            (m.reads_audio, "audio"),
            (m.reads_system, "system"),
        ]
        .into_iter()
        .filter(|(reads, _)| *reads)
        .map(|(_, input)| input.to_owned())
        .collect();
        Self {
            id: e.id,
            name: m.name,
            kind: match e.kind {
                EffectKind::Builtin => KindView::Builtin,
                EffectKind::User => KindView::User,
            },
            state: match e.state {
                EffectState::Ready => EffectStateView::Ready,
                EffectState::Stale => EffectStateView::Stale,
                EffectState::Broken => EffectStateView::Broken,
            },
            error: e.error,
            description: m.description,
            params: m.params,
            api_version: m.api_version,
            inputs,
            kinds: m.kinds,
            requires: m.requires,
            game: m.game,
            swatch: e.swatch,
        }
    }
}

/// An effect with its source, for `get_effect`.
#[derive(Serialize, JsonSchema, Debug, PartialEq)]
pub(crate) struct EffectSourceView {
    pub effect: EffectView,
    /// Its TypeScript, as the file holds it.
    pub source: String,
}

// ---------------------------------------------------------------- presets

#[derive(Serialize, JsonSchema, Debug, PartialEq)]
pub(crate) struct PresetsView {
    pub presets: Vec<PresetView>,
}

/// Named settings for one effect, offered on every device and region.
#[derive(Serialize, JsonSchema, Debug, PartialEq)]
pub(crate) struct PresetView {
    /// The effect it sets.
    pub effect: String,
    pub name: String,
    /// Every setting the effect declared when the preset was saved.
    pub values: Map<String, Value>,
    pub bindings: BTreeMap<String, String>,
}

pub(crate) fn presets_view(settings: &Settings, effect: Option<&str>) -> PresetsView {
    PresetsView {
        presets: settings
            .presets
            .iter()
            .filter(|p| effect.is_none_or(|e| p.effect == e))
            .map(|p| PresetView {
                effect: p.effect.clone(),
                name: p.name.clone(),
                values: p.values.clone(),
                bindings: p.bindings.clone(),
            })
            .collect(),
    }
}

// ---------------------------------------------------------------- automations

#[derive(Serialize, JsonSchema, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationsView {
    /// Paused: no rule applies until they resume; trying one still works.
    pub paused: bool,
    /// The rules in priority order: the first that applies on a device wins.
    pub rules: Vec<RuleView>,
}

#[derive(Serialize, JsonSchema, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuleView {
    /// The rule, in the shape `check_automation` takes; as written when it does
    /// not read.
    pub rule: Value,
    /// Why it does nothing: it does not read, or cannot apply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The devices it interrupts now.
    pub applies_now: Vec<DeviceRef>,
}

pub(crate) fn automations_view(
    settings: &Settings,
    engine: &[DeviceEngineStatus],
) -> AutomationsView {
    let rules = settings
        .rules
        .iter()
        .zip(settings.rules())
        .map(|(raw, parsed)| {
            let (rule, error) = match parsed {
                Ok(rule) => (
                    serde_json::to_value(&rule).unwrap_or_else(|_| raw.clone()),
                    None,
                ),
                Err(error) => (raw.clone(), Some(error)),
            };
            let id = rule.get("id").and_then(Value::as_str).unwrap_or_default();
            let applies_now = engine
                .iter()
                .filter(|s| {
                    !id.is_empty() && s.status.interruption.as_ref().is_some_and(|i| i.rule == id)
                })
                .map(|s| s.device)
                .collect();
            RuleView {
                rule,
                error,
                applies_now,
            }
        })
        .collect();
    AutomationsView {
        paused: settings.preferences.automations_paused,
        rules,
    }
}

#[derive(Serialize, JsonSchema, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CheckView {
    /// The rule reads and can apply.
    pub ok: bool,
    /// Why not, in English.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// For a cron rule, its next three starts, in the computer's local time.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub next_starts: Vec<String>,
    /// The devices it names that Candeo does not know.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unknown_devices: Vec<DeviceRef>,
}

/// Whether a rule would be accepted, without saving it: `set_rules`' check,
/// its next starts from `now`, and the devices nobody knows.
pub(crate) fn check_view<Tz: TimeZone>(
    rule: &Value,
    known: &[DeviceRef],
    now: &chrono::DateTime<Tz>,
) -> CheckView
where
    Tz::Offset: std::fmt::Display,
{
    match resolver::parse(std::slice::from_ref(rule)).remove(0) {
        Err(error) => CheckView {
            ok: false,
            error: Some(error),
            next_starts: Vec::new(),
            unknown_devices: Vec::new(),
        },
        Ok(rule) => {
            let mut next_starts = Vec::new();
            if let (Trigger::Cron { .. }, Ok(cron)) = (&rule.when, rule.cron()) {
                let mut from = now.clone();
                for _ in 0..3 {
                    let Ok(next) = cron.find_next_occurrence(&from, false) else {
                        break;
                    };
                    next_starts.push(next.to_rfc3339_opts(chrono::SecondsFormat::Secs, false));
                    from = next;
                }
            }
            CheckView {
                ok: true,
                error: None,
                next_starts,
                unknown_devices: rule
                    .devices
                    .iter()
                    .filter(|d| !known.contains(d))
                    .copied()
                    .collect(),
            }
        }
    }
}

// ---------------------------------------------------------------- signals

/// The signals held, by name. Their values are never handed out: `PRIVACY.md`
/// promises they are sent nowhere, and an assistant passes what it reads to
/// its model.
#[derive(Serialize, JsonSchema, Debug, PartialEq)]
pub(crate) struct SignalsView {
    /// Candeo receives signals: the signals API is on in Settings.
    pub receiving: bool,
    pub signals: Vec<HeldView>,
}

#[derive(Serialize, JsonSchema, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HeldView {
    pub name: String,
    /// Milliseconds until it expires; absent for a signal held until erased.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_ms: Option<i64>,
}

pub(crate) fn signals_view(receiving: bool, held: &[SignalView], now: i64) -> SignalsView {
    SignalsView {
        receiving,
        signals: held
            .iter()
            .filter(|s| s.expires.is_none_or(|at| at > now))
            .map(|s| HeldView {
                name: s.name.clone(),
                expires_in_ms: s.expires.map(|at| at - now),
            })
            .collect(),
    }
}

// ---------------------------------------------------------------- the API

/// The installed effects API's version, and where its declaration and guide are.
#[derive(Serialize, JsonSchema, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EffectsApiView {
    /// The version an effect declares as `apiVersion`; an effect written for a
    /// newer one does not load.
    pub api_version: u32,
    pub candeo_version: String,
    /// The resources this result embeds: the declaration, then the guide.
    pub resources: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automations::InterruptionStatus;
    use crate::catalog::Origin;
    use crate::failure::Failure;
    use crate::signals::store::Scalar;
    use crate::storage::{EffectParamsRecord, Manifest};

    const KEYBOARD: DeviceRef = DeviceRef {
        vid: 0x1532,
        pid: 0x0292,
    };

    fn device(name: &str) -> DeviceInfo {
        DeviceInfo {
            name: name.into(),
            vid: KEYBOARD.vid,
            pid: KEYBOARD.pid,
            present: true,
            state: DeviceState::Adopted,
            open: true,
            error: None,
            surveyed_firmware: "v1.5".into(),
            firmware: Some("v1.5".into()),
            warnings: Vec::new(),
            origin: Origin::Yours,
            // Every field that could carry a path or a serial is made up, so a
            // leak would show in the JSON below.
            file: Some("XY24ABCDEFG0001.json".into()),
            definitions: Vec::new(),
            unloaded_choice: Some("C:\\Users\\someone\\devices\\mine.json".into()),
            unverified: false,
            lights: "keys",
            capabilities: vec!["matrix", "geometry"],
            light_count: 106,
            hardware_brightness: true,
            regions: Vec::new(),
            split: false,
        }
    }

    fn running(effect: &str, error: Option<&str>, private: bool) -> DeviceEngineStatus {
        DeviceEngineStatus {
            device: KEYBOARD,
            status: EngineStatus {
                running: error.is_none(),
                effect_id: Some(effect.into()),
                error: error.map(str::to_owned),
                reaching_keyboard: error.is_none(),
                to_keyboard: true,
                reads_private: private,
                ..EngineStatus::default()
            },
        }
    }

    #[test]
    fn a_device_says_what_runs_with_its_kept_settings() {
        let mut settings = Settings::default();
        settings.effect_params.push(EffectParamsRecord {
            vid: KEYBOARD.vid,
            pid: KEYBOARD.pid,
            effect: "shipped:Rain".into(),
            values: serde_json::json!({ "speed": 3 })
                .as_object()
                .unwrap()
                .clone(),
            bindings: BTreeMap::from([("colour".into(), "signal:status".into())]),
        });
        let view = devices_view(
            &[device("DeathStalker V2 Pro")],
            &[running("shipped:Rain", None, false)],
            &settings,
        );
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(
            json["devices"][0]["runs"],
            serde_json::json!({
                "effect": "shipped:Rain",
                "running": true,
                "params": { "speed": 3 },
                "bindings": { "colour": "signal:status" },
            })
        );
        assert_eq!(
            json["devices"][0]["device"],
            serde_json::json!({ "vid": 0x1532, "pid": 0x0292 })
        );
        assert_eq!(json["devices"][0]["state"], "adopted");
        assert_eq!(json["automationsPaused"], false);
        // No file name, no path, no serial reaches the assistant.
        let text = json.to_string();
        for leak in ["XY24ABCDEFG0001", "someone", "mine.json", "surveyed"] {
            assert!(!text.contains(leak), "{leak} in {text}");
        }
    }

    /// The error of an effect reading keys or signals stays in the window: MCP
    /// says it failed, not what it said.
    #[test]
    fn the_error_of_an_effect_reading_keys_is_withheld() {
        let settings = Settings::default();
        let secret = "pressed: h, u, n, t, e, r, 2";
        let view = devices_view(
            &[device("Keyboard")],
            &[running("user:Typing", Some(secret), true)],
            &settings,
        );
        let runs = view.devices[0].runs.as_ref().unwrap();
        assert_eq!(runs.error, None);
        assert!(runs.error_withheld);
        assert!(!serde_json::to_string(&view).unwrap().contains("pressed"));

        let view = devices_view(
            &[device("Keyboard")],
            &[running("user:Broken", Some("x is not defined"), false)],
            &settings,
        );
        let runs = view.devices[0].runs.as_ref().unwrap();
        assert_eq!(runs.error.as_deref(), Some("x is not defined"));
        assert!(!runs.error_withheld);
    }

    /// While a rule interrupts the device, what runs carries the rule's settings,
    /// and the rule says until when.
    #[test]
    fn an_interrupted_device_shows_the_rules_effect_and_settings() {
        let mut settings = Settings::default();
        settings.rules = vec![serde_json::json!({
            "id": "hourly", "name": "Every hour", "enabled": true,
            "devices": [{ "vid": KEYBOARD.vid, "pid": KEYBOARD.pid }],
            "when": { "kind": "cron", "expr": "0 * * * *" },
            "show": { "effect": "shipped:Clock", "params": { "hands": "red" } },
            "for": { "seconds": 10 }
        })];
        let mut status = running("shipped:Clock", None, false);
        status.status.interruption = Some(InterruptionStatus {
            rule: "hourly".into(),
            name: "Every hour".into(),
            effect: "shipped:Clock".into(),
            until: Some(1_000),
        });
        let view = devices_view(&[device("Keyboard")], &[status], &settings);
        let device = &view.devices[0];
        assert_eq!(device.runs.as_ref().unwrap().params["hands"], "red");
        assert_eq!(device.interruption.as_ref().unwrap().until, Some(1_000));
    }

    #[test]
    fn an_unopened_device_says_why_by_its_code_alone() {
        let mut info = device("Keyboard");
        info.open = false;
        info.error =
            Some(Failure::new("deviceAccess").with("detail", r"\\?\hid#vid_1532&pid_0292#7&2b"));
        let view = devices_view(&[info], &[], &Settings::default());
        assert_eq!(view.devices[0].open_error.as_deref(), Some("deviceAccess"));
        assert!(view.devices[0].runs.is_none());
        assert!(!serde_json::to_string(&view).unwrap().contains("hid#"));
    }

    #[test]
    fn the_state_resource_is_what_runs_and_nothing_else() {
        let view = devices_view(
            &[device("Keyboard")],
            &[running("shipped:Rain", None, false)],
            &Settings::default(),
        );
        let state = serde_json::to_value(state_view(view)).unwrap();
        assert_eq!(state["devices"][0]["runs"]["effect"], "shipped:Rain");
        assert!(state["devices"][0].get("name").is_none());
    }

    #[test]
    fn signals_are_named_never_valued() {
        let held = [
            SignalView {
                name: "build".into(),
                value: Scalar::Text("failed on the secret branch".into()),
                received: 0,
                expires: Some(10_000),
            },
            SignalView {
                name: "door".into(),
                value: Scalar::Flag(true),
                received: 0,
                expires: None,
            },
            SignalView {
                name: "gone".into(),
                value: Scalar::Number(1.0),
                received: 0,
                expires: Some(500),
            },
        ];
        let view = signals_view(true, &held, 1_000);
        assert_eq!(
            serde_json::to_value(&view).unwrap(),
            serde_json::json!({
                "receiving": true,
                "signals": [{ "name": "build", "expiresInMs": 9000 }, { "name": "door" }],
            })
        );
        assert!(!serde_json::to_string(&view).unwrap().contains("secret"));
    }

    #[test]
    fn a_rule_is_checked_and_its_next_starts_given() {
        let now = chrono::FixedOffset::east_opt(3600)
            .unwrap()
            .with_ymd_and_hms(2026, 11, 6, 21, 30, 0)
            .unwrap();
        let rule = serde_json::json!({
            "id": "night", "enabled": true,
            "devices": [{ "vid": KEYBOARD.vid, "pid": KEYBOARD.pid }, { "vid": 1, "pid": 2 }],
            "when": { "kind": "cron", "expr": "0 22 * * *" },
            "show": { "effect": "shipped:Starry night" },
            "for": { "seconds": 32400 }
        });
        let view = check_view(&rule, &[KEYBOARD], &now);
        assert!(view.ok);
        assert_eq!(
            view.next_starts,
            [
                "2026-11-06T22:00:00+01:00",
                "2026-11-07T22:00:00+01:00",
                "2026-11-08T22:00:00+01:00"
            ]
        );
        assert_eq!(view.unknown_devices, [DeviceRef { vid: 1, pid: 2 }]);

        let broken = serde_json::json!({ "id": "x", "devices": [], "when": { "kind": "cron", "expr": "every hour" }, "show": { "effect": "a" } });
        let view = check_view(&broken, &[KEYBOARD], &now);
        assert!(!view.ok);
        assert!(view.error.unwrap().contains("not a cron expression"));
    }

    #[test]
    fn rules_come_in_order_with_their_errors_and_where_they_apply() {
        let mut settings = Settings::default();
        settings.preferences.automations_paused = true;
        settings.rules = vec![
            serde_json::json!({ "id": "broken", "when": "soon" }),
            serde_json::json!({
                "id": "hourly", "enabled": true,
                "devices": [{ "vid": KEYBOARD.vid, "pid": KEYBOARD.pid }],
                "when": { "kind": "cron", "expr": "0 * * * *" },
                "show": { "effect": "shipped:Clock" }
            }),
        ];
        let mut status = running("shipped:Clock", None, false);
        status.status.interruption = Some(InterruptionStatus {
            rule: "hourly".into(),
            name: String::new(),
            effect: "shipped:Clock".into(),
            until: None,
        });
        let view = automations_view(&settings, &[status]);
        assert!(view.paused);
        assert_eq!(view.rules[0].rule["when"], "soon");
        assert!(view.rules[0].error.is_some());
        assert!(view.rules[0].applies_now.is_empty());
        assert_eq!(view.rules[1].rule["for"]["seconds"], 10, "defaults filled");
        assert_eq!(view.rules[1].applies_now, [KEYBOARD]);
    }

    #[test]
    fn an_effect_says_what_it_reads() {
        let entry = EffectEntry {
            id: "user:Typing".into(),
            kind: EffectKind::User,
            state: EffectState::Ready,
            error: None,
            hash: Some("abc".into()),
            modified: false,
            swatch: vec!["#ff0000".into()],
            manifest: Manifest {
                name: "Typing".into(),
                description: Value::String("Lights the keys pressed".into()),
                params: Map::new(),
                api_version: 1,
                reads_keys: true,
                reads_clock: false,
                reads_signals: false,
                reads_audio: true,
                reads_system: false,
                game: None,
                kinds: vec!["keys".into()],
                requires: vec!["geometry".into()],
            },
        };
        let view = EffectView::from(entry);
        assert_eq!(view.inputs, ["keys", "audio"]);
        assert_eq!(view.kind, KindView::User);
        assert!(serde_json::to_value(&view).unwrap().get("hash").is_none());
    }

    #[test]
    fn presets_are_filtered_by_effect() {
        let mut settings = Settings::default();
        for (effect, name) in [
            ("shipped:Rain", "Storm"),
            ("shipped:Sweep", "Slow"),
            ("shipped:Rain", "Drizzle"),
        ] {
            settings.presets.push(crate::presets::PresetRecord {
                effect: effect.into(),
                name: name.into(),
                values: Map::new(),
                bindings: BTreeMap::new(),
                unknown: Map::new(),
            });
        }
        let names =
            |view: PresetsView| view.presets.into_iter().map(|p| p.name).collect::<Vec<_>>();
        assert_eq!(
            names(presets_view(&settings, Some("shipped:Rain"))),
            ["Storm", "Drizzle"]
        );
        assert_eq!(names(presets_view(&settings, None)).len(), 3);
    }
}
