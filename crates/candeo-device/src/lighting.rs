//! What speaking to a **family** of devices takes.
//!
//! A protocol is not data: its framing, its report ids and its checksums are
//! rules, and no table of parameters guesses them. What can be data is
//! everything else — identity, interface, matrix, geometry — and that is where
//! the line is drawn here.
//!
//! - **Another device of a known family**: a [`Layout`] and nothing else.
//! - **A family nobody has surveyed yet**: one implementation of [`Lighting`],
//!   in a file of its own, and a `Layout` naming it.
//!
//! So `Keyboard` knows how to send bytes and check a refusal, and knows nothing
//! about who it is talking to (#34).

use candeo_protocol::{alienware, CommandId, Effect, Report, Rgb};

use crate::layout::{Layout, EMPTY};

/// One report on its way to a device.
pub struct Outgoing {
    /// The buffer as `HidD_SetFeature` wants it, report id included.
    pub bytes: Vec<u8>,
    /// The command it carries, when the device can be asked about one: that is
    /// what lets an inspection refuse to send it. `None` where a family has no
    /// such notion.
    pub command: Option<CommandId>,
}

impl Outgoing {
    fn plain(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            command: None,
        }
    }
}

/// How one family of devices is driven.
pub trait Lighting: Sync {
    /// The reports carrying one frame of the whole matrix.
    ///
    /// `frame` covers every cell, the empty ones included: a family that
    /// addresses keys one by one leaves them out itself.
    fn frame(&self, layout: &Layout, frame: &[Rgb]) -> Vec<Outgoing>;

    /// The reports setting overall brightness.
    ///
    /// A list, because a family may need more than one: the m18 R1 takes its
    /// level only after a frame has been opened and closed.
    fn brightness(&self, level: u8) -> Vec<Outgoing>;

    /// A segment of one row, for a family that addresses the matrix that way.
    /// `None` where it does not, and the command then says so rather than
    /// writing something else.
    fn row(&self, _row: u8, _col_start: u8, _colours: &[Rgb]) -> Option<Outgoing> {
        None
    }

    /// The report handing the lighting back to the firmware, for a family whose
    /// firmware draws by itself. `None` when it does not, and *Off* then means a
    /// black frame rather than a mode.
    fn firmware_effect(&self, effect: Effect) -> Option<Outgoing>;

    /// The effect the firmware runs now, for a family that can be asked.
    fn current_effect(&self, _device: &hidapi::HidDevice) -> Result<Option<Effect>, String> {
        Ok(None)
    }

    /// What the device is asked when it opens.
    ///
    /// `None` refuses the unit: a serial the caller does not accept. A family
    /// with no read yet answers [`crate::Inspection::unread`] without sending
    /// anything — a command of another maker's protocol is not a question, it is
    /// a guess.
    fn inspect(
        &self,
        device: &hidapi::HidDevice,
        accept: &mut dyn FnMut(Option<&str>) -> bool,
    ) -> Option<crate::Inspection>;
}

/// Razer: the matrix row by row, then the custom effect.
pub struct RazerRows;

impl Lighting for RazerRows {
    fn frame(&self, layout: &Layout, frame: &[Rgb]) -> Vec<Outgoing> {
        let cols = layout.cols as usize;
        let mut out: Vec<Outgoing> = (0..layout.rows)
            .map(|row| {
                let start = row as usize * cols;
                report(Report::write_row(row, 0, &frame[start..start + cols]))
            })
            .collect();
        // Not a validation: this is what switches the device to host control.
        out.push(report(Report::set_effect(Effect::Custom)));
        out
    }

    fn brightness(&self, level: u8) -> Vec<Outgoing> {
        vec![report(Report::set_brightness(level))]
    }

    fn row(&self, row: u8, col_start: u8, colours: &[Rgb]) -> Option<Outgoing> {
        Some(report(Report::write_row(row, col_start, colours)))
    }

    fn firmware_effect(&self, effect: Effect) -> Option<Outgoing> {
        Some(report(Report::set_effect(effect)))
    }

    fn current_effect(&self, device: &hidapi::HidDevice) -> Result<Option<Effect>, String> {
        crate::inspection::read_effect(device)
    }

    fn inspect(
        &self,
        device: &hidapi::HidDevice,
        accept: &mut dyn FnMut(Option<&str>) -> bool,
    ) -> Option<crate::Inspection> {
        crate::inspection::inspect_if(device, accept)
    }
}

fn report(report: Report) -> Outgoing {
    Outgoing {
        command: Some(report.id()),
        bytes: report.to_feature_buffer().to_vec(),
    }
}

/// Alienware: a frame of per-key colours, opened and closed.
pub struct AlienwareKeys;

impl Lighting for AlienwareKeys {
    fn frame(&self, layout: &Layout, frame: &[Rgb]) -> Vec<Outgoing> {
        // Each colour goes out under **the address the layout gives its
        // position**: this family names keys, and no arithmetic here could know
        // how the next model numbers them.
        //
        // Positions with no address are left out rather than sent black: the
        // device would take one for another key.
        let lit: Vec<(u8, Rgb)> = layout
            .matrix
            .iter()
            .zip(frame)
            .filter_map(|(&address, &colour)| (address != EMPTY).then_some((address as u8, colour)))
            .collect();
        let mut out = vec![Outgoing::plain(alienware::open().to_vec())];
        out.extend(
            lit.chunks(alienware::KEYS_PER_REPORT)
                .map(|chunk| Outgoing::plain(alienware::colours(chunk).to_vec())),
        );
        out.push(Outgoing::plain(alienware::close().to_vec()));
        out
    }

    /// An open and a close, then the level.
    ///
    /// The level alone is accepted and does nothing: this device takes it only
    /// after a frame has been opened and closed, measured both ways on
    /// 18/09/2026. The frame carries no colour — nothing on screen changes.
    fn brightness(&self, level: u8) -> Vec<Outgoing> {
        vec![
            Outgoing::plain(alienware::open().to_vec()),
            Outgoing::plain(alienware::close().to_vec()),
            Outgoing::plain(alienware::brightness(level).to_vec()),
        ]
    }

    fn firmware_effect(&self, _effect: Effect) -> Option<Outgoing> {
        None
    }

    fn inspect(
        &self,
        _device: &hidapi::HidDevice,
        accept: &mut dyn FnMut(Option<&str>) -> bool,
    ) -> Option<crate::Inspection> {
        accept(None).then(crate::Inspection::unread)
    }
}
