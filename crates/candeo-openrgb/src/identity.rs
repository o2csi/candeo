//! What identifies an OpenRGB device without carrying its serial or location
//! (`openrgb-output.md` §4.3, §6.9).
//!
//! A controller's `serial` is often read from the device itself, and its
//! `location` is a HID path holding the device instance ID on Windows, an
//! address for a network light: everything `AGENTS.md` keeps out of logs,
//! settings and reports. They are held in [`Private`], which prints as a
//! placeholder and offers no `Display`, so a log line or a `Failure` cannot
//! carry them by accident, and they are reduced to a [`Fingerprint`].

use std::fmt;

/// A unit's serial or location: in memory, never printed.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Private(String);

impl Private {
    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }

    /// The value itself, for the fingerprint and the guard only.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Private {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<private>")
    }
}

/// How an OpenRGB device is known from one connection to the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FingerprintKind {
    /// Of the serial: one unit.
    Serial,
    /// Of the vendor, the name and the zones: the weaker kind, which two
    /// identical units share (§6.8 lists them as duplicates).
    Description,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint {
    pub kind: FingerprintKind,
    value: u64,
}

impl fmt::Display for Fingerprint {
    /// `s:` or `d:` and sixteen hexadecimal digits: the key in `settings.json`
    /// and the name in the log.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.kind {
            FingerprintKind::Serial => 's',
            FingerprintKind::Description => 'd',
        };
        write!(f, "{kind}:{:016x}", self.value)
    }
}

impl Fingerprint {
    /// Of the serial when there is one, otherwise of what describes the device.
    pub(crate) fn of(serial: &Private, vendor: &str, name: &str, zones: &[(&str, u32)]) -> Self {
        if !serial.is_empty() {
            return Self {
                kind: FingerprintKind::Serial,
                value: fnv1a64(serial.expose().as_bytes()),
            };
        }
        let mut text = Vec::new();
        for part in [vendor, name] {
            text.extend_from_slice(part.as_bytes());
            text.push(0);
        }
        for (zone, leds) in zones {
            text.extend_from_slice(zone.as_bytes());
            text.push(0);
            text.extend_from_slice(&leds.to_le_bytes());
        }
        Self {
            kind: FingerprintKind::Description,
            value: fnv1a64(&text),
        }
    }
}

/// The serial's fingerprint as the log writes it for a native unit: the same
/// function as `journal::fingerprint` in the desktop application, so the
/// guard can compare an OpenRGB controller with a unit Candeo has open
/// (§6.8).
pub fn serial_fingerprint(serial: &str) -> String {
    format!("{:016x}", fnv1a64(serial.as_bytes()))
}

pub(crate) fn serial_fingerprint_of(serial: &Private) -> Option<String> {
    (!serial.is_empty()).then(|| serial_fingerprint(serial.expose()))
}

/// 64-bit FNV-1a, `journal::fingerprint`'s function.
fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = OFFSET_BASIS;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(PRIME);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERIAL: &str = "XY24ABCDEFG0001";

    #[test]
    fn a_private_value_never_prints() {
        let p = Private::new(SERIAL.into());
        let shown = format!("{p:?} {:?}", Some(&p));
        assert!(!shown.contains(SERIAL), "{shown}");
        assert!(!shown.contains("XY24"), "{shown}");
    }

    /// The published FNV-1a 64 vectors, so the function is the one
    /// `journal::fingerprint` uses and not a near copy.
    #[test]
    fn fnv1a64_matches_the_published_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn a_fingerprint_does_not_disclose_the_serial() {
        let f = Fingerprint::of(&Private::new(SERIAL.into()), "Vendor", "Name", &[]);
        let shown = f.to_string();
        assert!(shown.starts_with("s:") && shown.len() == 18, "{shown}");
        for piece in ["XY24", "ABCDEFG", "0001"] {
            assert!(!shown.contains(piece), "{piece} in {shown}");
        }
        assert_eq!(&shown[2..], serial_fingerprint(SERIAL));
    }

    #[test]
    fn without_a_serial_the_description_is_fingerprinted_and_marked_weaker() {
        let none = Private::default();
        let a = Fingerprint::of(&none, "Vendor", "Strip", &[("Zone", 30)]);
        let b = Fingerprint::of(&none, "Vendor", "Strip", &[("Zone", 31)]);
        assert_eq!(a.kind, FingerprintKind::Description);
        assert!(a.to_string().starts_with("d:"));
        assert_ne!(a, b, "the zone shape is part of it");
        assert_eq!(
            a,
            Fingerprint::of(&none, "Vendor", "Strip", &[("Zone", 30)])
        );
    }
}
