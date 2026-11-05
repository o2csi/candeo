//! Presets: named settings for one effect, offered on every device and region
//! (`docs/design/presets.md`).
//!
//! Kept in `settings.json` beside the settings they fill. A preset is data,
//! never code: picking one is the window writing a device's or a region's
//! settings with the commands it already has, so what lives here is what
//! changes the list — saving, updating, renaming, deleting.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::failure::Failure;
use crate::storage::{store, EffectKey, Settings};
use crate::CmdResult;

/// Longest name, in characters: an effect's limit, so that both fit the same
/// place in a list (§4).
pub(crate) const MAX_NAME_LEN: usize = 64;

/// Named settings for one effect (`docs/design/presets.md` §3).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PresetRecord {
    /// The key of the effect it sets: `shipped:Radial wave`.
    pub effect: String,
    /// As it was typed, trimmed; unique within its effect, ignoring case.
    pub name: String,
    /// Every parameter the effect declared when the preset was saved, where
    /// `effectParams` keeps only what departs from the declared values: a later
    /// default must not change a look someone named.
    ///
    /// Raw JSON, as the values of `effectParams` are, and for the same reason:
    /// the window checks them against what the effect declares when the preset
    /// is picked, and a second typing here would drift from it.
    pub values: serde_json::Map<String, serde_json::Value>,
    /// Parameters reading a signal, the sound or the computer, as in
    /// `effectParams`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bindings: crate::runtime::Bindings,
    /// What this version does not know, written back unchanged (#373).
    #[serde(flatten)]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

/// The name as kept: trimmed, refused when empty or too long. Any text
/// otherwise: a preset is no file, and none of a file name's rules apply.
fn checked_name(typed: &str) -> CmdResult<String> {
    let name = typed.trim();
    if name.is_empty() {
        return Err(Failure::new("presetNameEmpty"));
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(Failure::new("presetNameTooLong").with("max", MAX_NAME_LEN));
    }
    Ok(name.to_owned())
}

/// Two names differing by case alone would be one choice in a list.
fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

fn not_found(name: &str) -> Failure {
    Failure::new("presetNotFound").with("name", name.trim())
}

impl Settings {
    /// The presets of one effect, in the order they were saved.
    pub fn presets_of(&self, effect: &str) -> Vec<PresetRecord> {
        self.presets
            .iter()
            .filter(|p| p.effect == effect)
            .cloned()
            .collect()
    }

    fn preset_position(&self, effect: &str, name: &str) -> Option<usize> {
        self.presets
            .iter()
            .position(|p| p.effect == effect && same_name(&p.name, name))
    }

    /// Adds a preset, last of its effect's list.
    pub fn save_preset(
        &mut self,
        effect: &str,
        name: &str,
        values: serde_json::Map<String, serde_json::Value>,
        bindings: crate::runtime::Bindings,
    ) -> CmdResult<()> {
        let name = checked_name(name)?;
        if self.preset_position(effect, &name).is_some() {
            return Err(Failure::new("presetExists").with("name", name));
        }
        self.presets.push(PresetRecord {
            effect: effect.to_owned(),
            name,
            values,
            bindings,
            unknown: serde_json::Map::new(),
        });
        Ok(())
    }

    /// Replaces a preset's values and bindings, where it stands in the list.
    /// Returns true if something changed.
    pub fn update_preset(
        &mut self,
        effect: &str,
        name: &str,
        values: serde_json::Map<String, serde_json::Value>,
        bindings: crate::runtime::Bindings,
    ) -> CmdResult<bool> {
        let i = self
            .preset_position(effect, name.trim())
            .ok_or_else(|| not_found(name))?;
        let preset = &mut self.presets[i];
        if preset.values == values && preset.bindings == bindings {
            return Ok(false);
        }
        preset.values = values;
        preset.bindings = bindings;
        Ok(true)
    }

