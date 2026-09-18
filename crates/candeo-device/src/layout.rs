//! Physical description of the supported devices.

use candeo_protocol::Firmware;

use crate::lighting::{AlienwareKeys, Lighting, RazerRows, ALIENWARE_ZONES};

/// Matrix position without an LED.
///
/// Internal: the sentinel only serves to write and read [`Layout::matrix`], and
/// everything leaving this module has already been through it —
/// [`Layout::lit_count`] leaves it out of the count, [`Layout::at`] turns it into
/// `None`. A caller reading `matrix` directly would need it; none does, and the raw
/// value `u16::MAX` is written in the field's documentation for whoever would.
pub(crate) const EMPTY: u16 = u16::MAX;

/// [`Key::scancode`] of a key that sends nothing: no make code is 0.
pub const NO_SCANCODE: u16 = 0;

/// A key: its LED, its scancode, and its physical rectangle.
///
/// No legend: what is engraved depends on the layout variant, and the operating
/// system names a scancode in the layout it uses (see `keys::label` in the desktop
/// application).
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
/// Only [`Key::index`] comes from the hardware survey, described in
/// `docs/protocol/deathstalker-v2-pro.md` §6. [`Key::scancode`] follows from the
/// position, as the rectangle does.
pub struct Key {
    /// LED index, as it appears in [`Layout::matrix`].
    pub index: u16,
    /// What the keyboard sends for this key, in PS/2 set 1 as Windows reports it:
    /// the make code, `0xE0` in the high byte for an extended key (`0xE01D`, right
    /// Ctrl), `0xE11D` for Pause. It names the physical key whatever its legend —
    /// `0x11` is engraved Z on this AZERTY keyboard and W on a QWERTY one — and it is
    /// how a key press finds its LED. Both arms of the ISO Enter are `0x1C`.
    ///
    /// [`NO_SCANCODE`] for Fn, which the firmware handles without sending anything.
    pub scancode: u16,
    /// Left edge, in keyboard pitch units.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

/// An effect a firmware runs, as a layout declares it.
///
/// **How many colours it takes is part of the effect, not of the device.** A
/// spectrum paints its own palette and would ignore anything given to it; a
/// steady colour shows nothing without one — which is how the same keyboard
/// goes dark. The gallery asks for exactly as many as are used, so nobody picks
/// a colour that changes nothing.
pub struct FirmwareEffect {
    /// The id the gallery uses, `hardware:wave`.
    pub id: &'static str,
    /// How many colours it paints with: none, one, or two.
    pub colours: u8,
}

/// Which HID entry of a device carries the lighting.
///
/// Two makers, two habits, and picking the wrong entry gives a valid handle on
/// which every write is lost — so the rule belongs to the layout rather than to
/// the code that opens devices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Port {
    /// One interface of a composite device: how a peripheral separates its
    /// lighting from its keys.
    Interface(u8),
    /// One collection of an interface, by usage page and usage: a laptop
    /// keyboard puts several on the same interface, and only one of them lights.
    Collection { usage_page: u16, usage: u16 },
}

