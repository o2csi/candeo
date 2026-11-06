//! Several effects over the same lights (`docs/design/composition.md`).
//!
//! What a layer is drawn through — its mask, its blend mode, its opacity — and
//! how a device's frame is composed from its parts' frames. Pure: the render
//! loop calls it, and the tests call it without a loop.

use std::fmt;

use candeo_device::{Layout, Region};
use serde::{Deserialize, Serialize};

use super::dimming::Dimming;
use super::EffectStart;

/// Effects on one target at most, its base included (§3.7). Eight of the
/// heaviest shipped effects at their 99th percentile took about 17.6 ms on the
/// m18 R1 on 2026-11-06, within the 19 ms a frame leaves after the
/// DeathStalker's HID write; a ninth could make the loop slip.
pub const MAX_EFFECTS: usize = 8;

/// How a layer's colour combines with what is under it, per channel (§3.4).
///
/// Three of the four make a colour transparent by themselves — black under
/// *add* and *screen*, white under *multiply* — which is what lets an effect
/// that returns no transparency be a layer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Blend {
    /// The layer's colour: the mask alone says where.
    Cover,
    /// The sum, clipped at 255.
    Add,
    /// The default, so that adding a layer never hides what was there.
    #[default]
    Screen,
    /// Tints or darkens what is under it.
    Multiply,
}

impl Blend {
    /// One channel of `over` on `under`, 0 to 255, before opacity. `under`
    /// stays a number until the last layer: the frame is rounded once (§3.2).
    pub fn apply(self, under: f32, over: u8) -> f32 {
        let over = f32::from(over);
        match self {
            Blend::Cover => over,
            Blend::Add => (under + over).min(255.0),
            Blend::Screen => 255.0 - (255.0 - under) * (255.0 - over) / 255.0,
            Blend::Multiply => under * over / 255.0,
        }
    }
}

/// How a layer is drawn over what is under it (§3.1): its blend, its opacity,
/// and what that opacity follows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Look {
    #[serde(default)]
    pub blend: Blend,
    /// 0 to 100.
    #[serde(default = "full")]
    pub opacity: u8,
    /// What the opacity follows, from its floor to the opacity set: the shape
    /// of a brightness follow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follows: Option<Dimming>,
}

fn full() -> u8 {
    100
}

impl Default for Look {
    /// A new layer's: *screen*, at 100 %, following nothing.
    fn default() -> Self {
        Self {
            blend: Blend::Screen,
            opacity: 100,
            follows: None,
        }
    }
}

impl Look {
    /// The base's: it has no blend or opacity of its own and covers its target
    /// (§3.2).
    pub fn base() -> Self {
        Self {
            blend: Blend::Cover,
            ..Self::default()
        }
    }

    /// Its weight this frame, 0 to 1: the opacity, times the factor its follow
    /// gives.
    pub fn alpha(&self, factor: f32) -> f32 {
        f32::from(self.opacity.min(100)) / 100.0 * factor.clamp(0.0, 1.0)
    }
}

/// The lights of its target a layer may touch (§3.3): the union of what it
/// names, the whole target when it names nothing.
///
/// Unknown fields are refused: a kind of mask a later build adds would
/// otherwise be read here as the whole target.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Mask {
    /// A key group, by its id: `wasd` (`candeo_device::groups`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Keys by scancode, as definitions write them: `"01"`, `"e01d"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
    /// Lights by address, as regions name them: zones, and keys without a
    /// scancode.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lights: Vec<u16>,
    /// A region of the device, by its id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

/// Why a mask resolves to no light, in a sentence for the log. The window says
/// it in its own words, from the variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaskError {
    UnknownGroup(String),
    UnknownRegion(String),
    NotAScancode(String),
    /// Nothing it names is on this target: another keyboard, a group on a zones
    /// device.
    NothingToLight,
}

impl fmt::Display for MaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MaskError::UnknownGroup(id) => write!(f, "no key group is called “{id}”"),
            MaskError::UnknownRegion(id) => write!(f, "the device has no region “{id}”"),
            MaskError::NotAScancode(text) => write!(f, "“{text}” is not a scancode"),
            MaskError::NothingToLight => write!(f, "the mask lights nothing on this target"),
        }
    }
}

