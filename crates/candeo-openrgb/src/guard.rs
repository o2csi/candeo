//! Native first: a controller Candeo drives itself is never driven through
//! OpenRGB (`openrgb-output.md` §6.8, #368 step 5).
//!
//! Run after each read of the controller list, before anything is driven.
//! When it cannot tell that a HID controller is not one of Candeo's, it
//! refuses: the guard fails closed.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::controller::Controller;
use crate::identity::{serial_fingerprint_of, Fingerprint, Private};
use crate::model;

/// A USB vendor and product id, as a definition's `match` names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UsbId {
    pub vid: u16,
    pub pid: u16,
}

impl UsbId {
    pub const fn new(vid: u16, pid: u16) -> Self {
        Self { vid, pid }
    }

    /// `1532:0292`, the form of the detector map (§6.4).
    pub fn parse(text: &str) -> Option<Self> {
        let (vid, pid) = text.split_once(':')?;
        Some(Self::new(hex16(vid)?, hex16(pid)?))
    }
}

impl fmt::Display for UsbId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04x}:{:04x}", self.vid, self.pid)
    }
}

fn hex16(text: &str) -> Option<u16> {
    (text.len() == 4)
        .then(|| u16::from_str_radix(text, 16).ok())
        .flatten()
}

/// What Candeo drives natively on this computer.
#[derive(Debug, Clone, Default)]
pub struct Natives {
    /// The VID:PID of every definition for this computer: built in,
    /// unverified or yours, plugged in or not (§6.4).
    pub ids: BTreeSet<UsbId>,
    /// The serial fingerprints of the units Candeo has open, as
    /// `journal::fingerprint` writes them.
    pub serials: BTreeSet<String>,
}

/// Where a controller is, from its `location` (§4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// `HID: ` and a path, with the VID:PID it gives when it gives one.
    Hid(Option<UsbId>),
    /// SMBus, a network address, a serial port…
    Other,
}

/// What reads `/sys/class/hidraw/<node>/device/uevent` (Linux), given the
/// node's name; injected so the guard is tested without one.
pub type Uevent<'a> = &'a dyn Fn(&str) -> Option<String>;

pub fn place(location: &Private, uevent: Uevent<'_>) -> Place {
    let Some(path) = location.expose().strip_prefix("HID: ") else {
        return Place::Other;
    };
    if let Some(id) = windows_path_ids(path) {
        return Place::Hid(Some(id));
    }
    let node = path.strip_prefix("/dev/").filter(|n| is_hidraw_node(n));
    Place::Hid(node.and_then(uevent).and_then(|text| uevent_ids(&text)))
}

/// A Windows HID interface path names its device as `VID_xxxx&PID_xxxx`.
fn windows_path_ids(path: &str) -> Option<UsbId> {
    let upper = path.to_ascii_uppercase();
    let after = |key: &str| -> Option<u16> {
        let at = upper.find(key)? + key.len();
        hex16(upper.get(at..at + 4)?)
    };
    Some(UsbId::new(after("VID_")?, after("PID_")?))
}

fn is_hidraw_node(node: &str) -> bool {
    node.strip_prefix("hidraw")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// `HID_ID=0003:00001532:00000292`: bus, vendor, product, in the kernel's
/// uevent for a HID device.
fn uevent_ids(text: &str) -> Option<UsbId> {
    let value = text.lines().find_map(|l| l.strip_prefix("HID_ID="))?;
    let mut parts = value.trim().split(':');
    let _bus = parts.next()?;
    let vid = u32::from_str_radix(parts.next()?, 16).ok()?;
    let pid = u32::from_str_radix(parts.next()?, 16).ok()?;
    Some(UsbId::new(
        u16::try_from(vid).ok()?,
        u16::try_from(pid).ok()?,
    ))
}

/// Reads a hidraw node's uevent from sysfs.
pub fn sysfs_uevent(node: &str) -> Option<String> {
    if !is_hidraw_node(node) {
        return None;
    }
    std::fs::read_to_string(format!("/sys/class/hidraw/{node}/device/uevent")).ok()
}

/// What becomes of a controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// An OpenRGB device Candeo may drive.
    OpenRgb,
    /// One of Candeo's: set aside, not listed a second time.
    Native,
    /// A HID controller whose path gives no VID:PID: Candeo cannot tell it is
    /// not one of its own.
    Unknown,
    /// Shares its fingerprint with another: listed, neither driven.
    Duplicate,
}

