//! Physical description of the supported devices.

use candeo_protocol::Firmware;

/// Matrix position without an LED.
///
/// Internal: the sentinel only serves to write and read [`Layout::matrix`], and
/// everything leaving this module has already been through it —
/// [`Layout::lit_count`] leaves it out of the count, [`Layout::at`] turns it into
/// `None`. A caller reading `matrix` directly would need it; none does, and the raw
/// value `u16::MAX` is written in the field's documentation for whoever would.
pub(crate) const EMPTY: u16 = u16::MAX;

/// A key: its LED, its printed legend, and its physical rectangle.
///
/// Coordinates are in **keyboard pitch units**: 1 u = the width of an alphanumeric
/// key. The origin is at the top left, `y` grows downwards. A key covers
/// `[x, x + w[ × [y, y + h[`.
///
/// # Where these rectangles come from
///
/// **Not from the device.** It only exposes the 6 × 22 logical grid and declares no
/// dimension. The drawing is a **hand transcription** of the standard full-size ISO
/// layout, aligned by eye on the real keyboard. It is exact in the sense of a
/// convention, not of a survey: a drawing mistake is only caught by looking at the
/// simulator.
///
/// Only [`Key::index`] and [`Key::name`] come from the hardware survey, described in
/// `docs/protocol/deathstalker-v2-pro.md` §6.
pub struct Key {
    /// LED index, as it appears in [`Layout::matrix`].
    pub index: u16,
    /// Readable name, in the French (ISO) variant the keyboard declares.
    pub name: &'static str,
    /// Left edge, in keyboard pitch units.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

/// Layout of a device: identification, transport and matrix.
pub struct Layout {
    pub name: &'static str,
    pub vid: u16,
    pub pid: u16,
    /// Interface of the USB composite device that carries the lighting.
    pub interface: u8,
    /// Firmware this layout was surveyed against.
    ///
    /// **Data, not a sentence in a `.md`**: it is what the inspection on open
    /// compares with the version it reads, and a mismatch must be reportable
    /// without anyone having to dig up the survey. A layout contributed without its
    /// hardware at hand (#34) would otherwise have no way to tell "the code is
    /// wrong" from "the version changed".
    ///
    /// Read through the device command (`0x00`/`0x81`), **never** copied from
    /// `release_number`: HID enumeration returns the `bcdDevice` there, a frozen
    /// hardware revision that looks like a version and is not one.
    pub surveyed_firmware: Firmware,
    pub rows: u8,
    pub cols: u8,
    /// LED index per position, row by row. `u16::MAX` = no LED.
    pub matrix: &'static [u16],
    /// Name and geometry of each occupied position, in index order.
    pub keys: &'static [Key],
}

impl Layout {
    /// True if this HID enumeration entry is **the lighting interface** of this
    /// layout.
    ///
    /// A single rule, for opening as for presence: copying it in two places would
    /// let a device be reported as plugged in through an entry we would not open.
    ///
    /// # The `interface -1` entry is ruled out, and here
    ///
    /// The DeathStalker enumeration carries, on the same VID and PID, an entry with
    /// no interface number (`-1`), no product name, and a revision (`0x0101`) that
    /// is not the composite's (`0x0200`). **It is not the keyboard**: its parent in
    /// the device tree is a `RZVIRTUAL` node, created by the vendor driver (service
    /// `RzDev_0292`), not a USB interface — hence the missing number. See §1 of the
    /// survey. It therefore only exists where that driver is installed, and nothing
    /// guarantees it keeps this shape there.
    ///
    /// On Windows, opening the wrong entry yields a **valid** handle on which every
    /// write is lost. Requiring it to equal [`Self::interface`] is enough to rule it
    /// out, whatever shape it takes tomorrow.
    pub fn is_lighting_interface(&self, vid: u16, pid: u16, interface: i32) -> bool {
        vid == self.vid && pid == self.pid && interface == i32::from(self.interface)
    }

    /// Number of matrix positions — **not** the number of physical LEDs.
    ///
    /// This is the value a full frame must cover.
    pub const fn led_count(&self) -> usize {
        self.rows as usize * self.cols as usize
    }

    /// Number of positions that actually carry an LED.
    pub fn lit_count(&self) -> usize {
        self.matrix.iter().filter(|&&i| i != EMPTY).count()
    }

