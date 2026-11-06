//! Groups of keys named by what they are, by scancode
//! (`docs/design/composition.md` §3.3).
//!
//! A scancode names the physical key whatever its legend: the keys under a
//! gamer's left hand are `11 1e 1f 20`, engraved ZQSD on AZERTY and WASD on
//! QWERTY. A group written that way is the same cluster on every keyboard and
//! every national layout, narrowed to the keys a keyboard has. Its name for
//! people is in the window's catalogs; this table is what masks resolve.

use crate::layout::{Layout, NO_SCANCODE};

/// A named group of keys.
pub struct KeyGroup {
    /// What a mask names it by: `wasd`.
    pub id: &'static str,
    /// Its keys, as [`crate::Key::scancode`] gives them.
    pub scancodes: &'static [u16],
}

impl KeyGroup {
    /// The positions of `layout` holding one of its keys, ascending: those the
    /// keyboard has, and none on a zones device, whose lights carry no
    /// scancode.
    pub fn positions(&self, layout: &Layout) -> Vec<u16> {
        let mut positions: Vec<u16> = layout
            .keys
            .iter()
            .filter(|k| k.scancode != NO_SCANCODE && self.scancodes.contains(&k.scancode))
            .map(|k| k.index)
            .collect();
        positions.sort_unstable();
        positions
    }
}

/// Every group a mask may name, in the order a picker lists them.
pub const KEY_GROUPS: &[KeyGroup] = &[
    KeyGroup {
        id: "escape",
        scancodes: &[0x01],
    },
    KeyGroup {
        id: "wasd",
        scancodes: &[0x11, 0x1E, 0x1F, 0x20],
    },
    KeyGroup {
        id: "arrows",
        scancodes: &[0xE048, 0xE04B, 0xE050, 0xE04D],
    },
    KeyGroup {
        id: "functionRow",
        scancodes: &[
            0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40, 0x41, 0x42, 0x43, 0x44, 0x57, 0x58,
        ],
    },
    KeyGroup {
        id: "numberRow",
        scancodes: &[
            0x29, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D,
        ],
    },
    // The keypad's own codes: its 7, 8 and 9 send `47 48 49`, where Home, Up and
    // Page Up send them behind `e0`.
    KeyGroup {
        id: "numpad",
        scancodes: &[
            0x45, 0xE035, 0x37, 0x4A, 0x47, 0x48, 0x49, 0x4E, 0x4B, 0x4C, 0x4D, 0x4F, 0x50, 0x51,
            0xE01C, 0x52, 0x53,
        ],
    },
    // Shift, Ctrl, Alt and Windows, left then right.
    KeyGroup {
        id: "modifiers",
        scancodes: &[0x2A, 0x36, 0x1D, 0xE01D, 0x38, 0xE038, 0xE05B, 0xE05C],
    },
    // Insert, Home, Page Up, Delete, End, Page Down.
    KeyGroup {
        id: "navigation",
        scancodes: &[0xE052, 0xE047, 0xE049, 0xE053, 0xE04F, 0xE051],
    },
    KeyGroup {
        id: "space",
        scancodes: &[0x39],
    },
];

/// The group a mask names `id`, if there is one.
pub fn key_group(id: &str) -> Option<&'static KeyGroup> {
    KEY_GROUPS.iter().find(|g| g.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::builtin_layout;

    fn counted(file: &str) -> Vec<(&'static str, usize, usize)> {
        let layout = builtin_layout(file).expect("built in");
        KEY_GROUPS
            .iter()
            .map(|g| (g.id, g.positions(layout).len(), g.scancodes.len()))
            .collect()
    }

    /// The table of `composition.md` §3.3, counted from the two built-in
    /// keyboard definitions on 2026-11-06: neither has a right Windows key,
    /// and the m18 R1 keeps only Delete and End of the navigation block.
    #[test]
    fn each_group_narrows_to_the_keys_a_keyboard_has() {
        let both = [
            ("escape", 1, 1),
            ("wasd", 4, 4),
            ("arrows", 4, 4),
            ("functionRow", 12, 12),
            ("numberRow", 13, 13),
            ("numpad", 17, 17),
            ("modifiers", 7, 8),
        ];
        let deathstalker = counted("razer-deathstalker-v2-pro.json");
        let m18 = counted("alienware-m18-r1.json");
        for row in both {
            assert!(deathstalker.contains(&row), "DeathStalker {row:?}");
            assert!(m18.contains(&row), "m18 R1 {row:?}");
        }
        assert!(deathstalker.contains(&("navigation", 6, 6)));
        assert!(m18.contains(&("navigation", 2, 6)));
        assert!(deathstalker.contains(&("space", 1, 1)));
        assert!(m18.contains(&("space", 1, 1)));
    }

    /// WASD is the same four keys by scancode, whatever is engraved on them.
    #[test]
    fn a_group_names_keys_by_scancode() {
        let layout = builtin_layout("razer-deathstalker-v2-pro.json").unwrap();
        let wasd = key_group("wasd").unwrap().positions(layout);
        let mut scancodes: Vec<u16> = wasd
            .iter()
            .map(|&p| layout.key(p).unwrap().scancode)
            .collect();
        scancodes.sort_unstable();
        assert_eq!(scancodes, [0x11, 0x1E, 0x1F, 0x20]);
        assert!(wasd.windows(2).all(|w| w[0] < w[1]), "ascending");
    }

    #[test]
    fn a_zones_device_has_no_group() {
        let zones = builtin_layout("alienware-17-r5-zones.json").unwrap();
        assert!(KEY_GROUPS.iter().all(|g| g.positions(zones).is_empty()));
        assert!(key_group("vowels").is_none());
    }

    /// A mask's layout is the device's with its lights alone, built once for
    /// the same lights.
    #[test]
    fn a_masked_layout_is_the_device_with_its_lights_alone_and_built_once() {
        let layout = builtin_layout("razer-deathstalker-v2-pro.json").unwrap();
        let wasd = key_group("wasd").unwrap().positions(layout);
        let masked = layout.masked(&wasd);
        assert_eq!(masked.keys.len(), 4);
        assert_eq!(masked.lit_count(), 4);
        assert_eq!(masked.led_count(), layout.led_count(), "the device's frame");
        assert_eq!((masked.rows, masked.cols), (layout.rows, layout.cols));
        assert!(masked.firmware_effects.is_empty() && masked.regions.is_empty());
        let mut reversed = wasd.clone();
        reversed.reverse();
        assert!(std::ptr::eq(masked, layout.masked(&reversed)), "kept");
        assert!(!std::ptr::eq(masked, layout.masked(&wasd[..2])));
    }
}
