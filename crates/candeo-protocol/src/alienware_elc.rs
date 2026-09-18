//! Reports of the AW-ELC, the zones around the keyboard, as surveyed in
//! `docs/protocol/alienware-m18-r1.md` §2.
//!
//! A change opens, names the zones it touches, gives them a colour, then
//! commits. One such block can name several sets, so a whole surface goes out in
//! one.
//!
//! The byte after `00` in the framing reports is **a target**, not a counter:
//! `ff` is the common one, and other values name the stored profiles — the
//! power states among them. Walking it, as a transaction number would be walked,
//! writes each change to a different target.
//!
//! Two families sit side by side and only one shows: `03 21 …` lights what it
//! carries, `03 22 …` writes the configuration the device keeps. Everything here
//! is the first. The second is deliberately absent: writing a configuration
//! nobody asked for would change what the machine shows when Candeo is not there.

use crate::Rgb;

/// Size of a report. This device carries no report id of its own, so the byte
/// the HID API wants in front is not one of these.
pub const REPORT_LEN: usize = 33;

/// How many zone ids one selection carries. The survey never saw more than four
/// addressed at once, which is every zone this machine has.
pub const ZONES_PER_SELECT: usize = 4;

/// The common target, which is the lighting shown now. Other values name stored
/// profiles — a power state, the startup colour — and writing one of those
/// changes what the machine shows when nothing is running. Candeo does not.
pub const COMMON: u8 = 0xff;

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

/// Clears the target, then starts writing to it.
pub fn begin(target: u8) -> [[u8; REPORT_LEN]; 2] {
    [
        report(&[0x03, 0x21, 0x00, 0x04, 0x00, target]),
        report(&[0x03, 0x21, 0x00, 0x01, 0x00, target]),
    ]
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

/// Finishes the target, then makes it the one in force — which is when the
/// zones change.
pub fn commit(target: u8) -> [[u8; REPORT_LEN]; 2] {
    [
        report(&[0x03, 0x21, 0x00, 0x02, 0x00, target]),
        report(&[0x03, 0x21, 0x00, 0x06, 0x00, target]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The transaction, byte for byte as the capture holds it.
    #[test]
    fn a_transaction_is_what_the_capture_shows() {
        let tx = 0x5e;
        assert_eq!(begin(tx)[0][..6], [0x03, 0x21, 0x00, 0x04, 0x00, 0x5e]);
        assert_eq!(begin(tx)[1][..6], [0x03, 0x21, 0x00, 0x01, 0x00, 0x5e]);
        assert_eq!(commit(tx)[0][..6], [0x03, 0x21, 0x00, 0x02, 0x00, 0x5e]);
        assert_eq!(commit(tx)[1][..6], [0x03, 0x21, 0x00, 0x06, 0x00, 0x5e]);
        assert_eq!(begin(tx)[0].len(), REPORT_LEN);
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
