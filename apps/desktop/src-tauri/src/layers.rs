//! Layers over a target's applied effect, saved layers and saved masks
//! (`docs/design/composition.md` §3.11, `docs/design/layers-ui.md` §5).
//!
//! The records `settings.json` keeps, the checks a layer passes before it is
//! saved or started, and the commands of `src/api/layers.ts`. The engine runs
//! what they resolve to (`crate::runtime::composition`): a saved mask is
//! resolved here, and the engine only ever sees the mask it gives.
//!
//! # Lenient where the file may hold what this build does not know
//!
//! A blend, a kind of mask, a field a later build adds: read, written back
//! unchanged (#373), and not run. A record that cannot be read at all would
//! cost the whole file, so a blend is text and an opacity any integer here,
//! checked rather than typed, and every refusal is a sentence the window can
//! show rather than a command that does not deserialise.

use std::collections::{BTreeMap, BTreeSet};

use candeo_device::{Layout, Region};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::failure::Failure;
use crate::runtime::composition::{
    self, Blend, LayerStart, Look, Mask, MaskError, Refusal, StackStart, MAX_EFFECTS,
};
use crate::runtime::dimming::Dimming;
use crate::runtime::{Bindings, PartError};
use crate::storage::{EffectEntry, Manifest, Settings};
use crate::{AppState, CmdResult, DeviceRef, Scope};

/// Longest name of a saved layer or mask, in characters: an effect's and a
/// preset's, so that all fit the same place in a list.
pub(crate) const MAX_NAME_LEN: usize = 64;

/// Layers on one target at most. Eight effects run there, the base included
/// (§3.7); the base is counted whether one is applied or not, so that applying
/// one over a full stack never makes a ninth the engine would refuse.
pub(crate) const MAX_LAYERS: usize = MAX_EFFECTS - 1;

// ---------------------------------------------------------------- records

/// The lights of its target a layer may touch, as the file and the window
/// write them: the engine's [`Mask`], plus `saved`, which names a saved mask
/// and nothing beside it (`layers-ui.md` §5.5).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MaskRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Keys by scancode, as definitions write them: `"01"`, `"e01d"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
    /// Lights by address: zones, and keys without a scancode.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lights: Vec<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// A saved mask, by its id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved: Option<String>,
    /// A kind of mask a later build adds. Kept, and the mask lights nothing
    /// here: read as the whole target, it would light what nobody chose.
    #[serde(flatten)]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

/// A layer without its id: what a stack's layer, a saved layer and the
/// window's new layer share (`composition.md` §3.1). Fields at their default
/// are absent from the file.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LayerSpec {
    /// A library effect's key, `shipped:Equalizer`. One the library does not
    /// know keeps the layer in the file, marked missing, and runs nothing.
    #[serde(default)]
    pub effect: String,
    /// For a layer, what departs from the declared values, as `effectParams`
    /// keeps them; for a saved layer, every value, as a preset keeps them.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub values: serde_json::Map<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bindings: Bindings,
    /// Absent: the whole target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<MaskRecord>,
    /// Absent: *screen*. Text, so that a blend a later build adds reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend: Option<String>,
    /// 0 to 100; absent: 100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<i64>,
    /// What its opacity follows; absent: nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follows: Option<Dimming>,
    /// What this version does not know, written back unchanged (#373).
    #[serde(flatten)]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

/// One layer of a stack. Without an id, it is a new layer: what `add_layer`
/// takes, and gives one.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LayerRecord {
    /// Short, unique within its stack: what commands name the layer by.
    #[serde(default)]
    pub id: String,
    #[serde(flatten)]
    pub spec: LayerSpec,
}

/// A target's layers, bottom to top, the base excluded (`composition.md`
/// §3.11). Per model, as `activeEffects`: no serial.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StackRecord {
    pub vid: u16,
    pub pid: u16,
    /// The region of a split device it runs on; absent for a whole device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default)]
    pub stack: Vec<LayerRecord>,
    #[serde(flatten)]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

/// A named layer, put back as a copy (`layers-ui.md` §5.1).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavedLayerRecord {
    /// Unique ignoring case, 1 to [`MAX_NAME_LEN`] characters.
    #[serde(default)]
    pub name: String,
    #[serde(flatten)]
    pub spec: LayerSpec,
}

/// Named keys or lights, linked to the layers drawing through it
/// (`layers-ui.md` §5.4). `vid` and `pid` come with `lights` or `region`,
/// which mean something on one definition only.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavedMaskRecord {
    /// What layers name it by: renaming it changes no layer.
    #[serde(default)]
    pub id: String,
    /// Unique ignoring case, 1 to [`MAX_NAME_LEN`] characters.
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vid: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u16>,
    #[serde(flatten)]
    pub mask: MaskRecord,
}

/// A layer's blend, opacity and follow, as the window sends them to change
/// live: what changes without restarting the layer.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LookRecord {
    #[serde(default)]
    pub blend: Option<String>,
    #[serde(default)]
    pub opacity: Option<i64>,
    #[serde(default)]
    pub follows: Option<Dimming>,
}

/// A layer that named a deleted saved mask, so that *Undo* links it again.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LinkedLayer {
    pub vid: u16,
    pub pid: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    pub layer: String,
}

/// Where a deleted saved mask was named.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeletedMask {
    pub record: SavedMaskRecord,
    /// Its place in the list.
    pub at: usize,
    pub layers: Vec<LinkedLayer>,
    pub saved_layers: Vec<String>,
}

/// One mask the picker offers, with the frame positions it lights.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MaskChoice {
    pub id: String,
    pub positions: Vec<u16>,
}

/// What the mask picker offers on a target, those lighting nothing left out
/// (`layers-ui.md` §4.3, §5.4).
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MaskChoices {
    /// Key groups, in `KEY_GROUPS` order.
    pub groups: Vec<MaskChoice>,
    /// The definition's regions, on a whole device that declares them.
    pub regions: Vec<MaskChoice>,
    pub saved_masks: Vec<MaskChoice>,
}

/// The base a stack preview runs.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PreviewBase {
    pub effect: String,
    pub params: serde_json::Value,
    #[serde(default)]
    pub bindings: Bindings,
}

// ---------------------------------------------------------------- checks

/// A blend as the file or the window names it; absent is *screen*
/// (decision 5).
fn blend_of(text: Option<&str>) -> Result<Blend, Failure> {
    match text {
        None => Ok(Blend::Screen),
        Some("cover") => Ok(Blend::Cover),
        Some("add") => Ok(Blend::Add),
        Some("screen") => Ok(Blend::Screen),
        Some("multiply") => Ok(Blend::Multiply),
        Some(other) => Err(Failure::new("layerBlendUnknown").with("blend", other)),
    }
}

/// An opacity, 0 to 100; absent is 100.
fn opacity_of(opacity: Option<i64>) -> Result<u8, Failure> {
    match opacity {
        None => Ok(100),
        Some(value) => u8::try_from(value)
            .ok()
            .filter(|o| *o <= 100)
            .ok_or_else(|| Failure::new("layerOpacityInvalid").with("opacity", value)),
    }
}

impl LookRecord {
    /// The engine's look, checked: a blend it knows, an opacity from 0 to
    /// 100, a follow reading a source Rust reads.
    pub fn checked(&self) -> Result<Look, Failure> {
        Ok(Look {
            blend: blend_of(self.blend.as_deref())?,
            opacity: opacity_of(self.opacity)?,
            follows: self.follows.clone().map(Dimming::checked).transpose()?,
        })
    }

    /// As the file keeps it: no field at its default.
    fn stored(self) -> Self {
        Self {
            blend: self
                .blend
                .filter(|b| blend_of(Some(b)) != Ok(Blend::Screen)),
            opacity: self.opacity.filter(|o| *o != 100),
            follows: self.follows,
        }
    }
}

/// Why a mask resolves to nothing, as the window says it.
fn mask_failure(error: MaskError) -> Failure {
    match error {
        MaskError::UnknownGroup(group) => Failure::new("maskGroupUnknown").with("group", group),
        MaskError::UnknownRegion(region) => {
            Failure::new("maskRegionUnknown").with("region", region)
        }
        MaskError::NotAScancode(key) => Failure::new("maskKeyInvalid").with("key", key),
        MaskError::NothingToLight => Failure::new("maskLightsNothing"),
    }
}

fn too_many() -> Failure {
    Failure::new("layerTooMany").with("max", MAX_EFFECTS)
}

/// The engine's refusal, as the window says it. A layer given twice and an
/// order naming the wrong layers are this module's mistakes, not a gesture's.
pub(crate) fn refusal_failure(refusal: Refusal) -> Failure {
    match refusal {
        Refusal::TooManyEffects => too_many(),
        Refusal::Mask(e) => mask_failure(e),
        e @ (Refusal::LayerTwice(_) | Refusal::Order) => Failure::unexpected(e),
    }
}

impl MaskRecord {
    /// Names nothing: the whole target.
    fn is_whole(&self) -> bool {
        self.engine().is_whole() && self.saved.is_none() && self.unknown.is_empty()
    }

    /// The engine's mask: `saved` and what this build does not know left out.
    fn engine(&self) -> Mask {
        Mask {
            group: self.group.clone(),
            keys: self.keys.clone(),
            lights: self.lights.clone(),
            region: self.region.clone(),
        }
    }

    /// A mask naming the saved mask `id`.
    fn naming(id: &str) -> Self {
        Self {
            saved: Some(id.to_owned()),
            ..Self::default()
        }
    }

    /// Whether it names the saved mask `id`.
    fn names(&self, id: &str) -> bool {
        self.saved.as_deref() == Some(id)
    }

    /// What can be refused without a device: `saved` beside another field, a
    /// kind of mask this build does not know, a group nobody defined, a key
    /// that is no scancode.
    fn check_shape(&self) -> Result<(), Failure> {
        if self.saved.is_some() && (!self.engine().is_whole() || !self.unknown.is_empty()) {
            return Err(Failure::new("maskSavedNotAlone"));
        }
        if !self.unknown.is_empty() {
            return Err(Failure::new("maskUnknown"));
        }
        if let Some(group) = &self.group {
            if candeo_device::groups::key_group(group).is_none() {
                return Err(Failure::new("maskGroupUnknown").with("group", group));
            }
        }
        if let Some(key) = self
            .keys
            .iter()
            .find(|k| composition::scancode(k).is_none())
        {
            return Err(Failure::new("maskKeyInvalid").with("key", key));
        }
        Ok(())
    }
}

fn names_mask(mask: Option<&MaskRecord>, id: &str) -> bool {
    mask.is_some_and(|m| m.names(id))
}

impl SavedMaskRecord {
    /// Its mask on `device`: every field on the model it was drawn on, its
    /// group and keys elsewhere — `lights` and `region` mean other lights on
    /// another definition (§5.1). Refused when nothing is left to light there.
    fn mask_on(&self, device: &Layout) -> Result<Mask, Failure> {
        let mask = self.mask_for(Some((device.vid, device.pid)));
        mask.check_shape()?;
        if mask.saved.is_some() {
            return Err(Failure::new("maskSavedNotAlone"));
        }
        if mask.is_whole() {
            return Err(Failure::new("maskLightsNothing"));
        }
        Ok(mask.engine())
    }

    /// Its own fields as they mean something on `model`: all of them on its
    /// own model, or with no model given; its group and keys on another.
    fn mask_for(&self, model: Option<(u16, u16)>) -> MaskRecord {
        let mut mask = self.mask.clone();
        let here = model.is_none_or(|m| (self.vid, self.pid) == (Some(m.0), Some(m.1)));
        if !here {
            mask.lights.clear();
            mask.region = None;
        }
        mask
    }
}

/// The engine's mask a record stands for on `device`: its own fields, or those
/// of the saved mask it names.
fn engine_mask(
    record: Option<&MaskRecord>,
    saved: &[SavedMaskRecord],
    device: &Layout,
) -> Result<Mask, Failure> {
    let Some(record) = record else {
        return Ok(Mask::default());
    };
    record.check_shape()?;
    let Some(id) = &record.saved else {
        return Ok(record.engine());
    };
    saved
        .iter()
        .find(|m| &m.id == id)
        .ok_or_else(|| Failure::new("maskNotFound"))?
        .mask_on(device)
}