    /// Renames a preset, where it stands. A change of case alone is a rename:
    /// the only name it would collide with is its own. Returns true if
    /// something changed.
    pub fn rename_preset(&mut self, effect: &str, from: &str, to: &str) -> CmdResult<bool> {
        let i = self
            .preset_position(effect, from.trim())
            .ok_or_else(|| not_found(from))?;
        let to = checked_name(to)?;
        if self.preset_position(effect, &to).is_some_and(|j| j != i) {
            return Err(Failure::new("presetExists").with("name", to));
        }
        if self.presets[i].name == to {
            return Ok(false);
        }
        self.presets[i].name = to;
        Ok(true)
    }

    /// Deletes a preset. One already gone is no error: the list the window
    /// gets back says what there is. Returns true if something changed.
    pub fn delete_preset(&mut self, effect: &str, name: &str) -> bool {
        match self.preset_position(effect, name.trim()) {
            Some(i) => {
                self.presets.remove(i);
                true
            }
            None => false,
        }
    }

    /// Numbers a name that appears twice for one effect: *Calm*, then *Calm (2)*.
    ///
    /// Renaming an effect onto one gone from the folder whose presets were kept
    /// would otherwise put two presets of one name side by side, and the second
    /// could never be picked by name.
    pub(crate) fn number_twin_presets(&mut self) {
        let mut taken: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for preset in &mut self.presets {
            let names = taken.entry(preset.effect.clone()).or_default();
            if names.contains(&preset.name.to_lowercase()) {
                preset.name = crate::storage::free_name(&preset.name, names);
            }
            names.insert(preset.name.to_lowercase());
        }
    }
}

/// Reads the settings, applies a change to the presets, and writes them back
/// when it changed something: the read, the change and the write in one go, as
/// `remember_effect_params` does, so that a stale copy in the window never
/// overwrites the file.
fn change(
    app: &AppHandle,
    effect: &str,
    apply: impl FnOnce(&mut Settings) -> CmdResult<bool>,
) -> CmdResult<Vec<PresetRecord>> {
    EffectKey::parse(effect)?;
    let store = store(app)?;
    let mut settings = store.read_settings()?;
    if apply(&mut settings)? {
        store.write_settings(&settings)?;
    }
    Ok(settings.presets_of(effect))
}

/// Saves the settings shown as a preset of `effect`. Answers with the effect's
/// presets, the new one last.
#[tauri::command]
pub fn save_preset(
    app: AppHandle,
    effect: String,
    name: String,
    values: serde_json::Map<String, serde_json::Value>,
    bindings: crate::runtime::Bindings,
) -> CmdResult<Vec<PresetRecord>> {
    let bindings = crate::runtime::checked_bindings(bindings)?;
    change(&app, &effect, |settings| {
        settings.save_preset(&effect, &name, values, bindings)?;
        Ok(true)
    })
}

/// Replaces a preset's values and bindings with the settings shown.
#[tauri::command]
pub fn update_preset(
    app: AppHandle,
    effect: String,
    name: String,
    values: serde_json::Map<String, serde_json::Value>,
    bindings: crate::runtime::Bindings,
) -> CmdResult<Vec<PresetRecord>> {
    let bindings = crate::runtime::checked_bindings(bindings)?;
    change(&app, &effect, |settings| {
        settings.update_preset(&effect, &name, values, bindings)
    })
}

/// Renames a preset.
#[tauri::command]
pub fn rename_preset(
    app: AppHandle,
    effect: String,
    from: String,
    to: String,
) -> CmdResult<Vec<PresetRecord>> {
    change(&app, &effect, |settings| {
        settings.rename_preset(&effect, &from, &to)
    })
}