/// Layout of a device: identification, transport and matrix.
pub struct Layout {
    pub name: &'static str,
    pub vid: u16,
    pub pid: u16,
    /// Where the lighting is, in this device's HID enumeration.
    pub port: Port,
    /// The family whose reports it speaks — see [`crate::lighting`]. A device of
    /// a family already written is this line and the data around it, nothing
    /// more.
    pub lighting: &'static dyn Lighting,
    /// Firmware this layout was surveyed against, when the device says one.
    ///
    /// `None` where no command is known to read it — the Alienware keyboard, so
    /// far. A version that cannot be read is not a version of zero: the
    /// inspection has nothing to compare, and says nothing rather than warning
    /// about a difference it invented.
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
    pub surveyed_firmware: Option<Firmware>,
    /// The effects this **firmware** runs by itself, which the gallery offers
    /// for this device and no other.
    ///
    /// *Off* is not in it and is offered everywhere: a device whose firmware
    /// draws nothing still goes dark, on a frame of black. Everything else is a
    /// mode of the device, and offering one the firmware does not know would be
    /// letting someone choose an effect that never runs.
    pub firmware_effects: &'static [FirmwareEffect],
    pub rows: u8,
    pub cols: u8,
    /// **The address the device gives each position**, row by row; `u16::MAX`
    /// where no LED sits.
    ///
    /// A position is where a key is — what an effect paints and what the
    /// simulator draws — and an address is what the protocol calls it. On the
    /// Razer the two coincide; on the Alienware the device counts from one; a
    /// device numbering its keys in any other order is this table and nothing
    /// else. **Never arithmetic in a protocol module**: an offset that holds for
    /// one model is a bug waiting for the next.
    pub matrix: &'static [u16],
    /// Name and geometry of each occupied position, in position order.
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
    pub fn is_lighting_interface(
        &self,
        vid: u16,
        pid: u16,
        interface: i32,
        usage_page: u16,
        usage: u16,
    ) -> bool {
        if vid != self.vid || pid != self.pid {
            return false;
        }
        match self.port {
            Port::Interface(number) => interface == i32::from(number),
            Port::Collection {
                usage_page: page,
                usage: which,
            } => usage_page == page && usage == which,
        }
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

    /// The position at `row`, `col` — that is, where it sits in a frame — or
    /// `None` when no LED does.
    pub fn at(&self, row: u8, col: u8) -> Option<u16> {
        let i = row as usize * self.cols as usize + col as usize;
        let lit = matches!(self.matrix.get(i), Some(&v) if v != EMPTY);
        lit.then_some(i as u16)
    }

    /// What the device calls this position, for a protocol that names its keys.
    pub fn address(&self, position: u16) -> Option<u16> {
        match self.matrix.get(usize::from(position)) {
            Some(&v) if v != EMPTY => Some(v),
            _ => None,
        }
    }

    /// Key sitting at a given position.
    ///
    /// Every lit position has one: that is the invariant the
    /// `every_lit_position_has_a_key` test checks.
    pub fn key(&self, position: u16) -> Option<&'static Key> {
        let keys: &'static [Key] = self.keys;
        keys.iter().find(|k| k.index == position)
    }
}

/// Standard key, 1 u × 1 u.
const fn k(index: u16, scancode: u16, x: f32, y: f32) -> Key {
    Key {
        index,
        scancode,
        x,
        y,
        w: 1.0,
        h: 1.0,
    }
}

/// Wide key, one row high.
const fn kw(index: u16, scancode: u16, x: f32, y: f32, w: f32) -> Key {
    Key {
        index,
        scancode,
        x,
        y,
        w,
        h: 1.0,
    }
}