    /// LED index at a given position.
    pub fn at(&self, row: u8, col: u8) -> Option<u16> {
        let i = row as usize * self.cols as usize + col as usize;
        match self.matrix.get(i) {
            Some(&v) if v != EMPTY => Some(v),
            _ => None,
        }
    }

    /// Key carrying a given LED index.
    ///
    /// Every lit position has one: that is the invariant the
    /// `every_lit_position_has_a_key` test checks.
    pub fn key(&self, index: u16) -> Option<&'static Key> {
        let keys: &'static [Key] = self.keys;
        keys.iter().find(|k| k.index == index)
    }
}

/// Standard key, 1 u × 1 u.
const fn k(index: u16, name: &'static str, x: f32, y: f32) -> Key {
    Key {
        index,
        name,
        x,
        y,
        w: 1.0,
        h: 1.0,
    }
}

/// Wide key, one row high.
const fn kw(index: u16, name: &'static str, x: f32, y: f32, w: f32) -> Key {
    Key {
        index,
        name,
        x,
        y,
        w,
        h: 1.0,
    }
}

/// Key spanning two rows — the numeric keypad `+` and Enter.
const fn kh(index: u16, name: &'static str, x: f32, y: f32, w: f32, h: f32) -> Key {
    Key {
        index,
        name,
        x,
        y,
        w,
        h,
    }
}