impl Mask {
    /// True when it names nothing: the whole target.
    pub fn is_whole(&self) -> bool {
        self.group.is_none()
            && self.keys.is_empty()
            && self.lights.is_empty()
            && self.region.is_none()
    }

    /// Its positions in `device`'s frame, ascending, inside `target` — the
    /// region a stack runs on, `None` for the whole device.
    ///
    /// A key or a light the device lacks is left out: a mask carries to
    /// another keyboard narrowed to the keys it has. A name nobody knows is
    /// refused, and so is a mask left with nothing to light.
    pub fn resolve(&self, device: &Layout, target: Option<&Region>) -> Result<Vec<u16>, MaskError> {
        let within: Vec<u16> = match target {
            Some(region) => region.positions.to_vec(),
            None => device.keys.iter().map(|k| k.index).collect(),
        };
        let mut named: Vec<u16> = if self.is_whole() {
            within.clone()
        } else {
            let mut named = Vec::new();
            if let Some(id) = &self.group {
                let group = candeo_device::groups::key_group(id)
                    .ok_or_else(|| MaskError::UnknownGroup(id.clone()))?;
                named.extend(group.positions(device));
            }
            if !self.keys.is_empty() {
                let scancodes = self
                    .keys
                    .iter()
                    .map(|text| scancode(text).ok_or_else(|| MaskError::NotAScancode(text.clone())))
                    .collect::<Result<Vec<u16>, _>>()?;
                named.extend(
                    device
                        .keys
                        .iter()
                        .filter(|k| scancodes.contains(&k.scancode))
                        .map(|k| k.index),
                );
            }
            for &address in &self.lights {
                named.extend(
                    (0..device.led_count() as u16).filter(|&p| device.address(p) == Some(address)),
                );
            }
            if let Some(id) = &self.region {
                let region = device
                    .region(id)
                    .ok_or_else(|| MaskError::UnknownRegion(id.clone()))?;
                named.extend_from_slice(region.positions);
            }
            named
        };
        named.retain(|p| within.contains(p));
        named.sort_unstable();
        named.dedup();
        if named.is_empty() {
            return Err(MaskError::NothingToLight);
        }
        Ok(named)
    }
}

/// A scancode as definitions write it: hexadecimal, the `e0` or `e1` prefix in
/// the high byte. Zero is no key's.
fn scancode(text: &str) -> Option<u16> {
    // `from_str_radix` alone takes a sign.
    if !(1..=4).contains(&text.len()) || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u16::from_str_radix(text, 16)
        .ok()
        .filter(|&code| code != candeo_device::NO_SCANCODE)
}

/// Why a composition, or a change to it, is refused before anything loads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// More than [`MAX_EFFECTS`] effects on one target, its base included.
    TooManyEffects,
    /// Two layers of one stack under one id.
    LayerTwice(String),
    /// An order that does not name each layer of the stack once.
    Order,
    Mask(MaskError),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::TooManyEffects => {
                write!(
                    f,
                    "a target runs {MAX_EFFECTS} effects at most, its base included"
                )
            }
            Refusal::LayerTwice(id) => write!(f, "layer “{id}” is given twice"),
            Refusal::Order => write!(f, "the order does not name each layer once"),
            Refusal::Mask(e) => e.fmt(f),
        }
    }
}

/// Whether a target may run a base, or none, and `layers` layers.
pub fn within_limits(base: bool, layers: usize) -> Result<(), Refusal> {
    if usize::from(base) + layers > MAX_EFFECTS {
        return Err(Refusal::TooManyEffects);
    }
    Ok(())
}

/// A layer to start: its id in the stack, its effect, its mask and its look.
pub struct LayerStart {
    /// Short, unique within its stack: what commands name the layer by.
    pub id: String,
    pub start: EffectStart,
    pub mask: Mask,
    pub look: Look,
}