/// Key spanning two rows — the numeric keypad `+` and Enter.
const fn kh(index: u16, scancode: u16, x: f32, y: f32, w: f32, h: f32) -> Key {
    Key {
        index,
        scancode,
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
    name: "Razer DeathStalker V2 Pro",
    vid: 0x1532,
    pid: 0x0292,
    port: Port::Interface(3),
    lighting: &RazerRows,
    // `01 05`, read back through `0x00`/`0x81` on 12/09/2026 — the version the
    // device also declares elsewhere. See §8 of the survey.
    surveyed_firmware: Some(Firmware { major: 1, minor: 5 }),
    // Surveyed by reading each identifier back, see §8 of the survey; what each
    // one sends is in `RazerRows::firmware_effect`. Neither takes a colour:
    // Static and Breathing, which do, are not offered — the survey never
    // established where their colour sits in a read-back.
    firmware_effects: &[
        FirmwareEffect {
            id: "hardware:spectrumCycle",
            colours: 0,
        },
        FirmwareEffect {
            id: "hardware:wave",
            colours: 0,
        },
    ],
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
        // Row 0 — Esc, F1–F12 in three blocks of four, Print Screen, Scroll Lock, Pause. (16)
        k(  0, 0x01,    0.0,  0.0),
        k(  2, 0x3B,    2.0,  0.0), k(  3, 0x3C,  3.0, 0.0), k(  4, 0x3D,  4.0, 0.0), k(  5, 0x3E,  5.0, 0.0),
        k(  6, 0x3F,    6.5,  0.0), k(  7, 0x40,  7.5, 0.0), k(  8, 0x41,  8.5, 0.0), k(  9, 0x42,  9.5, 0.0),
        k( 10, 0x43,   11.0,  0.0), k( 11, 0x44, 12.0, 0.0), k( 12, 0x57, 13.0, 0.0), k( 13, 0x58, 14.0, 0.0),
        k( 14, 0xE037, 15.25, 0.0), k( 15, 0x46, 16.25, 0.0), k( 16, 0xE11D, 17.25, 0.0),

        // Row 1 — the key left of 1, 1–0, the two after, 2 u Backspace; Insert,
        // Home, Page Up; Num Lock, keypad / * −. (21)
        k( 22, 0x29,    0.0,  1.5), k( 23, 0x02,  1.0, 1.5), k( 24, 0x03,  2.0, 1.5),
        k( 25, 0x04,    3.0,  1.5), k( 26, 0x05,  4.0, 1.5), k( 27, 0x06,  5.0, 1.5),
        k( 28, 0x07,    6.0,  1.5), k( 29, 0x08,  7.0, 1.5), k( 30, 0x09,  8.0, 1.5),
        k( 31, 0x0A,    9.0,  1.5), k( 32, 0x0B, 10.0, 1.5), k( 33, 0x0C, 11.0, 1.5),
        k( 34, 0x0D,   12.0,  1.5), kw(35, 0x0E, 13.0, 1.5, 2.0),
        k( 36, 0xE052, 15.25, 1.5), k( 37, 0xE047, 16.25, 1.5), k( 38, 0xE049, 17.25, 1.5),
        k( 39, 0x45,   18.5,  1.5), k( 40, 0xE035, 19.5, 1.5), k( 41, 0x37, 20.5, 1.5),
        k( 42, 0x4A,   21.5,  1.5),

        // Row 2 — 1.5 u Tab, the twelve keys of the top row, TOP of the L-shaped
        // Enter; Delete, End, Page Down; keypad 7 8 9 +. (21)
        kw(44, 0x0F,    0.0,  2.5, 1.5),
        k( 45, 0x10,    1.5,  2.5), k( 46, 0x11,  2.5, 2.5), k( 47, 0x12,  3.5, 2.5),
        k( 48, 0x13,    4.5,  2.5), k( 49, 0x14,  5.5, 2.5), k( 50, 0x15,  6.5, 2.5),
        k( 51, 0x16,    7.5,  2.5), k( 52, 0x17,  8.5, 2.5), k( 53, 0x18,  9.5, 2.5),
        k( 54, 0x19,   10.5,  2.5), k( 55, 0x1A, 11.5, 2.5), k( 56, 0x1B, 12.5, 2.5),
        kw(57, 0x1C,   13.5,  2.5, 1.5),
        k( 58, 0xE053, 15.25, 2.5), k( 59, 0xE04F, 16.25, 2.5), k( 60, 0xE051, 17.25, 2.5),
        k( 61, 0x47,   18.5,  2.5), k( 62, 0x48, 19.5, 2.5), k( 63, 0x49, 20.5, 2.5),
        kh(64, 0x4E,   21.5,  2.5, 1.0, 2.0),

        // Row 3 — 1.75 u Caps Lock, the twelve keys of the home row, BOTTOM of the
        // L-shaped Enter; keypad 4 5 6. (17)
        kw(66, 0x3A,    0.0,  3.5, 1.75),
        k( 67, 0x1E,    1.75, 3.5), k( 68, 0x1F,  2.75, 3.5), k( 69, 0x20,  3.75, 3.5),
        k( 70, 0x21,    4.75, 3.5), k( 71, 0x22,  5.75, 3.5), k( 72, 0x23,  6.75, 3.5),
        k( 73, 0x24,    7.75, 3.5), k( 74, 0x25,  8.75, 3.5), k( 75, 0x26,  9.75, 3.5),
        k( 76, 0x27,   10.75, 3.5), k( 77, 0x28, 11.75, 3.5), k( 78, 0x2B, 12.75, 3.5),
        kw(79, 0x1C,   13.75, 3.5, 1.25),
        k( 83, 0x4B,   18.5,  3.5), k( 84, 0x4C, 19.5, 3.5), k( 85, 0x4D, 20.5, 3.5),

        // Row 4 — 1.25 u left Shift, the ISO key, the ten keys of the bottom row,
        // right Shift; ↑; keypad 1 2 3 Enter. (18)
        kw(88, 0x2A,    0.0,  4.5, 1.25),
        k( 89, 0x56,    1.25, 4.5),
        k( 90, 0x2C,    2.25, 4.5), k( 91, 0x2D,  3.25, 4.5), k( 92, 0x2E,  4.25, 4.5),
        k( 93, 0x2F,    5.25, 4.5), k( 94, 0x30,  6.25, 4.5), k( 95, 0x31,  7.25, 4.5),
        k( 96, 0x32,    8.25, 4.5), k( 97, 0x33,  9.25, 4.5), k( 98, 0x34, 10.25, 4.5),
        k( 99, 0x35,   11.25, 4.5),
        kw(101, 0x36,  12.25, 4.5, 2.75),
        k(103, 0xE048, 16.25, 4.5),
        k(105, 0x4F,   18.5,  4.5), k(106, 0x50, 19.5, 4.5), k(107, 0x51, 20.5, 4.5),
        kh(108, 0xE01C, 21.5, 4.5, 1.0, 2.0),

        // Row 5 — left Ctrl, Windows, Alt, 6.25 u Space, AltGr, Fn, Menu, right
        // Ctrl; ← ↓ →; keypad 0 and decimal point. (13)
        kw(110, 0x1D,   0.0,  5.5, 1.25),
        kw(111, 0xE05B, 1.25, 5.5, 1.25),
        kw(112, 0x38,   2.5,  5.5, 1.25),
        kw(116, 0x39,   3.75, 5.5, 6.25),
        kw(120, 0xE038, 10.0, 5.5, 1.25),
        kw(121, NO_SCANCODE, 11.25, 5.5, 1.25),
        kw(122, 0xE05D, 12.5, 5.5, 1.25),
        kw(123, 0xE01D, 13.75, 5.5, 1.25),
        k(124, 0xE04B, 15.25, 5.5), k(125, 0xE050, 16.25, 5.5), k(126, 0xE04D, 17.25, 5.5),
        kw(128, 0x52,  18.5,  5.5, 2.0),
        k(129, 0x53,   20.5,  5.5),
    ],
};

