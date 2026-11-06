//! A controller as Candeo sees it: zones, lights with their positions, and
//! whether it can be driven (`openrgb-output.md` §6.7, #368 step 2).
//!
//! Capabilities are those OpenRGB reports, not inferred: a zone without a
//! matrix map stays a zone, a matrix map gives each LED its place, LED names
//! are OpenRGB's, and a controller with no direct mode is described and not
//! driven.

use crate::controller::{Controller, Mode, MODE_HAS_BRIGHTNESS, NO_LED};
use crate::identity::Fingerprint;
use crate::Rgb;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub fingerprint: Fingerprint,
    /// The display name when OpenRGB gives one (protocol 6), its name otherwise.
    pub name: String,
    pub vendor: String,
    /// RGBControllerAPI.md, *Device Types*.
    pub kind: i32,
    pub zones: Vec<ZoneLights>,
    /// One per LED, in OpenRGB's order: what `UPDATELEDS` carries.
    pub lights: Vec<Light>,
    /// The direct mode's index, or why there is none.
    pub drive: Result<u16, NotDrivable>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneLights {
    pub name: String,
    /// RGBControllerAPI.md, *Zone Types*.
    pub kind: i32,
    /// Its first LED: zones follow one another in the LED list, the matrix
    /// map's indices being "relative to the zone's start index".
    pub first: usize,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Light {
    /// OpenRGB's name, the English one (RGBControllerAPI.md, *LED Display
    /// Names*).
    pub name: String,
    /// The regional name, when the controller gives one.
    pub display_name: Option<String>,
    pub zone: usize,
    /// Column and row in its zone's matrix map.
    pub position: Option<(u32, u32)>,
}

/// Why a controller is listed and not driven (§6.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotDrivable {
    NoLeds,
    /// Its zones or colours do not add up to its LEDs.
    Inconsistent,
    /// No mode takes a colour per LED.
    NoDirectMode,
    /// Its only per-LED modes save themselves to the device.
    DirectModeSavesItself,
}

pub fn describe(c: &Controller) -> Device {
    let shape: Vec<(&str, u32)> = c
        .zones
        .iter()
        .map(|z| (z.name.as_str(), z.leds_count))
        .collect();
    let fingerprint = Fingerprint::of(&c.serial, &c.vendor, &c.name, &shape);
    let mut zones = Vec::with_capacity(c.zones.len());
    let mut lights: Vec<Light> = c
        .leds
        .iter()
        .enumerate()
        .map(|(i, led)| Light {
            name: led.name.clone(),
            display_name: c
                .led_display_names
                .get(i)
                .filter(|n| !n.is_empty())
                .cloned(),
            zone: 0,
            position: None,
        })
        .collect();
    let mut first = 0usize;
    let mut consistent = true;
    for (z, zone) in c.zones.iter().enumerate() {
        let count = usize::try_from(zone.leds_count).unwrap_or(usize::MAX);
        let end = first.checked_add(count).filter(|end| *end <= lights.len());
        let Some(end) = end else {
            consistent = false;
            break;
        };
        for light in &mut lights[first..end] {
            light.zone = z;
        }
        if let Some(m) = &zone.matrix {
            let width = usize::try_from(m.width).unwrap_or(0);
            for (cell, &led) in m.cells.iter().enumerate() {
                if led == NO_LED || width == 0 {
                    continue;
                }
                let Ok(led) = usize::try_from(led) else {
                    continue;
                };
                if led >= count {
                    continue;
                }
                let x = u32::try_from(cell % width).unwrap_or(u32::MAX);
                let y = u32::try_from(cell / width).unwrap_or(u32::MAX);
                lights[first + led].position.get_or_insert((x, y));
            }
        }
        zones.push(ZoneLights {
            name: zone.name.clone(),
            kind: zone.kind,
            first,
            count,
        });
        first = end;
    }
    let consistent = consistent && first == lights.len() && c.colors.len() == lights.len();
    let drive = if lights.is_empty() {
        Err(NotDrivable::NoLeds)
    } else if !consistent {
        Err(NotDrivable::Inconsistent)
    } else {
        direct_mode(c)
    };
    Device {
        fingerprint,
        name: if c.display_name.is_empty() {
            c.name.clone()
        } else {
            c.display_name.clone()
        },
        vendor: c.vendor.clone(),
        kind: c.kind,
        zones,
        lights,
        drive,
    }
}

/// The mode Candeo selects on *Control*: per-LED colours and colour mode,
/// and not one that saves itself, `Direct` by name first (§6.7).
pub fn direct_mode(c: &Controller) -> Result<u16, NotDrivable> {
    let direct: Vec<(u16, &Mode)> = c
        .modes
        .iter()
        .enumerate()
        .filter(|(_, m)| m.is_direct())
        .filter_map(|(i, m)| u16::try_from(i).ok().map(|i| (i, m)))
        .collect();
    if direct.is_empty() {
        return Err(NotDrivable::NoDirectMode);
    }
    let safe: Vec<&(u16, &Mode)> = direct.iter().filter(|(_, m)| !m.saves_itself()).collect();
    safe.iter()
        .find(|(_, m)| m.name.eq_ignore_ascii_case("Direct"))
        .or_else(|| safe.first())
        .map(|(i, _)| *i)
        .ok_or(NotDrivable::DirectModeSavesItself)
}