/// The positions a mask record lights on a target of `device` — a region, or
/// the whole device with `None` — ascending.
pub(crate) fn positions(
    record: Option<&MaskRecord>,
    saved: &[SavedMaskRecord],
    device: &Layout,
    target: Option<&Region>,
) -> Result<Vec<u16>, Failure> {
    engine_mask(record, saved, device)?
        .resolve(device, target)
        .map_err(mask_failure)
}

impl LayerSpec {
    fn look(&self) -> LookRecord {
        LookRecord {
            blend: self.blend.clone(),
            opacity: self.opacity,
            follows: self.follows.clone(),
        }
    }

    fn set_look(&mut self, look: LookRecord) {
        let look = look.stored();
        self.blend = look.blend;
        self.opacity = look.opacity;
        self.follows = look.follows;
    }

    /// As a stack keeps it: no field at its default, and of its values only
    /// those departing from what `manifest` declares (§3.11).
    fn stored(mut self, manifest: Option<&Manifest>) -> Self {
        if let Some(manifest) = manifest {
            self.values = departures(self.values, manifest);
        }
        self.saved()
    }

    /// As a saved layer keeps it: every value, as a preset does, so that a
    /// later default does not change it; no other field at its default.
    fn saved(mut self) -> Self {
        let look = self.look();
        self.set_look(look);
        self.mask = self.mask.filter(|m| !m.is_whole());
        self
    }
}

/// What departs from the values `manifest` declares: a value at its declared
/// default goes, so that a later default reaches the layer. A value for a
/// parameter the manifest does not declare stays: a stale effect's manifest
/// declares nothing yet.
fn departures(
    values: serde_json::Map<String, serde_json::Value>,
    manifest: &Manifest,
) -> serde_json::Map<String, serde_json::Value> {
    values
        .into_iter()
        .filter(|(param, value)| {
            manifest
                .params
                .get(param)
                .and_then(|spec| spec.get("default"))
                != Some(value)
        })
        .collect()
}

/// What a layer starts with on a target, once checked.
#[derive(Debug, PartialEq)]
pub(crate) struct Planned {
    pub effect: String,
    /// The declared values, with the layer's over them.
    pub params: serde_json::Map<String, serde_json::Value>,
    pub bindings: Bindings,
    pub mask: Mask,
    pub look: Look,
}

/// Checks a layer for a target of `device` (§3.11) and says what it starts
/// with: an effect the library knows, a look within its bounds, a mask
/// lighting something there, an effect whose `kinds` and `requires` the mask's
/// layout meets.
pub(crate) fn check_layer(
    spec: &LayerSpec,
    library: &[EffectEntry],
    saved: &[SavedMaskRecord],
    device: &'static Layout,
    target: Option<&'static Region>,
) -> Result<Planned, Failure> {
    let entry = library
        .iter()
        .find(|e| e.id == spec.effect)
        .ok_or_else(|| Failure::new("effectNotFound").with("name", &spec.effect))?;
    let look = spec.look().checked()?;
    let bindings = crate::runtime::checked_bindings(spec.bindings.clone())?;
    let mask = engine_mask(spec.mask.as_ref(), saved, device)?;
    let masked = composition::masked(device, target, &mask).map_err(mask_failure)?;
    if !entry.manifest.applies_to(masked.layout) {
        return Err(Failure::new("layerNotHere").with("name", &entry.manifest.name));
    }
    Ok(Planned {
        effect: entry.id.clone(),
        params: crate::storage::starting_params(&entry.manifest, Some(&spec.values)),
        bindings,
        mask,
        look,
    })
}

/// Checks a layer to save, on no device (`layers-ui.md` §5.5): an effect the
/// library knows, a look within its bounds, a mask well formed and naming a
/// saved mask that exists. Whether it lights something is the target's
/// question, asked when it is put back.
fn check_saved_layer(
    spec: &LayerSpec,
    library: &[EffectEntry],
    saved: &[SavedMaskRecord],
) -> Result<(), Failure> {
    if !library.iter().any(|e| e.id == spec.effect) {
        return Err(Failure::new("effectNotFound").with("name", &spec.effect));
    }
    spec.look().checked()?;
    crate::runtime::checked_bindings(spec.bindings.clone())?;
    if let Some(mask) = &spec.mask {
        mask.check_shape()?;
        if let Some(id) = &mask.saved {
            if !saved.iter().any(|m| &m.id == id) {
                return Err(Failure::new("maskNotFound"));
            }
        }
    }
    Ok(())
}

/// Each layer kept for a target as it starts there, bottom to top, with what
/// it starts with or why it does not: an effect gone from the library, a mask
/// lighting nothing on this device. The file keeps them all. An id given twice
/// runs once, and past [`MAX_LAYERS`] runnable layers the rest wait, so that a
/// file edited by hand never has the engine refuse the whole stack.
pub(crate) fn planned(
    settings: &Settings,
    library: &[EffectEntry],
    device: &'static Layout,
    target: Option<&'static Region>,
) -> Vec<(String, Result<Planned, Failure>)> {
    let stack = settings.stack_of(DeviceRef::of(device), target.map(|r| r.id));
    let mut seen = BTreeSet::new();
    let mut runnable = 0;
    stack
        .iter()
        .map(|layer| {
            let verdict = if layer.id.is_empty() || !seen.insert(layer.id.as_str()) {
                Err(Failure::unexpected(format!(
                    "layer “{}” has no id of its own in its stack",
                    layer.id
                )))
            } else {
                check_layer(&layer.spec, library, &settings.saved_masks, device, target).and_then(
                    |planned| {
                        runnable += 1;
                        if runnable > MAX_LAYERS {
                            Err(too_many())
                        } else {
                            Ok(planned)
                        }
                    },
                )
            };
            (layer.id.clone(), verdict)
        })
        .collect()
}

// ---------------------------------------------------------------- names and ids

enum NameProblem {
    Empty,
    TooLong,
}

/// A name as kept: trimmed, at least one character, at most
/// [`MAX_NAME_LEN`]. Any text otherwise: no file is named after it.
fn trimmed_name(typed: &str) -> Result<String, NameProblem> {
    let name = typed.trim();
    if name.is_empty() {
        return Err(NameProblem::Empty);
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(NameProblem::TooLong);
    }
    Ok(name.to_owned())
}

fn layer_name(typed: &str) -> CmdResult<String> {
    trimmed_name(typed).map_err(|problem| match problem {
        NameProblem::Empty => Failure::new("layerNameEmpty"),
        NameProblem::TooLong => Failure::new("layerNameTooLong").with("max", MAX_NAME_LEN),
    })
}

fn mask_name(typed: &str) -> CmdResult<String> {
    trimmed_name(typed).map_err(|problem| match problem {
        NameProblem::Empty => Failure::new("maskNameEmpty"),
        NameProblem::TooLong => Failure::new("maskNameTooLong").with("max", MAX_NAME_LEN),
    })
}

/// Two names differing by case alone would be one choice in a list.
fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// The first of `prefix1`, `prefix2`… that `taken` does not hold: short, as
/// commands and agents name layers and masks by it.
fn fresh_id(prefix: &str, taken: &BTreeSet<&str>) -> String {
    (1..)
        .map(|n| format!("{prefix}{n}"))
        .find(|id| !taken.contains(id.as_str()))
        .expect("an unbounded range holds a free id")
}

// ---------------------------------------------------------------- the file

impl Settings {
    /// The layers kept for a target, bottom to top: a region's with `region`,
    /// the whole device's with `None`.
    pub fn stack_of(&self, device: DeviceRef, region: Option<&str>) -> &[LayerRecord] {
        self.layers
            .iter()
            .find(|s| (s.vid, s.pid) == (device.vid, device.pid) && s.region.as_deref() == region)
            .map_or(&[], |s| s.stack.as_slice())
    }

    /// The target's layers, made empty if it had none.
    fn stack_mut(&mut self, device: DeviceRef, region: Option<&str>) -> &mut Vec<LayerRecord> {
        let found = self.layers.iter().position(|s| {
            (s.vid, s.pid) == (device.vid, device.pid) && s.region.as_deref() == region
        });
        let i = found.unwrap_or_else(|| {
            self.layers.push(StackRecord {
                vid: device.vid,
                pid: device.pid,
                region: region.map(str::to_owned),
                stack: Vec::new(),
                unknown: serde_json::Map::new(),
            });
            self.layers.len() - 1
        });
        &mut self.layers[i].stack
    }

    /// Same economy as the rest of the file: no entry without a layer.
    fn drop_empty_stacks(&mut self) {
        self.layers.retain(|s| !s.stack.is_empty());
    }

    fn layer_mut(
        &mut self,
        device: DeviceRef,
        region: Option<&str>,
        id: &str,
    ) -> CmdResult<&mut LayerRecord> {
        self.layers
            .iter_mut()
            .find(|s| (s.vid, s.pid) == (device.vid, device.pid) && s.region.as_deref() == region)
            .and_then(|s| s.stack.iter_mut().find(|l| l.id == id))
            .ok_or_else(|| Failure::new("layerNotFound"))
    }

    /// Puts a layer at `at`, counted from the bottom, or on top.
    fn insert_layer(
        &mut self,
        device: DeviceRef,
        region: Option<&str>,
        layer: LayerRecord,
        at: Option<usize>,
    ) {
        let stack = self.stack_mut(device, region);
        let at = at.map_or(stack.len(), |at| at.min(stack.len()));
        stack.insert(at, layer);
    }

    /// Takes a layer out of its stack. Returns true if it was there.
    fn remove_layer(&mut self, device: DeviceRef, region: Option<&str>, id: &str) -> bool {
        let stack = self.stack_mut(device, region);
        let before = stack.len();
        stack.retain(|l| l.id != id);
        let removed = stack.len() != before;
        self.drop_empty_stacks();
        removed
    }

    /// Puts a stack in this order, bottom to top: `ids` names each of its
    /// layers once. Returns true if the order changed.
    fn order_layers(
        &mut self,
        device: DeviceRef,
        region: Option<&str>,
        ids: &[String],
    ) -> CmdResult<bool> {
        let stack = self.stack_mut(device, region);
        let place = |layer: &LayerRecord| ids.iter().position(|id| *id == layer.id);
        let each_once = ids.len() == stack.len()
            && stack.iter().all(|l| place(l).is_some())
            && ids.iter().enumerate().all(|(n, id)| !ids[..n].contains(id));
        if !each_once {
            self.drop_empty_stacks();
            return Err(Failure::unexpected(Refusal::Order));
        }
        let before: Vec<String> = stack.iter().map(|l| l.id.clone()).collect();
        stack.sort_by_key(|l| place(l));
        let changed = before != ids;
        self.drop_empty_stacks();
        Ok(changed)
    }

    /// Forgets the stacks of a model's regions: it is made whole
    /// (`composition.md` §3.11). Returns true if one went.
    pub(crate) fn forget_region_stacks(&mut self, vid: u16, pid: u16) -> bool {
        let before = self.layers.len();
        self.layers
            .retain(|s| !((s.vid, s.pid) == (vid, pid) && s.region.is_some()));
        self.layers.len() != before
    }

    /// Forgets the layers and saved layers running a deleted effect, as its
    /// presets go.
    pub(crate) fn forget_effect_layers(&mut self, effect: &str) {
        for stack in &mut self.layers {
            stack.stack.retain(|l| l.spec.effect != effect);
        }
        self.drop_empty_stacks();
        self.saved_layers.retain(|l| l.spec.effect != effect);
    }

    /// Every effect a layer or a saved layer names, for a rename to reach.
    pub(crate) fn layer_effects_mut(&mut self) -> impl Iterator<Item = &mut String> {
        self.layers
            .iter_mut()
            .flat_map(|s| s.stack.iter_mut().map(|l| &mut l.spec.effect))
            .chain(self.saved_layers.iter_mut().map(|l| &mut l.spec.effect))
    }