/// What runs on one target (§3.2): its base, the applied effect, and its
/// layers over it, bottom to top. Without a base, layers are drawn over black.
#[derive(Default)]
pub struct StackStart {
    pub base: Option<EffectStart>,
    pub layers: Vec<LayerStart>,
}

impl StackStart {
    /// A base alone, or nothing: a target as it ran before layers.
    pub fn base(base: Option<EffectStart>) -> Self {
        Self {
            base,
            layers: Vec::new(),
        }
    }

    /// Refused as a whole, before anything stops or loads: too many effects,
    /// an id given twice. A layer's mask is the layer's own verdict.
    pub fn check(&self) -> Result<(), Refusal> {
        within_limits(self.base.is_some(), self.layers.len())?;
        for (n, layer) in self.layers.iter().enumerate() {
            if self.layers[..n].iter().any(|l| l.id == layer.id) {
                return Err(Refusal::LayerTwice(layer.id.clone()));
            }
        }
        Ok(())
    }
}

/// A layer's mask resolved on its device: the layout its effect sees — its
/// mask as a device of its own (§3.1) — and the positions it may touch.
pub(super) struct Masked {
    pub layout: &'static Layout,
    pub owns: Vec<u16>,
}

/// Resolves `mask` on a target of `device`. A whole mask keeps the target's
/// own layout; any other is the device narrowed to its lights.
pub(super) fn masked(
    device: &'static Layout,
    target: Option<&'static Region>,
    mask: &Mask,
) -> Result<Masked, MaskError> {
    let owns = mask.resolve(device, target)?;
    let layout = match (mask.is_whole(), target) {
        (true, Some(region)) => region.layout,
        (true, None) => device,
        (false, _) => device.masked(&owns),
    };
    Ok(Masked { layout, owns })
}

/// One part's frame, as the composition reads it.
pub struct Drawn<'a> {
    /// The positions it may touch: its target's for a base, its mask's for a
    /// layer; `None` for every one.
    pub owns: Option<&'a [u16]>,
    /// Its last frame, `None` while it has drawn nothing.
    pub bytes: Option<&'a [u8]>,
    pub blend: Blend,
    /// 0 to 1: [`Look::alpha`].
    pub alpha: f32,
}

impl<'a> Drawn<'a> {
    /// A base: it covers what it owns. The loop reads it from [`Look::base`].
    #[cfg(test)]
    pub fn base(owns: Option<&'a [u16]>, bytes: Option<&'a [u8]>) -> Self {
        Self {
            owns,
            bytes,
            blend: Blend::Cover,
            alpha: 1.0,
        }
    }
}