/// Alienware m18 R1, the keyboard built into the laptop.
///
/// Its lighting is **one collection of one interface**, not an interface of its
/// own: the same interface also carries the keys someone types. It speaks
/// 64-byte feature reports — see `docs/protocol/alienware-m18-r1.md` and
/// [`candeo_protocol::alienware`].
///
/// # The grid, and what it does not carry
///
/// Seven bands of twenty positions, one per row, the numeric keypad continuing
/// each row rather than sitting in a block of its own. A frame covers the 140
/// cells, of which 103 light.
///
/// **The numbers below are positions, not the device's indexes**, which start at
/// one: the shift belongs to [`crate::lighting::AlienwareKeys`]. Here as for
/// every other device, `matrix` and [`Key::index`] are where a key sits, which
/// is what an effect paints.
///
/// The empty ones were found the only way there is: **written alone, and the
/// keyboard looked at**. Some are the cell a wide key leaves behind — Tab is 41,
/// the key after it 43 — and the others are spare entries of the maker's own
/// table, which addresses indexes that light nothing, index `0` included.
/// The L-shaped Enter has **one** LED, counted in the lower row.
///
/// The drawing is a transcription of the maker's layout picture, aligned by eye
/// on the lit keyboard: 20.5 u wide, main block from 0 to 15 u, the arrows on
/// the bottom row at 13.25 u with ↑ one row above, numeric keypad from 16.5 u.
/// Exact as a convention, not as a survey — like the DeathStalker's.
pub static ALIENWARE_M18_R1: Layout = Layout {
    name: "Alienware m18 R1",
    vid: 0x0d62,
    pid: 0xaab0,
    port: Port::Collection {
        usage_page: 0xff89,
        usage: 0x00cc,
    },
    lighting: &AlienwareKeys,
    // No command is known to read a version from this device yet.
    surveyed_firmware: None,
    // Sixteen kinds answer; these seven are the ones that show something,
    // watched one by one on the keyboard on 18/09/2026. The others stop
    // whatever was running and draw nothing — offering them would be offering
    // an effect that never starts — and `0c` is the dark one *Off* already is.
    //
    // Which ones take a colour was read the same way: given red, three of them
    // showed red and four kept their own palette.
    firmware_effects: &[
        // a steady colour
        FirmwareEffect {
            id: "hardware:m18-01",
            colours: 1,
        },
        // that colour, throbbing
        FirmwareEffect {
            id: "hardware:m18-02",
            colours: 1,
        },
        // a rainbow crossing the keys
        FirmwareEffect {
            id: "hardware:m18-03",
            colours: 0,
        },
        // colours following one another through black
        FirmwareEffect {
            id: "hardware:m18-08",
            colours: 0,
        },
        // the same without going dark
        FirmwareEffect {
            id: "hardware:m18-09",
            colours: 0,
        },
        // a lit band sweeping across and back
        FirmwareEffect {
            id: "hardware:m18-0a",
            colours: 1,
        },
        // hues cycling, faster
        FirmwareEffect {
            id: "hardware:m18-0e",
            colours: 0,
        },
    ],
    rows: 7,
    cols: 20,
    #[rustfmt::skip]
    matrix: &[
        // The device's own numbering, which starts at one — positions are where
        // these sit, and the two must not be confused.
        //     0      1      2      3      4      5      6      7      8      9     10     11     12     13     14     15     16     17     18     19
               1,     2,     3,     4,     5,     6,     7,     8,     9,    10,    11,    12,    13,    14,    15,    16,    17,    18,    19,    20,
        // Backspace is 36, not 34: two addresses in between drive nothing. An
        // offset would have missed it; a table cannot.
              21,    22,    23,    24,    25,    26,    27,    28,    29,    30,    31,    32,    33,    36, EMPTY, EMPTY,    37,    38,    39,    40,
              41, EMPTY,    43,    44,    45,    46,    47,    48,    49,    50,    51,    52,    53,    54, EMPTY, EMPTY,    57,    58,    59, EMPTY,
           EMPTY,    62,    63,    64,    65,    66,    67,    68,    69,    70,    71,    72,    73,    74,    75, EMPTY,    77,    78,    79,    80,
           EMPTY,    82,    83,    84,    85,    86,    87,    88,    89,    90,    91,    92,    93, EMPTY,    95, EMPTY,    97,    98,    99, EMPTY,
             101,   102, EMPTY,   104,   105, EMPTY, EMPTY,   108, EMPTY,   110, EMPTY,   112,   113, EMPTY,   115, EMPTY, EMPTY,   118,   119,   120,
           EMPTY, EMPTY, EMPTY, EMPTY, EMPTY, EMPTY, EMPTY, EMPTY, EMPTY, EMPTY, EMPTY, EMPTY, EMPTY,   134,   135,   136, EMPTY, EMPTY, EMPTY, EMPTY,
    ],
    #[rustfmt::skip]
    keys: &[
        // Row 0 — Esc, F1–F12, Print Screen, End, Delete; the four media keys
        // above the keypad. (20)
        k(  0, 0x01,    0.0,  0.0),
        k(  1, 0x3B,    1.0,  0.0), k(  2, 0x3C,  2.0, 0.0), k(  3, 0x3D,  3.0, 0.0),
        k(  4, 0x3E,    4.0,  0.0), k(  5, 0x3F,  5.0, 0.0), k(  6, 0x40,  6.0, 0.0),
        k(  7, 0x41,    7.0,  0.0), k(  8, 0x42,  8.0, 0.0), k(  9, 0x43,  9.0, 0.0),
        k( 10, 0x44,   10.0,  0.0), k( 11, 0x57, 11.0, 0.0), k( 12, 0x58, 12.0, 0.0),
        k( 13, 0xE037, 13.0,  0.0), k( 14, 0xE04F, 14.0, 0.0), k( 15, 0xE053, 15.0, 0.0),
        k( 16, 0xE020, 16.5,  0.0), k( 17, 0xE02E, 17.5, 0.0), k( 18, 0xE030, 18.5, 0.0),
        k( 19, NO_SCANCODE, 19.5, 0.0),

        // Row 1 — the key left of 1, the digits, 2 u Backspace; Num Lock and the
        // keypad's / * −. (18)
        k( 20, 0x29,    0.0,  1.0), k( 21, 0x02,  1.0, 1.0), k( 22, 0x03,  2.0, 1.0),
        k( 23, 0x04,    3.0,  1.0), k( 24, 0x05,  4.0, 1.0), k( 25, 0x06,  5.0, 1.0),
        k( 26, 0x07,    6.0,  1.0), k( 27, 0x08,  7.0, 1.0), k( 28, 0x09,  8.0, 1.0),
        k( 29, 0x0A,    9.0,  1.0), k( 30, 0x0B, 10.0, 1.0), k( 31, 0x0C, 11.0, 1.0),
        k( 32, 0x0D,   12.0,  1.0), kw(33, 0x0E, 13.0, 1.0, 2.0),
        k( 36, 0x45,   16.5,  1.0), k( 37, 0xE035, 17.5, 1.0), k( 38, 0x37, 18.5, 1.0),
        k( 39, 0x4A,   19.5,  1.0),

        // Row 2 — 1.5 u Tab, the top letter row; keypad 7 8 9. (16)
        kw(40, 0x0F,    0.0,  2.0, 1.5),
        k( 42, 0x10,    1.5,  2.0), k( 43, 0x11,  2.5, 2.0), k( 44, 0x12,  3.5, 2.0),
        k( 45, 0x13,    4.5,  2.0), k( 46, 0x14,  5.5, 2.0), k( 47, 0x15,  6.5, 2.0),
        k( 48, 0x16,    7.5,  2.0), k( 49, 0x17,  8.5, 2.0), k( 50, 0x18,  9.5, 2.0),
        k( 51, 0x19,   10.5,  2.0), k( 52, 0x1A, 11.5, 2.0), k( 53, 0x1B, 12.5, 2.0),
        k( 56, 0x47,   16.5,  2.0), k( 57, 0x48, 17.5, 2.0), k( 58, 0x49, 18.5, 2.0),

        // Row 3 — 1.75 u Caps Lock, the home row, the L-shaped Enter and its one
        // LED; keypad 4 5 6 and the two-row +. (18)
        kw(61, 0x3A,    0.0,  3.0, 1.75),
        k( 62, 0x1E,    1.75, 3.0), k( 63, 0x1F,  2.75, 3.0), k( 64, 0x20,  3.75, 3.0),
        k( 65, 0x21,    4.75, 3.0), k( 66, 0x22,  5.75, 3.0), k( 67, 0x23,  6.75, 3.0),
        k( 68, 0x24,    7.75, 3.0), k( 69, 0x25,  8.75, 3.0), k( 70, 0x26,  9.75, 3.0),
        k( 71, 0x27,   10.75, 3.0), k( 72, 0x28, 11.75, 3.0), k( 73, 0x2B, 12.75, 3.0),
        kh(74, 0x1C,   13.75, 2.0, 1.25, 2.0),
        k( 76, 0x4B,   16.5,  3.0), k( 77, 0x4C, 17.5, 3.0), k( 78, 0x4D, 18.5, 3.0),
        kh(79, 0x4E,   19.5,  2.0, 1.0, 2.0),

        // Row 4 — 1.25 u left Shift, the ISO key, the bottom letter row, 2.75 u
        // right Shift; ↑; keypad 1 2 3 and the two-row Enter. (17)
        kw(81, 0x2A,    0.0,  4.0, 1.25),
        k( 82, 0x56,    1.25, 4.0),
        k( 83, 0x2C,    2.25, 4.0), k( 84, 0x2D,  3.25, 4.0), k( 85, 0x2E,  4.25, 4.0),
        k( 86, 0x2F,    5.25, 4.0), k( 87, 0x30,  6.25, 4.0), k( 88, 0x31,  7.25, 4.0),
        k( 89, 0x32,    8.25, 4.0), k( 90, 0x33,  9.25, 4.0), k( 91, 0x34, 10.25, 4.0),
        k( 92, 0x35,   11.25, 4.0),
        kw(94, 0x36,   12.25, 4.0, 2.0),
        k(114, 0xE048, 14.5, 4.0),
        k( 96, 0x4F,   16.5,  4.0), k( 97, 0x50, 17.5, 4.0), k( 98, 0x51, 18.5, 4.0),
        kh(119, 0xE01C, 19.5, 4.0, 1.0, 2.0),

        // Row 5 — left Ctrl, Fn, Windows, Alt, 6.25 u Space, AltGr, the locked
        // Windows key, right Ctrl; ← ↓ →; keypad 0 and its decimal point. (14)
        kw(100, 0x1D,        0.0,  5.0, 1.25),
        k( 101, NO_SCANCODE, 1.25, 5.0),
        k( 103, 0xE05B,      2.25, 5.0),
        k( 104, 0x38,        3.25, 5.0),
        kw(107, 0x39,        4.25, 5.0, 6.25),
        kw(109, 0xE038,     10.5,  5.0, 1.25),
        k( 111, NO_SCANCODE, 11.75, 5.0),
        kw(112, 0xE01D,     12.75, 5.0, 0.75),
        k( 133, 0xE04B,     13.5,  5.0), k(134, 0xE050, 14.5, 5.0), k(135, 0xE04D, 15.5, 5.0),
        kw(117, 0x52,       16.5,  5.0, 2.0),
        k( 118, 0x53,       18.5,  5.0),
    ],
};