    /// Every effect a layer or a saved layer names.
    pub(crate) fn layer_effects(&self) -> impl Iterator<Item = &str> {
        self.layers
            .iter()
            .flat_map(|s| s.stack.iter().map(|l| l.spec.effect.as_str()))
            .chain(self.saved_layers.iter().map(|l| l.spec.effect.as_str()))
    }

    // ------------------------------------------------------------ saved layers

    fn saved_layer_position(&self, name: &str) -> Option<usize> {
        self.saved_layers
            .iter()
            .position(|l| same_name(&l.name, name))
    }

    /// Adds a saved layer, last of the list.
    fn save_layer(&mut self, name: &str, spec: LayerSpec) -> CmdResult<()> {
        let name = layer_name(name)?;
        if self.saved_layer_position(&name).is_some() {
            return Err(Failure::new("layerNameTaken").with("name", name));
        }
        self.saved_layers.push(SavedLayerRecord {
            name,
            spec: spec.saved(),
        });
        Ok(())
    }

    /// Renames a saved layer where it stands; a change of case alone is a
    /// rename. Returns true if something changed.
    fn rename_saved_layer(&mut self, from: &str, to: &str) -> CmdResult<bool> {
        let i = self
            .saved_layer_position(from.trim())
            .ok_or_else(|| Failure::new("layerSavedNotFound").with("name", from.trim()))?;
        let to = layer_name(to)?;
        if self.saved_layer_position(&to).is_some_and(|j| j != i) {
            return Err(Failure::new("layerNameTaken").with("name", to));
        }
        if self.saved_layers[i].name == to {
            return Ok(false);
        }
        self.saved_layers[i].name = to;
        Ok(true)
    }

    /// Deletes a saved layer; no layer changes. Returns true if it was there.
    fn delete_saved_layer(&mut self, name: &str) -> bool {
        match self.saved_layer_position(name.trim()) {
            Some(i) => {
                self.saved_layers.remove(i);
                true
            }
            None => false,
        }
    }

    /// Puts a deleted saved layer back at `at`: *Undo*.
    fn restore_saved_layer(&mut self, record: SavedLayerRecord, at: usize) -> CmdResult<()> {
        let name = layer_name(&record.name)?;
        if self.saved_layer_position(&name).is_some() {
            return Err(Failure::new("layerNameTaken").with("name", name));
        }
        let at = at.min(self.saved_layers.len());
        self.saved_layers.insert(
            at,
            SavedLayerRecord {
                name,
                spec: record.spec,
            },
        );
        Ok(())
    }

    // ------------------------------------------------------------ saved masks

    fn saved_mask_mut(&mut self, id: &str) -> CmdResult<&mut SavedMaskRecord> {
        self.saved_masks
            .iter_mut()
            .find(|m| m.id == id)
            .ok_or_else(|| Failure::new("maskNotFound"))
    }

    fn mask_name_taken(&self, name: &str, but: Option<&str>) -> CmdResult<()> {
        if self
            .saved_masks
            .iter()
            .any(|m| same_name(&m.name, name) && Some(m.id.as_str()) != but)
        {
            return Err(Failure::new("maskNameTaken").with("name", name));
        }
        Ok(())
    }

    /// An id no saved mask holds and no layer names: a layer still naming a
    /// deleted mask lights nothing, and must not light a new one.
    fn fresh_mask_id(&self) -> String {
        let named = self
            .layers
            .iter()
            .flat_map(|s| s.stack.iter().map(|l| &l.spec))
            .chain(self.saved_layers.iter().map(|l| &l.spec))
            .filter_map(|spec| spec.mask.as_ref()?.saved.as_deref());
        let taken: BTreeSet<&str> = self
            .saved_masks
            .iter()
            .map(|m| m.id.as_str())
            .chain(named)
            .collect();
        fresh_id("k", &taken)
    }

    /// Adds a saved mask, last of the list, `mask` already checked
    /// ([`saved_mask_of`]).
    fn save_mask(
        &mut self,
        name: &str,
        mask: MaskRecord,
        model: Option<DeviceRef>,
    ) -> CmdResult<SavedMaskRecord> {
        let name = mask_name(name)?;
        self.mask_name_taken(&name, None)?;
        let record = SavedMaskRecord {
            id: self.fresh_mask_id(),
            name,
            vid: model.map(|m| m.vid),
            pid: model.map(|m| m.pid),
            mask,
        };
        self.saved_masks.push(record.clone());
        Ok(record)
    }

    /// How many layers and saved layers name the saved mask `id`.
    fn mask_users(&self, id: &str) -> usize {
        let layers = self
            .layers
            .iter()
            .flat_map(|s| &s.stack)
            .filter(|l| names_mask(l.spec.mask.as_ref(), id))
            .count();
        let saved = self
            .saved_layers
            .iter()
            .filter(|l| names_mask(l.spec.mask.as_ref(), id))
            .count();
        layers + saved
    }

    fn rename_mask(&mut self, id: &str, name: &str) -> CmdResult<bool> {
        let name = mask_name(name)?;
        self.mask_name_taken(&name, Some(id))?;
        let record = self.saved_mask_mut(id)?;
        if record.name == name {
            return Ok(false);
        }
        record.name = name;
        Ok(true)
    }

    /// Deletes a saved mask, writing its own fields into each layer and saved
    /// layer naming it, so that each runs on unchanged (`layers-ui.md` §5.4).
    ///
    /// A layer on another model than the one its lights were drawn on lit
    /// nothing through them: it keeps naming the id, which lights nothing
    /// either, rather than being given lights that mean others there.
    fn delete_mask(&mut self, id: &str) -> CmdResult<DeletedMask> {
        let at = self
            .saved_masks
            .iter()
            .position(|m| m.id == id)
            .ok_or_else(|| Failure::new("maskNotFound"))?;
        let record = self.saved_masks.remove(at);
        let mut layers = Vec::new();
        for stack in &mut self.layers {
            let own = record.mask_for(Some((stack.vid, stack.pid)));
            if own.is_whole() {
                continue;
            }
            for layer in &mut stack.stack {
                if names_mask(layer.spec.mask.as_ref(), id) {
                    layer.spec.mask = Some(own.clone());
                    layers.push(LinkedLayer {
                        vid: stack.vid,
                        pid: stack.pid,
                        region: stack.region.clone(),
                        layer: layer.id.clone(),
                    });
                }
            }
        }
        let mut saved_layers = Vec::new();
        for saved in &mut self.saved_layers {
            if names_mask(saved.spec.mask.as_ref(), id) {
                saved.spec.mask = Some(record.mask_for(None));
                saved_layers.push(saved.name.clone());
            }
        }
        Ok(DeletedMask {
            record,
            at,
            layers,
            saved_layers,
        })
    }

    /// Puts a deleted saved mask back where it was and links again what named
    /// it: *Undo*. Under another id if a new mask took its own meanwhile.
    fn undelete_mask(&mut self, deleted: DeletedMask) -> CmdResult<()> {
        let mut record = deleted.record;
        record.name = mask_name(&record.name)?;
        self.mask_name_taken(&record.name, None)?;
        if record.id.is_empty() || self.saved_masks.iter().any(|m| m.id == record.id) {
            record.id = self.fresh_mask_id();
        }
        let id = record.id.clone();
        let at = deleted.at.min(self.saved_masks.len());
        self.saved_masks.insert(at, record);
        for linked in &deleted.layers {
            let device = DeviceRef {
                vid: linked.vid,
                pid: linked.pid,
            };
            if let Ok(layer) = self.layer_mut(device, linked.region.as_deref(), &linked.layer) {
                layer.spec.mask = Some(MaskRecord::naming(&id));
            }
        }
        for name in &deleted.saved_layers {
            if let Some(i) = self.saved_layer_position(name) {
                self.saved_layers[i].spec.mask = Some(MaskRecord::naming(&id));
            }
        }
        Ok(())
    }
}

/// A mask as saved: well formed, lighting something, anchored to `model` when
/// it names lights or a region, which mean something on that definition only.
/// Group and keys carry to any keyboard; given a model, they must light
/// something on the device they were drawn on.
fn saved_mask_of(
    mask: MaskRecord,
    model: Option<&'static Layout>,
) -> CmdResult<(MaskRecord, Option<DeviceRef>)> {
    if mask.saved.is_some() {
        return Err(Failure::unexpected("a saved mask cannot name another"));
    }
    mask.check_shape()?;
    if mask.is_whole() {
        return Err(Failure::new("maskLightsNothing"));
    }
    let anchored = !mask.lights.is_empty() || mask.region.is_some();
    if let Some(layout) = model {
        mask.engine().resolve(layout, None).map_err(mask_failure)?;
    }
    match (anchored, model) {
        (true, None) => Err(Failure::unexpected(
            "a mask of lights or of a region needs the model it was drawn on",
        )),
        (true, Some(layout)) => Ok((mask, Some(DeviceRef::of(layout)))),
        (false, _) => Ok((mask, None)),
    }
}

// ---------------------------------------------------------------- masks on a target

/// What the mask picker offers on a target of `device`, each with what it
/// lights there; those lighting nothing are left out (`layers-ui.md` §4.3).
fn choices(settings: &Settings, device: &Layout, target: Option<&Region>) -> MaskChoices {
    let lit = |mask: Mask| mask.resolve(device, target).ok();
    let groups = candeo_device::groups::KEY_GROUPS
        .iter()
        .filter_map(|g| {
            let positions = lit(Mask {
                group: Some(g.id.to_owned()),
                ..Mask::default()
            })?;
            Some(MaskChoice {
                id: g.id.to_owned(),
                positions,
            })
        })
        .collect();
    // A region is a mask on a device left whole (§3.5): a region's own stack
    // has no other region to name.
    let regions = match target {
        Some(_) => Vec::new(),
        None => device
            .regions
            .iter()
            .filter_map(|r| {
                let positions = lit(Mask {
                    region: Some(r.id.to_owned()),
                    ..Mask::default()
                })?;
                Some(MaskChoice {
                    id: r.id.to_owned(),
                    positions,
                })
            })
            .collect(),
    };
    let saved_masks = settings
        .saved_masks
        .iter()
        .filter_map(|m| {
            let positions = m.mask_on(device).ok().and_then(lit)?;
            Some(MaskChoice {
                id: m.id.clone(),
                positions,
            })
        })
        .collect();
    MaskChoices {
        groups,
        regions,
        saved_masks,
    }
}

/// The mask lighting exactly `positions` on a target of `device`: keys by
/// scancode where they have one, as definitions write them, lights by address
/// otherwise — zones, and keys like Fn. Positions outside the target are left
/// out.
fn mask_of(device: &Layout, target: Option<&Region>, positions: &[u16]) -> MaskRecord {
    let mut wanted: Vec<u16> = positions
        .iter()
        .copied()
        .filter(|p| target.is_none_or(|r| r.positions.contains(p)))
        .collect();
    wanted.sort_unstable();
    wanted.dedup();
    let mut mask = MaskRecord::default();
    for position in wanted {
        match device.key(position) {
            Some(key) if key.scancode != candeo_device::NO_SCANCODE => {
                let code = format!("{:02x}", key.scancode);
                if !mask.keys.contains(&code) {
                    mask.keys.push(code);
                }
            }
            _ => {
                if let Some(address) = device.address(position) {
                    if !mask.lights.contains(&address) {
                        mask.lights.push(address);
                    }
                }
            }
        }
    }
    mask
}

// ---------------------------------------------------------------- the engine

/// A command's target: the device, its layout, and the region named, checked
/// as every command's is (`crate::part_of`).
struct Aim {
    device: DeviceRef,
    layout: &'static Layout,
    target: Option<&'static Region>,
}

impl Aim {
    fn of(app: &AppHandle, device: DeviceRef, region: Option<&str>) -> CmdResult<Self> {
        let scope = crate::part_of(app, device, region, false)?;
        Ok(Self {
            device,
            layout: crate::find_layout(device)?,
            target: match scope {
                Scope::Whole => None,
                Scope::Region(region) => Some(region),
            },
        })
    }

