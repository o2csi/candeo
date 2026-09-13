//! Storage for effects and settings.
//!
//! Two separate locations, described in
//! [`docs/design/effects-runtime.md`](../../../../docs/design/effects-runtime.md) §3:
//!
//! ```text
//! app_data_dir()/effects/<name>/   source.ts · effect.js · manifest.json · swatch.json
//! app_config_dir()/settings.json   preferences · devices · applied effect · effect parameters
//! ```
//!
//! The effect is **content**; the choice of the active effect is
//! **configuration**. On Windows both directories are the same, on Linux they
//! are not — hence going through the Tauri API rather than a constant.
//!
//! # An effect is its uid, not its directory
//!
//! Every effect carries a `uid` (a lowercase UUID) in its manifest, and that is
//! the id the rest of the application uses: the engine, `settings.json`, the
//! tray menu. The directory name is derived from the effect's name when it is
//! first saved, for whoever browses the folder, and nothing decides on it
//! afterwards: renaming an effect keeps its directory, its uid and its settings.
//! See [`docs/design/effects-library.md`](../../../../docs/design/effects-library.md) §1.
//!
//! # The shape of the file: preferences on one side, devices on the other
//!
//! `settings.json` holds two things that do not belong together: what applies to
//! the whole application ([`Preferences`]) and what is **indexed by device**
//! (`devices`, `activeEffects`, `effectParams`). Mixing them at the root is what
//! produced the three single-device leftovers removed here: `activeEffect`,
//! `device` and `brightness` described **one** effect, **one** device and **one**
//! level, while the engine has run one effect per device since issue #26. It was
//! not the wrong value, it was the wrong **shape** — and bringing it back as it
//! was would have produced a file that misdescribes reality.
//!
//! The rule that follows holds for everything added later: **a global preference
//! goes in `preferences`, anything that depends on a keyboard goes in an indexed
//! list.** The language, when it arrives, therefore has nothing to decide.
//!
//! All file handling lives in [`Store`], which receives its base paths as
//! arguments; the Tauri commands only resolve them. That is what makes it all
//! testable in a temporary directory, without an application.
//!
//! # Built-in effects are part of the library
//!
//! They have no directory — they are compiled into the binary, see
//! [`crate::builtins`] — but the caller does not need to know: listing, reading
//! the JavaScript or the source finds them like the others.
//!
//! **On a uid clash, the built-in wins**, and the clash is refused at install
//! anyway. The direction of the priority is not arbitrary: an entry marked
//! `builtin` in the gallery must run the shipped code, and nothing else. The
//! reverse would let a user effect slip in under a known uid, with the
//! built-in's manifest shown on screen and other code running — exactly what we
//! refuse. Refusing the uid at install makes the situation impossible; the
//! priority at read time is the second barrier, for a directory that arrived by
//! another path (a manual copy).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::builtins;
use crate::journal::LogLevel;
use crate::runtime::swatch::{self, Swatch};
use crate::{AppState, CmdResult, DeviceRef};

/// Version of the effects API provided by this version of the application.
///
/// A manifest declares the version the effect was written against: that is what
/// will allow cleanly refusing an effect written against an API that no longer
/// exists, rather than letting it fail on the first frame.
pub const EFFECTS_API_VERSION: u32 = 1;

/// Maximum length of an effect id, and so of a directory name.
const MAX_ID_LEN: usize = 64;

const SOURCE_FILE: &str = "source.ts";
const JS_FILE: &str = "effect.js";
const MANIFEST_FILE: &str = "manifest.json";
/// Color swatch, next to the manifest. See [`crate::runtime::swatch`].
const SWATCH_FILE: &str = "swatch.json";

/// Names reserved by Windows: a directory with such a name is refused by the
/// system, in any directory.
const RESERVED_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

// ---------------------------------------------------------------- exposed types

/// An effect's manifest, written as is to `manifest.json`.
///
/// camelCase like the other exposed types: the manifest comes from the editor
/// and goes back to it, and `params` already holds JSON written on the
/// TypeScript side. A single snake_case field in the middle would only show at
/// run time.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// The effect's identity. See the module documentation.
    ///
    /// Empty only in a `manifest.json` written before uids, until
    /// [`Store::migrate_effect_ids`] gives it one. Left out of the library
    /// listing, where it travels as [`EffectEntry::id`].
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub uid: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Declared parameters, as the interface will present them.
    ///
    /// Kept as raw JSON: their shape is that of `ParamSpec` on the TypeScript
    /// side, it evolves with the editor, and the Rust side does not interpret
    /// them. Typing them here would create a second, unused source of truth.
    #[serde(default)]
    pub params: serde_json::Map<String, serde_json::Value>,
    /// Version of the effects API used when writing it.
    pub api_version: u32,
}

/// Kind of effect: written by the user, or compiled into the binary.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EffectKind {
    /// Shipped with the application, with no directory on disk.
    Builtin,
    /// Installed by the user, under `effects/<id>/`.
    User,
}

/// Library entry: the manifest, plus what is not part of it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EffectEntry {
    /// The effect's uid, moved out of [`Manifest::uid`] so the listing does not
    /// carry it twice.
    pub id: String,
    pub kind: EffectKind,
    /// Color swatch, **sampled by running the effect**.
    ///
    /// It is not in the manifest, and that is not a filing detail: the manifest
    /// is what the author declares, the swatch is what the effect does. Mixing
    /// them would reopen the door to a hand-written swatch, and so to a swatch
    /// that lies.
    ///
    /// Carried by the entry so that the list is enough to show it: a thumbnail
    /// needing a second call per effect would make as many round trips as the
    /// library has entries.
    ///
    /// Empty when it could not be computed — see [`crate::runtime::swatch`].
    /// The interface then falls back to a neutral dot.
    pub swatch: Swatch,
    #[serde(flatten)]
    pub manifest: Manifest,
}

/// Brightness of a keyboard that was just plugged in: full.
///
/// It is the least surprising default, and it is also the value the file
/// **does not write** — see [`DeviceRecord::brightness`].
pub const DEFAULT_BRIGHTNESS: u8 = 255;

/// Decision made for a device, once, and kept.
///
/// The default is [`Detected`](DeviceState::Detected): **a device never seen
/// before is not controlled**. Writing to a USB device we barely understand is
/// not harmless, and for a catalog that grows — keyboards, mice, memory, fans —
/// adopting by default is how you break someone's hardware.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DeviceState {
    /// Listed, but **not** opened. The state of every device nobody has made a
    /// decision about yet.
    #[default]
    Detected,
    /// Opened automatically at startup, without asking anything.
    Adopted,
    /// Left alone, and stays that way.
    Ignored,
}

/// What `settings.json` keeps about a device: its identity, and the decision.
///
/// # Identity is VID / PID / serial number
///
/// **Neither the variant nor the firmware.** The same keyboard reported itself as
/// `v1.4 / Unkown Variant` then `v1.5 / Quartz` during the protocol survey: a
/// binding that matches on those fields breaks on update, and the adopted device
/// becomes a stranger overnight.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRecord {
    pub vid: u16,
    pub pid: u16,
    /// Serial number, when the system reports one.
    ///
    /// Absent from the file rather than `null`: most entries will not have one,
    /// and a repeated empty key tells nothing to whoever rereads their settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    pub state: DeviceState,
    /// Brightness kept for **this** device.
    ///
    /// # Why here rather than at the root
    ///
    /// It already is everywhere else: `Keyboard::set_brightness` is a device
    /// command (`0x0f`/`0x04`), separate from the running effect, and
    /// `set_brightness(device, level)` has taken a [`DeviceRef`] since day one.
    /// The global scalar in `Settings` was the only place that said otherwise —
    /// and two keyboards have no reason to share a level.
    ///
    /// # `None` means "the default", not "off"
    ///
    /// The file then holds nothing at all: writing [`DEFAULT_BRIGHTNESS`] for
    /// every device merely plugged in would grow it with entries that decide
    /// nothing. Same economy as `devices` and `effectParams` — an entry exists
    /// only if someone moved something. It is also why an entry back to
    /// `detected` **without** brightness disappears: see [`Self::is_inert`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<u8>,
}

impl DeviceRecord {
    /// True if this entry no longer keeps any decision.
    ///
    /// `detected` without brightness says exactly what the **absence** of an
    /// entry says. Keeping it would tell nothing to whoever rereads their
    /// settings, and would grow the file by one line per device touched once.
    fn is_inert(&self) -> bool {
        self.state == DeviceState::Detected && self.brightness.is_none()
    }

    /// True if this entry designates the enumerated device.
    ///
    /// VID and PID must match; the serial is compared only if **both sides**
    /// carry one. This is not laxity, it is the only rule that holds both ways:
    ///
    /// - the serial tells apart two units of the same model — without it,
    ///   adopting one would adopt the other;
    /// - but a silent enumeration — hidraw without a udev rule, a hub that
    ///   passes nothing on — must not unmatch an already adopted device,
    ///   otherwise the decision would have to be made again on every plug-in.
    pub fn matches(&self, vid: u16, pid: u16, serial: Option<&str>) -> bool {
        if self.vid != vid || self.pid != pid {
            return false;
        }
        match (self.serial.as_deref(), serial) {
            (Some(ours), Some(theirs)) => ours == theirs,
            _ => true,
        }
    }
}

/// The effect **applied** on a device, the one driving its LEDs.
///
/// # A list, not a scalar
///
/// The field before it — `activeEffect: Option<String>` — described **one**
/// active effect, while the engine has run one per device since issue #26. No
/// value could make that field right: its shape was wrong. One entry per
/// device, absent until something has been applied, is the only one that
/// describes what the engine actually does.
///
/// # It is not what is being previewed
///
/// **Preview never writes here.** Previewing an effect does not keep it: "Apply"
/// decides, and it is the action that sends to the keyboard. See
/// [`crate::runtime::start_preview`], which has no access to the disk.
///
/// # The key is the [`DeviceRef`], without serial number
///
/// Same reason as [`EffectParamsRecord`]: the engine loops are indexed by
/// VID/PID, two units of the same model share one, and telling them apart here
/// would promise a separation the engine does not keep.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveEffectRecord {
    pub vid: u16,
    pub pid: u16,
    /// Id of the applied effect.
    pub effect: String,
}

/// An effect's parameters, kept for **one** device.
///
/// # Why device and effect together
///
/// "The wave, but slower" is tuned on a given keyboard: the same effect has no
/// reason to run at the same speed on two devices, and two effects on the same
/// device do not have the same parameters. The key is therefore the pair, and
/// switching effects then coming back finds its parameters again.
///
/// # Without the serial number, unlike [`DeviceRecord`]
///
/// Deliberate: every engine command targets a [`DeviceRef`], that is a VID and a
/// PID. Two units of the same model already share their render loop — telling
/// them apart *here* would promise a separation the rest of the application does
/// not keep, and the setting would seem lost one time out of two. Adoption, on
/// the other hand, decides to open a specific device: it needs the serial, and
/// that is why it carries it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EffectParamsRecord {
    pub vid: u16,
    pub pid: u16,
    /// Id of the tuned effect.
    pub effect: String,
    /// The values, as the interface sends them to the engine.
    ///
    /// Raw JSON, like [`Manifest::params`]: their shape is that of `ParamValue`
    /// on the TypeScript side — a number, a string, a boolean or a `{r,g,b}`
    /// color — and the Rust side does not interpret them. Typing them here would
    /// create a second source of truth, which would diverge at the first
    /// parameter type added.
    pub values: serde_json::Map<String, serde_json::Value>,
}

/// What applies to the whole application, and to no device in particular.
///
/// A separate object rather than fields at the root: this layout is what
/// prevents the confusion this module just came out of. Anything that depends on
/// a keyboard lives in an indexed list; what does not lives here, and the
/// language — when it arrives — will have nothing to decide.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Preferences {
    /// Log level, when someone changed it from the application.
    ///
    /// `None` — and so absent from the file — means "the default", not "no
    /// log": writing the default would suggest a decision where there was none,
    /// and would freeze along the way a choice the next version might want to
    /// revisit.
    ///
    /// **It survives a restart**, and that is a trade-off: automatically going
    /// back to the default would protect against a full disk, persistence serves
    /// whoever is tracking a bug **at startup** — device adoption is one — who
    /// cannot be asked to raise the level after the fact. The price is paid by
    /// the notice the interface shows about it. See [`crate::journal`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_level: Option<LogLevel>,
}

/// Persistent settings.
///
/// `#[serde(default)]` on the whole struct: a `settings.json` written by an
/// earlier version, missing a field added since, reloads without error instead
/// of leaving the application silent at startup.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Shape of the file, for migrations that cannot be told from the content.
    ///
    /// A file without it predates the version field, and so the uids: it reads
    /// as 0, not as [`SETTINGS_VERSION`], which is what the new settings of a
    /// first launch get. See [`Store::migrate_effect_ids`].
    #[serde(default)]
    pub version: u32,
    /// What depends on no device. See [`Preferences`].
    pub preferences: Preferences,
    /// Decisions made device by device, brightness included.
    ///
    /// Holds only those that **differ from the default**: a device absent from
    /// this list is `detected` and at full brightness, which is exactly the state
    /// of a device never encountered. The file therefore does not grow by one
    /// entry for each device plugged in once.
    pub devices: Vec<DeviceRecord>,
    /// The effect **applied** on each device. See [`ActiveEffectRecord`].
    ///
    /// Same economy as the rest: no entry until something has been applied, and
    /// the entry goes when the effect stops or is deleted.
    pub active_effects: Vec<ActiveEffectRecord>,
    /// Effect parameters kept, per device and per effect.
    ///
    /// Same economy as `devices`: an entry exists only if someone **moved** a
    /// slider. Restoring the declared values removes it, rather than writing a
    /// copy of the defaults that the next version of the effect would
    /// contradict.
    pub effect_params: Vec<EffectParamsRecord>,
    /// The log level as an earlier version wrote it, **at the root**.
    ///
    /// Read, never written back (`skip_serializing`): [`Store::read_settings`]
    /// moves it into [`Preferences`], and it disappears from the file on the first
    /// write. Without this bridge, moving `logLevel` would have reset to the
    /// default the level of whoever was **in the middle of** chasing a failure —
    /// that is, at the worst moment, since it is the only one where this setting
    /// matters.
    ///
    /// The rejected alternative: do nothing and accept it. It cost twelve fewer
    /// lines and one lost debugging run. To remove once no `settings.json` older
    /// than v2.1 is around any more.
    #[serde(default, rename = "logLevel", skip_serializing)]
    legacy_log_level: Option<LogLevel>,
}

