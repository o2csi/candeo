//! What a device is: its identity, its grid and the lights on it, as a
//! definition file fills them (`crate::definition`). No device is written here.

use candeo_protocol::Firmware;

use crate::lighting::Lighting;

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
    /// How it is drawn inside that rectangle. The rectangle stays what an effect
    /// measures with: the shape changes the drawing, not the geometry.
    pub shape: Shape,
}

/// How a light is drawn inside its rectangle (`docs/design/studio.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// The rectangle itself: a key.
    Rect,
    /// A disc, or an ellipse, inscribed in the rectangle.
    Disc,
    /// The upper half of a rounded ring, a thick stroke inside the rectangle.
    ArchUp,
    /// Its lower half.
    ArchDown,
}

/// A part of a device that lights nothing — a lid, a base, a port — drawn
/// faintly under its lights, in the same units. A keyboard has none: its keys
/// are its outline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outline {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Corner radius.
    pub r: f32,
}

/// What a device's lights **are**.
///
/// Not a taxonomy of models: two values, because two things differ in what can
/// be asked of them. A key can be pressed, so an effect reading presses has
/// something to read; a zone cannot, so offering it that effect would offer one
/// that never sees anything. Everything else — where the lights sit, how many,
/// what shape — is the matrix and the geometry, which describe both alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lights {
    /// Keys, which can be pressed.
    Keys,
    /// Zones: a ring, a logo, a strip. Lit, never typed on.
    Zones,
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
    /// What its lights are — see [`Lights`]. The gallery reads it to stop
    /// offering, to a surface nobody types on, an effect that reads key presses.
    pub lights: Lights,
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
    /// What the simulator draws under the lights. Empty for a keyboard.
    pub outline: &'static [Outline],
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
