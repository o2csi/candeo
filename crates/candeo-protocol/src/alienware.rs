//! Reports of the Alienware m18 R1 keyboard, as surveyed in
//! `docs/protocol/alienware-m18-r1.md`.
//!
//! Another maker, another shape: 64-byte **feature** reports carrying their own
//! id, where the Razer sends 90-byte reports with a checksum. What they have in
//! common is that both are built here, without touching a device, so the bytes
//! are a unit test rather than an evening in front of a keyboard.
//!
//! A frame is `open`, then `colours` for every key, then `close`. The device
//! applies what it received when the frame closes.

use crate::Rgb;

/// Report id and first byte of every report: this device carries the id in the
/// report itself, which `HidD_SetFeature` expects at the front of the buffer.
pub const REPORT_ID: u8 = 0xcc;

/// Size of the buffer, report id included.
pub const REPORT_LEN: usize = 64;

/// How many keys one `colours` report carries: 15 groups of `index, r, g, b` in
/// the 60 bytes that follow the four-byte header.
pub const KEYS_PER_REPORT: usize = 15;

/// One report: the id, a three-byte command, then its payload.
fn report(command: [u8; 3], payload: &[u8]) -> [u8; REPORT_LEN] {
    assert!(payload.len() <= REPORT_LEN - 4, "payload too long");
    let mut out = [0u8; REPORT_LEN];
    out[0] = REPORT_ID;
    out[1..4].copy_from_slice(&command);
    out[4..4 + payload.len()].copy_from_slice(payload);
    out
}

/// Opens a frame: what follows is one image, applied when it closes.
pub fn open() -> [u8; REPORT_LEN] {
    report([0x94, 0x00, 0x00], &[])
}

/// Closes the frame, and with it applies the colours.
pub fn close() -> [u8; REPORT_LEN] {
    report([0x93, 0x00, 0x00], &[])
}

/// Overall brightness, `0xff` being what the maker's software writes at full.
pub fn brightness(level: u8) -> [u8; REPORT_LEN] {
    report([0x8b, 0x01, level], &[])
}

/// Colours for up to [`KEYS_PER_REPORT`] keys, each named by its index.
///
/// More than that does not fit, and is a caller's mistake rather than something
/// to silently drop: a frame that loses keys on the way would show as a few dark
/// keys, which is far harder to read than a panic here.
pub fn colours(keys: &[(u8, Rgb)]) -> [u8; REPORT_LEN] {
    assert!(
        keys.len() <= KEYS_PER_REPORT,
        "a report carries {KEYS_PER_REPORT} keys at most"
    );
    let mut payload = [0u8; KEYS_PER_REPORT * 4];
    for (slot, (index, rgb)) in payload.chunks_mut(4).zip(keys) {
        slot.copy_from_slice(&[*index, rgb.r, rgb.g, rgb.b]);
    }
    report([0x8c, 0x02, 0x00], &payload[..keys.len() * 4])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three framing reports, as the capture holds them.
    #[test]
    fn framing_reports_are_what_the_capture_shows() {
        assert_eq!(open()[..4], [0xcc, 0x94, 0x00, 0x00]);
        assert_eq!(close()[..4], [0xcc, 0x93, 0x00, 0x00]);
        assert_eq!(brightness(0xff)[..4], [0xcc, 0x8b, 0x01, 0xff]);
        assert!(open()[4..].iter().all(|&b| b == 0), "the rest is zeroes");
        assert_eq!(open().len(), 64);
    }

    /// `cc 8c 02 00`, then `index, r, g, b` per key — the shape read off the bus.
    #[test]
    fn colours_carry_an_index_then_the_colour() {
        let report = colours(&[
            (43, Rgb::new(0xff, 0x00, 0x00)),
            (75, Rgb::new(0x00, 0x80, 0x00)),
        ]);
        assert_eq!(report[..4], [0xcc, 0x8c, 0x02, 0x00]);
        assert_eq!(report[4..8], [43, 0xff, 0x00, 0x00]);
        assert_eq!(report[8..12], [75, 0x00, 0x80, 0x00]);
        assert!(report[12..].iter().all(|&b| b == 0), "nothing else is said");
    }

    /// Fifteen keys fill the report exactly.
    #[test]
    fn fifteen_keys_fill_a_report() {
        let keys: Vec<(u8, Rgb)> = (1..=15).map(|i| (i, Rgb::new(i, i, i))).collect();
        let report = colours(&keys);
        assert_eq!(report[60..64], [15, 15, 15, 15]);
    }

    #[test]
    #[should_panic(expected = "15 keys at most")]
    fn sixteen_keys_do_not_fit() {
        let keys: Vec<(u8, Rgb)> = (1..=16).map(|i| (i, Rgb::new(0, 0, 0))).collect();
        colours(&keys);
    }
}