/// Current shape of `settings.json`.
///
/// - 0: effects referenced by their directory name;
/// - 1: effects referenced by their uid.
pub const SETTINGS_VERSION: u32 = 1;

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            preferences: Preferences::default(),
            devices: Vec::new(),
            active_effects: Vec::new(),
            effect_params: Vec::new(),
            legacy_log_level: None,
        }
    }
}

impl Settings {
    /// Moves into [`Preferences`] what an earlier file carried at the root.
    ///
    /// What is already in place wins: a file written by this version is right
    /// against a legacy key a text editor may have left in it.
    fn absorb_legacy(&mut self) {
        if let Some(legacy_level) = self.legacy_log_level.take() {
            self.preferences.log_level.get_or_insert(legacy_level);
        }
    }

    /// Index of the entry describing this device, if there is one.
    ///
    /// Exact identity first — serial included, `None` included — then the
    /// tolerant rule of [`DeviceRecord::matches`]. The order matters: an entry
    /// without a serial must not decide in place of one that carries a serial,
    /// otherwise two units of the same model would be mixed up as soon as one of
    /// them had been adopted without a serial.
    fn position(&self, vid: u16, pid: u16, serial: Option<&str>) -> Option<usize> {
        self.devices
            .iter()
            .position(|r| r.vid == vid && r.pid == pid && r.serial.as_deref() == serial)
            .or_else(|| {
                self.devices
                    .iter()
                    .position(|r| r.matches(vid, pid, serial))
            })
    }

    /// Decision kept for this device, or [`DeviceState::Detected`].
    pub fn device_state(&self, vid: u16, pid: u16, serial: Option<&str>) -> DeviceState {
        self.position(vid, pid, serial)
            .map(|i| self.devices[i].state)
            .unwrap_or_default()
    }

    /// Keeps a decision for this device.
    pub fn set_device_state(
        &mut self,
        vid: u16,
        pid: u16,
        serial: Option<&str>,
        state: DeviceState,
    ) {
        match self.position(vid, pid, serial) {
            Some(i) => {
                let record = &mut self.devices[i];
                record.state = state;
                // The serial is filled in if we just learned it, but never
                // erased: a silent enumeration must not make the entry lose
                // what tells it apart from the unit next to it.
                if record.serial.is_none() {
                    record.serial = serial.map(str::to_owned);
                }
            }
            None => self.devices.push(DeviceRecord {
                vid,
                pid,
                serial: serial.map(str::to_owned),
                state,
                brightness: None,
            }),
        }
        self.prune();
    }

    /// The brightness kept for this device, or [`DEFAULT_BRIGHTNESS`].
    pub fn brightness(&self, vid: u16, pid: u16, serial: Option<&str>) -> u8 {
        self.position(vid, pid, serial)
            .and_then(|i| self.devices[i].brightness)
            .unwrap_or(DEFAULT_BRIGHTNESS)
    }

    /// Keeps a brightness. [`DEFAULT_BRIGHTNESS`] **forgets** the entry.
    ///
    /// The counterpart of "restore the declared values" for effect parameters:
    /// pushing the slider all the way up must not write 255 to the file, it must
    /// remove the line. A device for which this was the only decision then
    /// disappears entirely — see [`DeviceRecord::is_inert`].
    ///
    /// Returns true if something changed, so the temporary file and its rename
    /// are skipped when there is nothing to write.
    pub fn set_brightness(&mut self, vid: u16, pid: u16, serial: Option<&str>, level: u8) -> bool {
        let kept = (level != DEFAULT_BRIGHTNESS).then_some(level);
        match self.position(vid, pid, serial) {
            Some(i) => {
                if self.devices[i].brightness == kept {
                    return false;
                }
                self.devices[i].brightness = kept;
                // Same rule as [`Self::set_device_state`]: the serial is filled
                // in if we just learned it, it is never erased.
                if self.devices[i].serial.is_none() {
                    self.devices[i].serial = serial.map(str::to_owned);
                }
            }
            None => {
                // The default, on a device we keep nothing about: there is no
                // entry to create just to put nothing in it.
                let Some(level) = kept else { return false };
                self.devices.push(DeviceRecord {
                    vid,
                    pid,
                    serial: serial.map(str::to_owned),
                    state: DeviceState::default(),
                    brightness: Some(level),
                });
            }
        }
        self.prune();
        true
    }

    /// Removes device entries that no longer keep anything.
    fn prune(&mut self) {
        self.devices.retain(|r| !r.is_inert());
    }

    /// The effect applied on this device, if there is one.
    pub fn active_effect(&self, vid: u16, pid: u16) -> Option<&str> {
        self.active_effects
            .iter()
            .find(|r| r.vid == vid && r.pid == pid)
            .map(|r| r.effect.as_str())
    }

    /// Keeps the applied effect, or forgets it with `None`.
    ///
    /// Returns true if something changed: starting the same effect twice on the
    /// same keyboard — which is what a double-click does — must not rewrite the
    /// file.
    pub fn set_active_effect(&mut self, vid: u16, pid: u16, effect: Option<&str>) -> bool {
        let position = self
            .active_effects
            .iter()
            .position(|r| r.vid == vid && r.pid == pid);

        match (position, effect) {
            (Some(i), None) => {
                self.active_effects.remove(i);
                true
            }
            (Some(i), Some(e)) if self.active_effects[i].effect == e => false,
            (Some(i), Some(e)) => {
                self.active_effects[i].effect = e.to_owned();
                true
            }
            (None, None) => false,
            (None, Some(e)) => {
                self.active_effects.push(ActiveEffectRecord {
                    vid,
                    pid,
                    effect: e.to_owned(),
                });
                true
            }
        }
    }

    /// The parameters kept for this effect on this device, if any.
    pub fn effect_params(
        &self,
        vid: u16,
        pid: u16,
        effect: &str,
    ) -> Option<&serde_json::Map<String, serde_json::Value>> {
        self.effect_params
            .iter()
            .find(|r| r.vid == vid && r.pid == pid && r.effect == effect)
            .map(|r| &r.values)
    }

    /// Keeps parameters. An **empty** map erases the entry.
    ///
    /// That is what makes "restore the declared values" a forget and not a copy:
    /// the effect then starts again from its manifest, including when a later
    /// version changes its defaults.
    pub fn set_effect_params(
        &mut self,
        vid: u16,
        pid: u16,
        effect: &str,
        values: serde_json::Map<String, serde_json::Value>,
    ) {
        let position = self
            .effect_params
            .iter()
            .position(|r| r.vid == vid && r.pid == pid && r.effect == effect);

        match (position, values.is_empty()) {
            (Some(i), true) => {
                self.effect_params.remove(i);
            }
            (Some(i), false) => self.effect_params[i].values = values,
            (None, true) => {}
            (None, false) => self.effect_params.push(EffectParamsRecord {
                vid,
                pid,
                effect: effect.to_owned(),
                values,
            }),
        }
    }

    /// Forgets an effect **everywhere**: its parameters, and where it is applied.
    ///
    /// Called when the effect is deleted. Without it, its parameters would stay
    /// in `settings.json` for an id nothing designates any more, and the file
    /// would only grow.
    ///
    /// # Why where it is applied goes too, and not only the parameters
    ///
    /// This is the trap issue #48 had pointed out and that #64 settles here:
    /// `delete_effect` left a **dangling id**. As long as the field was dead, the
    /// question did not arise; now that `activeEffects` is written, it does, and
    /// the two possible answers do not cost the same:
    ///
    /// - **purge on delete** — chosen: the file never holds an id the library
    ///   does not know, and that invariant can be checked without running
    ///   anything;
    /// - silently fall back at startup — rejected *as the only measure*: a silent
    ///   failure at launch is exactly the kind of breakage that costs a debugging
    ///   run, and the id would survive as many startups as you like.
    ///
    /// The fallback is still needed as a **second** barrier — an effect directory
    /// removed by hand does not go through here — but it is no longer the only
    /// one.
    ///
    /// Returns true if something was removed, so the file is not rewritten when
    /// there is nothing to change in it.
    pub fn forget_effect(&mut self, effect: &str) -> bool {
        let before = self.effect_params.len() + self.active_effects.len();
        self.effect_params.retain(|r| r.effect != effect);
        self.active_effects.retain(|r| r.effect != effect);
        self.effect_params.len() + self.active_effects.len() != before
    }

    /// Rewrites every effect reference found in `renames`, from old id to uid.
    ///
    /// A reference that is not in the table stays as it is: it names an effect
    /// removed by hand, which the startup fallback already copes with, and
    /// guessing what it meant would be worse.
    fn rename_effects(&mut self, renames: &BTreeMap<String, String>) {
        for record in &mut self.active_effects {
            if let Some(uid) = renames.get(&record.effect) {
                record.effect.clone_from(uid);
            }
        }
        for record in &mut self.effect_params {
            if let Some(uid) = renames.get(&record.effect) {
                record.effect.clone_from(uid);
            }
        }
    }
}

/// The values an effect must start with: what it **declares**, overridden by
/// what was **kept** for this device.
///
/// # Why this computation exists in Rust
///
/// The window already does it, in two pieces — `startingParams` for the manifest
/// defaults, `merge` for the override. But the tray icon starts an effect **with
/// no window**: it cannot borrow anything from the TypeScript, and starting an
/// effect with an empty parameter object would not give the same lighting as the
/// same click made from the gallery. See [`crate::tray`].
///
/// Both implementations of the rule must therefore stay in agreement. What they
/// say, and it is the only thing to remember: **limited to declared
/// parameters**. A value kept for a parameter the effect no longer has
/// disappears on its own, instead of travelling indefinitely to a loop that no
/// longer reads it — and a declared parameter with no kept value takes its
/// default, never nothing.
///
/// A manifest parameter that declares no `default` is left out: `params` is raw
/// JSON the Rust side does not interpret (see [`Manifest::params`]), and
/// inventing a value for a kind of parameter we do not know would be worse than
/// letting the effect apply its own.
pub(crate) fn starting_params(
    manifest: &Manifest,
    stored: Option<&serde_json::Map<String, serde_json::Value>>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut out = serde_json::Map::new();
    for (id, spec) in &manifest.params {
        if let Some(default_value) = spec.get("default") {
            out.insert(id.clone(), default_value.clone());
        }
    }
    if let Some(stored) = stored {
        for (id, value) in stored {
            // Only what the manifest still declares: the same limit as on the
            // window side, and it is what makes an orphaned value disappear.
            if manifest.params.contains_key(id) {
                out.insert(id.clone(), value.clone());
            }
        }
    }
    out
}

// ---------------------------------------------------------------- ids

/// True if `id` is a device name reserved by Windows.
fn is_reserved(id: &str) -> bool {
    RESERVED_NAMES.contains(&id)
}

/// Checks that an id can safely be used as a directory name.
///
/// The check is an **allow list**: `a-z`, `0-9` and the hyphen. Everything else
/// is refused, which rules out at once `..`, path separators, the colon of a
/// Windows drive and control characters — without depending on a deny list we
/// would forget to extend.
pub(crate) fn validate_id(id: &str) -> CmdResult<()> {
    if id.is_empty() {
        return Err("identifiant d'effet vide".into());
    }
    if id.len() > MAX_ID_LEN {
        return Err(format!(
            "identifiant d'effet trop long : {} caractères, {MAX_ID_LEN} au plus",
            id.len()
        ));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(format!(
            "identifiant d'effet invalide : « {id} » — seuls les caractères a-z, 0-9 et le tiret sont acceptés"
        ));
    }
    if id.starts_with('-') || id.ends_with('-') {
        return Err(format!(
            "identifiant d'effet invalide : « {id} » — il ne peut ni commencer ni finir par un tiret"
        ));
    }
    if is_reserved(id) {
        return Err(format!(
            "« {id} » est un nom réservé par Windows, il ne peut pas servir de dossier"
        ));
    }
    Ok(())
}

/// Derives a safe id from a name typed by the user.
///
/// The name is never trusted: it is not validated, it is **replaced** by what it
/// has that can be represented. The result always satisfies [`validate_id`] —
/// that is what the test `hostile_names_yield_a_safe_id` checks.
///
/// Two effects with the same name get the same id, so the second overwrites the
/// first: saving an effect again from the editor updates it instead of piling up
/// copies.
pub fn derive_id(name: &str) -> String {
    let mut id = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            id.push(ch.to_ascii_lowercase());
        } else if !id.ends_with('-') {
            id.push('-');
        }
    }
    let id: String = id.trim_matches('-').chars().take(MAX_ID_LEN).collect();
    let id = id.trim_end_matches('-');

    if id.is_empty() {
        return "effet".into();
    }
    if is_reserved(id) {
        return format!("{id}-effet");
    }
    id.to_string()
}

