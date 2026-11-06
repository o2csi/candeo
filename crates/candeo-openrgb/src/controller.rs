//! What a controller is, as `REQUEST_CONTROLLER_DATA` describes it, for
//! protocols 4, 5 and 6.
//!
//! The layouts are those of OpenRGBSDK.md at `release_1.1`, sections *Device
//! Data*, *Mode Data*, *Zone Data*, *Segment Data*, *Matrix Map Data*, *LED
//! Data* and *LED Display Name Data*, field by field and in their order; the
//! flag and value meanings are RGBControllerAPI.md's (*Mode Flags*, *Color
//! Mode Values*, *Zone Types*, *Controller Flags*, *Matrix Map*). Each field's
//! "Protocol Version" column decides whether it is read.

use crate::identity::Private;
use crate::reader::Reader;
use crate::wire::EncodeError;
use crate::Rgb;

/// RGBControllerAPI.md, *Mode Flags*: bit 4, the mode has a brightness.
pub const MODE_HAS_BRIGHTNESS: u32 = 1 << 4;
/// Bit 5, the mode takes a colour per LED.
pub const MODE_HAS_PER_LED_COLOR: u32 = 1 << 5;
/// Bit 8, the mode can be saved by hand: `SAVEMODE`, which Candeo never sends.
pub const MODE_MANUAL_SAVE: u32 = 1 << 8;
/// Bit 9, "Mode automatically saves": selecting it writes the device's
/// memory. The one sign of it the documentation gives, so such a mode is
/// never selected (§6.7).
pub const MODE_AUTOMATIC_SAVE: u32 = 1 << 9;
/// RGBControllerAPI.md, *Color Mode Values*: 1, per-LED.
pub const COLOR_MODE_PER_LED: u32 = 1;
/// RGBControllerAPI.md, *Matrix Map*: a cell with no LED.
pub const NO_LED: u32 = 0xFFFF_FFFF;
/// RGBControllerAPI.md, *Zone Types*: 2, a matrix.
pub const ZONE_MATRIX: i32 = 2;