    fn region(&self) -> Option<&'static str> {
        self.target.map(|r| r.id)
    }
}

/// The ids of the layers the target's loop runs, bottom to top, while it runs
/// the target's own stack; `None` otherwise — nothing runs, a firmware effect
/// does, or a rule's effect over the whole device, which carries no layer
/// (overlays are step 3's) — and only the file changes.
fn running(app: &AppHandle, aim: &Aim) -> Option<Vec<String>> {
    if crate::automations::interrupts(app, aim.device) {
        return None;
    }
    app.state::<AppState>()
        .engine
        .layers_of(aim.device, aim.region())
}

/// Starts `planned` as layer `id` of the running target, in place of its
/// namesake or on top. One that does not load leaves the stack as it was.
fn start_live(app: &AppHandle, aim: &Aim, id: &str, planned: Planned) -> CmdResult<()> {
    let start = crate::runtime::effect_start(
        app,
        &planned.effect,
        &serde_json::Value::Object(planned.params),
        planned.bindings,
    )?;
    let effect = start.effect_id.clone();
    let layer = LayerStart {
        id: id.to_owned(),
        start,
        mask: planned.mask,
        look: planned.look,
    };
    match app
        .state::<AppState>()
        .engine
        .start_layer(aim.device, aim.region(), layer)
    {
        // Ended meanwhile: the file is what the next start reads.
        Ok(()) | Err(PartError::NotRunning) => Ok(()),
        Err(PartError::Refused(refusal)) => Err(refusal_failure(refusal)),
        Err(PartError::Failed(error)) => Err(crate::runtime::not_started(&effect, error)),
    }
}

/// Puts the running layers in the file's order. A layer lighting nothing on
/// this device has no place in the loop, and is left out of it.
fn order_live(app: &AppHandle, aim: &Aim, settings: &Settings) {
    let Some(running) = running(app, aim) else {
        return;
    };
    let ids: Vec<String> = settings
        .stack_of(aim.device, aim.region())
        .iter()
        .map(|l| l.id.clone())
        .filter(|id| running.contains(id))
        .collect();
    if ids == running || ids.len() != running.len() {
        return;
    }
    if let Err(e) = app
        .state::<AppState>()
        .engine
        .order_layers(aim.device, aim.region(), ids)
    {
        tracing::warn!(device = %aim.device, region = aim.region(), "layers not reordered: {e}");
    }
}

/// The layers kept for a target, ready to start over its base: what
/// [`planned`] lets run, its effect loaded. Each one left out is said in the
/// log and stays in the file.
pub(crate) fn starts(
    app: &AppHandle,
    settings: &Settings,
    device: &'static Layout,
    target: Option<&'static Region>,
) -> Vec<LayerStart> {
    let at = DeviceRef::of(device);
    let region = target.map(|r| r.id);
    if settings.stack_of(at, region).is_empty() {
        return Vec::new();
    }
    let library = match crate::storage::store(app).and_then(|s| s.list_effects()) {
        Ok(library) => library,
        Err(e) => {
            tracing::warn!(device = %at, region, "layers not started: {e}");
            return Vec::new();
        }
    };
    planned(settings, &library, device, target)
        .into_iter()
        .filter_map(|(id, planned)| {
            let started = planned.and_then(|p| {
                let start = crate::runtime::effect_start(
                    app,
                    &p.effect,
                    &serde_json::Value::Object(p.params),
                    p.bindings,
                )?;
                Ok(LayerStart {
                    id: id.clone(),
                    start,
                    mask: p.mask,
                    look: p.look,
                })
            });
            match started {
                Ok(layer) => Some(layer),
                // Its row says *nothing to light here*: no incident.
                Err(e) if e.code == "maskLightsNothing" => None,
                Err(e) => {
                    tracing::warn!(device = %at, region, layer = id, "layer not started: {e}");
                    None
                }
            }
        })
        .collect()
}

/// Starts again, with the mask as it now is, every running layer naming the
/// saved mask `id`, on every device; one it no longer lights anything for
/// there stops (`layers-ui.md` §5.4).
fn follow_mask(app: &AppHandle, settings: &Settings, id: &str) {
    let library = match crate::storage::store(app).and_then(|s| s.list_effects()) {
        Ok(library) => library,
        Err(e) => {
            tracing::warn!("layers not given their new mask: {e}");
            return;
        }
    };
    for stack in &settings.layers {
        let device = DeviceRef {
            vid: stack.vid,
            pid: stack.pid,
        };
        let Ok(layout) = crate::find_layout(device) else {
            continue;
        };
        let target = match &stack.region {
            None => None,
            Some(region) => match layout.region(region) {
                Some(region) => Some(region),
                None => continue,
            },
        };
        let aim = Aim {
            device,
            layout,
            target,
        };
        let Some(running_now) = running(app, &aim) else {
            continue;
        };
        for layer in stack
            .stack
            .iter()
            .filter(|l| names_mask(l.spec.mask.as_ref(), id))
        {
            let started = check_layer(&layer.spec, &library, &settings.saved_masks, layout, target)
                .and_then(|planned| start_live(app, &aim, &layer.id, planned));
            if let Err(e) = started {
                if running_now.contains(&layer.id) {
                    app.state::<AppState>()
                        .engine
                        .stop_layer(device, aim.region(), &layer.id);
                }
                tracing::info!(device = %device, region = aim.region(), layer = layer.id, "layer stopped with its mask: {e}");
            }
        }
        order_live(app, &aim, settings);
    }
}

// ---------------------------------------------------------------- commands

/// Who learns that the file changed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tell {
    /// Nobody: a setting remembered once a slider stops, which the window has
    /// just set itself, as `remember_effect_params` tells nobody.
    Nobody,
    /// The window, which reads the file again: what its other views show
    /// changed.
    Window,
    /// The window and the tray, whose lines count the layers drawing.
    Everyone,
}

fn tell(app: &AppHandle, who: Tell) {
    if who == Tell::Everyone {
        crate::tray::refresh(app);
    }
    if who != Tell::Nobody {
        crate::tray::notify_state_changed(app);
    }
}

/// Reads the settings, applies `apply`, and writes them back when it changed
/// something, then tells `who`: the read, the change and the write in one go,
/// as `remember_effect_params` does, so that a stale copy in the window never
/// overwrites the file.
fn change<T>(
    app: &AppHandle,
    who: Tell,
    apply: impl FnOnce(&mut Settings) -> CmdResult<(bool, T)>,
) -> CmdResult<T> {
    let store = crate::storage::store(app)?;
    let mut settings = store.read_settings()?;
    let (changed, answer) = apply(&mut settings)?;
    if changed {
        store.write_settings(&settings)?;
        tell(app, who);
    }
    Ok(answer)
}

/// What the mask picker offers on a target.
#[tauri::command]
pub fn mask_choices(
    app: AppHandle,
    device: DeviceRef,
    region: Option<String>,
) -> CmdResult<MaskChoices> {
    let aim = Aim::of(&app, device, region.as_deref())?;
    let settings = crate::storage::store(&app)?.read_settings()?;
    Ok(choices(&settings, aim.layout, aim.target))
}

/// The positions `mask` lights on a target, ascending; empty when it lights
/// nothing there or names what nobody knows.
#[tauri::command]
pub fn resolve_mask(
    app: AppHandle,
    device: DeviceRef,
    region: Option<String>,
    mask: MaskRecord,
) -> CmdResult<Vec<u16>> {
    let aim = Aim::of(&app, device, region.as_deref())?;
    let settings = crate::storage::store(&app)?.read_settings()?;
    Ok(positions(Some(&mask), &settings.saved_masks, aim.layout, aim.target).unwrap_or_default())
}

/// The mask lighting exactly `positions` on a target: what *Choose keys* ends
/// with.
#[tauri::command]
pub fn mask_of_positions(
    app: AppHandle,
    device: DeviceRef,
    region: Option<String>,
    positions: Vec<u16>,
) -> CmdResult<MaskRecord> {
    let aim = Aim::of(&app, device, region.as_deref())?;
    Ok(mask_of(aim.layout, aim.target, &positions))
}

/// Adds a layer on top of a target's stack, or at `at` to put back a removed
/// one, runs it where the target's stack runs, and remembers it. Answers with
/// its id: the one it came with when its stack does not hold it, a new one
/// otherwise.
///
/// Live first: a layer that does not load leaves the stack as it was, in the
/// loop and in the file. A target not running — not controlled, unplugged, a
/// firmware effect or a rule over it — gets the layer in the file only.
#[tauri::command]
pub fn add_layer(
    app: AppHandle,
    device: DeviceRef,
    region: Option<String>,
    layer: LayerRecord,
    at: Option<usize>,
) -> CmdResult<String> {
    let aim = Aim::of(&app, device, region.as_deref())?;
    let store = crate::storage::store(&app)?;
    let mut settings = store.read_settings()?;
    let library = store.list_effects()?;
    let stack = settings.stack_of(device, aim.region());
    if stack.len() >= MAX_LAYERS {
        return Err(too_many());
    }
    let planned = check_layer(
        &layer.spec,
        &library,
        &settings.saved_masks,
        aim.layout,
        aim.target,
    )?;
    let taken: BTreeSet<&str> = stack.iter().map(|l| l.id.as_str()).collect();
    let id = if layer.id.is_empty() || taken.contains(layer.id.as_str()) {
        fresh_id("l", &taken)
    } else {
        layer.id.clone()
    };
    let manifest = library
        .iter()
        .find(|e| e.id == layer.spec.effect)
        .map(|e| &e.manifest);
    let record = LayerRecord {
        id: id.clone(),
        spec: layer.spec.stored(manifest),
    };
    if running(&app, &aim).is_some() {
        start_live(&app, &aim, &id, planned)?;
    }
    settings.insert_layer(device, aim.region(), record, at);
    store.write_settings(&settings)?;
    order_live(&app, &aim, &settings);
    tell(&app, Tell::Everyone);
    Ok(id)
}

/// Replaces the layer with `layer.id` where it stands: another effect,
/// another mask, a saved layer put in its place.
#[tauri::command]
pub fn replace_layer(
    app: AppHandle,
    device: DeviceRef,
    region: Option<String>,
    layer: LayerRecord,
) -> CmdResult<()> {
    let aim = Aim::of(&app, device, region.as_deref())?;
    let store = crate::storage::store(&app)?;
    let mut settings = store.read_settings()?;
    let library = store.list_effects()?;
    settings.layer_mut(device, aim.region(), &layer.id)?;
    let planned = check_layer(
        &layer.spec,
        &library,
        &settings.saved_masks,
        aim.layout,
        aim.target,
    )?;
    let manifest = library
        .iter()
        .find(|e| e.id == layer.spec.effect)
        .map(|e| &e.manifest);
    if running(&app, &aim).is_some() {
        start_live(&app, &aim, &layer.id, planned)?;
    }
    let kept = settings.layer_mut(device, aim.region(), &layer.id)?;
    kept.spec = layer.spec.stored(manifest);
    store.write_settings(&settings)?;
    order_live(&app, &aim, &settings);
    tell(&app, Tell::Everyone);
    Ok(())
}

/// Takes a layer out of its stack, live and in the file. One already gone is
/// no error.
#[tauri::command]
pub fn remove_layer(
    app: AppHandle,
    state: State<'_, AppState>,
    device: DeviceRef,
    region: Option<String>,
    id: String,
) -> CmdResult<()> {
    let aim = Aim::of(&app, device, region.as_deref())?;
    if running(&app, &aim).is_some() {
        state.engine.stop_layer(device, aim.region(), &id);
    }
    change(&app, Tell::Everyone, |settings| {
        Ok((settings.remove_layer(device, aim.region(), &id), ()))
    })
}

