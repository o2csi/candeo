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

use std::sync::Mutex;
use std::time::{Duration, Instant};

use candeo_protocol::{alienware, alienware_elc, CommandId, Effect, Report, Rgb};

use crate::layout::{Layout, EMPTY};

/// How a report reaches the device, which is not a detail of plumbing.
///
/// The AW-ELC accepts an **output report on the control endpoint** and ignores
/// the very same bytes written any other way — established by capturing our own
/// writes beside the maker's software, which differed only in the path they took
/// (`docs/protocol/alienware-m18-r1.md` §2).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Wire {
    /// `HidD_SetFeature`: the two keyboards.
    Feature,
    /// `HidD_SetOutputReport`: the zones.
    Output,
}

/// One report on its way to a device.
pub struct Outgoing {
    /// The buffer as the HID API wants it, report id included — a leading zero
    /// where the device carries none.
    pub bytes: Vec<u8>,
    /// Which call carries it.
    pub wire: Wire,
    /// The command it carries, when the device can be asked about one: that is
    /// what lets an inspection refuse to send it. `None` where a family has no
    /// such notion.
    pub command: Option<CommandId>,
}

impl Outgoing {
    fn plain(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            wire: Wire::Feature,
            command: None,
        }
    }

    /// A report for a device with no report id of its own: the API still wants a
    /// byte in front, and a zero is what the maker's software sends.
    fn output(bytes: &[u8]) -> Self {
        let mut buffer = Vec::with_capacity(bytes.len() + 1);
        buffer.push(0);
        buffer.extend_from_slice(bytes);
        Self {
            bytes: buffer,
            wire: Wire::Output,
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
    /// level only after a frame has been opened and closed. `None` where a
    /// family has no brightness at all — the zones around that keyboard — and
    /// the screen then offers no slider rather than one that changes nothing.
    fn brightness(&self, _level: u8) -> Option<Vec<Outgoing>> {
        None
    }

    /// A segment of one row, for a family that addresses the matrix that way.
    /// `None` where it does not, and the command then says so rather than
    /// writing something else.
    fn row(&self, _row: u8, _col_start: u8, _colours: &[Rgb]) -> Option<Outgoing> {
        None
    }

    /// The report handing the lighting back to the firmware, for the effect the
    /// gallery calls `id`.
    ///
    /// **Named, not enumerated.** One family's modes are not another's: the
    /// Razer has a spectrum and a wave, this Alienware keyboard has sixteen
    /// kinds of its own. A shared enumeration would carry each family's
    /// vocabulary into every other, and every new device would widen it.
    ///
    /// `colours` carries what the gallery was asked for, as many as the layout
    /// says the effect takes; an effect painting its own palette gets none.
    ///
    /// `None` for an id this family does not run — including where it runs none
    /// at all, and *Off* is then a black frame rather than a mode.
    fn firmware_effect(&self, id: &str, colours: &[Rgb]) -> Option<Outgoing>;

    /// The effect the firmware runs now, by its id, for a family that can be
    /// asked. `None` for one the gallery does not offer — a mode needing a
    /// colour, or the host-driven one — since nobody could select it back.
    fn current_effect(&self, _device: &hidapi::HidDevice) -> Result<Option<String>, String> {
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

    fn brightness(&self, level: u8) -> Option<Vec<Outgoing>> {
        Some(vec![report(Report::set_brightness(level))])
    }

    fn row(&self, row: u8, col_start: u8, colours: &[Rgb]) -> Option<Outgoing> {
        Some(report(Report::write_row(row, col_start, colours)))
    }

    /// Only what the survey established on this firmware: `Static` and
    /// `Breathing` take a colour the gallery has nowhere to ask for, and
    /// `Reactive` and `Starlight` are refused by the device
    /// (`docs/protocol/deathstalker-v2-pro.md` §8).
    ///
    /// The wave's two values are the only ones ever captured; their real range
    /// is an open question of the survey, so offering a setting would be
    /// inventing a scale.
    fn firmware_effect(&self, id: &str, _colours: &[Rgb]) -> Option<Outgoing> {
        let effect = match id {
            "hardware:off" => Effect::Off,
            "hardware:spectrumCycle" => Effect::SpectrumCycle,
            "hardware:wave" => Effect::Wave {
                direction: 0x02,
                speed: 0x28,
            },
            _ => return None,
        };
        Some(report(Report::set_effect(effect)))
    }

    fn current_effect(&self, device: &hidapi::HidDevice) -> Result<Option<String>, String> {
        Ok(crate::inspection::read_effect(device)?.and_then(|effect| {
            let id = match effect {
                Effect::Off => "hardware:off",
                Effect::SpectrumCycle => "hardware:spectrumCycle",
                Effect::Wave { .. } => "hardware:wave",
                // Static and Breathing carry a colour the gallery cannot pass
                // back, and Custom is the host's own frames.
                _ => return None,
            };
            Some(id.to_owned())
        }))
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
        wire: Wire::Feature,
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
    fn brightness(&self, level: u8) -> Option<Vec<Outgoing>> {
        Some(vec![
            Outgoing::plain(alienware::open().to_vec()),
            Outgoing::plain(alienware::close().to_vec()),
            Outgoing::plain(alienware::brightness(level).to_vec()),
        ])
    }

    /// The sixteen kinds its firmware runs, `hardware:m18-00` to
    /// `hardware:m18-0f`, and *Off*.
    ///
    /// **Off is a kind here, not a black frame.** Once the firmware animates, it
    /// redraws over anything the host pushes: an image of black is overwritten
    /// within the second, and the keyboard never goes dark.
    ///
    /// Colours are taken as given, and the kinds that paint their own palette
    /// are unaffected by them: the layout says which take one, so an effect that
    /// would ignore a colour is never asked for one.
    fn firmware_effect(&self, id: &str, colours: &[Rgb]) -> Option<Outgoing> {
        let kind = match id {
            "hardware:off" => ALIENWARE_STEADY,
            _ => u8::from_str_radix(id.strip_prefix(ALIENWARE_EFFECT)?, 16).ok()?,
        };
        // A kind that paints what it is given, given nothing, shows nothing —
        // which is exactly what `Off` is here.
        let one = colours.first().copied().unwrap_or_default();
        let two = colours.get(1).copied().unwrap_or(one);
        Some(Outgoing::plain(
            alienware::effect(kind, 0x05, one, two).to_vec(),
        ))
    }

    fn inspect(
        &self,
        _device: &hidapi::HidDevice,
        accept: &mut dyn FnMut(Option<&str>) -> bool,
    ) -> Option<crate::Inspection> {
        accept(None).then(crate::Inspection::unread)
    }
}

/// What an Alienware keyboard's firmware effects are called, before the two
/// hexadecimal digits of the kind.
pub const ALIENWARE_EFFECT: &str = "hardware:m18-";

/// The kind that paints the colour it is given and nothing else, which on black
/// is how this keyboard goes dark. Read off the keyboard on 18/09/2026, where
/// every kind carrying no colour showed nothing.
const ALIENWARE_STEADY: u8 = 0x01;

/// Alienware: the zones around a keyboard, addressed by id.
///
/// Its own family, not a variant of [`AlienwareKeys`]: another device, another
/// report shape, another way out of the machine — and a change is a numbered
/// transaction rather than a frame.
pub struct AlienwareZones {
    /// The last image sent and when, so that an unchanged one sends nothing and
    /// a changing one goes out at a pace the device survives.
    ///
    /// **This device takes changes, not a stream.** An engine pushing thirty
    /// images a second puts some three hundred reports a second on a bus where
    /// the maker's software sends seven, and the writes then start failing:
    /// measured on 18/09/2026, `HidD_SetOutputReport` refusing until the runtime
    /// gave up on the device. Four lights do not need more than [`PACE`].
    last: Mutex<(Vec<Rgb>, Option<Instant>)>,
}

/// The shortest gap between two transactions: ten a second, still more than the
/// maker's software sends, and slow enough that every write goes through.
const PACE: Duration = Duration::from_millis(100);

impl AlienwareZones {
    /// `const`, so that the family can be a `static` a layout points at —
    /// which is why there is no `Default` here.
    pub const fn new() -> Self {
        Self {
            last: Mutex::new((Vec::new(), None)),
        }
    }
}

impl Default for AlienwareZones {
    fn default() -> Self {
        Self::new()
    }
}

/// The instance layouts name: a family holding a counter cannot be a unit
/// struct, and one counter per device would say nothing more.
pub static ALIENWARE_ZONES: AlienwareZones = AlienwareZones::new();

impl Lighting for AlienwareZones {
    fn frame(&self, layout: &Layout, frame: &[Rgb]) -> Vec<Outgoing> {
        {
            let (sent, when) = &mut *self.last.lock().expect("no panic holds this lock");
            let too_soon = when.is_some_and(|at| at.elapsed() < PACE);
            if sent == frame || too_soon {
                return Vec::new();
            }
            sent.clear();
            sent.extend_from_slice(frame);
            *when = Some(Instant::now());
        }

        // Always the common target: the others are stored profiles, and writing
        // one would change what the machine shows with nothing running.
        let target = alienware_elc::COMMON;
        let mut out: Vec<Outgoing> = alienware_elc::begin(target)
            .iter()
            .map(|report| Outgoing::output(report))
            .collect();

        // Zones sharing a colour are named together, as the maker's software
        // does for the two halves of the ring: one selection, one colour.
        let mut groups: Vec<(Rgb, Vec<u8>)> = Vec::new();
        for (&address, &colour) in layout.matrix.iter().zip(frame) {
            if address == EMPTY {
                continue;
            }
            match groups.iter_mut().find(|(seen, _)| *seen == colour) {
                Some((_, zones)) => zones.push(address as u8),
                None => groups.push((colour, vec![address as u8])),
            }
        }
        for (colour, zones) in groups {
            for chunk in zones.chunks(alienware_elc::ZONES_PER_SELECT) {
                out.push(Outgoing::output(&alienware_elc::select(chunk)));
                out.push(Outgoing::output(&alienware_elc::colour(colour)));
            }
        }

        out.extend(
            alienware_elc::commit(target)
                .iter()
                .map(|report| Outgoing::output(report)),
        );
        out
    }

    // No brightness: nothing in the survey dims these zones, and the level of a
    // colour is the colour itself.

    fn firmware_effect(&self, _id: &str, _colours: &[Rgb]) -> Option<Outgoing> {
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
