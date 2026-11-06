//! Controller data built by hand from OpenRGBSDK.md at `release_1.1`, one
//! table row at a time, to test the parser against the document rather than
//! against the encoder. Each block names the section it follows; the values
//! are made up (`AGENTS.md`, *Privacy*).
//!
//! Little endian throughout, an assumption the document does not settle
//! (`openrgb-output.md` §6.6, §9).

/// Made up, in the form `AGENTS.md` gives for fixtures.
pub(crate) const SERIAL: &str = "XY24ABCDEFG0001";
/// A Windows HID interface path for a made-up 1234:5678, interface 2. The
/// GUID is the public HID interface class; the instance part is made up.
pub(crate) const WINDOWS_LOCATION: &str =
    "HID: \\\\?\\HID#VID_1234&PID_5678&MI_02#0&fixture&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}";

/// A little-endian byte writer with the document's types.
pub(crate) struct Doc(Vec<u8>);

impl Doc {
    pub(crate) fn new() -> Self {
        Self(Vec::new())
    }

    pub(crate) fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub(crate) fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub(crate) fn i32(&mut self, v: i32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }

    /// `*_len` then `char[*_len]`, the length "including null termination"
    /// (*Device Data*).
    pub(crate) fn str(&mut self, s: &str) -> &mut Self {
        self.u16(u16::try_from(s.len() + 1).unwrap());
        self.0.extend_from_slice(s.as_bytes());
        self.0.push(0);
        self
    }

    /// An `RGBColor`: `0x00BBGGRR` (RGBControllerAPI.md), little endian.
    pub(crate) fn rgb(&mut self, r: u8, g: u8, b: u8) -> &mut Self {
        self.u32(u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16)
    }

    pub(crate) fn raw(&mut self, bytes: &[u8]) -> &mut Self {
        self.0.extend_from_slice(bytes);
        self
    }

    pub(crate) fn bytes(&self) -> Vec<u8> {
        self.0.clone()
    }
}

/// *Mode Data*.
#[allow(clippy::too_many_arguments)]
fn mode(
    d: &mut Doc,
    v: u32,
    name: &str,
    value: i32,
    flags: u32,
    color_mode: u32,
    colors: &[(u8, u8, u8)],
) {
    d.str(name); // mode_name_len, mode_name
    if v < 6 {
        d.i32(value); // mode_value, "<6"
    }
    d.u32(flags); // mode_flags
    d.u32(0).u32(0); // mode_speed_min, mode_speed_max
    d.u32(0).u32(100); // mode_brightness_min, mode_brightness_max (3)
    d.u32(0).u32(u32::try_from(colors.len()).unwrap()); // mode_colors_min, mode_colors_max
    d.u32(0); // mode_speed
    d.u32(50); // mode_brightness (3)
    d.u32(0); // mode_direction
    d.u32(color_mode); // mode_color_mode
    d.u16(u16::try_from(colors.len()).unwrap()); // mode_num_colors
    for &(r, g, b) in colors {
        d.rgb(r, g, b); // mode_colors
    }
}