/// Checks that `uid` is a lowercase UUID: `8-4-4-4-12` hexadecimal digits.
///
/// Stricter than [`validate_id`], which it implies: a uid is also a directory
/// name, a settings key and part of a tray menu id, and one shape everywhere
/// keeps it from being mistaken for a readable name.
pub(crate) fn validate_uid(uid: &str) -> CmdResult<()> {
    let groups: Vec<&str> = uid.split('-').collect();
    let shaped = groups.len() == 5
        && groups.iter().zip([8, 4, 4, 4, 12]).all(|(group, len)| {
            group.len() == len
                && group
                    .chars()
                    .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        });
    if shaped {
        Ok(())
    } else {
        Err(format!("identifiant d'effet invalide : « {uid} »"))
    }
}

/// A new uid, for an effect saved before uids existed.
fn mint_uid() -> String {
    uuid::Uuid::new_v4().to_string()
}

// ---------------------------------------------------------------- storage

/// Disk access to effects and settings.
///
/// The base paths come from outside: nothing here knows about Tauri, which makes
/// the whole thing testable in a temporary directory.
pub struct Store {
    effects_dir: PathBuf,
    settings_file: PathBuf,
}

impl Store {
    /// `data_dir` holds the content, `config_dir` the configuration.
    pub fn new(data_dir: &Path, config_dir: &Path) -> Self {
        Self {
            effects_dir: data_dir.join("effects"),
            settings_file: config_dir.join("settings.json"),
        }
    }

    /// Writes an effect's directory and returns its uid.
    ///
    /// The three files are written together: `source.ts` to reopen the effect in
    /// the editor, `effect.js` to run it, `manifest.json` to describe it. The
    /// `.js` is not a regenerable cache — the transpiler lives in Monaco, so
    /// rebuilding it would require opening the window, while an effect must be
    /// able to start without an interface.
    ///
    /// # Which directory
    ///
    /// The one already holding this uid, if there is one: saving again or
    /// renaming updates the effect in place. Otherwise a new one, named after the
    /// effect, with a numeric suffix when the name is taken — two effects may
    /// share a name, never a uid.
    pub fn install_effect(
        &self,
        source_ts: &str,
        js: &str,
        manifest: &Manifest,
    ) -> CmdResult<String> {
        if manifest.name.trim().is_empty() {
            return Err("l'effet doit avoir un nom".into());
        }
        if manifest.api_version == 0 {
            return Err("le manifeste ne déclare pas la version de l'API d'effets".into());
        }
        if manifest.api_version > EFFECTS_API_VERSION {
            return Err(format!(
                "effet écrit pour la version {} de l'API d'effets ; cette version de candeo n'en connaît que la {EFFECTS_API_VERSION}",
                manifest.api_version
            ));
        }
        validate_uid(&manifest.uid)?;
        // Built-in uids are reserved. Accepting the clash would force a choice
        // afterwards, on every read, between two effects with the same uid — and
        // the library would show two of them under the same key. The editor
        // gives a copy its own uid, so this only happens to a uid typed by hand.
        if builtins::find(&manifest.uid).is_some() {
            return Err(
                "cet identifiant est celui d'un effet intégré : retirez la ligne uid pour enregistrer votre version"
                    .into(),
            );
        }

        let dir = match self.find_dir(&manifest.uid)? {
            Some(dir) => dir,
            None => self.effects_dir.join(self.free_dir_name(&manifest.name)),
        };
        create_dir(&dir)?;

        let json = serde_json::to_string_pretty(manifest)
            .map_err(|e| format!("manifeste non sérialisable : {e}"))?;
        write(&dir.join(SOURCE_FILE), source_ts)?;
        write(&dir.join(JS_FILE), js)?;
        write(&dir.join(MANIFEST_FILE), &json)?;

        // The swatch is sampled **here**, once, and not every time the list is
        // shown: it is a thumbnail that does not move as long as the effect does
        // not. Saving an effect again goes through this point, so recomputes it —
        // an effect that turned blue does not keep its red thumbnail.
        write_swatch(&dir, js);

        Ok(manifest.uid.clone())
    }

    /// A directory name for a new effect: derived from its name, with `-2`,
    /// `-3`… when taken.
    ///
    /// The stem is shortened to make room for the suffix: a name longer than
    /// [`MAX_ID_LEN`] would otherwise give a directory the library no longer
    /// reads.
    fn free_dir_name(&self, name: &str) -> String {
        let base = derive_id(name);
        if !self.effects_dir.join(&base).exists() {
            return base;
        }
        (2..)
            .map(|n| {
                let suffix = format!("-{n}");
                let stem = base[..base.len().min(MAX_ID_LEN - suffix.len())].trim_end_matches('-');
                format!("{stem}{suffix}")
            })
            .find(|candidate| !self.effects_dir.join(candidate).exists())
            .expect("an unbounded range always has a free name")
    }

    /// Every readable effect directory, in the order of their names.
    ///
    /// Manifests without a uid are included: the migration needs them. A damaged
    /// or foreign directory is skipped rather than failing the whole list: a
    /// library of twenty effects must not disappear because one of them has an
    /// unreadable manifest.
    fn installed(&self) -> CmdResult<Vec<Installed>> {
        // Missing directory: first launch, no effect installed. This is not an
        // error.
        let entries = match fs::read_dir(&self.effects_dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(format!(
                    "lecture de {} impossible : {e}",
                    self.effects_dir.display()
                ))
            }
        };

        let mut installed = Vec::new();
        for entry in entries {
            let entry =
                entry.map_err(|e| format!("lecture de la bibliothèque interrompue : {e}"))?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if validate_id(&name).is_err() {
                continue;
            }
            let Ok(raw) = fs::read_to_string(entry.path().join(MANIFEST_FILE)) else {
                continue;
            };
            let Ok(manifest) = serde_json::from_str::<Manifest>(&raw) else {
                continue;
            };
            installed.push(Installed {
                dir: entry.path(),
                name,
                manifest,
            });
        }

        // Stable order: the file system guarantees none, and a gallery that
        // reorders itself on every opening is unreadable. By directory name
        // rather than uid, which would shuffle it.
        installed.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(installed)
    }

    /// The directory of the installed effect with this uid.
    ///
    /// A scan of the manifests, on every call: the library holds tens of effects,
    /// and this runs when an effect starts, is opened or deleted — never per
    /// frame. An index would be one more thing to keep true when a directory is
    /// copied or removed by hand.
    ///
    /// Two directories with the same uid can only come from a manual copy; the
    /// first by name wins, the same one [`Self::list_effects`] shows.
    fn find_dir(&self, uid: &str) -> CmdResult<Option<PathBuf>> {
        Ok(self
            .installed()?
            .into_iter()
            .find(|i| i.manifest.uid == uid)
            .map(|i| i.dir))
    }

    /// An effect's executable JavaScript, **built-in or installed**.
    ///
    /// This is what the engine loads. Built-ins are looked up first: see the
    /// priority justified at the top of the module.
    ///
    /// For a user effect, it is also why the `.js` is written to disk at install —
    /// reading it needs neither the editor nor the window.
    pub fn effect_js(&self, uid: &str) -> CmdResult<String> {
        if let Some(b) = builtins::find(uid) {
            return Ok(b.js.to_string());
        }
        self.read_file(uid, JS_FILE)
    }

    /// An effect's source, to reopen it in the editor.
    ///
    /// A built-in effect has no `.ts`: its JavaScript **is** its source. Making it
    /// readable from the editor is the whole point of shipping it — you start from
    /// an effect that works, change it, and save it as a copy.
    pub fn effect_source(&self, uid: &str) -> CmdResult<String> {
        if let Some(b) = builtins::find(uid) {
            return Ok(b.js.to_string());
        }
        self.read_file(uid, SOURCE_FILE)
    }

    fn read_file(&self, uid: &str, file: &str) -> CmdResult<String> {
        validate_uid(uid)?;
        let Some(dir) = self.find_dir(uid)? else {
            return Err(format!("aucun effet installé sous l'identifiant « {uid} »"));
        };
        let path = dir.join(file);
        fs::read_to_string(&path)
            .map_err(|e| format!("lecture de {} impossible : {e}", path.display()))
    }

    /// Full library: built-in effects **and** user effects.
    ///
    /// Built-ins are compiled into the binary and have no directory; they show up
    /// anyway, told apart by [`EffectKind`], so the interface has a single list to
    /// display.
    pub fn list_effects(&self) -> CmdResult<Vec<EffectEntry>> {
        let mut effects = builtin_effects();

        for Installed {
            dir, mut manifest, ..
        } in self.installed()?
        {
            // Without a valid uid, the effect has no id to be started or
            // opened by: it predates uids and waits for the migration at the
            // next launch.
            if validate_uid(&manifest.uid).is_err() {
                continue;
            }
            // A uid already listed is left out: a directory taking over a
            // built-in's uid, or a manual copy of another directory. The list is
            // indexed by id, it cannot hold two of them, and the first one is
            // what starts anyway. It can still be deleted — see
            // [`Self::check_deletable`].
            if effects.iter().any(|e| e.id == manifest.uid) {
                continue;
            }
            effects.push(EffectEntry {
                id: std::mem::take(&mut manifest.uid),
                kind: EffectKind::User,
                swatch: read_swatch(&dir),
                manifest,
            });
        }

        Ok(effects)
    }

    /// Tells whether this effect can be deleted, **without deleting anything**.
    ///
    /// The directory is checked **before** the built-in case: that is what allows
    /// removing a directory that takes over a built-in uid, invisible in the list
    /// and never run, but present on disk. The order is the reverse of the one
    /// used to resolve at run time ([`Self::effect_js`]), which checks built-ins
    /// first so a shipped effect cannot be taken over. Both asymmetries serve the
    /// same goal and must not be aligned.
    ///
    /// Separate from [`Self::delete_effect`] because the command stops the loops
    /// **between** the refusal and the erasure: refusing afterwards would make a
    /// running built-in effect pay for a stop it was not owed.
    pub fn check_deletable(&self, uid: &str) -> CmdResult<()> {
        self.deletable_dir(uid).map(|_| ())
    }

    fn deletable_dir(&self, uid: &str) -> CmdResult<PathBuf> {
        validate_uid(uid)?;
        if let Some(dir) = self.find_dir(uid)? {
            return Ok(dir);
        }
        if builtins::find(uid).is_some() {
            return Err(
                "cet effet est intégré : il est livré avec l'application et ne peut pas être supprimé"
                    .into(),
            );
        }
        Err(format!("aucun effet installé sous l'identifiant « {uid} »"))
    }

    /// Deletes the effect's directory.
    pub fn delete_effect(&self, uid: &str) -> CmdResult<()> {
        // Checked again rather than assumed: between the command's refusal and
        // this call, the directory may have disappeared — and this is where it is
        // reported.
        let dir = self.deletable_dir(uid)?;
        fs::remove_dir_all(&dir)
            .map_err(|e| format!("suppression de {} impossible : {e}", dir.display()))
    }

    /// Old effect ids, and the uid each one now designates.
    ///
    /// The shipped effects' fixed table, then the name of every installed
    /// directory: before uids, an effect's id was its directory name. Computed
    /// from the disk rather than recorded, so it needs no state of its own.
    ///
    /// A shipped effect wins over a directory of the same name: before uids, the
    /// built-in was the one that ran under that id.
    pub fn legacy_effect_ids(&self) -> CmdResult<BTreeMap<String, String>> {
        let mut renames: BTreeMap<String, String> = builtins::ALL
            .iter()
            .map(|b| (b.slug.to_owned(), b.uid.to_owned()))
            .collect();
        for Installed { name, manifest, .. } in self.installed()? {
            if validate_uid(&manifest.uid).is_ok() && name != manifest.uid {
                renames.entry(name).or_insert(manifest.uid);
            }
        }
        Ok(renames)
    }

    /// Moves a library from ids to uids. Runs at every startup, and does nothing
    /// once done.
    ///
    /// 1. Every installed effect without a uid gets one, written to its
    ///    `manifest.json`. Its `source.ts` is left alone: the editor writes the
    ///    uid into the source at the next save, with the TypeScript parser —
    ///    editing a source here would mean guessing where the object starts.
    /// 2. A `settings.json` from before uids has its effect references rewritten,
    ///    **once**: the version recorded afterwards is what keeps a later
    ///    directory, whose name happens to be an old id, from being taken for it.
    pub fn migrate_effect_ids(&self) -> CmdResult<Migration> {
        let mut migration = Migration::default();

        for Installed { dir, manifest, .. } in self.installed()? {
            if validate_uid(&manifest.uid).is_ok() {
                continue;
            }
            let manifest = Manifest {
                uid: mint_uid(),
                ..manifest
            };
            let json = serde_json::to_string_pretty(&manifest)
                .map_err(|e| format!("manifeste non sérialisable : {e}"))?;
            write(&dir.join(MANIFEST_FILE), &json)?;
            migration.effects += 1;
        }

        let mut settings = self.read_settings()?;
        if settings.version < 1 {
            settings.rename_effects(&self.legacy_effect_ids()?);
            settings.version = 1;
            self.write_settings(&settings)?;
            migration.settings = true;
        }

        Ok(migration)
    }

    /// Reads `settings.json`, or returns the default values if it does not exist.
    pub fn read_settings(&self) -> CmdResult<Settings> {
        let raw = match fs::read_to_string(&self.settings_file) {
            Ok(raw) => raw,
            // First launch: no file, so the defaults. An error here would force
            // the interface to treat the nominal case as an incident.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
            Err(e) => {
                return Err(format!(
                    "lecture de {} impossible : {e}",
                    self.settings_file.display()
                ))
            }
        };
        let mut settings: Settings = serde_json::from_str(&raw).map_err(|e| {
            format!(
                "réglages illisibles dans {} : {e}",
                self.settings_file.display()
            )
        })?;
        // Here and nowhere else: this is the only path by which a file enters the
        // application, so the only place where a legacy key can be translated
        // once and for all.
        settings.absorb_legacy();
        Ok(settings)
    }

    /// Writes `settings.json`.
    ///
    /// Goes through a temporary file then a rename: a power cut in the middle of
    /// writing would otherwise leave truncated settings, and so an application
    /// that no longer starts.
    ///
    /// A single temporary name, and it can stay that way: **synchronous** Tauri
    /// commands run on the main thread, so two read-modify-write sequences do not
    /// interleave. A unique name per write was tried then removed — it defended
    /// against an interleaving nothing produces, and left a file behind on every
    /// failure, where a fixed name is simply overwritten on the next attempt.
    ///
    /// ⚠️ **This reasoning holds within one process, not between two.** Two candeo
    /// instances would write to the *same* `settings.json.tmp`, and one would
    /// rename what the other is writing: the rename would stay atomic, but what it
    /// published would not — truncated settings, or the other instance's. What
    /// keeps this name fixed is therefore [`crate::single_instance`], and the two
    /// decisions are not undone one without the other: making candeo
    /// multi-instance would require revisiting this name, and revisiting it
    /// without that would buy nothing.
    pub fn write_settings(&self, settings: &Settings) -> CmdResult<()> {
        let Some(parent) = self.settings_file.parent() else {
            return Err("chemin de réglages sans dossier parent".into());
        };
        create_dir(parent)?;

        let json = serde_json::to_string_pretty(settings)
            .map_err(|e| format!("réglages non sérialisables : {e}"))?;
        let tmp = self.settings_file.with_extension("json.tmp");
        write(&tmp, &json)?;
        fs::rename(&tmp, &self.settings_file).map_err(|e| {
            format!(
                "écriture de {} impossible : {e}",
                self.settings_file.display()
            )
        })
    }

    /// Rewrites `settings.json` with the default values.
    ///
    /// **Touches no effect**, and could not: it only writes to `settings_file`.
    /// This is the distinction the whole module carries — an effect is content,
    /// the choice of the active effect is configuration — and here it protects
    /// something: whoever only wants to un-adopt a keyboard must not lose
    /// hand-written code.
    ///
    /// The file is rewritten rather than deleted. Both read back the same —
    /// [`Self::read_settings`] returns the defaults when there is no file — but a
    /// file that disappears looks like damage, whereas a file reset to defaults
    /// can be read and compared.
    pub fn reset_settings(&self) -> CmdResult<()> {
        self.write_settings(&Settings::default())
    }
}

