//! Reports of the AW-ELC, the zones around the keyboard, as surveyed in
//! `docs/protocol/alienware-m18-r1.md` §2.
//!
//! A change opens, names the zones it touches, gives them a colour, then shows.
//! One such block can name several sets, so a whole surface goes out in one.
//!
//! Three families sit side by side, and the maker's software uses all three:
//!
//! - `03 21 00 01 ff ff` … `03 21 00 03 00 ff`, its **live preview**: lights what
//!   it carries and keeps nothing. Everything here is this one.
//! - `03 21 00 04 00 <id>` … `03 21 00 06 00 <id>`, applying a profile. With
//!   `ff` for the id, what Candeo sent until 2026-09-25, the device acknowledges
//!   every report and shows nothing, on a machine where the live preview lights.
//! - `03 22 …`, the profiles the device keeps for its power states. Writing one
//!   would change what the machine shows when Candeo is not there: deliberately
//!   absent.

use crate::Rgb;

/// Size of a report. This device carries no report id of its own, so the byte
/// the HID API wants in front is not one of these.
pub const REPORT_LEN: usize = 33;

/// How many zone ids one selection carries. The survey never saw more than four
/// addressed at once, which is every zone this machine has.
pub const ZONES_PER_SELECT: usize = 4;

/// What follows the mode on a fixed colour: a duration, the sub-command of the
/// effect (`d0` is the plain colour), a zero, then a tempo — `fa` meaning
/// steady, since a fixed colour animates nothing.
const STEADY: [u8; 4] = [0x07, 0xd0, 0x00, 0xfa];

fn report(bytes: &[u8]) -> [u8; REPORT_LEN] {
    assert!(bytes.len() <= REPORT_LEN, "a report is {REPORT_LEN} bytes");
    let mut out = [0u8; REPORT_LEN];
    out[..bytes.len()].copy_from_slice(bytes);
    out
}

/// Starts a change of what is lit now.
pub fn open() -> [u8; REPORT_LEN] {
    report(&[0x03, 0x21, 0x00, 0x01, 0xff, 0xff])
}

/// Names the zones the next [`colour`] applies to.
pub fn select(zones: &[u8]) -> [u8; REPORT_LEN] {
    assert!(
        !zones.is_empty() && zones.len() <= ZONES_PER_SELECT,
        "a selection names one to {ZONES_PER_SELECT} zones"
    );
    let mut bytes = vec![0x03, 0x23, 0x01, 0x00, zones.len() as u8];
    bytes.extend_from_slice(zones);
    report(&bytes)
}

/// One fixed colour, for the zones last selected.
pub fn colour(rgb: Rgb) -> [u8; REPORT_LEN] {
    let mut bytes = vec![0x03, 0x24, 0x00];
    bytes.extend_from_slice(&STEADY);
    bytes.extend_from_slice(&[rgb.r, rgb.g, rgb.b]);
    report(&bytes)
}

/// Shows the change: the zones take their colours here.
pub fn show() -> [u8; REPORT_LEN] {
    report(&[0x03, 0x21, 0x00, 0x03, 0x00, 0xff])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The live preview, byte for byte as the capture of 2026-09-25 holds it.
    #[test]
    fn a_change_is_what_the_capture_shows() {
        assert_eq!(open()[..6], [0x03, 0x21, 0x00, 0x01, 0xff, 0xff]);
        assert_eq!(show()[..6], [0x03, 0x21, 0x00, 0x03, 0x00, 0xff]);
        assert_eq!(open().len(), REPORT_LEN);
    }

    /// The ring's two halves go out together, as the maker's software sends them.
    #[test]
    fn a_selection_counts_its_zones() {
        assert_eq!(
            select(&[0, 1])[..7],
            [0x03, 0x23, 0x01, 0x00, 0x02, 0x00, 0x01]
        );
        assert_eq!(select(&[4])[..6], [0x03, 0x23, 0x01, 0x00, 0x01, 0x04]);
    }

    #[test]
    fn a_colour_carries_the_timing_that_was_seen() {
        let white = colour(Rgb {
            r: 0xff,
            g: 0xff,
            b: 0xff,
        });
        assert_eq!(
            white[..10],
            [0x03, 0x24, 0x00, 0x07, 0xd0, 0x00, 0xfa, 0xff, 0xff, 0xff]
        );
    }

    #[test]
    #[should_panic(expected = "a selection names")]
    fn a_selection_of_nothing_is_a_mistake() {
        select(&[]);
    }
}