/// Puts a stack in this order, bottom to top.
#[tauri::command]
pub fn order_layers(
    app: AppHandle,
    device: DeviceRef,
    region: Option<String>,
    ids: Vec<String>,
) -> CmdResult<()> {
    let aim = Aim::of(&app, device, region.as_deref())?;
    let store = crate::storage::store(&app)?;
    let mut settings = store.read_settings()?;
    if settings.order_layers(device, aim.region(), &ids)? {
        store.write_settings(&settings)?;
        order_live(&app, &aim, &settings);
        tell(&app, Tell::Window);
    }
    Ok(())
}

/// A layer's blend, opacity and follow, live only: a slider being dragged.
/// [`remember_layer_look`] writes it.
#[tauri::command]
pub fn set_layer_look(
    app: AppHandle,
    state: State<'_, AppState>,
    device: DeviceRef,
    region: Option<String>,
    id: String,
    look: LookRecord,
) -> CmdResult<()> {
    let aim = Aim::of(&app, device, region.as_deref())?;
    let look = look.checked()?;
    if running(&app, &aim).is_some() {
        state.engine.set_look(device, aim.region(), &id, look);
    }
    Ok(())
}

/// Remembers a layer's blend, opacity and follow.
#[tauri::command]
pub fn remember_layer_look(
    app: AppHandle,
    device: DeviceRef,
    region: Option<String>,
    id: String,
    look: LookRecord,
) -> CmdResult<()> {
    let aim = Aim::of(&app, device, region.as_deref())?;
    look.checked()?;
    change(&app, Tell::Nobody, |settings| {
        let layer = settings.layer_mut(device, aim.region(), &id)?;
        let before = layer.spec.clone();
        layer.spec.set_look(look);
        Ok((layer.spec != before, ()))
    })
}

/// A layer's settings, live only.
#[tauri::command]
pub fn set_layer_params(
    app: AppHandle,
    state: State<'_, AppState>,
    device: DeviceRef,
    region: Option<String>,
    id: String,
    params: serde_json::Value,
) -> CmdResult<()> {
    let aim = Aim::of(&app, device, region.as_deref())?;
    if running(&app, &aim).is_some() {
        let params = crate::runtime::serialised(&params)?;
        state
            .engine
            .set_params(device, aim.region(), Some(&id), params);
    }
    Ok(())
}

/// Remembers a layer's settings, keeping what departs from the declared
/// values.
#[tauri::command]
pub fn remember_layer_params(
    app: AppHandle,
    device: DeviceRef,
    region: Option<String>,
    id: String,
    params: serde_json::Map<String, serde_json::Value>,
) -> CmdResult<()> {
    let aim = Aim::of(&app, device, region.as_deref())?;
    let library = crate::storage::store(&app)?.list_effects()?;
    change(&app, Tell::Nobody, |settings| {
        let layer = settings.layer_mut(device, aim.region(), &id)?;
        let values = match library.iter().find(|e| e.id == layer.spec.effect) {
            Some(entry) => departures(params, &entry.manifest),
            None => params,
        };
        if layer.spec.values == values {
            return Ok((false, ()));
        }
        layer.spec.values = values;
        Ok((true, ()))
    })
}

/// A layer's bindings, live and remembered at once: a binding changes rarely.
#[tauri::command]
pub fn set_layer_bindings(
    app: AppHandle,
    state: State<'_, AppState>,
    device: DeviceRef,
    region: Option<String>,
    id: String,
    bindings: Bindings,
) -> CmdResult<()> {
    let aim = Aim::of(&app, device, region.as_deref())?;
    let bindings = crate::runtime::checked_bindings(bindings)?;
    if running(&app, &aim).is_some() {
        state
            .engine
            .set_bindings(device, aim.region(), Some(&id), bindings.clone());
    }
    change(&app, Tell::Nobody, |settings| {
        let layer = settings.layer_mut(device, aim.region(), &id)?;
        if layer.spec.bindings == bindings {
            return Ok((false, ()));
        }
        layer.spec.bindings = bindings;
        Ok((true, ()))
    })
}

/// Previews a stack, as `start_preview` does one effect: no hardware output,
/// nothing written (`composition.md` §3.9). Without a base, the layers are
/// drawn over black. A layer that cannot run on the layout borrowed — its
/// effect gone, its mask lighting nothing — is left out, as on the device;
/// one whose effect does not load says why in its row.
#[tauri::command]
pub fn start_preview_stack(
    app: AppHandle,
    state: State<'_, AppState>,
    device: Option<DeviceRef>,
    region: Option<String>,
    base: Option<PreviewBase>,
    layers: Vec<LayerRecord>,
) -> CmdResult<()> {
    let (layout, target) = match (device, region.as_deref()) {
        (None, None) => (crate::default_layout(), None),
        (None, Some(region)) => {
            return Err(Failure::unexpected(format!(
                "the preview of region “{region}” names no device"
            )))
        }
        (Some(d), region) => match crate::part_of(&app, d, region, true)? {
            Scope::Whole => (crate::find_layout(d)?, None),
            Scope::Region(region) => (crate::find_layout(d)?, Some(region)),
        },
    };
    let base = base
        .map(|b| {
            let bindings = crate::runtime::checked_bindings(b.bindings)?;
            crate::runtime::effect_start(&app, &b.effect, &b.params, bindings)
        })
        .transpose()?;
    let store = crate::storage::store(&app)?;
    let settings = store.read_settings()?;
    let library = if layers.is_empty() {
        Vec::new()
    } else {
        store.list_effects()?
    };
    let mut starts = Vec::new();
    let mut seen = BTreeSet::new();
    for layer in &layers {
        if starts.len() == MAX_LAYERS {
            break;
        }
        if !seen.insert(layer.id.as_str()) {
            continue;
        }
        let planned = check_layer(&layer.spec, &library, &settings.saved_masks, layout, target);
        let Ok(planned) = planned else { continue };
        let start = crate::runtime::effect_start(
            &app,
            &planned.effect,
            &serde_json::Value::Object(planned.params),
            planned.bindings,
        )?;
        starts.push(LayerStart {
            id: layer.id.clone(),
            start,
            mask: planned.mask,
            look: planned.look,
        });
    }
    let effect = base.as_ref().map(|b| b.effect_id.clone());
    let stack = StackStart {
        base,
        layers: starts,
    };
    let verdict = state
        .engine
        .start_preview_stack(DeviceRef::of(layout), target, stack, layout)
        .map_err(refusal_failure)?;
    match (verdict.base, effect) {
        (Err(error), Some(effect)) => Err(crate::runtime::not_started(&effect, error)),
        _ => Ok(()),
    }
}

/// A previewed layer's settings, live.
#[tauri::command]
pub fn set_preview_layer_params(
    state: State<'_, AppState>,
    id: String,
    params: serde_json::Value,
) -> CmdResult<()> {
    state
        .engine
        .set_preview_params(Some(&id), crate::runtime::serialised(&params)?);
    Ok(())
}

/// A previewed layer's bindings, live.
#[tauri::command]
pub fn set_preview_layer_bindings(
    state: State<'_, AppState>,
    id: String,
    bindings: Bindings,
) -> CmdResult<()> {
    state
        .engine
        .set_preview_bindings(Some(&id), crate::runtime::checked_bindings(bindings)?);
    Ok(())
}

/// A previewed layer's blend, opacity and follow, live.
#[tauri::command]
pub fn set_preview_look(state: State<'_, AppState>, id: String, look: LookRecord) -> CmdResult<()> {
    state.engine.set_preview_look(&id, look.checked()?);
    Ok(())
}

/// Saves a layer under `name`, last of the list. Answers with the saved
/// layers.
#[tauri::command]
pub fn save_layer(
    app: AppHandle,
    name: String,
    layer: LayerSpec,
) -> CmdResult<Vec<SavedLayerRecord>> {
    let library = crate::storage::store(&app)?.list_effects()?;
    // A layer of a stack sent whole: its id is its stack's, not the copy's.
    let mut layer = layer;
    layer.unknown.remove("id");
    change(&app, Tell::Window, |settings| {
        check_saved_layer(&layer, &library, &settings.saved_masks)?;
        settings.save_layer(&name, layer)?;
        Ok((true, settings.saved_layers.clone()))
    })
}

#[tauri::command]
pub fn rename_saved_layer(
    app: AppHandle,
    from: String,
    to: String,
) -> CmdResult<Vec<SavedLayerRecord>> {
    change(&app, Tell::Window, |settings| {
        let renamed = settings.rename_saved_layer(&from, &to)?;
        Ok((renamed, settings.saved_layers.clone()))
    })
}

/// Deletes a saved layer; no layer changes. One already gone is no error.
#[tauri::command]
pub fn delete_saved_layer(app: AppHandle, name: String) -> CmdResult<Vec<SavedLayerRecord>> {
    change(&app, Tell::Window, |settings| {
        let deleted = settings.delete_saved_layer(&name);
        Ok((deleted, settings.saved_layers.clone()))
    })
}

/// Puts a deleted saved layer back at `at`: *Undo*.
#[tauri::command]
pub fn restore_saved_layer(
    app: AppHandle,
    record: SavedLayerRecord,
    at: usize,
) -> CmdResult<Vec<SavedLayerRecord>> {
    change(&app, Tell::Window, |settings| {
        settings.restore_saved_layer(record, at)?;
        Ok((true, settings.saved_layers.clone()))
    })
}

/// The layout of the model a mask is drawn on, if one is named.
fn model_layout(model: Option<DeviceRef>) -> CmdResult<Option<&'static Layout>> {
    model.map(crate::find_layout).transpose()
}

/// Saves a mask under `name`, last of the list, and answers with its record.
#[tauri::command]
pub fn save_mask(
    app: AppHandle,
    name: String,
    mask: MaskRecord,
    model: Option<DeviceRef>,
) -> CmdResult<SavedMaskRecord> {
    let (mask, anchor) = saved_mask_of(mask, model_layout(model)?)?;
    change(&app, Tell::Window, |settings| {
        Ok((true, settings.save_mask(&name, mask, anchor)?))
    })
}

/// Changes a saved mask's lights: every layer naming it follows, on every
/// device, live. Answers with how many layers and saved layers name it.
///
/// `model` anchors lights or a region the new mask names; without it, the
/// mask's own model does.
#[tauri::command]
pub fn update_mask(
    app: AppHandle,
    id: String,
    mask: MaskRecord,
    model: Option<DeviceRef>,
) -> CmdResult<usize> {
    let store = crate::storage::store(&app)?;
    let mut settings = store.read_settings()?;
    let record = settings.saved_mask_mut(&id)?;
    let model = match model {
        Some(model) => Some(model),
        None => record
            .vid
            .zip(record.pid)
            .map(|(vid, pid)| DeviceRef { vid, pid }),
    };
    let (mask, anchor) = saved_mask_of(mask, model_layout(model)?)?;
    let record = settings.saved_mask_mut(&id)?;
    let updated = SavedMaskRecord {
        id: record.id.clone(),
        name: record.name.clone(),
        vid: anchor.map(|m| m.vid),
        pid: anchor.map(|m| m.pid),
        mask,
    };
    if *record != updated {
        *record = updated;
        store.write_settings(&settings)?;
        follow_mask(&app, &settings, &id);
        tell(&app, Tell::Everyone);
    }
    Ok(settings.mask_users(&id))
}

#[tauri::command]
pub fn rename_mask(app: AppHandle, id: String, name: String) -> CmdResult<Vec<SavedMaskRecord>> {
    change(&app, Tell::Window, |settings| {
        let renamed = settings.rename_mask(&id, &name)?;
        Ok((renamed, settings.saved_masks.clone()))
    })
}

/// Deletes a saved mask: each layer and saved layer naming it gets its lights
/// written in, and runs on unchanged.
#[tauri::command]
pub fn delete_mask(app: AppHandle, id: String) -> CmdResult<DeletedMask> {
    change(&app, Tell::Window, |settings| {
        Ok((true, settings.delete_mask(&id)?))
    })
}