/// An effect directory as [`Store::installed`] read it.
struct Installed {
    dir: PathBuf,
    /// The directory name: the effect's id before uids.
    name: String,
    manifest: Manifest,
}

/// What [`Store::migrate_effect_ids`] changed, for the log.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Migration {
    /// Installed effects that received a uid.
    pub effects: usize,
    /// True if `settings.json` was rewritten.
    pub settings: bool,
}

/// Effects compiled into the binary, in the shape the gallery expects.
///
/// The manifest is rebuilt on every call rather than kept: five small JSON
/// objects, against a lazy initialization and its lock. Invalid parameter JSON
/// would give a manifest without parameters here, which a test in
/// [`crate::builtins`] forbids — better a failing test than a panic at
/// application startup.
fn builtin_effects() -> Vec<EffectEntry> {
    builtins::ALL
        .iter()
        .zip(builtins::swatches())
        .map(|(b, swatch)| EffectEntry {
            id: b.uid.to_string(),
            kind: EffectKind::Builtin,
            // Built-ins have no directory: their swatch lives in memory,
            // computed once per run. The why is in [`builtins::swatches`].
            swatch: swatch.clone(),
            manifest: Manifest {
                uid: String::new(),
                name: b.name.to_string(),
                description: b.description.to_string(),
                params: serde_json::from_str(b.params).unwrap_or_default(),
                api_version: EFFECTS_API_VERSION,
            },
        })
        .collect()
}

/// Samples the effect's swatch and writes it next to its manifest — or erases the
/// one that was there.
///
/// **Nothing is reported, not even an error.** A swatch is a nicety: it must never
/// prevent installing an otherwise valid effect. An effect that throws, does not
/// load or loops during sampling therefore installs normally, just without a
/// thumbnail.
///
/// Erasing matters as much as writing: a modified effect that no longer samples
/// would otherwise keep the old file and show the colors of a version that no
/// longer exists.
fn write_swatch(dir: &Path, js: &str) {
    // The default layout, never the one of the plugged-in keyboard: a swatch that
    // depended on the hardware present at install would be comparable neither
    // from one effect to another, nor from one machine to another.
    let swatch = swatch::sample(js, crate::default_layout());
    let path = dir.join(SWATCH_FILE);

    if swatch.is_empty() {
        let _ = fs::remove_file(&path);
        return;
    }
    if let Ok(json) = serde_json::to_string(&swatch) {
        let _ = fs::write(&path, json);
    }
}

/// The swatch of an installed effect, empty if there is none.
///
/// No recomputation here: listing the library must remain a disk read. Sampling
/// at display time would make opening the gallery depend on the behavior of every
/// installed effect — and a swatch does not change between two displays.
///
/// An effect installed by an earlier version therefore has no swatch until it is
/// saved again. That is the price of this rule, and it is paid with a neutral
/// dot, not with an error.
fn read_swatch(dir: &Path) -> Swatch {
    fs::read_to_string(dir.join(SWATCH_FILE))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn create_dir(path: &Path) -> CmdResult<()> {
    fs::create_dir_all(path).map_err(|e| format!("création de {} impossible : {e}", path.display()))
}

fn write(path: &Path, contents: &str) -> CmdResult<()> {
    fs::write(path, contents)
        .map_err(|e| format!("écriture de {} impossible : {e}", path.display()))
}

// ---------------------------------------------------------------- commands

/// Resolves the system locations. No path is hard-coded: on Windows both calls
/// return the same directory, on Linux they do not.
pub(crate) fn store(app: &AppHandle) -> CmdResult<Store> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("dossier de données introuvable : {e}"))?;
    let config = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("dossier de configuration introuvable : {e}"))?;
    Ok(Store::new(&data, &config))
}

/// Installs an effect and returns its uid.
#[tauri::command]
pub fn install_effect(
    app: AppHandle,
    source_ts: String,
    js: String,
    manifest: Manifest,
) -> CmdResult<String> {
    store(&app)?.install_effect(&source_ts, &js, &manifest)
}

/// Built-in and installed effects, with their kind.
#[tauri::command]
pub fn list_effects(app: AppHandle) -> CmdResult<Vec<EffectEntry>> {
    store(&app)?.list_effects()
}

/// Deletes an effect, **and everything `settings.json` kept about it**: its
/// parameters, and where it is applied on devices.
///
/// Both go together: leaving the parameters behind would grow `settings.json`
/// with entries pointing at an id nothing names any more, and an effect
/// reinstalled later under the same name would silently inherit the parameters of
/// its vanished namesake. Leaving the **applied effect** behind would also leave a
/// dangling id, which we would try to start the day the effect is resumed at
/// startup — see [`Settings::forget_effect`], where this choice is made.
///
/// Forgetting comes **after** the deletion: if the deletion fails, the effect is
/// still there and its parameters must be too.
///
/// # Three steps, in this order
///
/// 1. **the refusal**, before everything else: a built-in effect or an id that
///    designates nothing gets a no without anything having been stopped;
/// 2. **stopping the loops**, on every device where the effect runs, and before
///    the erasure: the engine runs an `effect.js` read at startup and kept in
///    memory, so it would carry on with no visible error on a directory that is
///    gone;
/// 3. **the erasure**, then forgetting the parameters.
///
/// Stopping on the Rust side rather than in the window: it is the only place that
/// guarantees it whoever the caller is, and the invariant — no loop runs a deleted
/// effect — only holds if it holds everywhere.
#[tauri::command]
pub fn delete_effect(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let store = store(&app)?;
    store.check_deletable(&id)?;
    state.engine.stop_everywhere(&id);
    store.delete_effect(&id)?;

    let mut settings = store.read_settings()?;
    if settings.forget_effect(&id) {
        store.write_settings(&settings)?;
    }
    Ok(())
}

/// Returns an effect's source, to reopen it in the editor.
///
/// It is the counterpart of `install_effect`: without it, an installed effect
/// could no longer be modified — which is precisely why the `.ts` is written to
/// disk next to the `.js`. A built-in effect returns its JavaScript, which is its
/// source.
#[tauri::command]
pub fn read_effect_source(app: AppHandle, id: String) -> CmdResult<String> {
    store(&app)?.effect_source(&id)
}