/// Razer DeathStalker V2 Pro, wired.
///
/// Matrix surveyed by querying the device: 6 × 22 = **132** positions — the size
/// of a frame — of which **106** carry an LED.
///
/// The two numbers are not interchangeable: sending 106 leaves the last rows frozen
/// on their previous value. See `docs/protocol/deathstalker-v2-pro.md` §6.
///
/// The geometry of [`Layout::keys`] is a manual transcription of the full-size ISO
/// layout: the device declares nothing of the kind. See [`Key`]. The drawing is
/// 22.5 u wide and 6.5 u high: main block from 0 to 15 u, navigation cluster from
/// 15.25 to 18.25 u, numeric keypad from 18.5 to 22.5 u.
pub static DEATHSTALKER_V2_PRO: Layout = Layout {
    name: "Razer DeathStalker V2 Pro (filaire)",
    vid: 0x1532,
    pid: 0x0292,
    interface: 3,
    // `01 05`, read back through `0x00`/`0x81` on 12/09/2026 — the version the
    // device also declares elsewhere. See §8 of the survey.
    surveyed_firmware: Firmware { major: 1, minor: 5 },
    rows: 6,
    cols: 22,
    #[rustfmt::skip]
    matrix: &[
        //  0      1    2    3    4    5    6    7    8    9   10   11   12   13   14   15   16   17     18     19     20     21
            0, EMPTY,   2,   3,   4,   5,   6,   7,   8,   9,  10,  11,  12,  13,  14,  15,  16, EMPTY, EMPTY, EMPTY, EMPTY, EMPTY,
           22,    23,  24,  25,  26,  27,  28,  29,  30,  31,  32,  33,  34,  35,  36,  37,  38,    39,    40,    41,    42, EMPTY,
           44,    45,  46,  47,  48,  49,  50,  51,  52,  53,  54,  55,  56,  57,  58,  59,  60,    61,    62,    63,    64, EMPTY,
           66,    67,  68,  69,  70,  71,  72,  73,  74,  75,  76,  77,  78,  79, EMPTY, EMPTY, EMPTY, 83,   84,    85, EMPTY, EMPTY,
           88,    89,  90,  91,  92,  93,  94,  95,  96,  97,  98,  99, EMPTY, 101, EMPTY, 103, EMPTY, 105,  106,   107,   108, EMPTY,
          110,   111, 112, EMPTY, EMPTY, EMPTY, 116, EMPTY, EMPTY, EMPTY, 120, 121, 122, 123, 124, 125, 126, EMPTY, 128,  129, EMPTY, EMPTY,
    ],
    #[rustfmt::skip]
    keys: &[
        // Row 0 — function keys. Three blocks of four, then the print trio. (16)
        k(  0, "Échap",       0.0,  0.0),
        k(  2, "F1",          2.0,  0.0), k(  3, "F2",     3.0,  0.0), k(  4, "F3",     4.0,  0.0), k(  5, "F4",  5.0,  0.0),
        k(  6, "F5",          6.5,  0.0), k(  7, "F6",     7.5,  0.0), k(  8, "F7",     8.5,  0.0), k(  9, "F8",  9.5,  0.0),
        k( 10, "F9",         11.0,  0.0), k( 11, "F10",   12.0,  0.0), k( 12, "F11",   13.0,  0.0), k( 13, "F12", 14.0,  0.0),
        k( 14, "ImprÉcran",  15.25, 0.0), k( 15, "ArrêtDéfil", 16.25, 0.0), k( 16, "Pause", 17.25, 0.0),

        // Row 1 — AZERTY digits, 2 u Backspace, navigation, top of the keypad. (21)
        k( 22, "²",           0.0,  1.5), k( 23, "&",      1.0,  1.5), k( 24, "é",      2.0,  1.5),
        k( 25, "\"",          3.0,  1.5), k( 26, "'",      4.0,  1.5), k( 27, "(",      5.0,  1.5),
        k( 28, "-",           6.0,  1.5), k( 29, "è",      7.0,  1.5), k( 30, "_",      8.0,  1.5),
        k( 31, "ç",           9.0,  1.5), k( 32, "à",     10.0,  1.5), k( 33, ")",     11.0,  1.5),
        k( 34, "=",          12.0,  1.5), kw(35, "Retour arrière", 13.0, 1.5, 2.0),
        k( 36, "Inser",      15.25, 1.5), k( 37, "Origine", 16.25, 1.5), k( 38, "PgPréc", 17.25, 1.5),
        k( 39, "VerrNum",    18.5,  1.5), k( 40, "Pavé /", 19.5,  1.5), k( 41, "Pavé *", 20.5,  1.5),
        k( 42, "Pavé −",     21.5,  1.5),

        // Row 2 — 1.5 u Tab, top row, TOP of the L-shaped Enter, navigation, keypad. (21)
        kw(44, "Tab",         0.0,  2.5, 1.5),
        k( 45, "A",           1.5,  2.5), k( 46, "Z",      2.5,  2.5), k( 47, "E",      3.5,  2.5),
        k( 48, "R",           4.5,  2.5), k( 49, "T",      5.5,  2.5), k( 50, "Y",      6.5,  2.5),
        k( 51, "U",           7.5,  2.5), k( 52, "I",      8.5,  2.5), k( 53, "O",      9.5,  2.5),
        k( 54, "P",          10.5,  2.5), k( 55, "^",     11.5,  2.5), k( 56, "$",     12.5,  2.5),
        kw(57, "Entrée",     13.5,  2.5, 1.5),
        k( 58, "Suppr",      15.25, 2.5), k( 59, "Fin",   16.25, 2.5), k( 60, "PgSuiv", 17.25, 2.5),
        k( 61, "Pavé 7",     18.5,  2.5), k( 62, "Pavé 8", 19.5, 2.5), k( 63, "Pavé 9", 20.5,  2.5),
        kh(64, "Pavé +",     21.5,  2.5, 1.0, 2.0),

        // Row 3 — 1.75 u Caps Lock, home row, BOTTOM of the L-shaped Enter, keypad. (17)
        kw(66, "VerrMaj",     0.0,  3.5, 1.75),
        k( 67, "Q",           1.75, 3.5), k( 68, "S",      2.75, 3.5), k( 69, "D",      3.75, 3.5),
        k( 70, "F",           4.75, 3.5), k( 71, "G",      5.75, 3.5), k( 72, "H",      6.75, 3.5),
        k( 73, "J",           7.75, 3.5), k( 74, "K",      8.75, 3.5), k( 75, "L",      9.75, 3.5),
        k( 76, "M",          10.75, 3.5), k( 77, "ù",     11.75, 3.5), k( 78, "*",     12.75, 3.5),
        kw(79, "Entrée",     13.75, 3.5, 1.25),
        k( 83, "Pavé 4",     18.5,  3.5), k( 84, "Pavé 5", 19.5, 3.5), k( 85, "Pavé 6", 20.5,  3.5),

        // Row 4 — short left Shift (1.25 u) + ISO key, bottom row, ↑, keypad. (18)
        kw(88, "Maj gauche",  0.0,  4.5, 1.25),
        k( 89, "<",           1.25, 4.5),
        k( 90, "W",           2.25, 4.5), k( 91, "X",      3.25, 4.5), k( 92, "C",      4.25, 4.5),
        k( 93, "V",           5.25, 4.5), k( 94, "B",      6.25, 4.5), k( 95, "N",      7.25, 4.5),
        k( 96, ",",           8.25, 4.5), k( 97, ";",      9.25, 4.5), k( 98, ":",     10.25, 4.5),
        k( 99, "!",          11.25, 4.5),
        kw(101, "Maj droite", 12.25, 4.5, 2.75),
        k(103, "↑",          16.25, 4.5),
        k(105, "Pavé 1",     18.5,  4.5), k(106, "Pavé 2", 19.5, 4.5), k(107, "Pavé 3", 20.5,  4.5),
        kh(108, "Pavé Entrée", 21.5, 4.5, 1.0, 2.0),

        // Row 5 — 1.25 u modifiers, 6.25 u Space, inverted-T arrows. (13)
        kw(110, "Ctrl gauche", 0.0,  5.5, 1.25),
        kw(111, "Win",         1.25, 5.5, 1.25),
        kw(112, "Alt",         2.5,  5.5, 1.25),
        kw(116, "Espace",      3.75, 5.5, 6.25),
        kw(120, "AltGr",      10.0,  5.5, 1.25),
        kw(121, "Fn",         11.25, 5.5, 1.25),
        kw(122, "Menu",       12.5,  5.5, 1.25),
        kw(123, "Ctrl droit", 13.75, 5.5, 1.25),
        k(124, "←",           15.25, 5.5), k(125, "↓", 16.25, 5.5), k(126, "→", 17.25, 5.5),
        kw(128, "Pavé 0",     18.5,  5.5, 2.0),
        k(129, "Pavé .",      20.5,  5.5),
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    /// The enumeration surveyed on 12/09/2026, entry by entry: only `MI_03` carries
    /// the lighting. The `-1` entry, on the same VID and PID, must be neither
    /// opened nor counted as present.
    #[test]
    fn only_the_lighting_interface_is_kept() {
        let l = &DEATHSTALKER_V2_PRO;
        let kept: Vec<i32> = [-1, 0, 1, 2, 3]
            .into_iter()
            .filter(|&i| l.is_lighting_interface(0x1532, 0x0292, i))
            .collect();
        assert_eq!(kept, vec![3]);
        assert!(!l.is_lighting_interface(0x1532, 0x0290, 3), "other product");
    }

    #[test]
    fn matrix_dimensions_are_consistent() {
        let l = &DEATHSTALKER_V2_PRO;
        assert_eq!(l.matrix.len(), l.led_count());
        assert_eq!(l.led_count(), 132);
    }

    /// Two numbers coexist and do not mean the same thing.
    ///
    /// - **132**: the cells of the 6×22 matrix, and the LED count the zone
    ///   declares. It is the size of a frame — sending less leaves the last rows
    ///   frozen.
    /// - **106**: the cells that actually carry a key LED.
    ///
    /// Confirmed by querying the hardware again. An earlier survey reported 107
    /// occupied positions: it was a counting artifact, the [`EMPTY`] sentinel value
    /// having been counted as a distinct index.
    #[test]
    fn counts_match_device_report() {
        assert_eq!(DEATHSTALKER_V2_PRO.led_count(), 132, "frame size");
        assert_eq!(DEATHSTALKER_V2_PRO.lit_count(), 106, "lit keys");
    }

    /// No LED index may appear at two positions.
    #[test]
    fn led_indices_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for &i in DEATHSTALKER_V2_PRO.matrix.iter().filter(|&&i| i != EMPTY) {
            assert!(seen.insert(i), "index {i} appears twice in the matrix");
        }
    }

    #[test]
    fn known_positions_resolve() {
        let l = &DEATHSTALKER_V2_PRO;
        assert_eq!(l.at(0, 0), Some(0));
        assert_eq!(l.at(0, 1), None, "gap after Escape");
        assert_eq!(l.at(1, 0), Some(22));
        assert_eq!(l.at(5, 0), Some(110));
    }

    /// The key count per row, as surveyed on the hardware.
    ///
    /// It is the simplest check of a transcription: the sum is 106, and a row off
    /// by one key shows it immediately.
    #[test]
    fn rows_have_expected_key_counts() {
        let l = &DEATHSTALKER_V2_PRO;
        let expected = [16, 21, 21, 17, 18, 13];
        assert_eq!(expected.iter().sum::<usize>(), 106);

        for (row, &n) in expected.iter().enumerate() {
            let lit = (0..l.cols)
                .filter(|&c| l.at(row as u8, c).is_some())
                .count();
            assert_eq!(lit, n, "row {row}");
        }
    }

    /// The key table and the matrix describe the same keyboard.
    ///
    /// Both ways: every lit position has a name and a geometry, and no key
    /// describes a position that does not exist.
    #[test]
    fn every_lit_position_has_a_key() {
        let l = &DEATHSTALKER_V2_PRO;

        for row in 0..l.rows {
            for col in 0..l.cols {
                let Some(index) = l.at(row, col) else {
                    continue;
                };
                let key = l
                    .key(index)
                    .unwrap_or_else(|| panic!("index {index} at ({row}, {col}) has no key"));
                assert!(!key.name.is_empty(), "index {index} has no name");
                assert!(key.w > 0.0 && key.h > 0.0, "index {index} has no area");
            }
        }

        for key in l.keys {
            assert!(
                l.matrix.contains(&key.index),
                "\"{}\" (index {}) missing from the matrix",
                key.name,
                key.index
            );
        }
        assert_eq!(l.keys.len(), l.lit_count());
    }

    /// Two keys cannot occupy the same space.
    ///
    /// The ISO Enter is no exception: its two LEDs are modeled by the two
    /// **adjoining** rectangles that make up the L, not by a duplicated rectangle.
    /// See [`iso_enter_tiles_the_l_shape`].
    #[test]
    fn key_rectangles_do_not_overlap() {
        let keys = DEATHSTALKER_V2_PRO.keys;
        for (i, a) in keys.iter().enumerate() {
            for b in &keys[i + 1..] {
                let disjoint =
                    a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
                assert!(
                    disjoint,
                    "\"{}\" ({}) and \"{}\" ({}) overlap",
                    a.name, a.index, b.name, b.index
                );
            }
        }
    }

    /// The ISO Enter carries **two** LEDs: 57 in row 2, 79 in row 3.
    ///
    /// That is the hardware, not a survey defect — a vertical gradient is visible on
    /// it. So we model one key per LED, each covering the part of the L it lights:
    /// the upper arm 1.5 u wide, the lower arm 1.25 u wide and shifted to the right.
    /// Their union is exactly the L-shaped Enter, and their intersection is empty.
    #[test]
    fn iso_enter_tiles_the_l_shape() {
        let l = &DEATHSTALKER_V2_PRO;
        let upper = l.key(57).expect("Enter, row 2");
        let lower = l.key(79).expect("Enter, row 3");

        assert_eq!(upper.name, "Entrée");
        assert_eq!(lower.name, "Entrée");
        assert_eq!(l.at(2, 13), Some(57));
        assert_eq!(l.at(3, 13), Some(79));

        // Both arms rest on the same right edge — that of the main block, at 15 u —
        // and touch without overlapping.
        assert_eq!(upper.x + upper.w, 15.0);
        assert_eq!(lower.x + lower.w, 15.0);
        assert_eq!(upper.y + upper.h, lower.y);
        assert!(
            lower.x > upper.x,
            "the notch of the L is left of the lower arm"
        );
    }

    /// The space bar is 6.25 u wide but has only **one** LED, at (5, 6).
    #[test]
    fn space_bar_is_wide_but_single() {
        let l = &DEATHSTALKER_V2_PRO;
        assert_eq!(l.at(5, 6), Some(116));

        let space = l.key(116).expect("space bar");
        assert_eq!(space.name, "Espace");
        assert_eq!(space.w, 6.25);
        assert_eq!(l.keys.iter().filter(|k| k.name == "Espace").count(), 1);
    }

    /// The drawing fits within the footprint of a full-size ISO keyboard.
    #[test]
    fn drawing_fits_a_full_size_iso() {
        let keys = DEATHSTALKER_V2_PRO.keys;
        let width = keys.iter().fold(0.0f32, |m, k| m.max(k.x + k.w));
        let height = keys.iter().fold(0.0f32, |m, k| m.max(k.y + k.h));
        assert_eq!(width, 22.5, "total width, numeric keypad included");
        assert_eq!(height, 6.5, "total height, function row included");
        assert!(keys.iter().all(|k| k.x >= 0.0 && k.y >= 0.0));
    }
}