/// The lights around that keyboard: the ring at the rear, the logo on the lid,
/// the power button. Surveyed in `docs/protocol/alienware-m18-r1.md` §2.
///
/// A second device, not a second matrix of the same one: another product id,
/// another report shape, another family. One row of four cells, because that is
/// all there is — the drawing below places them where they are on the machine,
/// which is what tells someone which light a colour will reach.
///
/// **The power button is not here, and that is the finding.** Its own firmware
/// pulses it — white on mains, green on battery, more slowly there — and takes
/// it back within the second, verified from the application on 18/09/2026.
/// Offering a light that never keeps what it is given would be a lie on screen,
/// and the machine already does the useful thing without anyone running.
pub static ALIENWARE_M18_R1_ZONES: Layout = Layout {
    name: "Alienware m18 R1 zones",
    vid: 0x187c,
    pid: 0x0551,
    port: Port::Collection {
        usage_page: 0xff00,
        usage: 0x0001,
    },
    lighting: &ALIENWARE_ZONES,
    // No command is known to read a version from this device yet.
    surveyed_firmware: None,
    // It runs no effect of its own that anyone has surveyed.
    firmware_effects: &[],
    rows: 1,
    cols: 3,
    // The zone ids the device answers to. `3` lights nothing on this machine and
    // `4`, the power button, never keeps what it is given.
    matrix: &[0, 1, 2],
    #[rustfmt::skip]
    keys: &[
        // A top-down view of the machine: the lid above, the ring across the
        // back. No scancode — nothing here is a key.
        kh(0, NO_SCANCODE, 0.0, 1.5, 8.0, 0.5),   // ring, upper half
        kh(1, NO_SCANCODE, 0.0, 2.0, 8.0, 0.5),   // ring, lower half
        kh(2, NO_SCANCODE, 3.0, 0.0, 2.0, 1.0),   // logo on the lid
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    /// A scancode designates one key, so that a key press lights one LED. The two
    /// LEDs of the ISO Enter are the one exception, and a real one: one key, two
    /// arms. Fn is the only key that sends nothing.
    #[test]
    fn scancodes_name_each_key_once_except_the_enter_arms() {
        let mut seen = std::collections::BTreeMap::<u16, Vec<u16>>::new();
        for key in DEATHSTALKER_V2_PRO.keys {
            seen.entry(key.scancode).or_default().push(key.index);
        }
        let shared: Vec<_> = seen.iter().filter(|(_, i)| i.len() > 1).collect();
        assert_eq!(shared, [(&0x1C, &vec![57, 79])]);
        assert_eq!(seen.get(&NO_SCANCODE), Some(&vec![121]), "Fn only");
        assert!(
            seen.keys()
                .all(|&s| matches!(s >> 8, 0x00 | 0xE0) || s == 0xE11D),
            "a make code, extended or not, or Pause"
        );
    }

    /// The enumeration surveyed on 12/09/2026, entry by entry: only `MI_03` carries
    /// the lighting. The `-1` entry, on the same VID and PID, must be neither
    /// opened nor counted as present.
    #[test]
    fn only_the_lighting_interface_is_kept() {
        let l = &DEATHSTALKER_V2_PRO;
        let kept: Vec<i32> = [-1, 0, 1, 2, 3]
            .into_iter()
            .filter(|&i| l.is_lighting_interface(0x1532, 0x0292, i, 0x0001, 0x0006))
            .collect();
        assert_eq!(kept, vec![3]);
        assert!(
            !l.is_lighting_interface(0x1532, 0x0290, 3, 0x0001, 0x0006),
            "other product"
        );
    }

    /// The laptop keyboard puts four collections on one interface, and only the
    /// one carrying report `0xcc` lights. The interface number says nothing here.
    #[test]
    fn a_collection_is_what_names_the_laptop_lighting() {
        let l = &ALIENWARE_M18_R1;
        let collections = [
            (0x0001, 0x0006),
            (0xff89, 0x0010),
            (0xff89, 0x00cc),
            (0x000c, 0x0001),
        ];
        let kept: Vec<(u16, u16)> = collections
            .into_iter()
            .filter(|&(page, usage)| l.is_lighting_interface(0x0d62, 0xaab0, 0, page, usage))
            .collect();
        assert_eq!(kept, vec![(0xff89, 0x00cc)]);
        assert!(
            !l.is_lighting_interface(0x1532, 0x0292, 0, 0xff89, 0x00cc),
            "another device, same collection"
        );
    }

    /// Keys the laptop grid carries.
    const KEYS: usize = 103;

    /// The grid the survey established: seven rows of twenty, and the holes
    /// where a key is wider than a cell or where the device lights nothing.
    #[test]
    fn the_laptop_grid_matches_the_survey() {
        let l = &ALIENWARE_M18_R1;
        assert_eq!(l.matrix.len(), l.led_count());
        assert_eq!(l.led_count(), 140);
        assert_eq!(l.lit_count(), KEYS);
        assert_eq!(l.keys.len(), KEYS);
        assert_eq!(l.at(0, 0), Some(0), "Esc");
        assert_eq!(l.at(2, 0), Some(40), "Tab");
        assert_eq!(l.at(2, 1), None, "Tab is wider than its cell");
        assert_eq!(l.at(2, 2), Some(42), "A");
        assert_eq!(l.at(3, 14), Some(74), "Enter");
        assert_eq!(l.at(5, 7), Some(107), "Space");
        assert_eq!(l.at(5, 17), Some(117), "the keypad zero");
    }

    /// **A position is not an address.** This device counts its keys from one,
    /// and the layout is where that is said — never an offset in a protocol
    /// module, which would hold for this model and break on the next.
    #[test]
    fn the_laptop_addresses_are_the_device_s_own_numbers() {
        let l = &ALIENWARE_M18_R1;
        assert_eq!(l.address(0), Some(1), "Esc is the device's key 1");
        assert_eq!(l.address(42), Some(43), "A");
        assert_eq!(l.address(107), Some(108), "Space");
        assert_eq!(l.address(41), None, "the cell Tab leaves behind");
        assert_eq!(l.address(139), None, "past the last key");
        // And where a device numbers its keys by position, the table says so too.
        assert_eq!(DEATHSTALKER_V2_PRO.address(0), Some(0));
        assert_eq!(DEATHSTALKER_V2_PRO.address(22), Some(22));
    }

    /// Every lit cell of the laptop grid has one key, each index appears once,
    /// and the drawing stays inside the keyboard without two keys overlapping.
    #[test]
    fn the_laptop_drawing_covers_each_lit_cell_once() {
        let l = &ALIENWARE_M18_R1;
        let mut seen = std::collections::BTreeSet::new();
        for key in l.keys {
            assert!(seen.insert(key.index), "index {} twice", key.index);
            assert!(
                l.address(key.index).is_some(),
                "{} is not a lit position",
                key.index
            );
            assert!(
                key.x >= 0.0 && key.x + key.w <= 20.5,
                "{} sticks out",
                key.index
            );
            assert!(
                key.y >= 0.0 && key.y + key.h <= 7.0,
                "{} sticks out",
                key.index
            );
        }
        assert_eq!(seen.len(), KEYS);
        for a in l.keys {
            for b in l.keys {
                if a.index >= b.index {
                    continue;
                }
                let apart =
                    a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
                assert!(apart, "{} and {} overlap", a.index, b.index);
            }
        }
    }

    /// The keys someone presses reach one LED each, as on any keyboard: the
    /// exceptions here are the two Fn-like keys, which send nothing.
    #[test]
    fn the_laptop_scancodes_name_each_key_once() {
        let mut seen = std::collections::BTreeMap::<u16, Vec<u16>>::new();
        for key in ALIENWARE_M18_R1.keys {
            seen.entry(key.scancode).or_default().push(key.index);
        }
        let shared: Vec<_> = seen
            .iter()
            .filter(|(code, indexes)| **code != NO_SCANCODE && indexes.len() > 1)
            .collect();
        assert!(shared.is_empty(), "shared scancodes: {shared:?}");
        assert_eq!(seen.get(&NO_SCANCODE).map(Vec::len), Some(3));
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
    /// Both ways: every lit position has a geometry, and no key describes a
    /// position that does not exist.
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
                assert!(key.w > 0.0 && key.h > 0.0, "index {index} has no area");
            }
        }

        for key in l.keys {
            assert!(
                l.matrix.contains(&key.index),
                "index {} missing from the matrix",
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
                assert!(disjoint, "indices {} and {} overlap", a.index, b.index);
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

        assert_eq!(upper.scancode, 0x1C);
        assert_eq!(lower.scancode, 0x1C);
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
        assert_eq!(space.scancode, 0x39);
        assert_eq!(space.w, 6.25);
        assert_eq!(l.keys.iter().filter(|k| k.scancode == 0x39).count(), 1);
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