/// The device's frame, `len` bytes, from its parts' frames in the order given:
/// each target's base, then its layers, bottom to top (§3.2).
///
/// Black first; then each part, at each position it owns, mixes its blend with
/// what is under it, `under + (blended − under) × alpha`; outside, it changes
/// nothing. A part that has drawn nothing yet changes nothing either. Colours
/// stay numbers until the last part and are rounded once, so the same parts'
/// frames always give the same bytes — the preview's and the device's (§3.9).
/// A base covering at full is a copy: a device without layers gets the bytes
/// its effect drew.
pub fn compose<'a>(len: usize, parts: impl IntoIterator<Item = Drawn<'a>>) -> Vec<u8> {
    let mut frame = vec![0f32; len];
    for part in parts {
        let Some(bytes) = part.bytes else { continue };
        let alpha = part.alpha.clamp(0.0, 1.0);
        if alpha <= 0.0 {
            continue;
        }
        let mut mix = |at: usize| {
            let (Some(under), Some(over)) = (frame.get_mut(at..at + 3), bytes.get(at..at + 3))
            else {
                return;
            };
            for (under, &over) in under.iter_mut().zip(over) {
                let blended = part.blend.apply(*under, over);
                *under = if alpha >= 1.0 {
                    blended
                } else {
                    *under + (blended - *under) * alpha
                };
            }
        };
        match part.owns {
            None => (0..len).step_by(3).for_each(&mut mix),
            Some(positions) => positions
                .iter()
                .for_each(|&position| mix(usize::from(position) * 3)),
        }
    }
    frame
        .into_iter()
        .map(|v| v.round().clamp(0.0, 255.0) as u8)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deathstalker() -> &'static Layout {
        candeo_device::definition::builtin_layout("razer-deathstalker-v2-pro.json").unwrap()
    }

    fn m18() -> &'static Layout {
        candeo_device::definition::builtin_layout("alienware-m18-r1.json").unwrap()
    }

    fn r5() -> &'static Layout {
        candeo_device::definition::builtin_layout("alienware-17-r5-zones.json").unwrap()
    }

    /// The positions holding these scancodes on `layout`, ascending.
    fn at(layout: &Layout, scancodes: &[u16]) -> Vec<u16> {
        let mut positions: Vec<u16> = layout
            .keys
            .iter()
            .filter(|k| scancodes.contains(&k.scancode))
            .map(|k| k.index)
            .collect();
        positions.sort_unstable();
        positions
    }

    /// Each mode on known bytes, per channel, as §3.4 writes them.
    #[test]
    fn each_blend_mode_gives_its_formula_on_known_bytes() {
        let under = 100.0;
        assert_eq!(Blend::Cover.apply(under, 200), 200.0);
        assert_eq!(Blend::Add.apply(under, 100), 200.0);
        assert_eq!(Blend::Add.apply(under, 200), 255.0, "clipped");
        // 255 − 155 × 55 / 255
        assert!((Blend::Screen.apply(under, 200) - 221.568_63).abs() < 1e-3);
        // 100 × 200 / 255
        assert!((Blend::Multiply.apply(under, 200) - 78.431_37).abs() < 1e-3);
    }

    /// What makes each mode transparent by itself: black for *add* and
    /// *screen*, white for *multiply*; *cover* never is.
    #[test]
    fn add_and_screen_leave_black_and_multiply_leaves_white() {
        for under in [0.0, 37.0, 255.0] {
            assert_eq!(Blend::Add.apply(under, 0), under);
            assert_eq!(Blend::Screen.apply(under, 0), under);
            assert_eq!(Blend::Multiply.apply(under, 255), under);
        }
        assert_eq!(Blend::Cover.apply(37.0, 0), 0.0);
        assert_eq!(Blend::default(), Blend::Screen, "a new layer's");
    }

    fn solid(len: usize, colour: [u8; 3]) -> Vec<u8> {
        colour.iter().copied().cycle().take(len).collect()
    }

    /// One frame through the four modes, at full opacity and at half, over a
    /// known base: the bytes the device is sent.
    #[test]
    fn compose_blends_each_mode_on_known_bytes() {
        let base = solid(3, [100, 50, 0]);
        let over = solid(3, [200, 100, 255]);
        let frame = |blend: Blend, alpha: f32| {
            compose(
                3,
                [
                    Drawn::base(None, Some(&base)),
                    Drawn {
                        owns: None,
                        bytes: Some(&over),
                        blend,
                        alpha,
                    },
                ],
            )
        };
        assert_eq!(frame(Blend::Cover, 1.0), [200, 100, 255]);
        assert_eq!(frame(Blend::Add, 1.0), [255, 150, 255]);
        // 255 − (155 × 55, 205 × 155, 255 × 0) / 255
        assert_eq!(frame(Blend::Screen, 1.0), [222, 130, 255]);
        // (100 × 200, 50 × 100, 0) / 255
        assert_eq!(frame(Blend::Multiply, 1.0), [78, 20, 0]);
        // Half way, rounded once: 100 + (200 − 100) / 2, 50 + 25, 127.5.
        assert_eq!(frame(Blend::Cover, 0.5), [150, 75, 128]);
        // Under screen, half way: 100 + (221.57 − 100) / 2 = 160.78.
        assert_eq!(frame(Blend::Screen, 0.5), [161, 90, 128]);
        assert_eq!(frame(Blend::Cover, 0.0), [100, 50, 0], "invisible");
    }

    fn cover<'a>(owns: Option<&'a [u16]>, bytes: &'a [u8], alpha: f32) -> Drawn<'a> {
        Drawn {
            owns,
            bytes: Some(bytes),
            blend: Blend::Cover,
            alpha,
        }
    }

    /// Colours stay numbers between layers. Two layers of 1 at 40 % over black
    /// give 0.4 then 0.64, rounded once to 1; rounding after each would give 0
    /// twice.
    #[test]
    fn the_frame_is_rounded_once_after_the_last_layer() {
        let black = solid(3, [0, 0, 0]);
        let one = solid(3, [1, 1, 1]);
        let base = || Drawn::base(None, Some(&black));
        let twice = compose(3, [base(), cover(None, &one, 0.4), cover(None, &one, 0.4)]);
        assert_eq!(twice, [1, 1, 1]);
        let once = compose(3, [base(), cover(None, &one, 0.4)]);
        assert_eq!(once, [0, 0, 0]);
    }

    /// A layer changes nothing outside its mask, and the later layer goes over
    /// the earlier one where they meet: the order is the stack's.
    #[test]
    fn a_layer_changes_its_mask_alone_and_the_order_counts() {
        let base = solid(4 * 3, [10, 10, 10]);
        let red = solid(4 * 3, [200, 0, 0]);
        let blue = solid(4 * 3, [0, 0, 200]);
        let red_on = [0u16, 1];
        let blue_on = [1u16, 2];
        let red_then_blue = compose(
            12,
            [
                Drawn::base(None, Some(&base)),
                cover(Some(&red_on), &red, 1.0),
                cover(Some(&blue_on), &blue, 1.0),
            ],
        );
        assert_eq!(red_then_blue, [200, 0, 0, 0, 0, 200, 0, 0, 200, 10, 10, 10]);
        let blue_then_red = compose(
            12,
            [
                Drawn::base(None, Some(&base)),
                cover(Some(&blue_on), &blue, 1.0),
                cover(Some(&red_on), &red, 1.0),
            ],
        );
        assert_eq!(blue_then_red[3..6], [200, 0, 0], "the top one wins");
    }

    /// What `compose` did before layers, byte for byte: black where no part
    /// owns, each owned position from its part, a part owning everything
    /// copied whole, a part that has drawn nothing black.
    #[test]
    fn bases_alone_compose_as_regions_did() {
        let keyboard = [9u8; 4 * 3];
        let lid = [7u8; 4 * 3];
        let frame = compose(
            12,
            [
                Drawn::base(Some(&[0u16, 2][..]), Some(&keyboard[..])),
                Drawn::base(Some(&[1u16][..]), Some(&lid[..])),
                Drawn::base(Some(&[3u16][..]), None),
            ],
        );
        assert_eq!(frame, [9, 9, 9, 7, 7, 7, 9, 9, 9, 0, 0, 0]);
        let any: Vec<u8> = (0..=255).cycle().take(132 * 3).collect();
        assert_eq!(compose(any.len(), [Drawn::base(None, Some(&any))]), any);
    }

    #[test]
    fn opacity_and_its_follow_make_the_weight() {
        let look = Look {
            opacity: 80,
            ..Look::default()
        };
        assert!((look.alpha(1.0) - 0.8).abs() < 1e-6);
        assert!((look.alpha(0.5) - 0.4).abs() < 1e-6);
        let too_much = Look {
            opacity: 180,
            ..Look::default()
        };
        assert_eq!(too_much.alpha(2.0), 1.0, "held to 100 % and to 1");
        assert_eq!(Look::base().blend, Blend::Cover);
    }

    // ------------------------------------------------------------ masks

    fn group(id: &str) -> Mask {
        Mask {
            group: Some(id.into()),
            ..Mask::default()
        }
    }

    fn keys(codes: &[&str]) -> Mask {
        Mask {
            keys: codes.iter().map(|c| (*c).to_owned()).collect(),
            ..Mask::default()
        }
    }

    fn lights(addresses: &[u16]) -> Mask {
        Mask {
            lights: addresses.to_vec(),
            ..Mask::default()
        }
    }

    fn region(id: &str) -> Mask {
        Mask {
            region: Some(id.into()),
            ..Mask::default()
        }
    }

    /// By key group: the group's scancodes, wherever the keyboard puts them.
    #[test]
    fn a_group_mask_takes_its_keys_by_scancode() {
        let l = deathstalker();
        assert_eq!(
            group("wasd").resolve(l, None),
            Ok(at(l, &[0x11, 0x1E, 0x1F, 0x20]))
        );
        assert_eq!(group("functionRow").resolve(l, None).unwrap().len(), 12);
        // The m18 R1 keeps two keys of the navigation block: narrowed.
        assert_eq!(group("navigation").resolve(m18(), None).unwrap().len(), 2);
    }

    /// By keys, as definitions write their scancodes; a key the keyboard
    /// lacks is left out.
    #[test]
    fn a_keys_mask_takes_its_scancodes_and_leaves_out_the_missing() {
        let l = deathstalker();
        assert_eq!(
            keys(&["01", "E01D"]).resolve(l, None),
            Ok(at(l, &[0x01, 0xE01D]))
        );
        // No right Windows key on the DeathStalker.
        assert_eq!(keys(&["01", "e05c"]).resolve(l, None), Ok(at(l, &[0x01])));
        assert_eq!(
            keys(&["e05c"]).resolve(l, None),
            Err(MaskError::NothingToLight)
        );
        for wrong in ["", "0", "zz", "1e01d", "-1"] {
            assert_eq!(
                keys(&[wrong]).resolve(l, None),
                Err(MaskError::NotAScancode(wrong.into())),
                "{wrong}"
            );
        }
    }

    /// By lights, by address as regions name them: zones, and keys without a
    /// scancode.
    #[test]
    fn a_lights_mask_takes_its_addresses() {
        let l = r5();
        let mut want: Vec<u16> = [3u16, 7]
            .iter()
            .map(|&a| {
                (0..l.led_count() as u16)
                    .find(|&p| l.address(p) == Some(a))
                    .unwrap()
            })
            .collect();
        want.sort_unstable();
        assert_eq!(lights(&[7, 3, 99]).resolve(l, None), Ok(want));
        assert_eq!(
            lights(&[99]).resolve(l, None),
            Err(MaskError::NothingToLight)
        );
        // Fn sends nothing: only its address names it.
        let fn_key = deathstalker()
            .keys
            .iter()
            .find(|k| k.scancode == candeo_device::NO_SCANCODE)
            .expect("Fn");
        let address = deathstalker().address(fn_key.index).unwrap();
        assert_eq!(
            lights(&[address]).resolve(deathstalker(), None),
            Ok(vec![fn_key.index])
        );
    }

    /// By region, on a device left whole; and on a region's own stack, a mask
    /// is held inside its region.
    #[test]
    fn a_region_mask_and_a_region_s_stack_hold_to_the_region() {
        let l = r5();
        let lid = l.region("lid").unwrap();
        assert_eq!(region("lid").resolve(l, None), Ok(lid.positions.to_vec()));
        assert_eq!(
            Mask::default().resolve(l, Some(lid)),
            Ok(lid.positions.to_vec())
        );
        // Lights of the keyboard and of the lid, on the lid's stack: the lid's.
        let logo = 1u16;
        let keyboard_zone = 4u16;
        let on_lid = lights(&[logo, keyboard_zone])
            .resolve(l, Some(lid))
            .unwrap();
        assert_eq!(on_lid.len(), 1);
        assert!(lid.positions.contains(&on_lid[0]));
        assert_eq!(
            region("keyboard").resolve(l, Some(lid)),
            Err(MaskError::NothingToLight)
        );
        assert_eq!(
            region("screen").resolve(l, None),
            Err(MaskError::UnknownRegion("screen".into()))
        );
    }

    /// A mask is the union of what it names; naming nothing is the whole
    /// target; a group on a zones device lights nothing.
    #[test]
    fn a_mask_is_a_union_and_nothing_named_is_the_whole_target() {
        let l = deathstalker();
        let both = Mask {
            group: Some("wasd".into()),
            keys: vec!["01".into(), "11".into()],
            ..Mask::default()
        };
        assert_eq!(
            both.resolve(l, None),
            Ok(at(l, &[0x01, 0x11, 0x1E, 0x1F, 0x20])),
            "W once"
        );
        let whole = Mask::default().resolve(l, None).unwrap();
        assert_eq!(whole.len(), l.lit_count());
        assert_eq!(
            group("wasd").resolve(r5(), None),
            Err(MaskError::NothingToLight)
        );
        assert_eq!(
            group("vowels").resolve(l, None),
            Err(MaskError::UnknownGroup("vowels".into()))
        );
    }

    /// The layout a layer's effect sees is its mask as a device: its lights
    /// alone, the device's grid and frame length; a whole mask keeps its
    /// target's own layout.
    #[test]
    fn a_layer_s_effect_sees_its_mask_as_its_device() {
        let l = deathstalker();
        let m = masked(l, None, &group("wasd")).unwrap();
        assert_eq!(m.layout.keys.len(), 4);
        assert_eq!(m.layout.led_count(), l.led_count());
        assert_eq!(m.owns, at(l, &[0x11, 0x1E, 0x1F, 0x20]));
        assert!(std::ptr::eq(
            masked(l, None, &Mask::default()).unwrap().layout,
            l
        ));
        let lid = r5().region("lid").unwrap();
        assert!(std::ptr::eq(
            masked(r5(), Some(lid), &Mask::default()).unwrap().layout,
            lid.layout
        ));
    }

    /// The shape `settings.json` will keep (§3.11): fields at their default
    /// left out, an unknown kind of mask refused rather than read as the whole
    /// target.
    #[test]
    fn masks_and_looks_read_and_write_as_the_file_keeps_them() {
        let mask: Mask = serde_json::from_str(r#"{ "group": "numberRow" }"#).unwrap();
        assert_eq!(mask, group("numberRow"));
        assert_eq!(
            serde_json::to_string(&keys(&["01"])).unwrap(),
            r#"{"keys":["01"]}"#
        );
        assert!(serde_json::from_str::<Mask>(r#"{ "except": "wasd" }"#).is_err());
        let look: Look = serde_json::from_str("{}").unwrap();
        assert_eq!(look, Look::default());
        let look: Look = serde_json::from_str(r#"{ "blend": "multiply", "opacity": 80 }"#).unwrap();
        assert_eq!((look.blend, look.opacity), (Blend::Multiply, 80));
        assert!(serde_json::from_str::<Look>(r#"{ "blend": "overlay" }"#).is_err());
    }

    // ------------------------------------------------------------ limits

    fn effect() -> EffectStart {
        EffectStart {
            effect_id: "e".into(),
            js: String::new(),
            params: "{}".into(),
            bindings: Default::default(),
        }
    }

    fn layer(id: &str) -> LayerStart {
        LayerStart {
            id: id.into(),
            start: effect(),
            mask: Mask::default(),
            look: Look::default(),
        }
    }

    /// Eight effects per target, the base included; a ninth is refused, and so
    /// is an id given twice.
    #[test]
    fn a_ninth_effect_and_an_id_given_twice_are_refused() {
        let stack = |base: bool, n: usize| StackStart {
            base: base.then(effect),
            layers: (0..n).map(|i| layer(&format!("l{i}"))).collect(),
        };
        assert_eq!(stack(true, 7).check(), Ok(()));
        assert_eq!(stack(false, 8).check(), Ok(()));
        assert_eq!(stack(true, 8).check(), Err(Refusal::TooManyEffects));
        assert_eq!(stack(false, 9).check(), Err(Refusal::TooManyEffects));
        let twice = StackStart {
            base: None,
            layers: vec![layer("a"), layer("b"), layer("a")],
        };
        assert_eq!(twice.check(), Err(Refusal::LayerTwice("a".into())));
        assert_eq!(within_limits(true, 8), Err(Refusal::TooManyEffects));
    }
}