/// A keyboard of four keys on a 2 × 3 matrix and a logo, written for protocol
/// `v` (4, 5 or 6), as a `REQUEST_CONTROLLER_DATA` answer's payload.
///
/// Modes, with RGBControllerAPI.md's *Mode Flags* and *Color Mode Values*:
/// `Direct` (per-LED colour, bit 5, and brightness, bit 4; colour mode 1),
/// `Static` (mode-specific colour, bit 6; saved by hand, bit 8; colour mode 2),
/// `Wave` (per-LED, and "automatically saves", bit 9).
pub(crate) fn keyboard(v: u32) -> Vec<u8> {
    let mut d = Doc::new();
    // ---- Device Data
    d.i32(5); // type: 5, a keyboard (RGBControllerAPI.md, Device Types)
    d.str("Fixture Keyboard"); // name
    d.str("Fixture"); // vendor (1)
    d.str("A keyboard made up for tests"); // description
    d.str("1.0"); // version
    d.str(SERIAL); // serial
    d.str(WINDOWS_LOCATION); // location
    d.u16(3); // num_modes
    d.i32(1); // active_mode: Static
    mode(&mut d, v, "Direct", 0x10, 1 << 5 | 1 << 4, 1, &[]);
    mode(
        &mut d,
        v,
        "Static",
        0x11,
        1 << 6 | 1 << 8,
        2,
        &[(0xff, 0, 0)],
    );
    mode(&mut d, v, "Wave", 0x12, 1 << 5 | 1 << 9, 1, &[]);
    d.u16(2); // num_zones
              // ---- Zone Data, the keys: a matrix (Zone Types, 2) of 4 LEDs
    d.str("Keys"); // zone_name
    d.i32(2); // zone_type
    d.u32(4).u32(4).u32(4); // zone_leds_min, zone_leds_max, zone_leds_count
                            // zone_matrix_len, then Matrix Map Data: height, width, height × width
                            // cells, 0xFFFFFFFF where there is no LED (RGBControllerAPI.md, Matrix Map)
    d.u16(8 + 4 * 6).u32(2).u32(3);
    for cell in [0, 1, u32::MAX, 2, 3, u32::MAX] {
        d.u32(cell);
    }
    d.u16(0); // num_segments (4)
    if v >= 5 {
        d.u32(0x1000); // zone_flags (5): bit 12, "manually configured size"
    }
    if v >= 6 {
        d.i32(-1); // zone_active_mode (6): none of its own
        d.u16(0); // zone_num_modes (6)
        d.str(""); // zone_display_name (6)
    }
    // ---- Zone Data, the logo: one LED (Zone Types, 0) with one segment
    d.str("Logo"); // zone_name
    d.i32(0); // zone_type
    d.u32(1).u32(1).u32(1); // zone_leds_min, zone_leds_max, zone_leds_count
    d.u16(0); // zone_matrix_len: no matrix map
    d.u16(1); // num_segments (4)
              // ---- Segment Data
    d.str("Whole logo"); // segment_name (4)
    d.i32(1); // segment_type (4)
    d.u32(0).u32(1); // segment_start_idx, segment_leds_count (4)
    if v >= 6 {
        d.u16(0); // segment_matrix_len (6)
        d.u32(0); // segment_flags (6)
    }
    if v >= 5 {
        d.u32(0); // zone_flags (5)
    }
    if v >= 6 {
        d.i32(-1).u16(0).str(""); // zone_active_mode, zone_num_modes, zone_display_name (6)
    }
    // ---- LED Data
    d.u16(5); // num_leds
    for (i, name) in ["Key: A", "Key: B", "Key: C", "Key: D", "Logo"]
        .iter()
        .enumerate()
    {
        d.str(name); // led_name
        if v < 6 {
            d.u32(0x100 + u32::try_from(i).unwrap()); // led_value, "<6"
        }
    }
    d.u16(5); // num_colors
    for i in 0..4 {
        d.rgb(i, 0, 0); // colors
    }
    d.rgb(1, 2, 3);
    if v >= 5 {
        d.u16(5); // num_led_display_names (5)
        for name in ["Touche A", "Touche B", "Touche C", "Touche D", ""] {
            d.str(name); // LED Display Name Data
        }
        d.u32(1); // flags (5): bit 0, local (Controller Flags)
    }
    if v >= 6 {
        d.str("Fixture Keyboard (display)"); // display_name (6)
        d.u32(0); // configuration_len (6), uint32, and no configuration
    }
    let body = d.bytes();
    // ---- NET_PACKET_ID_REQUEST_CONTROLLER_DATA, Response: data_size, then
    // Device Data. "Size of all data in packet": read here as the whole
    // payload, itself included (§6.6).
    let mut payload = Doc::new();
    payload
        .u32(u32::try_from(body.len() + 4).unwrap())
        .raw(&body);
    payload.bytes()
}

/// A strip of `leds` LEDs and no matrix, from no serial, on SMBus: what a
/// device Candeo has no definition for looks like, protocol 6.
pub(crate) fn strip(name: &str, leds: u16, location: &str, serial: &str) -> Vec<u8> {
    let mut d = Doc::new();
    d.i32(4)
        .str(name)
        .str("Fixture")
        .str("")
        .str("")
        .str(serial)
        .str(location);
    d.u16(1).i32(0);
    mode(&mut d, 6, "Direct", 0, 1 << 5, 1, &[]);
    d.u16(1);
    d.str("Strip")
        .i32(1)
        .u32(0)
        .u32(u32::from(leds))
        .u32(u32::from(leds));
    d.u16(0).u16(0).u32(0).i32(-1).u16(0).str("");
    d.u16(leds);
    for i in 0..leds {
        d.str(&format!("LED {i}"));
    }
    d.u16(leds);
    for _ in 0..leds {
        d.rgb(0, 0, 0);
    }
    d.u16(0).u32(1).str("").u32(0);
    let body = d.bytes();
    let mut payload = Doc::new();
    payload
        .u32(u32::try_from(body.len() + 4).unwrap())
        .raw(&body);
    payload.bytes()
}