/// Puts a deleted mask back, and links again what named it: *Undo*.
#[tauri::command]
pub fn undelete_mask(app: AppHandle, deleted: DeletedMask) -> CmdResult<()> {
    change(&app, Tell::Window, |settings| {
        settings.undelete_mask(deleted)?;
        Ok((true, ()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{EffectKind, EffectState};
    use serde_json::json;

    fn deathstalker() -> &'static Layout {
        candeo_device::definition::builtin_layout("razer-deathstalker-v2-pro.json").unwrap()
    }

    fn m18() -> &'static Layout {
        candeo_device::definition::builtin_layout("alienware-m18-r1.json").unwrap()
    }

    fn r5() -> &'static Layout {
        candeo_device::definition::builtin_layout("alienware-17-r5-zones.json").unwrap()
    }

    fn entry(
        id: &str,
        kinds: &[&str],
        requires: &[&str],
        params: serde_json::Value,
    ) -> EffectEntry {
        EffectEntry {
            id: id.into(),
            kind: EffectKind::Builtin,
            state: EffectState::Ready,
            error: None,
            hash: None,
            modified: false,
            swatch: Vec::new(),
            manifest: Manifest {
                name: id.split_once(':').unwrap().1.into(),
                description: json!(""),
                params: params.as_object().cloned().unwrap_or_default(),
                api_version: 1,
                reads_keys: false,
                reads_clock: false,
                reads_signals: false,
                reads_audio: false,
                reads_system: false,
                game: None,
                kinds: kinds.iter().map(|k| (*k).to_owned()).collect(),
                requires: requires.iter().map(|r| (*r).to_owned()).collect(),
            },
        }
    }

    /// A keyboard effect with two declared values, one for any light, and one
    /// needing rows and columns.
    fn library() -> Vec<EffectEntry> {
        vec![
            entry(
                "shipped:Wave",
                &["keys"],
                &[],
                json!({
                    "speed": { "kind": "number", "default": 1 },
                    "colour": { "kind": "color", "default": { "r": 255, "g": 0, "b": 0 } }
                }),
            ),
            entry("shipped:Glow", &["all"], &[], json!({})),
            entry("shipped:Rows", &["all"], &["matrix"], json!({})),
        ]
    }

    fn spec(value: serde_json::Value) -> LayerSpec {
        serde_json::from_value(value).expect("a layer")
    }

    fn mask(value: serde_json::Value) -> MaskRecord {
        serde_json::from_value(value).expect("a mask")
    }

    fn code<T: std::fmt::Debug>(result: Result<T, Failure>) -> &'static str {
        result.expect_err("refused").code
    }

    fn check(value: serde_json::Value, device: &'static Layout) -> Result<Planned, Failure> {
        check_layer(&spec(value), &library(), &[], device, None)
    }

    fn of(layout: &Layout) -> DeviceRef {
        DeviceRef::of(layout)
    }

    // ------------------------------------------------------------ records

    /// The examples of `composition.md` §3.11 and `layers-ui.md` §5.5 read,
    /// and write back as they were: no field at its default is added.
    #[test]
    fn the_records_read_and_write_as_the_design_writes_them() {
        let file = json!({
            "layers": [{
                "vid": 5426, "pid": 658,
                "stack": [
                    {
                        "id": "a3",
                        "effect": "shipped:Equalizer",
                        "values": { "background": { "r": 0, "g": 0, "b": 0 } },
                        "mask": { "group": "numberRow" },
                        "follows": { "source": "sound:volume", "floor": 0 }
                    },
                    {
                        "id": "b7",
                        "effect": "shipped:Status row",
                        "mask": { "group": "functionRow" },
                        "blend": "cover",
                        "opacity": 80
                    }
                ]
            }],
            "savedMasks": [
                { "id": "k1", "name": "Game keys", "keys": ["11", "1e", "1f", "20", "12", "13", "21", "39", "2a"] },
                { "id": "k2", "name": "Lid logo", "vid": 6268, "pid": 2615, "lights": [3, 7] }
            ],
            "savedLayers": [
                {
                    "name": "CI on Escape",
                    "effect": "shipped:Breathing",
                    "values": { "colour": { "r": 242, "g": 163, "b": 60 }, "speed": 1 },
                    "mask": { "group": "escape" },
                    "blend": "cover"
                },
                {
                    "name": "Equalizer on game keys",
                    "effect": "shipped:Equalizer",
                    "values": { "bands": 16 },
                    "mask": { "saved": "k1" }
                }
            ]
        });
        let settings: Settings = serde_json::from_value(file.clone()).expect("settings");
        assert_eq!(settings.layers[0].stack[1].spec.opacity, Some(80));
        assert_eq!(settings.saved_masks[1].vid, Some(6268));
        let written = serde_json::to_value(&settings).unwrap();
        for key in ["layers", "savedMasks", "savedLayers"] {
            assert_eq!(written[key], file[key], "{key}");
        }
        let empty = serde_json::to_value(Settings::default()).unwrap();
        for key in ["layers", "savedMasks", "savedLayers"] {
            assert!(empty.get(key).is_none(), "{key} written empty");
        }
    }

    /// #373: a field, a blend or a kind of mask a later build adds reads, is
    /// written back unchanged, and runs nothing rather than something nobody
    /// chose.
    #[test]
    fn what_this_build_does_not_know_is_kept_and_not_run() {
        let stack = json!({
            "vid": 5426, "pid": 658,
            "fromLater": 1,
            "stack": [
                { "id": "l1", "effect": "shipped:Wave", "fromLater": true },
                { "id": "l2", "effect": "shipped:Wave", "blend": "lighten" },
                { "id": "l3", "effect": "shipped:Wave", "mask": { "exceptGroup": "wasd" } }
            ]
        });
        let record: StackRecord = serde_json::from_value(stack.clone()).unwrap();
        assert_eq!(serde_json::to_value(&record).unwrap(), stack);
        let mut settings = Settings::default();
        settings.layers.push(record);
        let planned = planned(&settings, &library(), deathstalker(), None);
        assert!(planned[0].1.is_ok(), "an unknown field changes nothing");
        assert_eq!(planned[1].1.as_ref().unwrap_err().code, "layerBlendUnknown");
        assert_eq!(planned[2].1.as_ref().unwrap_err().code, "maskUnknown");
    }

    // ------------------------------------------------------------ checks

    #[test]
    fn a_blend_nobody_knows_is_refused() {
        let refused = check(
            json!({ "effect": "shipped:Wave", "blend": "overlay" }),
            deathstalker(),
        );
        let failure = refused.unwrap_err();
        assert_eq!(failure.code, "layerBlendUnknown");
        assert_eq!(failure.params["blend"], "overlay");
        for blend in ["cover", "add", "screen", "multiply"] {
            assert!(check(
                json!({ "effect": "shipped:Wave", "blend": blend }),
                deathstalker()
            )
            .is_ok());
        }
    }

    #[test]
    fn an_opacity_outside_0_to_100_is_refused() {
        for opacity in [-1, 101, 300] {
            let refused = check(
                json!({ "effect": "shipped:Wave", "opacity": opacity }),
                deathstalker(),
            );
            assert_eq!(code(refused), "layerOpacityInvalid", "{opacity}");
        }
        for opacity in [0, 55, 100] {
            let planned = check(
                json!({ "effect": "shipped:Wave", "opacity": opacity }),
                deathstalker(),
            );
            assert_eq!(i64::from(planned.unwrap().look.opacity), opacity);
        }
    }

    #[test]
    fn a_follow_or_a_binding_nobody_reads_is_refused() {
        let follows = check(
            json!({ "effect": "shipped:Wave", "follows": { "source": "nowhere", "floor": 0 } }),
            deathstalker(),
        );
        assert_eq!(code(follows), "dimmingInvalid");
        let bound = check(
            json!({ "effect": "shipped:Wave", "bindings": { "speed": "nowhere" } }),
            deathstalker(),
        );
        assert_eq!(code(bound), "bindingInvalid");
    }

    #[test]
    fn a_saved_mask_beside_another_field_is_refused() {
        let refused = check(
            json!({ "effect": "shipped:Wave", "mask": { "saved": "k1", "group": "wasd" } }),
            deathstalker(),
        );
        assert_eq!(code(refused), "maskSavedNotAlone");
    }

    #[test]
    fn a_group_region_or_key_nobody_knows_is_refused() {
        let group = check(
            json!({ "effect": "shipped:Wave", "mask": { "group": "thumbs" } }),
            deathstalker(),
        );
        assert_eq!(code(group), "maskGroupUnknown");
        let key = check(
            json!({ "effect": "shipped:Wave", "mask": { "keys": ["zz"] } }),
            deathstalker(),
        );
        assert_eq!(code(key), "maskKeyInvalid");
        let region = check(
            json!({ "effect": "shipped:Glow", "mask": { "region": "trunk" } }),
            r5(),
        );
        assert_eq!(code(region), "maskRegionUnknown");
        assert!(check(
            json!({ "effect": "shipped:Glow", "mask": { "region": "lid" } }),
            r5()
        )
        .is_ok());
    }

    /// A group on a zones device lights nothing: refused when the layer is
    /// saved, in the card's sentence (`layers-ui.md` §7).
    #[test]
    fn a_mask_lighting_nothing_is_refused() {
        let refused = check(
            json!({ "effect": "shipped:Glow", "mask": { "group": "wasd" } }),
            r5(),
        );
        assert_eq!(code(refused), "maskLightsNothing");
        let elsewhere = check_layer(
            &spec(json!({ "effect": "shipped:Glow", "mask": { "lights": [1] } })),
            &library(),
            &[],
            r5(),
            r5().region("keyboard"),
        );
        assert_eq!(
            code(elsewhere),
            "maskLightsNothing",
            "a light outside the region"
        );
    }

    #[test]
    fn a_saved_mask_nobody_saved_lights_nothing() {
        let refused = check(
            json!({ "effect": "shipped:Wave", "mask": { "saved": "k9" } }),
            deathstalker(),
        );
        assert_eq!(code(refused), "maskNotFound");
        assert!(positions(
            Some(&mask(json!({ "saved": "k9" }))),
            &[],
            deathstalker(),
            None
        )
        .is_err());
    }

    /// `kinds` and `requires` are checked against the mask's layout, as
    /// against a region's (§3.1).
    #[test]
    fn an_effect_the_mask_layout_cannot_run_is_refused() {
        let keys_only = check(json!({ "effect": "shipped:Wave" }), r5());
        assert_eq!(code(keys_only), "layerNotHere");
        let rows = check(json!({ "effect": "shipped:Rows" }), r5());
        assert_eq!(code(rows), "layerNotHere");
        assert!(check(json!({ "effect": "shipped:Glow" }), r5()).is_ok());
        assert!(check(
            json!({ "effect": "shipped:Rows", "mask": { "group": "wasd" } }),
            deathstalker()
        )
        .is_ok());
    }

    /// An effect the library does not know is refused to a new layer; one
    /// read from the file is kept, marked missing, and runs nothing.
    #[test]
    fn an_effect_the_library_does_not_know_is_kept_and_marked_missing() {
        assert_eq!(
            code(check(json!({ "effect": "user:Gone" }), deathstalker())),
            "effectNotFound"
        );
        let settings: Settings = serde_json::from_value(json!({
            "layers": [{ "vid": 5426, "pid": 658, "stack": [
                { "id": "l1", "effect": "user:Gone" },
                { "id": "l2", "effect": "shipped:Wave" }
            ] }]
        }))
        .unwrap();
        let planned = planned(&settings, &library(), deathstalker(), None);
        assert_eq!(planned[0].1.as_ref().unwrap_err().code, "effectNotFound");
        assert!(planned[1].1.is_ok(), "the rest of the stack runs");
        assert_eq!(settings.stack_of(of(deathstalker()), None).len(), 2, "kept");
    }

    /// Eight effects at most, the base included: a file edited past it runs
    /// the first seven layers rather than nothing.
    #[test]
    fn more_than_eight_effects_are_refused() {
        let stack: Vec<serde_json::Value> = (1..=9)
            .map(|n| json!({ "id": format!("l{n}"), "effect": "shipped:Wave" }))
            .collect();
        let settings: Settings = serde_json::from_value(json!({
            "layers": [{ "vid": 5426, "pid": 658, "stack": stack }]
        }))
        .unwrap();
        let planned = planned(&settings, &library(), deathstalker(), None);
        assert_eq!(
            planned.iter().filter(|(_, p)| p.is_ok()).count(),
            MAX_LAYERS
        );
        assert_eq!(planned[7].1.as_ref().unwrap_err().code, "layerTooMany");
        assert_eq!(planned[7].1.as_ref().unwrap_err().params["max"], "8");
        assert_eq!(
            refusal_failure(Refusal::TooManyEffects).code,
            "layerTooMany"
        );
    }

    #[test]
    fn a_layer_given_twice_runs_once() {
        let settings: Settings = serde_json::from_value(json!({
            "layers": [{ "vid": 5426, "pid": 658, "stack": [
                { "id": "l1", "effect": "shipped:Wave" },
                { "id": "l1", "effect": "shipped:Glow" }
            ] }]
        }))
        .unwrap();
        let planned = planned(&settings, &library(), deathstalker(), None);
        assert!(planned[0].1.is_ok());
        assert_eq!(planned[1].1.as_ref().unwrap_err().code, "unexpected");
    }

    /// A saved layer is checked on no device: whether it lights something is
    /// the target's question, asked when it is put back.
    #[test]
    fn a_saved_layer_is_checked_without_a_device() {
        let saved = [SavedMaskRecord {
            id: "k1".into(),
            name: "Game keys".into(),
            mask: mask(json!({ "keys": ["11"] })),
            ..SavedMaskRecord::default()
        }];
        let ok = spec(json!({ "effect": "shipped:Wave", "mask": { "saved": "k1" } }));
        assert!(check_saved_layer(&ok, &library(), &saved).is_ok());
        let gone = spec(json!({ "effect": "shipped:Wave", "mask": { "saved": "k2" } }));
        assert_eq!(
            code(check_saved_layer(&gone, &library(), &saved)),
            "maskNotFound"
        );
        let missing = spec(json!({ "effect": "user:Gone" }));
        assert_eq!(
            code(check_saved_layer(&missing, &library(), &saved)),
            "effectNotFound"
        );
        let beside =
            spec(json!({ "effect": "shipped:Wave", "mask": { "saved": "k1", "keys": ["01"] } }));
        assert_eq!(
            code(check_saved_layer(&beside, &library(), &saved)),
            "maskSavedNotAlone"
        );
        let blend = spec(json!({ "effect": "shipped:Wave", "blend": "dodge" }));
        assert_eq!(
            code(check_saved_layer(&blend, &library(), &saved)),
            "layerBlendUnknown"
        );
        let opacity = spec(json!({ "effect": "shipped:Wave", "opacity": 120 }));
        assert_eq!(
            code(check_saved_layer(&opacity, &library(), &saved)),
            "layerOpacityInvalid"
        );
    }

    // ------------------------------------------------------------ economy

    /// A layer keeps what departs from the declared values and no field at its
    /// default; a saved layer keeps every value, as a preset does.
    #[test]
    fn a_layer_keeps_what_departs_from_the_declared_values() {
        let sent = spec(json!({
            "effect": "shipped:Wave",
            "values": { "speed": 1, "colour": { "r": 0, "g": 0, "b": 255 } },
            "mask": {},
            "blend": "screen",
            "opacity": 100
        }));
        let manifest = &library()[0].manifest;
        let stored = serde_json::to_value(sent.clone().stored(Some(manifest))).unwrap();
        assert_eq!(
            stored,
            json!({ "effect": "shipped:Wave", "values": { "colour": { "r": 0, "g": 0, "b": 255 } } })
        );
        let saved = serde_json::to_value(sent.saved()).unwrap();
        assert_eq!(saved["values"]["speed"], 1, "every value");
        assert!(saved.get("blend").is_none() && saved.get("opacity").is_none());
    }

    #[test]
    fn a_new_layer_gets_an_id_free_in_its_stack() {
        assert_eq!(fresh_id("l", &BTreeSet::new()), "l1");
        assert_eq!(fresh_id("l", &BTreeSet::from(["l1", "l2"])), "l3");
        assert_eq!(fresh_id("l", &BTreeSet::from(["l2"])), "l1");
    }

    /// Undo puts a removed layer back where it was.
    #[test]
    fn a_layer_put_back_takes_its_place() {
        let device = of(deathstalker());
        let mut settings = Settings::default();
        for id in ["l1", "l2", "l3"] {
            let layer = LayerRecord {
                id: id.into(),
                spec: spec(json!({ "effect": "shipped:Wave" })),
            };
            settings.insert_layer(device, None, layer, None);
        }
        let removed = settings.stack_of(device, None)[1].clone();
        assert!(settings.remove_layer(device, None, "l2"));
        assert!(!settings.remove_layer(device, None, "l2"), "already gone");
        settings.insert_layer(device, None, removed, Some(1));
        let ids: Vec<&str> = settings
            .stack_of(device, None)
            .iter()
            .map(|l| l.id.as_str())
            .collect();
        assert_eq!(ids, ["l1", "l2", "l3"]);
        for id in ["l1", "l2", "l3"] {
            settings.remove_layer(device, None, id);
        }
        assert!(settings.layers.is_empty(), "no entry without a layer");
    }

    #[test]
    fn an_order_names_each_layer_once() {
        let device = of(deathstalker());
        let mut settings = Settings::default();
        for id in ["l1", "l2"] {
            let layer = LayerRecord {
                id: id.into(),
                spec: spec(json!({ "effect": "shipped:Wave" })),
            };
            settings.insert_layer(device, None, layer, None);
        }
        let ids = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(settings.order_layers(device, None, &ids(&["l1"])).is_err());
        assert!(settings
            .order_layers(device, None, &ids(&["l1", "l1"]))
            .is_err());
        assert!(settings
            .order_layers(device, None, &ids(&["l2", "l1"]))
            .unwrap());
        assert!(!settings
            .order_layers(device, None, &ids(&["l2", "l1"]))
            .unwrap());
        assert_eq!(settings.stack_of(device, None)[0].id, "l2");
    }

    // ------------------------------------------------------------ names

    #[test]
    fn a_saved_layer_s_name_is_checked() {
        let mut settings = Settings::default();
        let layer = || spec(json!({ "effect": "shipped:Wave" }));
        assert_eq!(code(settings.save_layer("  ", layer())), "layerNameEmpty");
        let long = "x".repeat(MAX_NAME_LEN + 1);
        assert_eq!(
            code(settings.save_layer(&long, layer())),
            "layerNameTooLong"
        );
        settings.save_layer(" CI on Escape ", layer()).unwrap();
        assert_eq!(settings.saved_layers[0].name, "CI on Escape", "trimmed");
        assert_eq!(
            code(settings.save_layer("ci ON escape", layer())),
            "layerNameTaken"
        );
        settings.save_layer("Other", layer()).unwrap();
        assert_eq!(
            code(settings.rename_saved_layer("Other", "CI ON ESCAPE")),
            "layerNameTaken"
        );
        assert!(
            settings
                .rename_saved_layer("ci on escape", "CI ON ESCAPE")
                .unwrap(),
            "case alone"
        );
        assert_eq!(
            code(settings.rename_saved_layer("Gone", "Any")),
            "layerSavedNotFound"
        );
        assert!(settings.delete_saved_layer("other"));
        assert!(!settings.delete_saved_layer("other"), "already gone");
        let deleted = settings.saved_layers.remove(0);
        settings.save_layer("Kept", layer()).unwrap();
        settings.restore_saved_layer(deleted.clone(), 0).unwrap();
        assert_eq!(
            settings.saved_layers[0].name, "CI ON ESCAPE",
            "back in its place"
        );
        assert_eq!(
            code(settings.restore_saved_layer(deleted, 0)),
            "layerNameTaken"
        );
    }

    #[test]
    fn a_saved_mask_s_name_is_checked() {
        let mut settings = Settings::default();
        let keys = || mask(json!({ "keys": ["01"] }));
        assert_eq!(code(settings.save_mask("", keys(), None)), "maskNameEmpty");
        let long = "x".repeat(MAX_NAME_LEN + 1);
        assert_eq!(
            code(settings.save_mask(&long, keys(), None)),
            "maskNameTooLong"
        );
        let first = settings.save_mask("Game keys", keys(), None).unwrap();
        assert_eq!(first.id, "k1");
        assert_eq!(
            code(settings.save_mask("GAME KEYS", keys(), None)),
            "maskNameTaken"
        );
        let second = settings.save_mask("Escape", keys(), None).unwrap();
        assert_eq!(
            code(settings.rename_mask(&second.id, "game keys")),
            "maskNameTaken"
        );
        assert!(
            settings.rename_mask(&first.id, "GAME KEYS").unwrap(),
            "case alone"
        );
        assert_eq!(code(settings.rename_mask("k9", "Any")), "maskNotFound");
    }

    // ------------------------------------------------------------ saved masks

    /// Lights and regions mean something on one definition only: a mask of
    /// them is saved with its model, and lights nothing on another. Keys and
    /// groups carry.
    #[test]
    fn a_saved_mask_of_lights_belongs_to_its_model() {
        assert_eq!(
            code(saved_mask_of(mask(json!({ "lights": [3] })), None)),
            "unexpected"
        );
        assert_eq!(
            code(saved_mask_of(mask(json!({})), None)),
            "maskLightsNothing"
        );
        assert_eq!(
            code(saved_mask_of(mask(json!({ "group": "wasd" })), Some(r5()))),
            "maskLightsNothing",
            "a group drawn on zones lights nothing there"
        );
        let (lights, model) = saved_mask_of(mask(json!({ "lights": [3] })), Some(r5())).unwrap();
        assert_eq!(model, Some(of(r5())));
        let (keys, model) =
            saved_mask_of(mask(json!({ "keys": ["01"] })), Some(deathstalker())).unwrap();
        assert_eq!(model, None, "keys carry to any keyboard");

        let record = |mask: MaskRecord, model: Option<DeviceRef>| SavedMaskRecord {
            id: "k1".into(),
            name: "Mask".into(),
            vid: model.map(|m| m.vid),
            pid: model.map(|m| m.pid),
            mask,
        };
        let lights = record(lights, Some(of(r5())));
        assert!(lights.mask_on(r5()).is_ok());
        assert_eq!(code(lights.mask_on(deathstalker())), "maskLightsNothing");
        let keys = record(keys, None);
        assert!(keys.mask_on(deathstalker()).is_ok());
        assert!(keys.mask_on(m18()).is_ok());
    }

    /// Settings with saved mask `k1` of keys and `k2` of the R5's lights, a
    /// layer naming each on the DeathStalker, and a saved layer naming `k1`.
    fn named_masks() -> Settings {
        serde_json::from_value(json!({
            "savedMasks": [
                { "id": "k1", "name": "Game keys", "keys": ["11", "1e"] },
                { "id": "k2", "name": "Lid logo", "vid": 0x187c, "pid": 0x0550, "lights": [3] }
            ],
            "layers": [{ "vid": 5426, "pid": 658, "stack": [
                { "id": "l1", "effect": "shipped:Wave", "mask": { "saved": "k1" } },
                { "id": "l2", "effect": "shipped:Wave", "mask": { "saved": "k2" } }
            ] }],
            "savedLayers": [{ "name": "Wave on game keys", "effect": "shipped:Wave", "mask": { "saved": "k1" } }]
        }))
        .unwrap()
    }

    #[test]
    fn a_saved_mask_is_resolved_where_a_layer_runs() {
        let settings = named_masks();
        let planned = planned(&settings, &library(), deathstalker(), None);
        let wasd = candeo_device::groups::key_group("wasd")
            .unwrap()
            .positions(deathstalker());
        let positions = planned[0]
            .1
            .as_ref()
            .unwrap()
            .mask
            .resolve(deathstalker(), None)
            .unwrap();
        assert_eq!(positions.len(), 2);
        assert!(positions.iter().all(|p| wasd.contains(p)));
        assert_eq!(
            planned[1].1.as_ref().unwrap_err().code,
            "maskLightsNothing",
            "the R5's lights mean nothing on the DeathStalker"
        );
        assert_eq!(settings.mask_users("k1"), 2);
        assert_eq!(settings.mask_users("k2"), 1);
    }

    /// Deleting a saved mask writes its keys into what named it, so that each
    /// runs on unchanged; *Undo* links them again.
    #[test]
    fn deleting_a_mask_writes_its_keys_into_what_named_it() {
        let mut settings = named_masks();
        let deleted = settings.delete_mask("k1").unwrap();
        assert_eq!(deleted.at, 0);
        assert_eq!(
            deleted.layers,
            [LinkedLayer {
                vid: 5426,
                pid: 658,
                region: None,
                layer: "l1".into()
            }]
        );
        assert_eq!(deleted.saved_layers, ["Wave on game keys"]);
        let keys = Some(mask(json!({ "keys": ["11", "1e"] })));
        let stack = settings.stack_of(
            DeviceRef {
                vid: 5426,
                pid: 658,
            },
            None,
        );
        assert_eq!(stack[0].spec.mask, keys);
        assert_eq!(settings.saved_layers[0].spec.mask, keys);
        assert_eq!(code(settings.delete_mask("k1")), "maskNotFound");

        settings.undelete_mask(deleted).unwrap();
        assert_eq!(settings.saved_masks[0].id, "k1");
        assert_eq!(settings.mask_users("k1"), 2);
    }

    /// A layer on another model than the lights of the mask it names lit
    /// nothing: it keeps naming the gone id, which lights nothing either, and
    /// no new mask takes that id.
    #[test]
    fn a_mask_of_another_model_s_lights_leaves_its_layer_dark() {
        let mut settings = named_masks();
        let deleted = settings.delete_mask("k2").unwrap();
        assert!(deleted.layers.is_empty());
        let stack = settings.stack_of(
            DeviceRef {
                vid: 5426,
                pid: 658,
            },
            None,
        );
        assert_eq!(stack[1].spec.mask, Some(MaskRecord::naming("k2")));
        let new = settings
            .save_mask("New", mask(json!({ "keys": ["01"] })), None)
            .unwrap();
        assert_ne!(new.id, "k2");
        assert_ne!(new.id, "k1");
    }

    // ------------------------------------------------------------ the file

    /// A region's stacks go when its device is made whole, with the rest of
    /// the region's records; the whole device's stay.
    #[test]
    fn a_region_s_stacks_go_when_its_device_is_made_whole() {
        let r5 = of(r5());
        let mut settings = Settings::default();
        assert!(settings.set_split(r5.vid, r5.pid, Some(&["keyboard", "lid"])));
        let layer = || LayerRecord {
            id: "l1".into(),
            spec: spec(json!({ "effect": "shipped:Glow" })),
        };
        settings.insert_layer(r5, Some("lid"), layer(), None);
        settings.insert_layer(r5, None, layer(), None);
        assert!(settings.set_split(r5.vid, r5.pid, None));
        assert!(settings.stack_of(r5, Some("lid")).is_empty());
        assert_eq!(settings.stack_of(r5, None).len(), 1);
    }

    #[test]
    fn deleting_or_renaming_an_effect_reaches_layers_and_saved_layers() {
        let mut settings = named_masks();
        assert!(settings.rename_effect("shipped:Wave", "user:Swell"));
        assert!(settings.layer_effects().all(|e| e == "user:Swell"));
        assert!(settings.forget_effect("user:Swell"));
        assert!(settings.layers.is_empty());
        assert!(settings.saved_layers.is_empty());
        assert_eq!(settings.saved_masks.len(), 2, "masks name no effect");
    }

    // ------------------------------------------------------------ masks on a target

    #[test]
    fn the_picker_offers_what_lights_something_here() {
        let mut settings = named_masks();
        settings.saved_masks.push(SavedMaskRecord {
            id: "k3".into(),
            name: "Escape".into(),
            mask: mask(json!({ "keys": ["01"] })),
            ..SavedMaskRecord::default()
        });

        let keyboard = choices(&settings, deathstalker(), None);
        let ids: Vec<&str> = keyboard.groups.iter().map(|c| c.id.as_str()).collect();
        let all: Vec<&str> = candeo_device::groups::KEY_GROUPS
            .iter()
            .map(|g| g.id)
            .collect();
        assert_eq!(
            ids, all,
            "every group, in order: the DeathStalker has a key of each"
        );
        assert_eq!(keyboard.groups[1].positions.len(), 4, "wasd");
        assert!(keyboard.regions.is_empty(), "no region declared");
        let saved: Vec<&str> = keyboard.saved_masks.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(saved, ["k1", "k3"], "the R5's lights are left out");

        let zones = choices(&settings, r5(), None);
        assert!(zones.groups.is_empty(), "no scancode on zones");
        assert_eq!(zones.regions.len(), r5().regions.len());
        assert_eq!(zones.saved_masks.len(), 1, "its own lights");
        let lid = choices(&settings, r5(), r5().region("lid"));
        assert!(
            lid.regions.is_empty(),
            "a region's stack names no other region"
        );
        assert!(lid.saved_masks.is_empty(), "light 3 is the power button's");
    }

    /// *Choose keys* ends with keys by scancode, as definitions write them,
    /// and lights by address where a key has none; the mask lights exactly
    /// what was chosen.
    #[test]
    fn chosen_positions_make_a_mask_lighting_them() {
        let keyboard = deathstalker();
        let index = |scancode: u16| {
            keyboard
                .keys
                .iter()
                .find(|k| k.scancode == scancode)
                .unwrap()
                .index
        };
        let fn_key = index(candeo_device::NO_SCANCODE);
        let chosen = vec![fn_key, index(0x01), index(0xE01D)];
        let made = mask_of(keyboard, None, &chosen);
        assert_eq!(made.keys, ["01", "e01d"]);
        assert_eq!(made.lights, [keyboard.address(fn_key).unwrap()]);
        let mut expected = chosen.clone();
        expected.sort_unstable();
        assert_eq!(
            positions(Some(&made), &[], keyboard, None).unwrap(),
            expected
        );

        let lid = r5().region("lid").unwrap();
        let inside: Vec<u16> = lid.positions.iter().copied().take(2).collect();
        let outside = r5().region("power").unwrap().positions[0];
        let mut chosen = inside.clone();
        chosen.push(outside);
        let zones = mask_of(r5(), Some(lid), &chosen);
        assert!(zones.keys.is_empty());
        assert_eq!(
            positions(Some(&zones), &[], r5(), Some(lid)).unwrap(),
            inside,
            "a position outside the region is left out"
        );
    }

    // ------------------------------------------------------------ the contract

    const LAYERS_TS: &str = include_str!("../../src/api/layers.ts");

    /// The fields an interface of `layers.ts` declares itself, `extends` left
    /// to the caller.
    fn ts_fields(name: &str) -> BTreeSet<String> {
        let begin = LAYERS_TS
            .find(&format!("export interface {name} "))
            .unwrap_or_else(|| panic!("interface {name} not found in src/api/layers.ts"));
        let body = &LAYERS_TS[begin..];
        let end = body.find("\n}").expect("closed");
        body[..end]
            .lines()
            .skip(1)
            .filter_map(|line| {
                let (field, _) = line.trim().split_once(':')?;
                let field = field.trim_end_matches('?');
                (!field.is_empty() && field.chars().all(|c| c.is_ascii_alphanumeric()))
                    .then(|| field.to_owned())
            })
            .collect()
    }

    fn keys_of(value: &impl Serialize) -> BTreeSet<String> {
        serde_json::to_value(value)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }

    /// The records cross the IPC and land in `settings.json`, and nothing ties
    /// them to their TypeScript mirrors at compile time: every field filled,
    /// each serialised record names what `layers.ts` declares.
    #[test]
    fn the_records_mirror_layers_ts() {
        let full_mask = mask(json!({
            "group": "wasd", "keys": ["01"], "lights": [1], "region": "lid", "saved": "k1"
        }));
        let mask_fields = ts_fields("Mask");
        assert_eq!(keys_of(&full_mask), mask_fields, "Mask");

        let look = ts_fields("Look");
        let layer = LayerRecord {
            id: "l1".into(),
            spec: spec(json!({
                "effect": "shipped:Wave", "values": { "speed": 2 },
                "bindings": { "speed": "sound:bass" },
                "mask": { "group": "wasd" }, "blend": "cover", "opacity": 50,
                "follows": { "source": "sound:bass", "floor": 10 }
            })),
        };
        let mut expected = ts_fields("LayerRecord");
        expected.extend(look.iter().cloned());
        assert_eq!(keys_of(&layer), expected, "LayerRecord");
        let full_look = LookRecord {
            blend: Some("add".into()),
            opacity: Some(1),
            follows: layer.spec.follows.clone(),
        };
        assert_eq!(keys_of(&full_look), look, "Look");

        let stack = StackRecord {
            vid: 1,
            pid: 2,
            region: Some("lid".into()),
            stack: vec![layer.clone()],
            unknown: serde_json::Map::new(),
        };
        assert_eq!(keys_of(&stack), ts_fields("StackRecord"), "StackRecord");

        let saved_layer = SavedLayerRecord {
            name: "Wave".into(),
            spec: layer.spec.clone(),
        };
        let mut expected = ts_fields("SavedLayerRecord");
        expected.extend(ts_fields("LayerRecord").into_iter().filter(|f| f != "id"));
        expected.extend(look.iter().cloned());
        assert_eq!(keys_of(&saved_layer), expected, "SavedLayerRecord");

        let saved_mask = SavedMaskRecord {
            id: "k1".into(),
            name: "Lid".into(),
            vid: Some(1),
            pid: Some(2),
            mask: mask(json!({ "group": "wasd", "keys": ["01"], "lights": [1], "region": "lid" })),
        };
        let mut expected = ts_fields("SavedMaskRecord");
        expected.extend(mask_fields.into_iter().filter(|f| f != "saved"));
        assert_eq!(keys_of(&saved_mask), expected, "SavedMaskRecord");

        let deleted = DeletedMask {
            record: saved_mask,
            at: 0,
            layers: vec![LinkedLayer {
                vid: 1,
                pid: 2,
                region: Some("lid".into()),
                layer: "l1".into(),
            }],
            saved_layers: vec!["Wave".into()],
        };
        assert_eq!(keys_of(&deleted), ts_fields("DeletedMask"), "DeletedMask");
        assert_eq!(
            keys_of(&MaskChoices::default()),
            ts_fields("MaskChoices"),
            "MaskChoices"
        );
        let choice = MaskChoice {
            id: "wasd".into(),
            positions: vec![1],
        };
        assert_eq!(keys_of(&choice), ts_fields("MaskChoice"), "MaskChoice");
    }

    /// A stack's rows in the engine's status: `layers` absent while there is
    /// none, each row as `LayerEngineStatus` in `layers.ts` names it.
    #[test]
    fn the_status_says_the_layers_as_layers_ts_reads_them() {
        let row = crate::runtime::LayerEngineStatus {
            layer: "l1".into(),
            running: true,
            effect_id: Some("shipped:Rain".into()),
            error: Some("boom".into()),
            reads_private: true,
        };
        assert_eq!(keys_of(&row), ts_fields("LayerEngineStatus"));
        let mut status = crate::runtime::EngineStatus::default();
        assert!(serde_json::to_value(&status)
            .unwrap()
            .get("layers")
            .is_none());
        status.layers.push(row.clone());
        assert_eq!(
            serde_json::to_value(&status).unwrap()["layers"][0]["layer"],
            "l1"
        );
        let region = crate::runtime::RegionEngineStatus {
            region: "lid".into(),
            running: true,
            effect_id: None,
            error: None,
            reads_private: false,
            layers: Vec::new(),
        };
        assert!(serde_json::to_value(&region)
            .unwrap()
            .get("layers")
            .is_none());
    }
}