pub fn sort(controllers: &[&Controller], natives: &Natives, uevent: Uevent<'_>) -> Vec<Verdict> {
    let mut verdicts: Vec<Verdict> = controllers
        .iter()
        .map(|c| {
            let native_serial =
                serial_fingerprint_of(&c.serial).is_some_and(|f| natives.serials.contains(&f));
            match place(&c.location, uevent) {
                _ if native_serial => Verdict::Native,
                Place::Hid(Some(id)) if natives.ids.contains(&id) => Verdict::Native,
                Place::Hid(None) => Verdict::Unknown,
                Place::Hid(Some(_)) | Place::Other => Verdict::OpenRgb,
            }
        })
        .collect();
    let mut seen: BTreeMap<Fingerprint, usize> = BTreeMap::new();
    let fingerprints: Vec<Fingerprint> = controllers
        .iter()
        .map(|c| model::describe(c).fingerprint)
        .collect();
    for (i, f) in fingerprints.iter().enumerate() {
        if verdicts[i] == Verdict::OpenRgb {
            *seen.entry(*f).or_default() += 1;
        }
    }
    for (i, f) in fingerprints.iter().enumerate() {
        if verdicts[i] == Verdict::OpenRgb && seen.get(f).copied().unwrap_or(0) > 1 {
            verdicts[i] = Verdict::Duplicate;
        }
    }
    verdicts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::parse_controller;
    use crate::fixtures;
    use crate::identity::serial_fingerprint;

    fn no_sysfs(_: &str) -> Option<String> {
        None
    }

    fn keyboard() -> Controller {
        parse_controller(&fixtures::keyboard(6), 6).unwrap()
    }

    fn strip(name: &str, location: &str, serial: &str) -> Controller {
        parse_controller(&fixtures::strip(name, 3, location, serial), 6).unwrap()
    }

    #[test]
    fn a_windows_hid_path_gives_its_vid_and_pid() {
        let loc = Private::new(fixtures::WINDOWS_LOCATION.into());
        assert_eq!(
            place(&loc, &no_sysfs),
            Place::Hid(Some(UsbId::new(0x1234, 0x5678)))
        );
        let lower =
            Private::new("HID: \\\\?\\hid#vid_1532&pid_0292&mi_03#0&fake&0&0000#{x}".into());
        assert_eq!(
            place(&lower, &no_sysfs),
            Place::Hid(Some(UsbId::new(0x1532, 0x0292)))
        );
    }

    #[test]
    fn a_linux_hidraw_node_is_read_through_its_uevent() {
        let uevent = |node: &str| -> Option<String> {
            (node == "hidraw3").then(|| {
                "DRIVER=hid-generic\nHID_ID=0003:00001234:00005678\nHID_NAME=Fixture\n".into()
            })
        };
        let loc = Private::new("HID: /dev/hidraw3".into());
        assert_eq!(
            place(&loc, &uevent),
            Place::Hid(Some(UsbId::new(0x1234, 0x5678)))
        );
        let other = Private::new("HID: /dev/hidraw4".into());
        assert_eq!(place(&other, &uevent), Place::Hid(None));
    }

    /// Only `hidrawN` reaches sysfs: a path cannot make the guard read a file
    /// of its choosing.
    #[test]
    fn a_node_name_that_is_not_hidraw_is_not_read() {
        let read = std::cell::Cell::new(false);
        let uevent = |_: &str| -> Option<String> {
            read.set(true);
            None
        };
        for path in [
            "HID: /dev/../etc/passwd",
            "HID: /dev/hidraw",
            "HID: /dev/hidraw1/x",
            "HID: /dev/sda",
        ] {
            assert_eq!(
                place(&Private::new(path.into()), &uevent),
                Place::Hid(None),
                "{path}"
            );
        }
        assert!(!read.get());
        assert!(sysfs_uevent("../../etc").is_none());
    }

    #[test]
    fn other_locations_are_not_hid() {
        for loc in [
            "I2C: /dev/i2c-1, address 0x70",
            "Network: fixture.invalid",
            "",
            "COM3",
        ] {
            assert_eq!(
                place(&Private::new(loc.into()), &no_sysfs),
                Place::Other,
                "{loc}"
            );
        }
    }

    #[test]
    fn a_controller_candeo_has_a_definition_for_is_native() {
        let kb = keyboard();
        let mut natives = Natives::default();
        assert_eq!(sort(&[&kb], &natives, &no_sysfs), vec![Verdict::OpenRgb]);
        natives.ids.insert(UsbId::new(0x1234, 0x5678));
        assert_eq!(sort(&[&kb], &natives, &no_sysfs), vec![Verdict::Native]);
    }

    /// A unit Candeo has open, reached by OpenRGB some other way than its HID
    /// path, is caught by its serial.
    #[test]
    fn a_unit_open_natively_is_native_by_its_serial() {
        let s = strip("Strip", "USB: 1-2", fixtures::SERIAL);
        let natives = Natives {
            serials: [serial_fingerprint(fixtures::SERIAL)].into(),
            ..Natives::default()
        };
        assert_eq!(sort(&[&s], &natives, &no_sysfs), vec![Verdict::Native]);
    }

    #[test]
    fn a_hid_path_without_ids_fails_closed() {
        let s = strip("Strip", "HID: something unexpected", "");
        assert_eq!(
            sort(&[&s], &Natives::default(), &no_sysfs),
            vec![Verdict::Unknown]
        );
    }

    #[test]
    fn two_controllers_with_one_fingerprint_are_both_duplicates() {
        let a = strip("Strip", "I2C: bus 1", "");
        let b = strip("Strip", "I2C: bus 2", "");
        let c = strip("Other strip", "I2C: bus 3", "");
        assert_eq!(
            sort(&[&a, &b, &c], &Natives::default(), &no_sysfs),
            vec![Verdict::Duplicate, Verdict::Duplicate, Verdict::OpenRgb]
        );
    }

    #[test]
    fn usb_ids_read_and_write_the_map_form() {
        assert_eq!(UsbId::parse("1532:0292"), Some(UsbId::new(0x1532, 0x0292)));
        assert_eq!(UsbId::new(0x1532, 0x0292).to_string(), "1532:0292");
        for bad in ["1532-0292", "1532:292", "zzzz:0000", ""] {
            assert_eq!(UsbId::parse(bad), None, "{bad}");
        }
    }
}