/// Deletes a preset.
#[tauri::command]
pub fn delete_preset(app: AppHandle, effect: String, name: String) -> CmdResult<Vec<PresetRecord>> {
    change(&app, &effect, |settings| {
        Ok(settings.delete_preset(&effect, &name))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const WAVE: &str = "shipped:Radial wave";
    const RAIN: &str = "user:Rain";

    fn values(pairs: &[(&str, serde_json::Value)]) -> serde_json::Map<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect()
    }

    fn slow() -> serde_json::Map<String, serde_json::Value> {
        values(&[("speed", serde_json::json!(0.4))])
    }

    fn names(settings: &Settings, effect: &str) -> Vec<String> {
        settings
            .presets_of(effect)
            .into_iter()
            .map(|p| p.name)
            .collect()
    }

    fn code(result: CmdResult<impl std::fmt::Debug>) -> &'static str {
        result.expect_err("a refusal").code
    }

    #[test]
    fn a_preset_is_saved_last_of_its_effect_and_trimmed() {
        let mut settings = Settings::default();
        settings
            .save_preset(WAVE, " Slow blue ", slow(), Default::default())
            .unwrap();
        settings
            .save_preset(RAIN, "Storm", slow(), Default::default())
            .unwrap();
        settings
            .save_preset(WAVE, "Fast red", slow(), Default::default())
            .unwrap();

        assert_eq!(names(&settings, WAVE), ["Slow blue", "Fast red"]);
        assert_eq!(names(&settings, RAIN), ["Storm"]);
        assert!(settings.presets_of("shipped:Clock").is_empty());
    }

    #[test]
    fn a_name_is_unique_within_its_effect_ignoring_case() {
        let mut settings = Settings::default();
        settings
            .save_preset(WAVE, "Calm", slow(), Default::default())
            .unwrap();

        let twice = settings.save_preset(WAVE, "calm ", slow(), Default::default());
        assert_eq!(code(twice), "presetExists");
        settings
            .save_preset(RAIN, "Calm", slow(), Default::default())
            .expect("another effect may have its own Calm");
    }

    #[test]
    fn a_name_is_refused_empty_or_too_long() {
        let mut settings = Settings::default();
        assert_eq!(
            code(settings.save_preset(WAVE, "   ", slow(), Default::default())),
            "presetNameEmpty"
        );
        let long = "é".repeat(MAX_NAME_LEN + 1);
        assert_eq!(
            code(settings.save_preset(WAVE, &long, slow(), Default::default())),
            "presetNameTooLong"
        );
        settings
            .save_preset(WAVE, &"é".repeat(MAX_NAME_LEN), slow(), Default::default())
            .expect("characters are counted, not bytes");
    }

    #[test]
    fn free_text_is_a_name() {
        let mut settings = Settings::default();
        settings
            .save_preset(WAVE, "CON: a/b?", slow(), Default::default())
            .expect("what a file name refuses, a preset's name takes");
        assert_eq!(names(&settings, WAVE), ["CON: a/b?"]);
    }

    #[test]
    fn an_update_replaces_values_and_bindings_in_place() {
        let mut settings = Settings::default();
        for name in ["One", "Two", "Three"] {
            settings
                .save_preset(WAVE, name, slow(), Default::default())
                .unwrap();
        }
        let fast = values(&[("speed", serde_json::json!(2))]);
        let bound: crate::runtime::Bindings = [("hue".into(), "sound:tone".into())].into();

        assert!(settings
            .update_preset(WAVE, "two", fast.clone(), bound.clone())
            .unwrap());
        assert_eq!(names(&settings, WAVE), ["One", "Two", "Three"]);
        let two = &settings.presets_of(WAVE)[1];
        assert_eq!((&two.values, &two.bindings), (&fast, &bound));
        assert!(
            !settings.update_preset(WAVE, "Two", fast, bound).unwrap(),
            "nothing new: the file is not written"
        );
        assert_eq!(
            code(settings.update_preset(WAVE, "Four", slow(), Default::default())),
            "presetNotFound"
        );
    }

    #[test]
    fn a_rename_keeps_the_place_and_refuses_a_taken_name() {
        let mut settings = Settings::default();
        for name in ["One", "Two"] {
            settings
                .save_preset(WAVE, name, slow(), Default::default())
                .unwrap();
        }

        assert_eq!(
            code(settings.rename_preset(WAVE, "One", "TWO")),
            "presetExists"
        );
        assert_eq!(
            code(settings.rename_preset(WAVE, "One", " ")),
            "presetNameEmpty"
        );
        assert_eq!(
            code(settings.rename_preset(WAVE, "Six", "Seven")),
            "presetNotFound"
        );
        assert!(settings.rename_preset(WAVE, "One", "Uno").unwrap());
        assert!(
            settings.rename_preset(WAVE, "two", "TWO").unwrap(),
            "a change of case alone is a rename"
        );
        assert!(!settings.rename_preset(WAVE, "TWO", "TWO ").unwrap());
        assert_eq!(names(&settings, WAVE), ["Uno", "TWO"]);
    }

    #[test]
    fn a_deleted_preset_goes_and_one_already_gone_is_no_error() {
        let mut settings = Settings::default();
        settings
            .save_preset(WAVE, "Calm", slow(), Default::default())
            .unwrap();
        settings
            .save_preset(RAIN, "Calm", slow(), Default::default())
            .unwrap();

        assert!(settings.delete_preset(WAVE, "calm"));
        assert!(!settings.delete_preset(WAVE, "calm"));
        assert!(names(&settings, WAVE).is_empty());
        assert_eq!(names(&settings, RAIN), ["Calm"], "the other effect's stays");
    }

    #[test]
    fn deleting_an_effect_deletes_its_presets() {
        let mut settings = Settings::default();
        settings
            .save_preset(WAVE, "Calm", slow(), Default::default())
            .unwrap();
        settings
            .save_preset(RAIN, "Storm", slow(), Default::default())
            .unwrap();

        assert!(
            settings.forget_effect(WAVE),
            "a preset alone is something to forget"
        );
        assert!(settings.presets_of(WAVE).is_empty());
        assert_eq!(names(&settings, RAIN), ["Storm"]);
    }

    #[test]
    fn renaming_an_effect_moves_its_presets_and_numbers_a_twin() {
        let mut settings = Settings::default();
        settings
            .save_preset("user:Rain", "Calm", slow(), Default::default())
            .unwrap();
        // Kept for an effect gone from the folder, whose name the rename takes.
        settings
            .save_preset("user:Drizzle", "calm", slow(), Default::default())
            .unwrap();

        assert!(settings.rename_effect("user:Rain", "user:Drizzle"));
        assert_eq!(names(&settings, "user:Drizzle"), ["Calm", "calm (2)"]);
        assert!(settings.presets_of("user:Rain").is_empty());
    }

    #[test]
    fn a_shipped_effect_moved_to_your_folder_keeps_its_presets() {
        let mut settings = Settings::default();
        settings
            .save_preset("shipped:Drizzle", "Calm", slow(), Default::default())
            .unwrap();
        // What `migrate_sources` does with a shipped file nobody recorded.
        assert!(settings.rename_effect("shipped:Drizzle", "user:Drizzle"));
        assert_eq!(names(&settings, "user:Drizzle"), ["Calm"]);
    }

    #[test]
    fn the_file_holds_presets_only_when_there_are_some() {
        let mut settings = Settings::default();
        let empty = serde_json::to_value(&settings).unwrap();
        assert!(empty.get("presets").is_none(), "{empty}");

        settings
            .save_preset(WAVE, "Calm", slow(), Default::default())
            .unwrap();
        let written = serde_json::to_value(&settings).unwrap();
        assert_eq!(
            written["presets"],
            serde_json::json!([
                { "effect": WAVE, "name": "Calm", "values": { "speed": 0.4 } }
            ]),
            "no bindings while there is none"
        );
    }

    #[test]
    fn a_field_this_version_does_not_know_is_written_back() {
        let file = serde_json::json!({
            "presets": [{
                "effect": WAVE, "name": "Calm", "values": {},
                "bindings": { "hue": "sound:tone" }, "fromLater": { "starred": true }
            }]
        });
        let mut settings: Settings = serde_json::from_value(file).expect("settings");
        settings
            .rename_preset(WAVE, "Calm", "Quiet")
            .expect("renamed");

        let written = serde_json::to_value(&settings).unwrap();
        assert_eq!(written["presets"][0]["name"], "Quiet");
        assert_eq!(
            written["presets"][0]["fromLater"],
            serde_json::json!({ "starred": true })
        );
        assert_eq!(written["presets"][0]["bindings"]["hue"], "sound:tone");
    }

    #[test]
    fn a_file_without_presets_reads_as_none() {
        let settings: Settings = serde_json::from_value(serde_json::json!({
            "devices": [], "activeEffects": [], "effectParams": [], "rules": []
        }))
        .expect("settings");
        assert!(settings.presets.is_empty());
    }
}