/// The controller data could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("controller data malformed: {what} at byte {at}")]
pub struct ParseError {
    pub what: &'static str,
    pub at: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Controller {
    /// RGBControllerAPI.md, *Device Types*: 5 a keyboard, 4 an LED strip…
    pub kind: i32,
    pub name: String,
    pub vendor: String,
    pub description: String,
    pub version: String,
    /// Never printed: see [`crate::identity`].
    pub(crate) serial: Private,
    pub(crate) location: Private,
    pub active_mode: i32,
    pub modes: Vec<Mode>,
    pub zones: Vec<Zone>,
    pub leds: Vec<Led>,
    pub colors: Vec<Rgb>,
    /// Protocol 5: regional names for the keys, or empty.
    pub led_display_names: Vec<String>,
    /// Protocol 5: *Controller Flags*.
    pub flags: u32,
    /// Protocol 6.
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mode {
    pub name: String,
    /// Below protocol 6 only, sent back in `UPDATEMODE` (*Mode Data*).
    pub value: Option<i32>,
    pub flags: u32,
    pub speed_min: u32,
    pub speed_max: u32,
    pub brightness_min: u32,
    pub brightness_max: u32,
    pub colors_min: u32,
    pub colors_max: u32,
    pub speed: u32,
    pub brightness: u32,
    pub direction: u32,
    pub color_mode: u32,
    pub colors: Vec<Rgb>,
}

impl Mode {
    /// Whether selecting this mode writes the device's memory, as far as the
    /// flags say.
    pub fn saves_itself(&self) -> bool {
        self.flags & MODE_AUTOMATIC_SAVE != 0
    }

    /// A colour per LED, from the host.
    pub fn is_direct(&self) -> bool {
        self.flags & MODE_HAS_PER_LED_COLOR != 0 && self.color_mode == COLOR_MODE_PER_LED
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    pub name: String,
    /// *Zone Types*: 0 single, 1 linear, 2 matrix…
    pub kind: i32,
    pub leds_min: u32,
    pub leds_max: u32,
    pub leds_count: u32,
    pub matrix: Option<MatrixMap>,
    pub segments: Vec<Segment>,
    /// Protocol 5.
    pub flags: u32,
    /// Protocol 6: the zone's own mode, and those it offers.
    pub active_mode: Option<i32>,
    pub modes: Vec<Mode>,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub name: String,
    pub kind: i32,
    pub start: u32,
    pub leds_count: u32,
    /// Protocol 6.
    pub matrix: Option<MatrixMap>,
    pub flags: u32,
}

/// *Matrix Map Data*: `cells[y * width + x]` is an LED index relative to the
/// zone's first LED, or [`NO_LED`] (RGBControllerAPI.md: "Map\[Y\]\[X\]").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatrixMap {
    pub height: u32,
    pub width: u32,
    pub cells: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Led {
    pub name: String,
    /// Below protocol 6 only (*LED Data*).
    pub value: Option<u32>,
}

/// A `REQUEST_CONTROLLER_DATA` answer's payload, read for `protocol`, the
/// version sent in the request (*NET_PACKET_ID_REQUEST_CONTROLLER_DATA*).
///
/// `data_size` is "Size of all data in packet": whether it counts itself is
/// not written down, so it is only checked to fit (§6.6, §9).
pub fn parse_controller(payload: &[u8], protocol: u32) -> Result<Controller, ParseError> {
    let mut r = Reader::new(payload);
    let data_size = r.u32("data_size")?;
    if usize::try_from(data_size).map_or(true, |size| size > payload.len()) {
        return Err(ParseError {
            what: "data_size",
            at: 0,
        });
    }
    device(&mut r, protocol)
}

fn device(r: &mut Reader<'_>, v: u32) -> Result<Controller, ParseError> {
    let kind = r.i32("type")?;
    let name = r.string("name")?;
    // `vendor` since protocol 1: always here, Candeo speaking 4 to 6.
    let vendor = r.string("vendor")?;
    let description = r.string("description")?;
    let version = r.string("version")?;
    let serial = Private::new(r.string("serial")?);
    let location = Private::new(r.string("location")?);
    let num_modes = r.count("num_modes", 2)?;
    let active_mode = r.i32("active_mode")?;
    let modes = (0..num_modes)
        .map(|_| mode(r, v))
        .collect::<Result<_, _>>()?;
    let num_zones = r.count("num_zones", 2)?;
    let zones = (0..num_zones)
        .map(|_| zone(r, v))
        .collect::<Result<_, _>>()?;
    let num_leds = r.count("num_leds", 2)?;
    let leds = (0..num_leds).map(|_| led(r, v)).collect::<Result<_, _>>()?;
    let num_colors = r.count("num_colors", 4)?;
    let colors = r.colors(num_colors, "colors")?;
    let (led_display_names, flags) = if v >= 5 {
        let n = r.count("num_led_display_names", 2)?;
        let names = (0..n)
            .map(|_| r.string("led_display_name"))
            .collect::<Result<_, _>>()?;
        (names, r.u32("flags")?)
    } else {
        (Vec::new(), 0)
    };
    let display_name = if v >= 6 {
        let display_name = r.string("display_name")?;
        // `configuration`: device-specific settings in a form the document
        // does not give. Read past and not kept: nothing here needs it, and
        // nobody can say it holds no path.
        r.long_string("configuration")?;
        display_name
    } else {
        String::new()
    };
    Ok(Controller {
        kind,
        name,
        vendor,
        description,
        version,
        serial,
        location,
        active_mode,
        modes,
        zones,
        leds,
        colors,
        led_display_names,
        flags,
        display_name,
    })
}

fn mode(r: &mut Reader<'_>, v: u32) -> Result<Mode, ParseError> {
    let name = r.string("mode_name")?;
    let value = if v < 6 {
        Some(r.i32("mode_value")?)
    } else {
        None
    };
    let flags = r.u32("mode_flags")?;
    let speed_min = r.u32("mode_speed_min")?;
    let speed_max = r.u32("mode_speed_max")?;
    // Brightness since protocol 3: always here.
    let brightness_min = r.u32("mode_brightness_min")?;
    let brightness_max = r.u32("mode_brightness_max")?;
    let colors_min = r.u32("mode_colors_min")?;
    let colors_max = r.u32("mode_colors_max")?;
    let speed = r.u32("mode_speed")?;
    let brightness = r.u32("mode_brightness")?;
    let direction = r.u32("mode_direction")?;
    let color_mode = r.u32("mode_color_mode")?;
    let n = r.count("mode_num_colors", 4)?;
    let colors = r.colors(n, "mode_colors")?;
    Ok(Mode {
        name,
        value,
        flags,
        speed_min,
        speed_max,
        brightness_min,
        brightness_max,
        colors_min,
        colors_max,
        speed,
        brightness,
        direction,
        color_mode,
        colors,
    })
}

fn zone(r: &mut Reader<'_>, v: u32) -> Result<Zone, ParseError> {
    let name = r.string("zone_name")?;
    let kind = r.i32("zone_type")?;
    let leds_min = r.u32("zone_leds_min")?;
    let leds_max = r.u32("zone_leds_max")?;
    let leds_count = r.u32("zone_leds_count")?;
    let matrix = matrix(r, "zone_matrix_map")?;
    // Segments since protocol 4: always here.
    let n = r.count("num_segments", 2)?;
    let segments = (0..n).map(|_| segment(r, v)).collect::<Result<_, _>>()?;
    let flags = if v >= 5 { r.u32("zone_flags")? } else { 0 };
    let (active_mode, modes, display_name) = if v >= 6 {
        let active = r.i32("zone_active_mode")?;
        let n = r.count("zone_num_modes", 2)?;
        let modes = (0..n).map(|_| mode(r, v)).collect::<Result<_, _>>()?;
        (Some(active), modes, r.string("zone_display_name")?)
    } else {
        (None, Vec::new(), String::new())
    };
    Ok(Zone {
        name,
        kind,
        leds_min,
        leds_max,
        leds_count,
        matrix,
        segments,
        flags,
        active_mode,
        modes,
        display_name,
    })
}

fn segment(r: &mut Reader<'_>, v: u32) -> Result<Segment, ParseError> {
    let name = r.string("segment_name")?;
    let kind = r.i32("segment_type")?;
    let start = r.u32("segment_start_idx")?;
    let leds_count = r.u32("segment_leds_count")?;
    let (matrix, flags) = if v >= 6 {
        let m = matrix(r, "segment_matrix_map")?;
        (m, r.u32("segment_flags")?)
    } else {
        (None, 0)
    };
    Ok(Segment {
        name,
        kind,
        start,
        leds_count,
        matrix,
        flags,
    })
}

/// A `u16` length, then that many bytes of *Matrix Map Data*: height, width,
/// and `height × width` cells, which the length must match exactly
/// ("zone_matrix_len - 8" bytes of data).
fn matrix(r: &mut Reader<'_>, what: &'static str) -> Result<Option<MatrixMap>, ParseError> {
    let len = usize::from(r.u16(what)?);
    if len == 0 {
        return Ok(None);
    }
    let at = r.position();
    let bytes = r.take(len, what)?;
    let mut m = Reader::new(bytes);
    let bad = ParseError { what, at };
    let height = m.u32(what).map_err(|_| bad.clone())?;
    let width = m.u32(what).map_err(|_| bad.clone())?;
    let cells = usize::try_from(height)
        .ok()
        .zip(usize::try_from(width).ok())
        .and_then(|(h, w)| h.checked_mul(w))
        .filter(|cells| cells.checked_mul(4).and_then(|b| b.checked_add(8)) == Some(len))
        .ok_or_else(|| bad.clone())?;
    let cells = (0..cells)
        .map(|_| m.u32(what))
        .collect::<Result<_, _>>()
        .map_err(|_| bad)?;
    Ok(Some(MatrixMap {
        height,
        width,
        cells,
    }))
}

fn led(r: &mut Reader<'_>, v: u32) -> Result<Led, ParseError> {
    let name = r.string("led_name")?;
    let value = if v < 6 {
        Some(r.u32("led_value")?)
    } else {
        None
    };
    Ok(Led { name, value })
}

/// *Mode Data*, written for `protocol`: what `UPDATEMODE` carries after its
/// index. Below protocol 6, the `mode_value` the server gave is sent back, as
/// the document asks.
pub(crate) fn encode_mode(
    mode: &Mode,
    protocol: u32,
    out: &mut Vec<u8>,
) -> Result<(), EncodeError> {
    put_string(&mode.name, out)?;
    if protocol < 6 {
        let value = mode.value.ok_or(EncodeError::Protocol(protocol))?;
        out.extend_from_slice(&value.to_le_bytes());
    }
    for field in [
        mode.flags,
        mode.speed_min,
        mode.speed_max,
        mode.brightness_min,
        mode.brightness_max,
        mode.colors_min,
        mode.colors_max,
        mode.speed,
        mode.brightness,
        mode.direction,
        mode.color_mode,
    ] {
        out.extend_from_slice(&field.to_le_bytes());
    }
    let n = u16::try_from(mode.colors.len())
        .map_err(|_| EncodeError::TooManyColors(mode.colors.len()))?;
    out.extend_from_slice(&n.to_le_bytes());
    for c in &mode.colors {
        out.extend_from_slice(&c.to_wire());
    }
    Ok(())
}

/// A string with its terminating zero, behind a `u16` length that counts it.
fn put_string(s: &str, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    let len = u16::try_from(s.len() + 1).map_err(|_| EncodeError::TooLong(s.len()))?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(s.as_bytes());
    out.push(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{self, Doc};

    #[test]
    fn a_protocol_6_keyboard_reads_field_by_field() {
        let payload = fixtures::keyboard(6);
        let c = parse_controller(&payload, 6).unwrap();
        assert_eq!(c.kind, 5);
        assert_eq!(
            (c.name.as_str(), c.vendor.as_str()),
            ("Fixture Keyboard", "Fixture")
        );
        assert_eq!(c.serial.expose(), fixtures::SERIAL);
        assert_eq!(c.location.expose(), fixtures::WINDOWS_LOCATION);
        assert_eq!(c.active_mode, 1);
        assert_eq!(c.modes.len(), 3);
        assert_eq!(c.modes[0].name, "Direct");
        assert_eq!(c.modes[0].value, None, "mode_value is gone in protocol 6");
        assert!(c.modes[0].is_direct());
        assert_eq!(c.modes[1].colors, vec![Rgb::new(0xff, 0, 0)]);
        assert!(c.modes[2].saves_itself());
        assert_eq!(c.zones.len(), 2);
        let keys = &c.zones[0];
        assert_eq!((keys.kind, keys.leds_count), (ZONE_MATRIX, 4));
        let m = keys.matrix.as_ref().unwrap();
        assert_eq!((m.height, m.width), (2, 3));
        assert_eq!(m.cells, vec![0, 1, NO_LED, 2, 3, NO_LED]);
        assert_eq!(keys.active_mode, Some(-1));
        assert_eq!(c.zones[1].segments.len(), 1);
        assert_eq!(c.zones[1].segments[0].flags, 0);
        assert_eq!(c.leds.len(), 5);
        assert_eq!(
            c.leds[4],
            Led {
                name: "Logo".into(),
                value: None
            }
        );
        assert_eq!(c.colors.len(), 5);
        assert_eq!(c.colors[4], Rgb::new(1, 2, 3));
        assert_eq!(c.flags, 1);
        assert_eq!(c.display_name, "Fixture Keyboard (display)");
    }

    #[test]
    fn a_protocol_4_controller_carries_its_values_and_no_flags() {
        let c = parse_controller(&fixtures::keyboard(4), 4).unwrap();
        assert_eq!(c.modes[0].value, Some(0x10));
        assert_eq!(c.leds[0].value, Some(0x100));
        assert_eq!(c.flags, 0);
        assert!(c.led_display_names.is_empty());
        assert_eq!(c.zones[0].flags, 0);
        assert_eq!(c.zones[0].active_mode, None);
        assert_eq!(c.display_name, "");
    }

    #[test]
    fn a_protocol_5_controller_has_flags_and_display_names_but_no_zone_modes() {
        let c = parse_controller(&fixtures::keyboard(5), 5).unwrap();
        assert_eq!(c.flags, 1);
        assert_eq!(c.led_display_names.len(), 5);
        assert_eq!(c.led_display_names[0], "Touche A");
        assert_eq!(c.zones[0].flags, 0x1000);
        assert_eq!(c.zones[0].active_mode, None);
        assert_eq!(c.modes[0].value, Some(0x10));
    }

    /// Read for the wrong version, the same bytes do not line up: the version
    /// sent in the request is the one to read with.
    #[test]
    fn the_version_decides_the_layout() {
        assert!(parse_controller(&fixtures::keyboard(4), 6).is_err());
    }

    #[test]
    fn every_truncation_is_an_error_never_a_panic() {
        for v in 4..=6 {
            let full = fixtures::keyboard(v);
            for len in 0..full.len() {
                let mut cut = full[..len].to_vec();
                if cut.len() >= 4 {
                    // Keep data_size consistent with the shorter payload, so the
                    // body itself is what runs out.
                    let size = u32::try_from(cut.len()).unwrap();
                    cut[..4].copy_from_slice(&size.to_le_bytes());
                }
                assert!(parse_controller(&cut, v).is_err(), "v{v}, {len} bytes");
            }
        }
    }

    #[test]
    fn a_data_size_larger_than_the_payload_is_refused() {
        let mut p = fixtures::keyboard(6);
        let size = u32::try_from(p.len() + 1).unwrap();
        p[..4].copy_from_slice(&size.to_le_bytes());
        assert_eq!(parse_controller(&p, 6).unwrap_err().what, "data_size");
    }

    /// *Matrix Map Data*: the cells must fill exactly the length announced; a
    /// width × height that overflows or disagrees is refused.
    #[test]
    fn a_matrix_map_that_disagrees_with_its_length_is_refused() {
        for (height, width) in [(2u32, 2u32), (u32::MAX, u32::MAX), (0x4000_0000, 4)] {
            let mut d = Doc::new();
            d.u16(8 + 4 * 6).u32(height).u32(width);
            for _ in 0..6 {
                d.u32(0);
            }
            let bytes = d.bytes();
            assert!(
                matrix(&mut Reader::new(&bytes), "m").is_err(),
                "{height} × {width}"
            );
        }
    }

    #[test]
    fn a_mode_written_for_a_version_reads_back_the_same() {
        for v in 4..=6 {
            let c = parse_controller(&fixtures::keyboard(v), v).unwrap();
            for m in &c.modes {
                let mut out = Vec::new();
                encode_mode(m, v, &mut out).unwrap();
                let back = mode(&mut Reader::new(&out), v).unwrap();
                assert_eq!(&back, m, "v{v}");
            }
        }
    }

    #[test]
    fn below_protocol_6_a_mode_without_its_value_cannot_be_written() {
        let c = parse_controller(&fixtures::keyboard(6), 6).unwrap();
        let mut out = Vec::new();
        assert_eq!(
            encode_mode(&c.modes[0], 5, &mut out),
            Err(EncodeError::Protocol(5))
        );
    }

    #[test]
    fn the_controllers_debug_output_hides_serial_and_location() {
        let c = parse_controller(&fixtures::keyboard(6), 6).unwrap();
        let shown = format!("{c:?}");
        assert!(!shown.contains(fixtures::SERIAL), "{shown}");
        assert!(!shown.contains("VID_"), "{shown}");
        assert!(shown.contains("<private>"));
    }
}