/// Old effect ids and their uids. See [`Store::legacy_effect_ids`].
///
/// For the editor's drafts: they are keyed by effect id in the web view's
/// storage, which the startup migration cannot reach. To remove once no draft
/// older than uids is around any more.
#[tauri::command]
pub fn legacy_effect_ids(app: AppHandle) -> CmdResult<BTreeMap<String, String>> {
    store(&app)?.legacy_effect_ids()
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> CmdResult<Settings> {
    store(&app)?.read_settings()
}

#[tauri::command]
pub fn set_settings(app: AppHandle, settings: Settings) -> CmdResult<()> {
    store(&app)?.write_settings(&settings)
}

/// Resets the configuration to the default, and releases the devices.
///
/// # What it does not do
///
/// **No effect is touched.** Written effects live in `app_data_dir()/effects/`,
/// the configuration in `settings.json`: two locations, two actions. Removing an
/// effect is another command, [`delete_effect`], one per effect — mixing the two
/// would make whoever only wanted to un-adopt a keyboard lose hand-written code.
///
/// Nor is it a place to free resources on the effects side: each loop owns its
/// QuickJS `Runtime` and `Context`, and both are destroyed with it — the whole
/// JavaScript heap goes with them.
///
/// # The order
///
/// Devices are released **before** the write: resetting the device table while an
/// effect runs would leave loops that no decision designates any more. See
/// [`crate::release_devices`] for what "release" means in detail.
///
/// The store is resolved first, even before stopping: a configuration directory
/// that cannot be found must be reported without having turned anything off.
///
/// The log level goes back to the default with the rest, **and right away**: it
/// was just erased from the file, and leaving it applied until the next launch
/// would make the screen showing it lie.
#[tauri::command]
pub fn reset_settings(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let store = store(&app)?;
    crate::release_devices(&state);
    store.reset_settings()?;
    crate::journal::reset_level_to_default();
    Ok(())
}

/// Keeps an effect's parameters for a device, without touching the rest.
///
/// A dedicated command rather than a `set_settings` from the interface: the read,
/// the change and the write happen here, in one go.
///
/// It is not a guard against interleaving — synchronous commands run on the main
/// thread, they do not overlap. It is a guard against a **stale copy**: the window
/// reads the settings once, when the screen mounts, and a `set_settings` posted on
/// the first slider move would send that snapshot back as is, erasing everything
/// decided since. This is not a textbook case — the Rust side writes
/// `settings.json` on every `adopt_device`, and adopting a device is exactly what
/// one does between two adjustments.
///
/// It changes **nothing** in the running effect: adjusting live is
/// [`crate::runtime::set_effect_params`]. The two are separate because they have
/// neither the same rate nor the same destination — dozens of calls per second to
/// the render loop, a single one to disk when the slider stops.
#[tauri::command]
pub fn remember_effect_params(
    app: AppHandle,
    device: DeviceRef,
    effect: String,
    params: serde_json::Map<String, serde_json::Value>,
) -> CmdResult<()> {
    validate_id(&effect)?;
    let store = store(&app)?;
    let mut settings = store.read_settings()?;

    // Nothing new: the file is not rewritten. A slider moved then brought back
    // comes through here, and so does switching back and forth between two
    // effects — one disk write per pass would tell nobody anything.
    let stored = settings.effect_params(device.vid, device.pid, &effect);
    if stored == Some(&params) || (stored.is_none() && params.is_empty()) {
        return Ok(());
    }

    settings.set_effect_params(device.vid, device.pid, &effect, params);
    store.write_settings(&settings)
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// Two separate directories, as on Linux: a test that merged them would let a
    /// data / configuration mix-up slip through.
    fn temp_store() -> (tempfile::TempDir, Store) {
        let tmp = tempfile::tempdir().expect("temp dir");
        let store = Store::new(&tmp.path().join("data"), &tmp.path().join("config"));
        (tmp, store)
    }

    /// The installed part of the library. Built-ins are always there: the install
    /// tests are about the others.
    fn installed_effects(store: &Store) -> Vec<EffectEntry> {
        store
            .list_effects()
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == EffectKind::User)
            .collect()
    }

    /// Writes an effect directory by hand, without going through
    /// `install_effect`. It is the only way to get a reserved uid on disk — and
    /// so to check what happens then.
    fn plant_effect_dir(tmp: &tempfile::TempDir, dir_name: &str, uid: &str, js: &str) {
        let dir = tmp.path().join("data").join("effects").join(dir_name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(JS_FILE), js).unwrap();
        fs::write(dir.join(SOURCE_FILE), js).unwrap();
        fs::write(
            dir.join(MANIFEST_FILE),
            serde_json::to_string(&Manifest {
                uid: uid.into(),
                ..test_manifest("Usurpateur")
            })
            .unwrap(),
        )
        .unwrap();
    }

    /// The uid most tests install under: saving twice with it is saving the
    /// same effect again.
    const UID: &str = "0b5f1a3e-7c2d-4e8f-9a6b-1c2d3e4f5a6b";
    const OTHER_UID: &str = "7e6d5c4b-3a29-4817-8f6e-5d4c3b2a1908";

    fn test_manifest(name: &str) -> Manifest {
        Manifest {
            uid: UID.into(),
            name: name.into(),
            description: "Une onde de teinte se propage depuis le centre".into(),
            params: serde_json::json!({
                "speed": { "kind": "number", "label": "Vitesse", "min": 0, "max": 400, "default": 120 }
            })
            .as_object()
            .unwrap()
            .clone(),
            api_version: EFFECTS_API_VERSION,
        }
    }

    #[test]
    fn install_then_read_round_trips() {
        let (tmp, store) = temp_store();
        let manifest = test_manifest("Onde circulaire");

        let id = store
            .install_effect(
                "export const x: number = 1",
                "export const x = 1",
                &manifest,
            )
            .unwrap();
        assert_eq!(id, UID, "the id of an effect is its uid");

        // The directory is named after the effect, for whoever browses it.
        let dir = tmp
            .path()
            .join("data")
            .join("effects")
            .join("onde-circulaire");
        assert_eq!(
            fs::read_to_string(dir.join("source.ts")).unwrap(),
            "export const x: number = 1"
        );
        assert_eq!(
            fs::read_to_string(dir.join("effect.js")).unwrap(),
            "export const x = 1",
            "the .js is a deliverable, not a cache: it must be on disk"
        );

        let effects = installed_effects(&store);
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].id, id);
        // The uid travels as the entry's id, not twice.
        assert_eq!(
            effects[0].manifest,
            Manifest {
                uid: String::new(),
                ..manifest
            }
        );
        assert_eq!(store.effect_js(&id).unwrap(), "export const x = 1");
    }

    #[test]
    fn reinstalling_the_same_uid_updates_instead_of_duplicating() {
        let (_tmp, store) = temp_store();
        store
            .install_effect("v1", "v1", &test_manifest("Onde"))
            .unwrap();
        store
            .install_effect("v2", "v2", &test_manifest("Onde"))
            .unwrap();

        assert_eq!(installed_effects(&store).len(), 1);
        assert_eq!(store.effect_source(UID).unwrap(), "v2");
    }

    /// **Renaming no longer makes a new effect**: the uid finds the directory,
    /// whatever the name says now. The directory keeps its first name — nothing
    /// decides on it any more.
    #[test]
    fn renaming_keeps_the_directory_and_the_uid() {
        let (tmp, store) = temp_store();
        store
            .install_effect("v1", "v1", &test_manifest("Onde"))
            .unwrap();
        store
            .install_effect("v2", "v2", &test_manifest("Vague"))
            .unwrap();

        let effects = installed_effects(&store);
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].id, UID);
        assert_eq!(effects[0].manifest.name, "Vague");
        let effects_dir = tmp.path().join("data").join("effects");
        assert!(effects_dir.join("onde").is_dir());
        assert!(!effects_dir.join("vague").exists());
    }

    /// Two effects may share a name, never a uid: the second one gets a
    /// directory of its own instead of overwriting the first.
    #[test]
    fn two_effects_with_the_same_name_both_stay() {
        let (tmp, store) = temp_store();
        store
            .install_effect("a", "a", &test_manifest("Onde"))
            .unwrap();
        let other = Manifest {
            uid: OTHER_UID.into(),
            ..test_manifest("Onde")
        };
        store.install_effect("b", "b", &other).unwrap();

        assert_eq!(installed_effects(&store).len(), 2);
        assert_eq!(store.effect_source(UID).unwrap(), "a");
        assert_eq!(store.effect_source(OTHER_UID).unwrap(), "b");
        assert!(tmp
            .path()
            .join("data")
            .join("effects")
            .join("onde-2")
            .is_dir());
    }

    /// The suffix must not push a long name past what the library reads back.
    #[test]
    fn a_suffixed_directory_name_stays_valid() {
        let (_tmp, store) = temp_store();
        let long = "a".repeat(200);
        store.install_effect("", "", &test_manifest(&long)).unwrap();
        let other = Manifest {
            uid: OTHER_UID.into(),
            ..test_manifest(&long)
        };
        store.install_effect("", "", &other).unwrap();

        assert_eq!(installed_effects(&store).len(), 2);
        let next = store.free_dir_name(&long);
        validate_id(&next).unwrap_or_else(|e| panic!("\"{next}\": {e}"));
        assert!(next.ends_with("-3"), "name: {next}");
    }

    #[test]
    fn an_effect_without_a_valid_uid_is_refused() {
        let (tmp, store) = temp_store();
        for uid in ["", "onde", "0B5F1A3E-7C2D-4E8F-9A6B-1C2D3E4F5A6B"] {
            let manifest = Manifest {
                uid: uid.into(),
                ..test_manifest("Onde")
            };
            let err = store.install_effect("", "", &manifest).unwrap_err();
            assert!(err.contains("identifiant"), "\"{uid}\": {err}");
        }
        assert!(!tmp.path().join("data").join("effects").exists());
    }

    #[test]
    fn library_holds_only_builtins_before_any_install() {
        let (_tmp, store) = temp_store();
        let effects = store.list_effects().unwrap();

        assert_eq!(effects.len(), builtins::ALL.len());
        assert!(effects.iter().all(|e| e.kind == EffectKind::Builtin));
        // The gallery is never empty on first launch: that is the whole point
        // of shipped effects.
        assert!(!effects.is_empty());

        for (entry, b) in effects.iter().zip(&builtins::ALL) {
            assert_eq!(entry.id, b.uid);
            assert_eq!(entry.manifest.name, b.name);
            assert_eq!(entry.manifest.api_version, EFFECTS_API_VERSION);
            assert!(
                !entry.manifest.params.is_empty(),
                "\"{}\": parameters lost while reading the JSON",
                b.slug
            );
            // Built-ins have no directory, but they do have a swatch: it is
            // computed in memory, the first time the library is read.
            assert!(!entry.swatch.is_empty(), "\"{}\": no color swatch", b.slug);
        }
    }

    // ------------------------------------------------------------ swatch

    /// A single-color effect: its swatch is that color, four times.
    fn solid_effect(hex: &str) -> String {
        format!(
            "export default {{ name: 'Uni', render({{ layout, frame }}) {{ \
             for (const key of layout.keys) frame.set(key, {{ r: 0x{}, g: 0x{}, b: 0x{} }}) }} }}",
            &hex[0..2],
            &hex[2..4],
            &hex[4..6]
        )
    }

    fn swatch_on_disk(tmp: &tempfile::TempDir, dir_name: &str) -> Option<String> {
        let path = tmp
            .path()
            .join("data")
            .join("effects")
            .join(dir_name)
            .join(SWATCH_FILE);
        fs::read_to_string(path).ok()
    }

    /// The swatch is written at install, next to the manifest, and the list
    /// returns it without a second call.
    #[test]
    fn install_samples_the_swatch_from_the_effect() {
        let (tmp, store) = temp_store();
        store
            .install_effect("", &solid_effect("00ff00"), &test_manifest("Uni"))
            .unwrap();

        assert_eq!(
            swatch_on_disk(&tmp, "uni").as_deref(),
            Some(r##"["#00ff00","#00ff00","#00ff00","#00ff00"]"##),
            "the swatch must be stored next to the manifest"
        );
        assert_eq!(installed_effects(&store)[0].swatch, vec!["#00ff00"; 4]);
    }

    /// Saving a modified effect again redoes its swatch: that is the whole reason
    /// for sampling it rather than declaring it. An effect that turned red cannot
    /// keep its green thumbnail.
    #[test]
    fn saving_an_effect_again_resamples_its_swatch() {
        let (_tmp, store) = temp_store();
        store
            .install_effect("", &solid_effect("00ff00"), &test_manifest("Uni"))
            .unwrap();
        store
            .install_effect("", &solid_effect("ff0000"), &test_manifest("Uni"))
            .unwrap();

        assert_eq!(installed_effects(&store)[0].swatch, vec!["#ff0000"; 4]);
    }

    /// **A swatch that cannot be computed does not prevent the install.** It is
    /// user code: it is allowed to be broken, and the effect must still be stored —
    /// otherwise it could not even be reopened in the editor to fix it.
    #[test]
    fn a_throwing_effect_still_installs_without_a_swatch() {
        let (tmp, store) = temp_store();
        let js = "export default { name: 'Cassé', render() { throw new Error('boum') } }";

        let id = store
            .install_effect("source", js, &test_manifest("Cassé"))
            .unwrap();

        assert_eq!(
            store.effect_js(&id).unwrap(),
            js,
            "the effect must be written"
        );
        assert_eq!(swatch_on_disk(&tmp, "cass"), None);
        assert!(installed_effects(&store)[0].swatch.is_empty());
    }

    /// And the previous swatch is **erased**, not kept: showing the colors of a
    /// version that no longer exists would be worse than showing none.
    #[test]
    fn an_effect_that_breaks_loses_its_swatch() {
        let (tmp, store) = temp_store();
        store
            .install_effect("", &solid_effect("00ff00"), &test_manifest("Uni"))
            .unwrap();
        assert!(swatch_on_disk(&tmp, "uni").is_some());

        store
            .install_effect(
                "",
                "export default { render() { throw 1 } }",
                &test_manifest("Uni"),
            )
            .unwrap();

        assert_eq!(swatch_on_disk(&tmp, "uni"), None);
        assert!(installed_effects(&store)[0].swatch.is_empty());
    }

    #[test]
    fn delete_removes_the_directory() {
        let (tmp, store) = temp_store();
        let id = store
            .install_effect("", "", &test_manifest("Onde"))
            .unwrap();

        store.delete_effect(&id).unwrap();
        assert!(!tmp
            .path()
            .join("data")
            .join("effects")
            .join("onde")
            .exists());
        assert!(installed_effects(&store).is_empty());

        let err = store.delete_effect(&id).unwrap_err();
        assert!(err.contains("aucun effet installé"), "message: {err}");
    }

    /// The refusal must be obtained **without deleting anything**.
    ///
    /// That is what lets the command stop the loops between the refusal and the
    /// erasure: a running built-in effect gets a no without paying for its loop
    /// being stopped along the way.
    #[test]
    fn delete_refusal_is_obtained_without_deleting_anything() {
        let (tmp, store) = temp_store();
        let id = store
            .install_effect("", "", &test_manifest("Onde"))
            .unwrap();

        store.check_deletable(&id).unwrap();
        assert!(
            tmp.path()
                .join("data")
                .join("effects")
                .join("onde")
                .is_dir(),
            "the check took the directory away"
        );

        let err = store.check_deletable(builtins::ALL[0].uid).unwrap_err();
        assert!(err.contains("effet est intégré"), "message: {err}");

        let err = store.check_deletable(OTHER_UID).unwrap_err();
        assert!(err.contains("aucun effet installé"), "message: {err}");
    }

    // ------------------------------------------------------------ built-ins

    /// The engine asks for the JavaScript by id: built-ins must therefore resolve
    /// without a directory, otherwise they would never start.
    #[test]
    fn a_builtin_effect_reads_without_a_directory() {
        let (_tmp, store) = temp_store();

        for b in &builtins::ALL {
            assert_eq!(store.effect_js(b.uid).unwrap(), b.js);
            // The source too: a shipped effect is there to be read and modified,
            // and its JavaScript *is* its source.
            assert_eq!(store.effect_source(b.uid).unwrap(), b.js);
        }
    }

    /// Taking over a uid, both ways: through install, then through a directory
    /// planted by hand.
    #[test]
    fn a_user_effect_cannot_take_over_a_builtin_uid() {
        let (tmp, store) = temp_store();

        for builtin in &builtins::ALL {
            let manifest = Manifest {
                uid: builtin.uid.into(),
                ..test_manifest("Copie")
            };
            let err = store.install_effect("", "", &manifest).unwrap_err();
            assert!(err.contains("effet intégré"), "message: {err}");
            assert!(
                !tmp.path()
                    .join("data")
                    .join("effects")
                    .join("copie")
                    .exists(),
                "\"{}\": the refusal came after the write",
                builtin.slug
            );

            // The directory planted by hand does not take over either: the
            // shipped code is still what runs, and the gallery shows only one
            // entry under that uid — the built-in's.
            plant_effect_dir(
                &tmp,
                builtin.slug,
                builtin.uid,
                "export default { render() {} }",
            );
            assert_eq!(store.effect_js(builtin.uid).unwrap(), builtin.js);
            assert_eq!(store.effect_source(builtin.uid).unwrap(), builtin.js);

            let matching: Vec<_> = store
                .list_effects()
                .unwrap()
                .into_iter()
                .filter(|e| e.id == builtin.uid)
                .collect();
            assert_eq!(matching.len(), 1, "\"{}\": listed twice", builtin.slug);
            assert_eq!(matching[0].kind, EffectKind::Builtin);
            assert_eq!(matching[0].manifest.name, builtin.name);
        }
    }

    /// A built-in cannot be deleted — but a directory taking over its uid can:
    /// otherwise it would stay on disk, invisible and impossible to remove.
    #[test]
    fn a_builtin_cannot_be_deleted_but_its_impostor_can() {
        let (tmp, store) = temp_store();
        let uid = builtins::ALL[0].uid;

        let err = store.delete_effect(uid).unwrap_err();
        assert!(err.contains("effet est intégré"), "message: {err}");

        plant_effect_dir(&tmp, "imposteur", uid, "export default { render() {} }");
        // The disk first, including for the command's prior refusal: if it
        // checked built-ins first, the impostor would be refused before even
        // reaching the deletion.
        store.check_deletable(uid).unwrap();
        store.delete_effect(uid).unwrap();
        assert!(!tmp
            .path()
            .join("data")
            .join("effects")
            .join("imposteur")
            .exists());
    }

    /// A manual copy of a directory holds the same uid twice: the list shows one
    /// entry, and it is the one that runs.
    #[test]
    fn a_copied_directory_is_listed_once() {
        let (tmp, store) = temp_store();
        plant_effect_dir(&tmp, "onde", UID, "first");
        plant_effect_dir(&tmp, "onde-copie", UID, "second");

        assert_eq!(installed_effects(&store).len(), 1);
        assert_eq!(store.effect_js(UID).unwrap(), "first");
    }

    #[test]
    fn dangerous_ids_are_refused() {
        let (tmp, store) = temp_store();
        // A directory next to `effects/`, which no traversal must reach.
        let sibling = tmp.path().join("data").join("secrets");
        fs::create_dir_all(&sibling).unwrap();
        let too_long = "x".repeat(MAX_ID_LEN + 1);

        for id in [
            "",
            "..",
            "../secrets",
            "..\\secrets",
            "effects/../../secrets",
            "a/b",
            "a\\b",
            "C:\\Windows",
            "/etc/passwd",
            "con",
            "nul",
            "com1",
            "LPT1",
            "Onde",       // uppercase: outside the allow list
            "onde effet", // space
            "onde.js",    // dot
            "-onde",
            "onde-",
            too_long.as_str(),
            // Readable ids, valid before uids.
            "onde",
            "onde-radiale",
        ] {
            let Err(err) = store.delete_effect(id) else {
                panic!("\"{id}\" should have been refused");
            };
            // The refusal must come from validation, not from the disk: if the
            // message talks about an effect not found, the id was taken for an
            // acceptable path.
            assert!(
                !err.contains("aucun effet") && !err.contains("suppression"),
                "\"{id}\" reached the disk: {err}"
            );
        }
        assert!(sibling.is_dir(), "a sibling directory was touched");
    }

    #[test]
    fn hostile_names_yield_a_safe_id() {
        let too_long = "a".repeat(200);

        for name in [
            "../../etc/passwd",
            "..",
            "  ",
            "CON",
            "NUL",
            "Onde / Vague : v2",
            "🙂🙂🙂",
            "Ondulation",
            too_long.as_str(),
        ] {
            let id = derive_id(name);
            validate_id(&id).unwrap_or_else(|e| panic!("\"{name}\" -> \"{id}\": {e}"));
        }
        assert_eq!(derive_id("Onde / Vague : v2"), "onde-vague-v2");
        assert_eq!(derive_id("🙂🙂🙂"), "effet");
        assert_eq!(derive_id("CON"), "con-effet");
    }

    #[test]
    fn an_effect_written_for_a_future_api_is_refused() {
        let (_tmp, store) = temp_store();
        let mut manifest = test_manifest("Onde");
        manifest.api_version = EFFECTS_API_VERSION + 1;

        let err = store.install_effect("", "", &manifest).unwrap_err();
        assert!(err.contains("API d'effets"), "message: {err}");

        manifest.api_version = 0;
        assert!(store.install_effect("", "", &manifest).is_err());
    }

    #[test]
    fn an_effect_without_a_name_is_refused() {
        let (_tmp, store) = temp_store();
        let err = store
            .install_effect("", "", &test_manifest("   "))
            .unwrap_err();
        assert!(err.contains("nom"), "message: {err}");
    }

    #[test]
    fn without_a_file_settings_are_the_default() {
        let (_tmp, store) = temp_store();
        assert_eq!(store.read_settings().unwrap(), Settings::default());
    }

    #[test]
    fn settings_round_trip() {
        let (tmp, store) = temp_store();
        let settings = Settings {
            version: SETTINGS_VERSION,
            preferences: Preferences {
                log_level: Some(LogLevel::Debug),
            },
            devices: vec![DeviceRecord {
                vid: 0x1532,
                pid: 0x0292,
                serial: Some("XY01".into()),
                state: DeviceState::Adopted,
                brightness: Some(128),
            }],
            active_effects: vec![ActiveEffectRecord {
                vid: 0x1532,
                pid: 0x0292,
                effect: "onde".into(),
            }],
            effect_params: vec![EffectParamsRecord {
                vid: 0x1532,
                pid: 0x0292,
                effect: "respiration".into(),
                values: to_map(&[("period", serde_json::json!(12.5))]),
            }],
            legacy_log_level: None,
        };

        store.write_settings(&settings).unwrap();
        assert_eq!(store.read_settings().unwrap(), settings);
        assert!(
            tmp.path().join("config").join("settings.json").is_file(),
            "settings go to the configuration directory, not the data directory"
        );
    }

    #[test]
    fn a_setting_missing_from_the_file_takes_its_default() {
        let (tmp, store) = temp_store();
        let config = tmp.path().join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("settings.json"),
            r#"{"devices":[{"vid":5426,"pid":658,"state":"adopted","brightness":10}]}"#,
        )
        .unwrap();

        let settings = store.read_settings().unwrap();
        assert_eq!(settings.brightness(VID, PID, None), 10);
        assert_eq!(settings.active_effect(VID, PID), None);
        assert_eq!(settings.preferences, Preferences::default());
    }

    /// **The three single-device leftovers are gone from the file.** Keeping them
    /// would have produced a `settings.json` describing one active effect, one
    /// chosen device and one brightness level, while the engine has run one per
    /// device since issue #26.
    #[test]
    fn the_file_no_longer_has_single_device_fields() {
        let (_tmp, store) = temp_store();
        let mut settings = Settings::default();
        settings.set_device_state(VID, PID, Some("XY01"), DeviceState::Adopted);
        settings.set_active_effect(VID, PID, Some("onde"));
        settings.set_brightness(VID, PID, Some("XY01"), 40);
        store.write_settings(&settings).unwrap();

        let json = serde_json::to_string(&settings).unwrap();
        for dead in [r#""activeEffect""#, r#""device":"#, r#""brightness":40,"#] {
            assert!(!json.contains(dead), "\"{dead}\" remains: {json}");
        }
        // What replaces them is there, and indexed by device.
        assert!(json.contains(r#""activeEffects":[{"vid":5426,"pid":658,"effect":"onde"}]"#));
        assert!(json.contains(r#""brightness":40"#));
    }

    /// Brightness is a decision **of the device**: two keyboards do not share a
    /// level, and that is the whole point of the move.
    #[test]
    fn brightness_is_stored_per_device() {
        let mut settings = Settings::default();
        settings.set_device_state(VID, PID, Some("XY01"), DeviceState::Adopted);
        settings.set_device_state(VID, PID + 1, Some("ZZ02"), DeviceState::Adopted);

        assert!(settings.set_brightness(VID, PID, Some("XY01"), 40));
        assert_eq!(settings.brightness(VID, PID, Some("XY01")), 40);
        assert_eq!(
            settings.brightness(VID, PID + 1, Some("ZZ02")),
            DEFAULT_BRIGHTNESS,
            "the first device's level spilled over onto the second"
        );

        // Nothing new: no file rewrite for the same value.
        assert!(!settings.set_brightness(VID, PID, Some("XY01"), 40));
    }

    /// The default is not written, and going back to the maximum is what
    /// **removes**: the counterpart of "restore the declared values" on the
    /// effects side.
    #[test]
    fn default_brightness_leaves_no_entry() {
        let mut settings = Settings::default();

        // On a device we keep nothing about: no entry is created.
        assert!(!settings.set_brightness(VID, PID, None, DEFAULT_BRIGHTNESS));
        assert!(settings.devices.is_empty());

        // Set then brought back to the maximum: the entry appears then
        // disappears, because that was all it kept.
        assert!(settings.set_brightness(VID, PID, None, 40));
        assert_eq!(settings.devices.len(), 1);
        assert!(settings.set_brightness(VID, PID, None, DEFAULT_BRIGHTNESS));
        assert!(
            settings.devices.is_empty(),
            "an entry that no longer decides anything stayed: {:?}",
            settings.devices
        );

        // But an adoption decision does keep the entry.
        settings.set_device_state(VID, PID, None, DeviceState::Ignored);
        settings.set_brightness(VID, PID, None, 40);
        settings.set_brightness(VID, PID, None, DEFAULT_BRIGHTNESS);
        assert_eq!(settings.devices.len(), 1);
        assert_eq!(settings.device_state(VID, PID, None), DeviceState::Ignored);
    }

    /// The applied effect is an indexed list, not a scalar: two keyboards carry
    /// two effects, which is exactly what the engine does.
    #[test]
    fn the_applied_effect_is_stored_per_device() {
        let mut settings = Settings::default();

        assert!(settings.set_active_effect(VID, PID, Some("onde")));
        assert!(settings.set_active_effect(VID, PID + 1, Some("respiration")));
        assert_eq!(settings.active_effect(VID, PID), Some("onde"));
        assert_eq!(settings.active_effect(VID, PID + 1), Some("respiration"));

        // Starting the same effect again does not rewrite the file: it is a
        // double-click.
        assert!(!settings.set_active_effect(VID, PID, Some("onde")));
        // Switching effects replaces the entry, it does not stack a second one.
        assert!(settings.set_active_effect(VID, PID, Some("balayage")));
        assert_eq!(settings.active_effects.len(), 2);

        // Stopping forgets, rather than leaving an id that describes nothing.
        assert!(settings.set_active_effect(VID, PID, None));
        assert_eq!(settings.active_effect(VID, PID), None);
        assert_eq!(settings.active_effects.len(), 1);
        assert!(!settings.set_active_effect(VID, PID, None));
    }

    /// The log level **survives a restart** — the trade-off chosen for whoever
    /// tracks a bug at startup — but as long as nobody has changed it, it does not
    /// appear in the file: writing the default would suggest a decision where
    /// there was none.
    #[test]
    fn log_level_is_kept_and_written_only_when_chosen() {
        let (tmp, store) = temp_store();
        let settings_path = tmp.path().join("config").join("settings.json");

        store.write_settings(&Settings::default()).unwrap();
        let written = fs::read_to_string(&settings_path).unwrap();
        assert!(
            !written.contains("logLevel"),
            "the default was written: {written}"
        );

        let settings = Settings {
            preferences: Preferences {
                log_level: Some(LogLevel::Trace),
            },
            ..Settings::default()
        };
        store.write_settings(&settings).unwrap();
        assert!(fs::read_to_string(&settings_path)
            .unwrap()
            .contains(r#""logLevel": "trace""#));
        assert_eq!(
            store.read_settings().unwrap().preferences.log_level,
            Some(LogLevel::Trace)
        );
    }

    /// **The v2.1 bridge.** `logLevel` left the root for [`Preferences`]; an
    /// earlier file must still get there, otherwise the level would drop back to
    /// the default under whoever was precisely chasing a failure.
    #[test]
    fn a_log_level_written_at_the_root_is_recovered() {
        let (tmp, store) = temp_store();
        let config = tmp.path().join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("settings.json"), r#"{"logLevel":"debug"}"#).unwrap();

        let settings = store.read_settings().unwrap();
        assert_eq!(settings.preferences.log_level, Some(LogLevel::Debug));

        // And it does not go back to the root: the bridge translates once.
        store.write_settings(&settings).unwrap();
        let written = fs::read_to_string(config.join("settings.json")).unwrap();
        assert!(written.contains(r#""preferences""#), "written: {written}");
        assert_eq!(
            written.matches(r#""logLevel""#).count(),
            1,
            "the level is written twice: {written}"
        );
    }

    /// What is already in place wins over the legacy key: a file written by this
    /// version is right against a root key a text editor may have left in it.
    #[test]
    fn preferences_win_over_the_legacy_key() {
        let (tmp, store) = temp_store();
        let config = tmp.path().join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("settings.json"),
            r#"{"logLevel":"debug","preferences":{"logLevel":"error"}}"#,
        )
        .unwrap();

        assert_eq!(
            store.read_settings().unwrap().preferences.log_level,
            Some(LogLevel::Error)
        );
    }

    #[test]
    fn unreadable_settings_give_a_readable_message() {
        let (tmp, store) = temp_store();
        let config = tmp.path().join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("settings.json"), "{ ceci n'est pas du JSON").unwrap();

        let err = store.read_settings().unwrap_err();
        assert!(err.contains("réglages illisibles"), "message: {err}");
    }

    /// **The distinction this whole module keeps**, checked where it is most
    /// costly to lose: resetting the configuration to the default does not empty
    /// the library. Whoever only wants to un-adopt a keyboard must not lose
    /// hand-written code in the process.
    #[test]
    fn reset_forgets_configuration_and_keeps_effects() {
        let (tmp, store) = temp_store();
        let id = store
            .install_effect("la source", "le js", &test_manifest("Onde"))
            .unwrap();

        let mut settings = Settings::default();
        settings.set_device_state(VID, PID, Some("XY01"), DeviceState::Adopted);
        settings.set_brightness(VID, PID, Some("XY01"), 12);
        settings.set_active_effect(VID, PID, Some(&id));
        settings.set_effect_params(VID, PID, &id, to_map(&[("speed", serde_json::json!(3))]));
        store.write_settings(&settings).unwrap();

        store.reset_settings().unwrap();

        assert_eq!(store.read_settings().unwrap(), Settings::default());
        assert!(
            tmp.path().join("config").join("settings.json").is_file(),
            "the file disappeared instead of being reset"
        );

        // And the library is intact, source included: that is what cannot be
        // reinstalled.
        assert_eq!(installed_effects(&store).len(), 1);
        assert_eq!(store.effect_source(&id).unwrap(), "la source");
        assert_eq!(store.effect_js(&id).unwrap(), "le js");
    }

    // ------------------------------------------------------- adoption

    const VID: u16 = 0x1532;
    const PID: u16 = 0x0292;

    /// The default, and it is the heart of the decision: plugging in is not
    /// adopting.
    #[test]
    fn a_never_seen_device_is_detected_not_controlled() {
        let settings = Settings::default();
        assert_eq!(
            settings.device_state(VID, PID, Some("XY01")),
            DeviceState::Detected
        );
        assert!(settings.devices.is_empty());
    }

    #[test]
    fn a_decision_is_stored_then_changed() {
        let (_tmp, store) = temp_store();
        let mut settings = Settings::default();

        settings.set_device_state(VID, PID, Some("XY01"), DeviceState::Adopted);
        store.write_settings(&settings).unwrap();
        assert_eq!(
            store
                .read_settings()
                .unwrap()
                .device_state(VID, PID, Some("XY01")),
            DeviceState::Adopted
        );

        settings.set_device_state(VID, PID, Some("XY01"), DeviceState::Ignored);
        store.write_settings(&settings).unwrap();
        let reloaded = store.read_settings().unwrap();
        assert_eq!(
            reloaded.device_state(VID, PID, Some("XY01")),
            DeviceState::Ignored
        );
        // Changing one's mind modifies the entry, it does not stack a second one:
        // otherwise the oldest would end up answering in place of the right one.
        assert_eq!(reloaded.devices.len(), 1);
    }

    /// The serial is the identity, and this is what it is for: two identical
    /// keyboards, a single decision. Without it, adopting one would adopt the
    /// other.
    #[test]
    fn two_units_of_the_same_model_differ_by_serial() {
        let mut settings = Settings::default();
        settings.set_device_state(VID, PID, Some("XY01"), DeviceState::Adopted);

        assert_eq!(
            settings.device_state(VID, PID, Some("XY01")),
            DeviceState::Adopted
        );
        assert_eq!(
            settings.device_state(VID, PID, Some("XY02")),
            DeviceState::Detected,
            "the second unit inherited the decision made for the first"
        );

        settings.set_device_state(VID, PID, Some("XY02"), DeviceState::Ignored);
        assert_eq!(settings.devices.len(), 2);
        assert_eq!(
            settings.device_state(VID, PID, Some("XY01")),
            DeviceState::Adopted
        );
    }

    /// The other way: an enumeration that reports no serial — hidraw without a
    /// udev rule — still finds the adopted device. Making the decision again on
    /// every plug-in would be exactly the ceremony being removed.
    #[test]
    fn a_silent_enumeration_finds_the_adopted_device() {
        let mut settings = Settings::default();
        settings.set_device_state(VID, PID, Some("XY01"), DeviceState::Adopted);

        assert_eq!(settings.device_state(VID, PID, None), DeviceState::Adopted);

        // And the serial is not erased along the way, otherwise the second unit
        // would become indistinguishable from the first.
        settings.set_device_state(VID, PID, None, DeviceState::Adopted);
        assert_eq!(settings.devices[0].serial.as_deref(), Some("XY01"));
    }

    /// A decision made without a serial is completed as soon as the serial is
    /// learned, rather than leaving a broad entry next to a precise one.
    #[test]
    fn the_serial_completes_an_entry_that_had_none() {
        let mut settings = Settings::default();
        settings.set_device_state(VID, PID, None, DeviceState::Adopted);
        settings.set_device_state(VID, PID, Some("XY01"), DeviceState::Adopted);

        assert_eq!(settings.devices.len(), 1);
        assert_eq!(settings.devices[0].serial.as_deref(), Some("XY01"));
    }

    /// A file from an earlier version knows neither `devices` nor the current
    /// shape. It must reload without error, and rewriting it must not lose what
    /// was just decided.
    ///
    /// What **does not survive**, and it is the subject of issue #64: the three
    /// single-device fields. `activeEffect` and `device` were neither read nor
    /// written by anyone, and `brightness` at the root described a shared level
    /// two keyboards have no reason to have. Recovering them would have required
    /// choosing *which* device they designated — a question with no answer.
    #[test]
    fn an_older_file_reloads_and_keeps_its_settings() {
        let (tmp, store) = temp_store();
        let config = tmp.path().join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("settings.json"),
            r#"{"activeEffect":"onde-radiale","brightness":90,"device":{"vid":5426,"pid":658}}"#,
        )
        .unwrap();

        let mut settings = store.read_settings().unwrap();
        assert!(settings.devices.is_empty());
        assert!(settings.active_effects.is_empty());
        settings.set_device_state(VID, PID, None, DeviceState::Adopted);
        store.write_settings(&settings).unwrap();

        let reloaded = store.read_settings().unwrap();
        assert_eq!(reloaded.device_state(VID, PID, None), DeviceState::Adopted);
        assert_eq!(reloaded.brightness(VID, PID, None), DEFAULT_BRIGHTNESS);
    }

    /// Fields go out in camelCase, like every DTO, and an entry without serial or
    /// brightness writes no empty key.
    #[test]
    fn devices_serialize_in_camel_case() {
        let mut settings = Settings::default();
        settings.set_device_state(VID, PID, None, DeviceState::Adopted);
        let json = serde_json::to_string(&settings).unwrap();

        assert!(json.contains(r#""devices":[{"vid":5426,"pid":658,"state":"adopted"}]"#));
        assert!(json.contains(r#""activeEffects":[]"#));
        assert!(!json.contains("serial"), "empty key written: {json}");
        assert!(!json.contains("brightness"), "default written: {json}");
    }

    // ------------------------------------------------- effect parameters

    /// A map of values, written the way the interface sends it.
    fn to_map(pairs: &[(&str, serde_json::Value)]) -> serde_json::Map<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect()
    }

    /// The heart of issue #28: switching effects then coming back loses nothing,
    /// and two devices do not step on each other.
    #[test]
    fn params_are_stored_per_device_and_per_effect() {
        let mut settings = Settings::default();
        let slow = to_map(&[("speed", serde_json::json!(0.5))]);
        let fast = to_map(&[("speed", serde_json::json!(9.0))]);

        settings.set_effect_params(VID, PID, "balayage", slow.clone());
        settings.set_effect_params(VID, PID, "respiration", fast.clone());
        // Same effect, another device: one more entry, not an overwrite.
        settings.set_effect_params(VID, PID + 1, "balayage", fast.clone());

        assert_eq!(settings.effect_params(VID, PID, "balayage"), Some(&slow));
        assert_eq!(settings.effect_params(VID, PID, "respiration"), Some(&fast));
        assert_eq!(
            settings.effect_params(VID, PID + 1, "balayage"),
            Some(&fast)
        );
        assert_eq!(settings.effect_params(VID, PID, "onde-radiale"), None);
    }

    /// Moving the same slider a hundred times does not write a hundred entries:
    /// that is exactly what a mouse drag produces.
    #[test]
    fn tuning_the_same_effect_twice_replaces_the_entry() {
        let mut settings = Settings::default();
        for i in 0..5 {
            settings.set_effect_params(VID, PID, "balayage", to_map(&[("speed", i.into())]));
        }

        assert_eq!(settings.effect_params.len(), 1);
        assert_eq!(
            settings.effect_params(VID, PID, "balayage"),
            Some(&to_map(&[("speed", serde_json::json!(4))]))
        );
    }

    /// Restoring the declared values **forgets**, instead of writing a copy of
    /// them: the effect starts again from its manifest, including when a later
    /// version changes its defaults.
    #[test]
    fn restoring_declared_values_removes_the_entry() {
        let mut settings = Settings::default();
        settings.set_effect_params(VID, PID, "balayage", to_map(&[("speed", 3.into())]));
        settings.set_effect_params(VID, PID, "balayage", serde_json::Map::new());

        assert!(settings.effect_params.is_empty());
        assert_eq!(settings.effect_params(VID, PID, "balayage"), None);

        // And forgetting what was never set does not create an empty entry.
        settings.set_effect_params(VID, PID, "onde-radiale", serde_json::Map::new());
        assert!(settings.effect_params.is_empty());
    }

    /// A file from an earlier version does not know `effectParams`. It reloads —
    /// that is what `#[serde(default)]` on the struct guarantees — and rewriting it
    /// loses neither the adoption nor the brightness.
    #[test]
    fn a_file_without_effect_params_reloads_and_accepts_them() {
        let (tmp, store) = temp_store();
        let config = tmp.path().join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("settings.json"),
            r#"{"devices":[{"vid":5426,"pid":658,"state":"adopted","brightness":90}]}"#,
        )
        .unwrap();

        let mut settings = store.read_settings().unwrap();
        assert!(settings.effect_params.is_empty());

        let chosen = to_map(&[
            ("speed", serde_json::json!(0.5)),
            ("color", serde_json::json!({ "r": 0, "g": 180, "b": 255 })),
        ]);
        settings.set_effect_params(VID, PID, "balayage", chosen.clone());
        store.write_settings(&settings).unwrap();

        let reloaded = store.read_settings().unwrap();
        assert_eq!(reloaded.effect_params(VID, PID, "balayage"), Some(&chosen));
        assert_eq!(reloaded.brightness(VID, PID, None), 90);
        assert_eq!(reloaded.device_state(VID, PID, None), DeviceState::Adopted);
    }

    /// The four kinds of `ParamSpec` survive the disk as they are: the Rust side
    /// does not interpret them, and it must not damage them either. A color is a
    /// `{r,g,b}` object, not a string.
    #[test]
    fn the_four_kinds_of_values_round_trip() {
        let (_tmp, store) = temp_store();
        let chosen = to_map(&[
            ("speed", serde_json::json!(0.5)),
            ("bounce", serde_json::json!(true)),
            ("axis", serde_json::json!("vertical")),
            ("color", serde_json::json!({ "r": 255, "g": 96, "b": 0 })),
        ]);

        let mut settings = Settings::default();
        settings.set_effect_params(VID, PID, "balayage", chosen.clone());
        store.write_settings(&settings).unwrap();

        assert_eq!(
            store
                .read_settings()
                .unwrap()
                .effect_params(VID, PID, "balayage"),
            Some(&chosen)
        );
    }

    /// Deleting an effect takes its parameters away, on every device, and only
    /// its own. Otherwise `settings.json` would keep entries for an id nothing
    /// designates any more — and an effect reinstalled later under the same name
    /// would inherit the parameters of its namesake.
    #[test]
    fn forgetting_an_effect_removes_its_params_everywhere() {
        let mut settings = Settings::default();
        settings.set_effect_params(VID, PID, "balayage", to_map(&[("speed", 3.into())]));
        settings.set_effect_params(VID, PID + 1, "balayage", to_map(&[("speed", 9.into())]));
        settings.set_effect_params(VID, PID, "respiration", to_map(&[("period", 12.into())]));

        assert!(settings.forget_effect("balayage"));
        assert_eq!(settings.effect_params.len(), 1);
        assert_eq!(settings.effect_params(VID, PID, "balayage"), None);
        assert_eq!(settings.effect_params(VID, PID + 1, "balayage"), None);
        assert!(settings.effect_params(VID, PID, "respiration").is_some());

        // Nothing to remove: the file has no reason to be rewritten.
        assert!(!settings.forget_effect("balayage"));
        assert!(!settings.forget_effect("jamais-regle"));
    }

    /// **The trap of issue #48, settled.** Deleting the applied effect must purge
    /// its id, otherwise `settings.json` would name as applied an effect the
    /// library no longer knows — and the day the effect is resumed at startup, we
    /// would try to start an effect that is not there.
    #[test]
    fn forgetting_an_effect_also_purges_where_it_is_applied() {
        let mut settings = Settings::default();
        settings.set_active_effect(VID, PID, Some("a-supprimer"));
        settings.set_active_effect(VID, PID + 1, Some("a-supprimer"));
        settings.set_active_effect(VID, PID + 2, Some("epargne"));

        assert!(settings.forget_effect("a-supprimer"));
        assert_eq!(settings.active_effect(VID, PID), None);
        assert_eq!(settings.active_effect(VID, PID + 1), None);
        assert_eq!(
            settings.active_effect(VID, PID + 2),
            Some("epargne"),
            "the deletion took away another device's effect"
        );
    }

    /// Effect parameters go out in camelCase like the rest of the DTOs.
    #[test]
    fn effect_params_serialize_in_camel_case() {
        let mut settings = Settings::default();
        settings.set_effect_params(VID, PID, "balayage", to_map(&[("speed", 3.into())]));
        let json = serde_json::to_string(&settings).unwrap();

        assert!(
            json.contains(
                r#""effectParams":[{"vid":5426,"pid":658,"effect":"balayage","values":{"speed":3}}]"#
            ),
            "serialization: {json}"
        );
    }

    // ------------------------------------------------- migration to uids

    /// A data folder as the version before uids left it: one user effect whose
    /// id is its directory name, and settings pointing at it and at two
    /// shipped effects by their old ids.
    fn plant_pre_uid_library(tmp: &tempfile::TempDir) {
        let dir = tmp.path().join("data").join("effects").join("mon-effet");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(SOURCE_FILE),
            "export default defineEffect({ name: 'Mon effet' })",
        )
        .unwrap();
        fs::write(dir.join(JS_FILE), "export default { name: 'Mon effet' }").unwrap();
        fs::write(
            dir.join(MANIFEST_FILE),
            "{\n  \"name\": \"Mon effet\",\n  \"description\": \"\",\n  \"params\": {},\n  \"apiVersion\": 1\n}",
        )
        .unwrap();

        let config = tmp.path().join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("settings.json"),
            r#"{
              "preferences": {},
              "devices": [{ "vid": 5426, "pid": 658, "state": "adopted" }],
              "activeEffects": [
                { "vid": 5426, "pid": 658, "effect": "mon-effet" },
                { "vid": 5426, "pid": 659, "effect": "respiration" }
              ],
              "effectParams": [
                { "vid": 5426, "pid": 658, "effect": "balayage", "values": { "speed": 4 } },
                { "vid": 5426, "pid": 658, "effect": "disparu", "values": { "speed": 1 } }
              ]
            }"#,
        )
        .unwrap();
    }

    #[test]
    fn a_pre_uid_library_moves_to_uids() {
        let (tmp, store) = temp_store();
        plant_pre_uid_library(&tmp);
        assert!(
            installed_effects(&store).is_empty(),
            "an effect without a uid has no id to be listed under"
        );

        let migration = store.migrate_effect_ids().unwrap();
        assert_eq!(
            migration,
            Migration {
                effects: 1,
                settings: true
            }
        );

        let effects = installed_effects(&store);
        assert_eq!(effects.len(), 1);
        let uid = effects[0].id.clone();
        validate_uid(&uid).unwrap();
        assert_eq!(effects[0].manifest.name, "Mon effet");
        assert_eq!(
            store.effect_js(&uid).unwrap(),
            "export default { name: 'Mon effet' }"
        );
        assert_eq!(
            fs::read_to_string(
                tmp.path()
                    .join("data")
                    .join("effects")
                    .join("mon-effet")
                    .join(SOURCE_FILE)
            )
            .unwrap(),
            "export default defineEffect({ name: 'Mon effet' })",
            "the source is the editor's to change"
        );

        let settings = store.read_settings().unwrap();
        assert_eq!(settings.version, SETTINGS_VERSION);
        assert_eq!(settings.active_effect(VID, PID), Some(uid.as_str()));
        assert_eq!(
            settings.active_effect(VID, PID + 1),
            builtins::by_slug("respiration").map(|b| b.uid)
        );
        let balayage = builtins::by_slug("balayage").unwrap().uid;
        assert_eq!(
            settings.effect_params(VID, PID, balayage),
            Some(&to_map(&[("speed", serde_json::json!(4))]))
        );
        // An id that names nothing is left as it was, not guessed.
        assert!(settings.effect_params(VID, PID, "disparu").is_some());
        // And what has nothing to do with effects is untouched.
        assert_eq!(settings.device_state(VID, PID, None), DeviceState::Adopted);
    }

    /// At every startup, and nothing happens the second time: not one byte of
    /// the files changes.
    #[test]
    fn migrating_twice_changes_nothing() {
        let (tmp, store) = temp_store();
        plant_pre_uid_library(&tmp);
        store.migrate_effect_ids().unwrap();

        let manifest_path = tmp
            .path()
            .join("data")
            .join("effects")
            .join("mon-effet")
            .join(MANIFEST_FILE);
        let settings_path = tmp.path().join("config").join("settings.json");
        let manifest = fs::read_to_string(&manifest_path).unwrap();
        let settings = fs::read_to_string(&settings_path).unwrap();

        assert_eq!(store.migrate_effect_ids().unwrap(), Migration::default());
        assert_eq!(fs::read_to_string(&manifest_path).unwrap(), manifest);
        assert_eq!(fs::read_to_string(&settings_path).unwrap(), settings);
    }

    /// Once recorded, the version keeps a later directory whose name happens to
    /// be an old id from being taken for it.
    #[test]
    fn settings_already_on_uids_are_not_rewritten() {
        let (tmp, store) = temp_store();
        let mut settings = Settings::default();
        settings.set_active_effect(VID, PID, Some("onde"));
        store.write_settings(&settings).unwrap();
        store
            .install_effect("", "", &test_manifest("Onde"))
            .unwrap();

        assert_eq!(store.migrate_effect_ids().unwrap(), Migration::default());
        assert_eq!(
            store.read_settings().unwrap().active_effect(VID, PID),
            Some("onde")
        );
        let _ = tmp;
    }

    /// First launch: no settings file, and none is created for the migration.
    #[test]
    fn a_first_launch_writes_no_settings() {
        let (tmp, store) = temp_store();
        assert_eq!(store.migrate_effect_ids().unwrap(), Migration::default());
        assert!(!tmp.path().join("config").join("settings.json").exists());
    }

    /// A file written before the version field reads as version 0 — not as the
    /// current version the defaults carry, which would skip the migration.
    #[test]
    fn a_file_without_version_reads_as_version_zero() {
        let (tmp, store) = temp_store();
        let config = tmp.path().join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("settings.json"), r#"{"devices":[]}"#).unwrap();

        assert_eq!(store.read_settings().unwrap().version, 0);
        assert_eq!(Settings::default().version, SETTINGS_VERSION);
    }

    /// The table the window renames its drafts with: shipped effects by their
    /// old ids, user effects by their directory names, and a shipped effect
    /// winning over a directory of the same name, as it did at run time.
    #[test]
    fn legacy_ids_cover_shipped_and_installed_effects() {
        let (tmp, store) = temp_store();
        plant_pre_uid_library(&tmp);
        plant_effect_dir(&tmp, "respiration", OTHER_UID, "");
        store.migrate_effect_ids().unwrap();

        let renames = store.legacy_effect_ids().unwrap();
        for b in &builtins::ALL {
            assert_eq!(renames.get(b.slug).map(String::as_str), Some(b.uid));
        }
        let mine = &installed_effects(&store)
            .into_iter()
            .find(|e| e.manifest.name == "Mon effet")
            .unwrap()
            .id;
        assert_eq!(renames.get("mon-effet"), Some(mine));
    }

    #[test]
    fn uids_have_one_shape() {
        validate_uid(&mint_uid()).unwrap();
        validate_uid(UID).unwrap();
        for bad in [
            "",
            "onde-radiale",
            "0B5F1A3E-7C2D-4E8F-9A6B-1C2D3E4F5A6B",
            "0b5f1a3e7c2d4e8f9a6b1c2d3e4f5a6b",
            "0b5f1a3e-7c2d-4e8f-9a6b-1c2d3e4f5a6",
            "0b5f1a3e-7c2d-4e8f-9a6b-1c2d3e4f5a6g",
            "0b5f1a3e-7c2d-4e8f-9a6b-1c2d3e4f5a6b-",
            "../5f1a3e-7c2d-4e8f-9a6b-1c2d3e4f5a6b",
        ] {
            assert!(validate_uid(bad).is_err(), "\"{bad}\" was accepted");
        }
    }

    // ------------------------------------------------- starting values

    /// A manifest declaring three parameters, one of them without `default`.
    fn declared_manifest() -> Manifest {
        Manifest {
            uid: UID.into(),
            name: "Balayage".into(),
            description: String::new(),
            params: serde_json::json!({
                "speed":  { "kind": "number",  "label": "Vitesse", "default": 120 },
                "bounce": { "kind": "boolean", "label": "Rebond",  "default": false },
                "muet":   { "kind": "number",  "label": "Sans défaut" }
            })
            .as_object()
            .unwrap()
            .clone(),
            api_version: EFFECTS_API_VERSION,
        }
    }

    /// **The nominal case of the tray icon**: starting an effect with no window
    /// must give the same lighting as starting it from the gallery — so the
    /// manifest defaults, overridden by what was kept.
    #[test]
    fn starting_values_come_from_the_manifest_and_are_overridden() {
        let stored = to_map(&[("speed", serde_json::json!(40))]);
        let starting = starting_params(&declared_manifest(), Some(&stored));

        assert_eq!(starting.get("speed"), Some(&serde_json::json!(40)));
        assert_eq!(starting.get("bounce"), Some(&serde_json::json!(false)));
    }

    /// With nothing kept, what the effect declares, and nothing more: a parameter
    /// without `default` is left to the effect rather than guessed.
    #[test]
    fn a_param_without_default_is_not_invented() {
        let starting = starting_params(&declared_manifest(), None);

        assert_eq!(starting.len(), 2, "starting values: {starting:?}");
        assert!(!starting.contains_key("muet"));
    }

    /// **Limited to declared parameters**, as on the window side: a value kept for
    /// a parameter the effect no longer has disappears on its own, instead of
    /// travelling to a loop that no longer reads it.
    #[test]
    fn an_orphan_setting_does_not_reach_the_loop() {
        let stored = to_map(&[
            ("speed", serde_json::json!(40)),
            ("disparu", serde_json::json!(7)),
        ]);
        let starting = starting_params(&declared_manifest(), Some(&stored));

        assert!(!starting.contains_key("disparu"));
        assert_eq!(starting.get("speed"), Some(&serde_json::json!(40)));
    }

    // ------------------------------------------------- TypeScript mirrors

    /// The mirror, read as is.
    const CANDEO_TS: &str = include_str!("../../src/api/candeo.ts");

    /// The field names of an interface in `candeo.ts`.
    ///
    /// A plain text scan, and that is enough: there is no attempt to understand
    /// TypeScript, only to pick the identifier at the start of the lines of an
    /// `export interface X { … }` block. A comment line carries none — the filter
    /// on identifier characters rules it out, including when the sentence contains
    /// a colon.
    fn ts_fields(nom: &str) -> BTreeSet<String> {
        let header = format!("export interface {nom} {{");
        let begin = CANDEO_TS
            .find(&header)
            .unwrap_or_else(|| panic!("\"{header}\" not found in src/api/candeo.ts"));
        let body = &CANDEO_TS[begin..];
        let end = body
            .find("\n}")
            .unwrap_or_else(|| panic!("interface \"{nom}\" is not closed"));
        body[..end]
            .lines()
            .skip(1)
            .filter_map(|line| {
                let (field, _) = line.trim().split_once(':')?;
                let field = field.trim_end_matches('?');
                (!field.is_empty() && field.chars().all(|c| c.is_ascii_alphanumeric()))
                    .then(|| field.to_string())
            })
            .collect()
    }

    /// Compares the **serialized** fields of a struct with those `candeo.ts`
    /// declares for its mirror.
    ///
    /// Serialization rather than the declaration: it is what tells what actually
    /// lands in `settings.json`, `rename_all` and `skip_serializing` included.
    /// `legacy_log_level` is therefore rightly absent — it is read, never written
    /// back — and the mirror does not have to carry it.
    fn mirror(ts_name: &str, value: &impl Serialize) {
        let json = serde_json::to_value(value).expect("serialization");
        let rust: BTreeSet<String> = json
            .as_object()
            .unwrap_or_else(|| panic!("\"{ts_name}\" does not serialize to an object"))
            .keys()
            .cloned()
            .collect();
        let ts = ts_fields(ts_name);

        let missing_from_ts: Vec<&String> = rust.difference(&ts).collect();
        let missing_from_rust: Vec<&String> = ts.difference(&rust).collect();
        assert!(
            missing_from_ts.is_empty() && missing_from_rust.is_empty(),
            "\"{ts_name}\" diverged from its mirror:\n  \
             missing from src/api/candeo.ts: {missing_from_ts:?}\n  \
             missing from storage.rs: {missing_from_rust:?}"
        );
    }

    /// `Settings` is written on both sides of the IPC, and nothing ties the two
    /// together at compile time.
    ///
    /// A field added here and forgotten in `candeo.ts` does not show on read —
    /// `#[serde(default)]` fills it in — but the window that reads then rewrites
    /// the file **erases what its type does not name**. The field that nearly went
    /// that way is `logLevel`, precisely the one the log initialization depends
    /// on: the loss would have shown at the next startup, for whoever had just
    /// raised the level to understand a failure — the only moment this setting
    /// matters.
    ///
    /// Same guard as `STATE_CHANGED` and the window label: the word "mirror" is a
    /// promise, and this keeps it.
    #[test]
    fn settings_have_the_same_fields_on_both_sides() {
        // Every `Option` filled in: a field omitted by `skip_serializing_if`
        // would be missing from the comparison, and the test would let through
        // exactly what it watches for.
        let preferences = Preferences {
            log_level: Some(LogLevel::Debug),
        };
        mirror("Preferences", &preferences);
        mirror(
            "Settings",
            &Settings {
                preferences,
                ..Settings::default()
            },
        );
        mirror(
            "DeviceRecord",
            &DeviceRecord {
                vid: 0x1532,
                pid: 0x0290,
                serial: Some("SN".into()),
                state: DeviceState::Adopted,
                brightness: Some(DEFAULT_BRIGHTNESS),
            },
        );
        mirror(
            "ActiveEffectRecord",
            &ActiveEffectRecord {
                vid: 0x1532,
                pid: 0x0290,
                effect: "onde".into(),
            },
        );
        mirror(
            "EffectParamsRecord",
            &EffectParamsRecord {
                vid: 0x1532,
                pid: 0x0290,
                effect: "onde".into(),
                values: serde_json::Map::new(),
            },
        );
    }

    /// A drift of `EFFECTS_API_VERSION` was only caught in **one** direction.
    ///
    /// A manifest announcing a version newer than the Rust side is refused at
    /// install, and that is what the comment in `candeo.ts` calls "not silent".
    /// But in the other direction nothing fires: if the Rust side moved to 2
    /// without the TypeScript, the editor would keep stamping
    /// `apiVersion: 1` on effects written against the new API, and the comparison
    /// would accept them all — 1 is indeed lower than 2. The effects would be
    /// installed under a version they do not follow, and the day that version was
    /// used to refuse something, it would refuse the wrong things.
    #[test]
    fn effects_api_version_is_the_same_on_both_sides() {
        let raw_version = CANDEO_TS
            .lines()
            .find_map(|l| l.trim().strip_prefix("export const EFFECTS_API_VERSION = "))
            .expect("\"export const EFFECTS_API_VERSION\" not found in src/api/candeo.ts")
            .trim()
            .trim_end_matches(';');
        let declared: u32 = raw_version
            .parse()
            .unwrap_or_else(|e| panic!("unreadable version \"{raw_version}\" in candeo.ts: {e}"));

        assert_eq!(
            declared, EFFECTS_API_VERSION,
            "src/api/candeo.ts announces version {declared}, storage.rs {EFFECTS_API_VERSION}"
        );
    }
}
