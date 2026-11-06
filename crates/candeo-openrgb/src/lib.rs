//! Drives, through OpenRGB, the devices Candeo has no definition for
//! (`docs/design/openrgb-output.md`, #368). A proof of concept: nothing in the
//! application uses it yet.
//!
//! Two halves:
//!
//! - **The client**: [`wire`] frames the packets, [`controller`] reads what a
//!   controller is, [`client`] holds the connection. Written from OpenRGB's
//!   public documentation only, `Documentation/OpenRGBSDK.md` and
//!   `Documentation/RGBControllerAPI.md` at `release_1.1`, and from no OpenRGB
//!   source nor GPL-2.0 crate (§6.6): OpenRGB's code cannot be combined with
//!   Candeo's GPL-3.0-only (§1). Six packets can be encoded; the encoder
//!   refuses every other id.
//! - **The OpenRGB Candeo starts**: [`discovery`] finds the installed one and
//!   reads its version, [`config`] writes its private configuration with the
//!   detectors of Candeo's native devices off, [`launch`] decides whether to
//!   start it and with which arguments, [`process`] ties the child to Candeo's
//!   lifetime (§6.2 to §6.5).
//!
//! [`guard`] keeps native first (§6.8), [`identity`] keeps a unit's serial and
//! location out of anything that prints (§6.9), [`model`] turns a controller
//! into zones and lights and picks its direct mode (§6.7).

pub mod client;
pub mod config;
pub mod controller;
pub mod discovery;
pub mod guard;
pub mod identity;
pub mod launch;
pub mod model;
pub mod process;
pub mod wire;

mod reader;

#[cfg(test)]
mod fixtures;

/// A colour, as OpenRGB carries it: `0x00BBGGRR` (RGBControllerAPI.md), so
/// red, green, blue and a zero byte on the wire, little endian (§6.6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// The four bytes on the wire.
    pub(crate) const fn to_wire(self) -> [u8; 4] {
        [self.r, self.g, self.b, 0]
    }

    /// From the four bytes on the wire; the fourth is padding.
    pub(crate) const fn from_wire(bytes: [u8; 4]) -> Self {
        Self::new(bytes[0], bytes[1], bytes[2])
    }
}