/// The mode as `UPDATEMODE` sends it on *Control*: at its maximum
/// brightness when it has one.
pub fn take_over(mode: &Mode) -> Mode {
    let mut m = mode.clone();
    if m.flags & MODE_HAS_BRIGHTNESS != 0 {
        m.brightness = m.brightness_max;
    }
    m
}

/// The mode to put back on *Release*: the one active on *Control*, unless it
/// saves itself, in which case the lights keep the last frame (§6.7).
pub fn restore(c: &Controller) -> Option<(u16, Mode)> {
    let index = usize::try_from(c.active_mode).ok()?;
    let mode = c.modes.get(index)?;
    if mode.saves_itself() {
        return None;
    }
    Some((u16::try_from(index).ok()?, mode.clone()))
}

/// The colours of one frame, in OpenRGB's LED order.
pub fn frame(device: &Device, color: impl Fn(usize, &Light) -> Rgb) -> Vec<Rgb> {
    device
        .lights
        .iter()
        .enumerate()
        .map(|(i, l)| color(i, l))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::{parse_controller, MODE_AUTOMATIC_SAVE};
    use crate::fixtures;
    use crate::identity::FingerprintKind;

    fn keyboard() -> Controller {
        parse_controller(&fixtures::keyboard(6), 6).unwrap()
    }

    #[test]
    fn a_matrix_zone_gives_its_keys_positions_and_a_single_zone_none() {
        let d = describe(&keyboard());
        assert_eq!(d.zones.len(), 2);
        assert_eq!((d.zones[0].first, d.zones[0].count), (0, 4));
        assert_eq!((d.zones[1].first, d.zones[1].count), (4, 1));
        let positions: Vec<_> = d.lights.iter().map(|l| l.position).collect();
        assert_eq!(
            positions,
            vec![Some((0, 0)), Some((1, 0)), Some((0, 1)), Some((1, 1)), None]
        );
        assert_eq!(d.lights[4].zone, 1);
        assert_eq!(d.lights[0].name, "Key: A");
        assert_eq!(d.name, "Fixture Keyboard (display)");
        assert_eq!(d.fingerprint.kind, FingerprintKind::Serial);
    }

    #[test]
    fn regional_names_are_kept_beside_openrgbs() {
        let d = describe(&parse_controller(&fixtures::keyboard(5), 5).unwrap());
        assert_eq!(d.lights[0].display_name.as_deref(), Some("Touche A"));
        assert_eq!(
            d.lights[4].display_name, None,
            "an empty name overrides nothing"
        );
    }

    #[test]
    fn the_direct_mode_is_chosen_by_its_flags_then_its_name() {
        assert_eq!(describe(&keyboard()).drive, Ok(0));
        let mut c = keyboard();
        c.modes[0].name = "Software".into();
        assert_eq!(
            direct_mode(&c),
            Ok(0),
            "per-LED and not saving itself, whatever its name"
        );
    }

    /// `Wave` takes per-LED colours but saves itself: with `Direct` gone it is
    /// still refused.
    #[test]
    fn a_direct_mode_that_saves_itself_is_refused() {
        let mut c = keyboard();
        c.modes.remove(0);
        assert_eq!(direct_mode(&c), Err(NotDrivable::DirectModeSavesItself));
        c.modes.retain(|m| m.flags & MODE_AUTOMATIC_SAVE == 0);
        assert_eq!(direct_mode(&c), Err(NotDrivable::NoDirectMode));
    }

    #[test]
    fn zones_that_do_not_add_up_are_not_driven() {
        let mut c = keyboard();
        c.zones[1].leds_count = 2;
        assert_eq!(describe(&c).drive, Err(NotDrivable::Inconsistent));
        let mut c = keyboard();
        c.colors.pop();
        assert_eq!(describe(&c).drive, Err(NotDrivable::Inconsistent));
        let mut c = keyboard();
        c.leds.clear();
        c.colors.clear();
        c.zones.clear();
        assert_eq!(describe(&c).drive, Err(NotDrivable::NoLeds));
    }

    #[test]
    fn take_over_is_at_full_brightness_and_release_puts_back_what_was_there() {
        let c = keyboard();
        let m = take_over(&c.modes[0]);
        assert_eq!(m.brightness, c.modes[0].brightness_max);
        assert_eq!(
            restore(&c).map(|(i, m)| (i, m.name)),
            Some((1, "Static".into()))
        );
    }

    /// The mode found on *Control* saves itself: nothing is put back.
    #[test]
    fn a_previous_mode_that_saves_itself_is_not_put_back() {
        let mut c = keyboard();
        c.active_mode = 2;
        assert_eq!(restore(&c), None);
        c.active_mode = -1;
        assert_eq!(restore(&c), None);
        c.active_mode = 99;
        assert_eq!(restore(&c), None);
    }

    #[test]
    fn a_frame_is_one_colour_per_led_in_openrgbs_order() {
        let d = describe(&keyboard());
        let colors = frame(&d, |i, _| Rgb::new(u8::try_from(i).unwrap(), 0, 0));
        assert_eq!(colors.len(), 5);
        assert_eq!(colors[4], Rgb::new(4, 0, 0));
    }
}
