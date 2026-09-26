//! Devices described as data (`docs/design/device-sdk.md` §7, *A family as data
//! after all*, and §9).
//!
//! A definition file names the device, says how its reports are framed and what
//! is sent per light, and draws its lights. [`load`] reads one into a [`Layout`]
//! whose family is a [`Template`]: the one interpreter, written and tested here,
//! that every described device shares.
//!
//! Built-in definitions live in `devices/` next to this crate, are embedded, and
//! replay their reference frames in the tests.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use candeo_protocol::{CommandId, Effect, Firmware, Rgb};
use serde::Deserialize;

use crate::layout::{
    FirmwareEffect, Key, Layout, Lights, Outline, Port, Shape, EMPTY, NO_SCANCODE,
};
use crate::lighting::{Lighting, Outgoing, Wire};

/// The built-in definitions, by file name, as the repository holds them.
pub const BUILTIN: &[(&str, &str)] = &[
    (
        "razer-deathstalker-v2-pro.json",
        include_str!("../devices/razer-deathstalker-v2-pro.json"),
    ),
    (
        "alienware-m18-r1.json",
        include_str!("../devices/alienware-m18-r1.json"),
    ),
    (
        "alienware-m18-r1-zones.json",
        include_str!("../devices/alienware-m18-r1-zones.json"),
    ),
];

/// The id the gallery gives *Off*, offered on every device: a definition says
/// which of its firmware's kinds it is, or *Off* is a black frame.
const OFF: &str = "hardware:off";

// ---------------------------------------------------------------- the file

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    name: String,
    #[serde(rename = "match")]
    matching: Match,
    report: ReportSpec,
    frame: FrameSpec,
    #[serde(default)]
    brightness: Option<Vec<String>>,
    #[serde(default)]
    firmware: Option<FirmwareSpec>,
    /// The firmware version the survey was made against, `1.5`.
    #[serde(default)]
    surveyed: Option<String>,
    /// What is asked of the device on open, by the name of a read written in
    /// Rust: `razer`. None asks nothing.
    #[serde(default)]
    inspect: Option<String>,
    lights: LightsSpec,
    #[serde(default)]
    outline: Vec<PartSpec>,
    #[serde(default)]
    examples: Vec<Example>,
    /// What a reviewer should know that the survey says at length: not used.
    #[serde(default)]
    #[allow(dead_code)]
    notes: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Match {
    vid: String,
    pid: String,
    interface: Option<u8>,
    usage_page: Option<String>,
    usage: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportSpec {
    wire: String,
    length: usize,
    #[serde(default)]
    prefix: String,
    /// An integrity byte, named rather than written.
    #[serde(default)]
    checksum: Option<ChecksumSpec>,
    /// Where a report says which command it carries, class then command: what
    /// lets the inspection refuse one the device said it does not know.
    #[serde(default)]
    command: Option<[usize; 2]>,
}

/// The closed list of integrity functions (§7 of the design): each is tested
/// here once, and every device using it stays data.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChecksumSpec {
    /// The XOR of the bytes from one offset to another, both included.
    xor: [usize; 2],
    /// Where it goes.
    at: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameSpec {
    pace: Option<PaceSpec>,
    /// Sent once when a host effect starts, to stop what the firmware animates.
    #[serde(default, rename = "takeOver")]
    take_over: Vec<String>,
    #[serde(default)]
    open: Vec<String>,
    each: EachSpec,
    #[serde(default)]
    close: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PaceSpec {
    at_least_ms: u64,
    #[serde(default)]
    skip_unchanged: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EachSpec {
    /// `row`: one report per row of the grid, every cell of it, lit or not.
    by: Option<String>,
    group: Option<String>,
    chunk: Option<usize>,
    send: Vec<String>,
    /// One light's entry, repeated for each light of a chunk where `{lights}`
    /// stands.
    light: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FirmwareSpec {
    send: String,
    /// The kind *Off* is, when the firmware has one that shows nothing.
    off: Option<String>,
    effects: Vec<EffectSpec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EffectSpec {
    id: String,
    kind: String,
    #[serde(default)]
    colours: u8,
    /// This effect's own report, where it carries more than a kind.
    #[serde(default)]
    send: Option<String>,
    /// What it shows, for whoever reads the file: not used.
    #[serde(default)]
    #[allow(dead_code)]
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LightsSpec {
    kind: String,
    rows: Option<u8>,
    cols: Option<u8>,
    items: Vec<ItemSpec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemSpec {
    row: Option<u8>,
    col: Option<u8>,
    address: u16,
    #[serde(default)]
    scancode: Option<String>,
    #[serde(default)]
    shape: Option<String>,
    rect: [f32; 4],
    /// What it is, for whoever reads the file: not used.
    #[serde(default)]
    #[allow(dead_code)]
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PartSpec {
    rect: [f32; 4],
    #[serde(default)]
    r: f32,
}

/// Colours in, reports out, as a contributor saw them: what the tests replay.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Example {
    colours: Vec<String>,
    reports: Vec<String>,
}

// ---------------------------------------------------------------- the template

/// One byte of a report as a definition writes it: a fixed value, or what the
/// frame, the level or the effect fills in.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Token {
    Byte(u8),
    R,
    G,
    B,
    /// An effect's second colour, its first when it was given one only.
    R2,
    G2,
    B2,
    Address,
    /// How many lights the report carries.
    Count,
    /// Their addresses, one byte each.
    Addresses,
    /// Their entries, each as `each.light` writes one.
    Lights,
    Level,
    Kind,
    /// The row a report carries, and the first and last of its columns.
    Row,
    Start,
    End,
    /// How many bytes the report holds after this offset: a length the
    /// device reads, worked out once the report is filled.
    Length(usize),
}

fn tokens(text: &str) -> Result<Vec<Token>, String> {
    text.split_whitespace()
        .map(|word| match word {
            "{r}" => Ok(Token::R),
            "{g}" => Ok(Token::G),
            "{b}" => Ok(Token::B),
            "{r2}" => Ok(Token::R2),
            "{g2}" => Ok(Token::G2),
            "{b2}" => Ok(Token::B2),
            "{address}" => Ok(Token::Address),
            "{count}" => Ok(Token::Count),
            "{addresses}" => Ok(Token::Addresses),
            "{lights}" => Ok(Token::Lights),
            "{level}" => Ok(Token::Level),
            "{kind}" => Ok(Token::Kind),
            "{row}" => Ok(Token::Row),
            "{start}" => Ok(Token::Start),
            "{end}" => Ok(Token::End),
            _ if word.starts_with("{length:") && word.ends_with('}') => word[8..word.len() - 1]
                .parse()
                .map(Token::Length)
                .map_err(|_| format!("“{word}”: a length counts from a decimal offset")),
            _ if word.len() == 2 => u8::from_str_radix(word, 16)
                .map(Token::Byte)
                .map_err(|_| format!("“{word}” is neither a byte nor a placeholder")),
            _ => Err(format!("“{word}” is neither a byte nor a placeholder")),
        })
        .collect()
}

/// A report whose placeholders are all ones its place fills in.
fn template(text: &str, allowed: &[Token], place: &str) -> Result<Vec<Token>, String> {
    let t = tokens(text)?;
    if t.iter()
        .any(|t| !matches!(t, Token::Byte(_) | Token::Length(_)) && !allowed.contains(t))
    {
        return Err(format!(
            "“{text}” holds a placeholder {place} does not fill"
        ));
    }
    Ok(t)
}

fn hex_bytes(text: &str) -> Result<Vec<u8>, String> {
    Ok(template(text, &[], "a list of bytes")?
        .into_iter()
        .filter_map(|t| match t {
            Token::Byte(b) => Some(b),
            _ => None,
        })
        .collect())
}

fn hex_u16(text: &str, what: &str) -> Result<u16, String> {
    u16::from_str_radix(text, 16).map_err(|_| format!("{what} “{text}” is not hexadecimal"))
}

fn hex_u8(text: &str, what: &str) -> Result<u8, String> {
    u8::from_str_radix(text, 16).map_err(|_| format!("{what} “{text}” is not one hexadecimal byte"))
}

const BLACK: Rgb = Rgb::new(0, 0, 0);

/// What a report is filled with.
#[derive(Clone, Copy)]
struct Fill<'a> {
    colour: Rgb,
    colour2: Rgb,
    lights: &'a [(u8, Rgb)],
    level: u8,
    kind: u8,
    row: u8,
    start: u8,
}

const NOTHING: Fill<'static> = Fill {
    colour: BLACK,
    colour2: BLACK,
    lights: &[],
    level: 0,
    kind: 0,
    row: 0,
    start: 0,
};

/// A firmware's effects: the report, *Off*'s kind, and each effect's id, kind
/// and report of its own if it has one.
type FirmwareEffects = (
    Vec<Token>,
    Option<u8>,
    Vec<(String, u8, Option<Vec<Token>>)>,
);

/// A read written in Rust, named by a definition.
#[derive(Clone, Copy, PartialEq)]
enum Inspect {
    Nothing,
    /// The Razer protocol's: version, serial, and the commands the device
    /// knows (`docs/protocol/deathstalker-v2-pro.md` §8), and the effect its
    /// firmware runs.
    Razer,
}

/// The family of every described device: it turns a frame into the reports its
/// definition describes.
pub struct Template {
    wire: Wire,
    length: usize,
    prefix: Vec<u8>,
    open: Vec<Vec<Token>>,
    per_colour: bool,
    chunk: usize,
    send: Vec<Vec<Token>>,
    light: Vec<Token>,
    close: Vec<Vec<Token>>,
    by_row: bool,
    checksum: Option<(usize, usize, usize)>,
    command: Option<(usize, usize)>,
    brightness: Option<Vec<Vec<Token>>>,
    firmware: Option<FirmwareEffects>,
    inspect: Inspect,
    take_over: Vec<Vec<Token>>,
    pace: Option<(Duration, bool)>,
    /// The last image sent and when, for `pace`.
    last: Mutex<(Vec<Rgb>, Option<Instant>)>,
}

impl Template {
    fn put(
        &self,
        bytes: &mut Vec<u8>,
        tokens: &[Token],
        fill: Fill,
        lengths: &mut Vec<(usize, usize)>,
    ) {
        for token in tokens {
            match token {
                Token::Byte(b) => bytes.push(*b),
                Token::R => bytes.push(fill.colour.r),
                Token::G => bytes.push(fill.colour.g),
                Token::B => bytes.push(fill.colour.b),
                Token::R2 => bytes.push(fill.colour2.r),
                Token::G2 => bytes.push(fill.colour2.g),
                Token::B2 => bytes.push(fill.colour2.b),
                Token::Address => bytes.push(fill.lights.first().map_or(0, |l| l.0)),
                Token::Count => bytes.push(fill.lights.len() as u8),
                Token::Addresses => bytes.extend(fill.lights.iter().map(|l| l.0)),
                Token::Lights => {
                    for light in fill.lights {
                        let one = Fill {
                            colour: light.1,
                            lights: std::slice::from_ref(light),
                            ..fill
                        };
                        self.put(bytes, &self.light, one, lengths);
                    }
                }
                Token::Level => bytes.push(fill.level),
                Token::Kind => bytes.push(fill.kind),
                Token::Row => bytes.push(fill.row),
                Token::Start => bytes.push(fill.start),
                Token::End => bytes.push(fill.start + (fill.lights.len() as u8).saturating_sub(1)),
                Token::Length(from) => {
                    lengths.push((bytes.len(), *from));
                    bytes.push(0);
                }
            }
        }
    }

    fn render(&self, tokens: &[Token], fill: Fill) -> Outgoing {
        let p = self.prefix.len();
        let mut bytes = self.prefix.clone();
        let mut lengths = Vec::new();
        self.put(&mut bytes, tokens, fill, &mut lengths);
        let filled = bytes.len() - p;
        for (at, from) in lengths {
            bytes[at] = filled.saturating_sub(from) as u8;
        }
        // Checked at load: the longest a report can grow still fits.
        bytes.resize(p + self.length, 0);
        if let Some((from, to, at)) = self.checksum {
            bytes[p + at] = bytes[p + from..=p + to].iter().fold(0, |x, b| x ^ b);
        }
        Outgoing {
            command: self.command.map(|(class, command)| CommandId {
                class: bytes[p + class],
                command: bytes[p + command],
            }),
            bytes,
            wire: self.wire,
        }
    }

    /// The effect the firmware runs, back to the id the gallery offers: one it
    /// does not offer — a colour it cannot pass back, or the host's own
    /// frames — reads as none.
    fn effect_id(&self, effect: Effect) -> Option<String> {
        let (_, off, effects) = self.firmware.as_ref()?;
        let kind = match effect {
            Effect::Off => 0x00,
            Effect::SpectrumCycle => 0x03,
            Effect::Wave { .. } => 0x04,
            Effect::Custom => return None,
        };
        if Some(kind) == *off {
            return Some(OFF.to_owned());
        }
        effects
            .iter()
            .find(|(_, k, _)| *k == kind)
            .map(|(id, _, _)| id.clone())
    }

    /// The most bytes one template can grow to, a whole chunk of lights in it.
    fn widest(&self, tokens: &[Token]) -> usize {
        tokens
            .iter()
            .map(|t| match t {
                Token::Addresses => self.chunk,
                Token::Lights => self.chunk * self.widest(&self.light),
                _ => 1,
            })
            .sum()
    }
}

impl Lighting for Template {
    fn frame(&self, layout: &Layout, frame: &[Rgb]) -> Vec<Outgoing> {
        if let Some((gap, skip_unchanged)) = self.pace {
            let (sent, when) = &mut *self.last.lock().expect("no panic holds this lock");
            let too_soon = when.is_some_and(|at| at.elapsed() < gap);
            if (skip_unchanged && sent == frame) || too_soon {
                return Vec::new();
            }
            sent.clear();
            sent.extend_from_slice(frame);
            *when = Some(Instant::now());
        }

        let mut out: Vec<Outgoing> = self.open.iter().map(|t| self.render(t, NOTHING)).collect();

        // A row at a time, every cell of it: the device takes the whole grid,
        // and a cell left out would keep its last colour.
        if self.by_row {
            let cols = usize::from(layout.cols);
            for (row, cells) in frame.chunks(cols).enumerate() {
                let lights: Vec<(u8, Rgb)> = cells.iter().map(|c| (0, *c)).collect();
                let fill = Fill {
                    lights: &lights,
                    row: row as u8,
                    ..NOTHING
                };
                out.extend(self.send.iter().map(|t| self.render(t, fill)));
            }
            out.extend(self.close.iter().map(|t| self.render(t, NOTHING)));
            return out;
        }

        // Positions with no address are left out rather than sent black: the
        // device would take one for another light.
        let lit: Vec<(u8, Rgb)> = layout
            .matrix
            .iter()
            .zip(frame)
            .filter(|(address, _)| **address != EMPTY)
            .map(|(&address, &colour)| (address as u8, colour))
            .collect();
        // Lights sharing a colour go in one selection when the device takes it
        // that way, in the order they first appear; otherwise in their own order.
        let groups: Vec<Vec<(u8, Rgb)>> = if self.per_colour {
            let mut groups: Vec<Vec<(u8, Rgb)>> = Vec::new();
            for light in lit {
                match groups.iter_mut().find(|g| g[0].1 == light.1) {
                    Some(group) => group.push(light),
                    None => groups.push(vec![light]),
                }
            }
            groups
        } else {
            vec![lit]
        };
        for group in &groups {
            for chunk in group.chunks(self.chunk) {
                let fill = Fill {
                    colour: chunk[0].1,
                    lights: chunk,
                    ..NOTHING
                };
                out.extend(self.send.iter().map(|t| self.render(t, fill)));
            }
        }

        out.extend(self.close.iter().map(|t| self.render(t, NOTHING)));
        out
    }

    fn take_over(&self) -> Vec<Outgoing> {
        self.take_over
            .iter()
            .map(|t| self.render(t, NOTHING))
            .collect()
    }

    fn brightness(&self, level: u8) -> Option<Vec<Outgoing>> {
        let fill = Fill { level, ..NOTHING };
        self.brightness
            .as_ref()
            .map(|reports| reports.iter().map(|t| self.render(t, fill)).collect())
    }

    fn row(&self, row: u8, col_start: u8, colours: &[Rgb]) -> Option<Outgoing> {
        if !self.by_row || colours.is_empty() {
            return None;
        }
        let lights: Vec<(u8, Rgb)> = colours.iter().map(|c| (0, *c)).collect();
        let fill = Fill {
            lights: &lights,
            row,
            start: col_start,
            ..NOTHING
        };
        self.send.first().map(|t| self.render(t, fill))
    }

    fn firmware_effect(&self, id: &str, colours: &[Rgb]) -> Option<Outgoing> {
        let (common, off, effects) = self.firmware.as_ref()?;
        let (kind, send) = if id == OFF {
            ((*off)?, common)
        } else {
            let (_, kind, own) = effects.iter().find(|(known, _, _)| known == id)?;
            (*kind, own.as_ref().unwrap_or(common))
        };
        // A kind that paints what it is given, given nothing, shows nothing:
        // which is exactly what *Off* is where the firmware has no dark kind.
        let colour = colours.first().copied().unwrap_or(BLACK);
        let fill = Fill {
            colour,
            colour2: colours.get(1).copied().unwrap_or(colour),
            kind,
            ..NOTHING
        };
        Some(self.render(send, fill))
    }

    fn current_effect(&self, device: &hidapi::HidDevice) -> Result<Option<String>, String> {
        if self.inspect != Inspect::Razer {
            return Ok(None);
        }
        Ok(crate::inspection::read_effect(device)?.and_then(|e| self.effect_id(e)))
    }

    fn inspect(
        &self,
        device: &hidapi::HidDevice,
        accept: &mut dyn FnMut(Option<&str>) -> bool,
    ) -> Option<crate::Inspection> {
        match self.inspect {
            Inspect::Razer => crate::inspection::inspect_if(device, accept),
            // Nothing is asked of a device whose definition names no read.
            Inspect::Nothing => accept(None).then(crate::Inspection::unread),
        }
    }
}

// ---------------------------------------------------------------- loading

/// The built-in definitions, read once, in [`BUILTIN`]'s order: the DeathStalker
/// first, the layout used when no device is connected.
///
/// A built-in definition that does not load is a build nobody tested — the
/// tests replay every one — so it stops here rather than leaving a device
/// silently unknown.
pub fn builtin() -> &'static [&'static Layout] {
    static ALL: std::sync::OnceLock<Vec<&'static Layout>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        BUILTIN
            .iter()
            .map(|(name, json)| load(json).unwrap_or_else(|e| panic!("{name}: {e}")))
            .collect()
    })
}

/// A definition read, checked and turned into a layout, or why not, in a
/// sentence for whoever wrote it.
///
/// The layout lives as long as the process: devices are listed once, and every
/// part of the application holds them as `&'static`.
pub fn load(json: &str) -> Result<&'static Layout, String> {
    let (layout, _, _) = parse(json)?;
    Ok(Box::leak(Box::new(layout)))
}

fn family(d: &Definition, cols: usize) -> Result<Template, String> {
    use Token::*;
    let wire = match d.report.wire.as_str() {
        "output" => Wire::Output,
        "feature" => Wire::Feature,
        other => return Err(format!("wire “{other}”: “output” or “feature”")),
    };
    let per_colour = match d.frame.each.group.as_deref() {
        None => false,
        Some("colour") => true,
        Some(other) => return Err(format!("group “{other}”: only “colour” is known")),
    };
    let by_row = match d.frame.each.by.as_deref() {
        None => false,
        Some("row") => true,
        Some(other) => return Err(format!("by “{other}”: only “row” is known")),
    };
    if by_row && (per_colour || d.frame.each.chunk.is_some()) {
        return Err("a report per row takes neither a group nor a chunk".into());
    }
    // A row's report carries the whole row.
    let chunk = if by_row {
        cols
    } else {
        d.frame.each.chunk.unwrap_or(1)
    };
    if chunk == 0 {
        return Err("chunk 0: a report carries at least one light".into());
    }
    let fixed = |list: &[String], place: &str| -> Result<Vec<Vec<Token>>, String> {
        list.iter().map(|t| template(t, &[], place)).collect()
    };
    let light = match &d.frame.each.light {
        Some(text) => template(
            text,
            if by_row {
                &[R, G, B]
            } else {
                &[Address, R, G, B]
            },
            "a light's entry",
        )?,
        None => Vec::new(),
    };
    let mut in_send = if by_row {
        vec![Row, Start, End, Count]
    } else {
        vec![R, G, B, Address, Count, Addresses]
    };
    if !light.is_empty() {
        in_send.push(Lights);
    }
    let send = d
        .frame
        .each
        .send
        .iter()
        .map(|t| template(t, &in_send, "a frame's report"))
        .collect::<Result<Vec<_>, _>>()?;
    // A report holding several lights, each of its own colour, needs a place
    // for each colour.
    let one_colour = send.iter().flatten().any(|t| matches!(t, R | G | B));
    if chunk > 1 && !per_colour && (light.is_empty() || one_colour) {
        return Err("a report of several lights takes their colours through {lights}, or groups them by colour".into());
    }
    let in_effect = [Kind, R, G, B, R2, G2, B2];
    let firmware = match &d.firmware {
        None => None,
        Some(f) => Some((
            template(&f.send, &in_effect, "a firmware effect")?,
            f.off.as_deref().map(|k| hex_u8(k, "kind")).transpose()?,
            f.effects
                .iter()
                .map(|e| {
                    let own = e
                        .send
                        .as_deref()
                        .map(|t| template(t, &in_effect, "a firmware effect"))
                        .transpose()?;
                    Ok((e.id.clone(), hex_u8(&e.kind, "kind")?, own))
                })
                .collect::<Result<Vec<_>, String>>()?,
        )),
    };
    let inspect = match d.inspect.as_deref() {
        None => Inspect::Nothing,
        Some("razer") => Inspect::Razer,
        Some(other) => return Err(format!("inspect “{other}”: only “razer” is known")),
    };
    let length = d.report.length;
    let checksum = match &d.report.checksum {
        None => None,
        Some(c) => {
            let [from, to] = c.xor;
            if from > to || to >= length || c.at >= length {
                return Err("the checksum reads or writes past the report".into());
            }
            Some((from, to, c.at))
        }
    };
    let command = match d.report.command {
        None => None,
        Some([class, command]) if class < length && command < length => Some((class, command)),
        Some(_) => return Err("the command sits past the report".into()),
    };
    let brightness = match &d.brightness {
        None => None,
        Some(list) => Some(
            list.iter()
                .map(|t| template(t, &[Level], "brightness"))
                .collect::<Result<Vec<_>, _>>()?,
        ),
    };
    let t = Template {
        wire,
        length: d.report.length,
        prefix: hex_bytes(&d.report.prefix)?,
        open: fixed(&d.frame.open, "opening a frame")?,
        per_colour,
        chunk,
        send,
        light,
        close: fixed(&d.frame.close, "closing a frame")?,
        take_over: fixed(&d.frame.take_over, "taking the lights over")?,
        by_row,
        checksum,
        command,
        inspect,
        brightness,
        firmware,
        pace: d
            .frame
            .pace
            .as_ref()
            .map(|p| (Duration::from_millis(p.at_least_ms), p.skip_unchanged)),
        last: Mutex::new((Vec::new(), None)),
    };
    let reports = t
        .open
        .iter()
        .chain(&t.take_over)
        .chain(&t.send)
        .chain(&t.close)
        .chain(t.brightness.iter().flatten())
        .chain(t.firmware.iter().map(|f| &f.0))
        .chain(
            t.firmware
                .iter()
                .flat_map(|f| f.2.iter().filter_map(|e| e.2.as_ref())),
        );
    for report in reports {
        if t.widest(report) > t.length {
            return Err(format!("a report grows past its {} bytes", t.length));
        }
    }
    Ok(t)
}

/// The layout, the reference frames, and where each item of the file sits in
/// the frame — how a reference frame's colours, given per item, are placed.
fn parse(json: &str) -> Result<(Layout, Vec<Example>, Vec<u16>), String> {
    let d: Definition = serde_json::from_str(json).map_err(|e| e.to_string())?;

    let lights = match d.lights.kind.as_str() {
        "zones" => Lights::Zones,
        "keys" => Lights::Keys,
        other => return Err(format!("lights “{other}”: “keys” or “zones”")),
    };
    // A grid when the file gives one, every item at its row and column; one row
    // in the items' order otherwise.
    let (rows, cols) = match (d.lights.rows, d.lights.cols) {
        (Some(rows), Some(cols)) => (rows, cols),
        (None, None) => (
            1,
            u8::try_from(d.lights.items.len()).map_err(|_| "more than 255 lights in one row")?,
        ),
        _ => return Err("lights take both rows and cols, or neither".into()),
    };
    let template = family(&d, usize::from(cols))?;
    let surveyed_firmware = match d.surveyed.as_deref() {
        None => None,
        Some(text) => {
            let (major, minor) = text
                .split_once('.')
                .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))
                .ok_or_else(|| format!("surveyed “{text}” is not major.minor"))?;
            Some(Firmware { major, minor })
        }
    };
    let mut matrix = vec![EMPTY; usize::from(rows) * usize::from(cols)];
    let mut keys = Vec::with_capacity(d.lights.items.len());
    let mut placed = Vec::with_capacity(d.lights.items.len());
    for (n, item) in d.lights.items.iter().enumerate() {
        let (row, col) = match (item.row, item.col, d.lights.rows) {
            (Some(row), Some(col), Some(_)) => (row, col),
            (None, None, None) => (0, n as u8),
            _ => {
                return Err(format!(
                    "light {n}: a row and a column, exactly when the lights are a grid"
                ))
            }
        };
        if row >= rows || col >= cols {
            return Err(format!(
                "light {n}: row {row}, column {col} is outside the grid"
            ));
        }
        let position = usize::from(row) * usize::from(cols) + usize::from(col);
        if matrix[position] != EMPTY {
            return Err(format!("row {row}, column {col} is given twice"));
        }
        if matrix.contains(&item.address) {
            return Err(format!("address {} is given twice", item.address));
        }
        matrix[position] = item.address;
        placed.push(position as u16);
        let shape = match item.shape.as_deref() {
            None | Some("rect") => Shape::Rect,
            Some("disc") => Shape::Disc,
            Some("archUp") => Shape::ArchUp,
            Some("archDown") => Shape::ArchDown,
            Some(other) => return Err(format!("shape “{other}” is not one the simulator draws")),
        };
        let [x, y, w, h] = item.rect;
        keys.push(Key {
            index: position as u16,
            scancode: match &item.scancode {
                Some(code) => hex_u16(code, "scancode")?,
                None => NO_SCANCODE,
            },
            x,
            y,
            w,
            h,
            shape,
        });
    }
    keys.sort_by_key(|k| k.index);

    let firmware_effects: Vec<FirmwareEffect> = d
        .firmware
        .iter()
        .flat_map(|f| &f.effects)
        .map(|e| FirmwareEffect {
            id: Box::leak(e.id.clone().into_boxed_str()),
            colours: e.colours,
        })
        .collect();
    let outline: Vec<Outline> = d
        .outline
        .iter()
        .map(|p| {
            let [x, y, w, h] = p.rect;
            Outline { x, y, w, h, r: p.r }
        })
        .collect();

    let port = match (
        &d.matching.usage_page,
        &d.matching.usage,
        d.matching.interface,
    ) {
        (Some(page), Some(usage), None) => Port::Collection {
            usage_page: hex_u16(page, "usage page")?,
            usage: hex_u16(usage, "usage")?,
        },
        (None, None, Some(interface)) => Port::Interface(interface),
        _ => return Err("match an interface, or a usage page and a usage".into()),
    };

    let layout = Layout {
        name: Box::leak(d.name.into_boxed_str()),
        vid: hex_u16(&d.matching.vid, "vid")?,
        pid: hex_u16(&d.matching.pid, "pid")?,
        port,
        lighting: Box::leak(Box::new(template)),
        surveyed_firmware,
        firmware_effects: Box::leak(firmware_effects.into_boxed_slice()),
        lights,
        rows,
        cols,
        matrix: Box::leak(matrix.into_boxed_slice()),
        keys: Box::leak(keys.into_boxed_slice()),
        outline: Box::leak(outline.into_boxed_slice()),
    };
    Ok((layout, d.examples, placed))
}

/// Replays a definition's reference frames: the reports it gives must be the
/// ones its contributor saw, byte for byte (§8 of the design). Each is written
/// without the zeros that pad it to its length.
pub fn replay(json: &str) -> Result<(), String> {
    let (layout, examples, placed) = parse(json)?;
    if examples.is_empty() {
        return Err("no reference frame to replay".into());
    }
    let d: Definition = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let prefix = hex_bytes(&d.report.prefix)?.len();
    for (n, example) in examples.iter().enumerate() {
        // Given per light, in the order the file lists them; black past the
        // last one given.
        let mut frame = vec![BLACK; layout.led_count()];
        for (position, text) in placed.iter().zip(&example.colours) {
            frame[usize::from(*position)] = colour(text)?;
        }
        // A fresh family per example: pacing belongs to a device, not a test.
        let fresh = family(&d, usize::from(layout.cols))?;
        let sent: Vec<Vec<u8>> = fresh
            .frame(&layout, &frame)
            .into_iter()
            .map(|o| o.bytes[prefix..].to_vec())
            .collect();
        let expected: Vec<Vec<u8>> = example
            .reports
            .iter()
            .map(|r| {
                let mut bytes = hex_bytes(r)?;
                bytes.resize(d.report.length, 0);
                Ok(bytes)
            })
            .collect::<Result<_, String>>()?;
        if sent != expected {
            return Err(format!(
                "reference frame {}: sent {:?}, expected {:?}",
                n + 1,
                sent.iter().map(|b| shown(b)).collect::<Vec<_>>(),
                example.reports
            ));
        }
    }
    Ok(())
}

/// `rrggbb`, as a colour is written in a definition.
fn colour(text: &str) -> Result<Rgb, String> {
    let n = (text.len() == 6)
        .then(|| u32::from_str_radix(text, 16).ok())
        .flatten()
        .ok_or_else(|| format!("colour “{text}” is not rrggbb"))?;
    Ok(Rgb::new((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

/// A report as a definition writes it: hexadecimal, its padding left out.
fn shown(bytes: &[u8]) -> String {
    let end = bytes.iter().rposition(|b| *b != 0).map_or(0, |i| i + 1);
    bytes[..end]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAZER: &str = BUILTIN[0].1;
    const KEYBOARD: &str = BUILTIN[1].1;
    const ZONES: &str = BUILTIN[2].1;

    /// What the Razer firmware says it runs reads back as the id the gallery
    /// offers, as the Rust family read it: the host's own frames as none.
    #[test]
    fn the_razer_effect_read_back_is_the_gallery_s_id() {
        let d: Definition = serde_json::from_str(RAZER).unwrap();
        let t = family(&d, 22).unwrap();
        let wave = Effect::Wave {
            direction: 0x02,
            speed: 0x28,
        };
        assert_eq!(t.effect_id(Effect::Off).as_deref(), Some(OFF));
        assert_eq!(
            t.effect_id(Effect::SpectrumCycle).as_deref(),
            Some("hardware:spectrumCycle")
        );
        assert_eq!(t.effect_id(wave).as_deref(), Some("hardware:wave"));
        assert_eq!(t.effect_id(Effect::Custom), None);
    }

    /// §8: every built-in definition loads, and gives the reports its
    /// contributor saw.
    #[test]
    fn built_in_definitions_replay_their_reference_frames() {
        for (name, json) in BUILTIN {
            load(json).unwrap_or_else(|e| panic!("{name}: {e}"));
            replay(json).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }

    /// §4 of `docs/design/studio.md`: the zones seen from behind — the logo on
    /// the lid above the ring, the ring's upper half right above its lower one,
    /// and every light within what the outline draws.
    #[test]
    fn the_zones_are_drawn_from_behind() {
        let layout = load(ZONES).unwrap();
        assert_eq!(layout.lights, Lights::Zones);
        let by = |index: u16| {
            layout
                .keys
                .iter()
                .find(|k| k.index == index)
                .expect("a zone")
        };
        let (upper, lower, logo) = (by(0), by(1), by(2));
        assert_eq!(
            (upper.shape, lower.shape, logo.shape),
            (Shape::ArchUp, Shape::ArchDown, Shape::Disc)
        );
        assert!(logo.y + logo.h < upper.y, "the logo is above the ring");
        assert_eq!(upper.y + upper.h, lower.y, "the halves meet");
        let right = layout.outline.iter().fold(0.0f32, |m, o| m.max(o.x + o.w));
        let bottom = layout.outline.iter().fold(0.0f32, |m, o| m.max(o.y + o.h));
        for k in layout.keys {
            assert!(k.x >= 0.0 && k.y >= 0.0 && k.x + k.w <= right && k.y + k.h <= bottom);
        }
    }

    /// What §8 checks of any definition without the device: each lit position
    /// has one light, drawn inside the picture without overlapping another,
    /// and a scancode names one key only.
    #[test]
    fn every_built_in_definition_is_consistent() {
        for (name, json) in BUILTIN {
            let l = load(json).unwrap();
            let mut seen = std::collections::BTreeSet::new();
            for key in l.keys {
                assert!(
                    seen.insert(key.index),
                    "{name}: position {} twice",
                    key.index
                );
                assert!(
                    l.address(key.index).is_some(),
                    "{name}: {} is not lit",
                    key.index
                );
                assert!(
                    key.x >= 0.0 && key.y >= 0.0,
                    "{name}: {} sticks out",
                    key.index
                );
            }
            assert_eq!(
                seen.len(),
                l.lit_count(),
                "{name}: a lit position has no light"
            );
            for a in l.keys {
                for b in l.keys.iter().filter(|b| b.index > a.index) {
                    let apart = a.x + a.w <= b.x
                        || b.x + b.w <= a.x
                        || a.y + a.h <= b.y
                        || b.y + b.h <= a.y;
                    assert!(apart, "{name}: {} and {} overlap", a.index, b.index);
                }
            }
            // A scancode names one key; the ISO Enter is one key with two LEDs,
            // its arms touching, and both send `0x1C`.
            for a in l.keys.iter().filter(|k| k.scancode != NO_SCANCODE) {
                for b in l
                    .keys
                    .iter()
                    .filter(|b| b.index > a.index && b.scancode == a.scancode)
                {
                    let touch = (a.y + a.h == b.y || b.y + b.h == a.y)
                        && a.x < b.x + b.w
                        && b.x < a.x + a.w;
                    assert!(
                        touch,
                        "{name}: {} and {} share a scancode",
                        a.index, b.index
                    );
                }
            }
        }
    }

    /// The laptop keyboard puts four collections on one interface, and only the
    /// one carrying report `0xcc` lights. The interface number says nothing here.
    #[test]
    fn a_collection_is_what_names_the_laptop_lighting() {
        let l = load(KEYBOARD).unwrap();
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
    }

    /// The grid the survey established: seven rows of twenty, the holes where a
    /// key is wider than a cell or the device lights nothing, and the device's
    /// own numbers, which start at one — a position is not an address.
    #[test]
    fn the_laptop_grid_matches_the_survey() {
        let l = load(KEYBOARD).unwrap();
        assert_eq!(
            (l.led_count(), l.lit_count(), l.keys.len()),
            (140, 103, 103)
        );
        assert_eq!(l.at(0, 0), Some(0), "Esc");
        assert_eq!(l.at(2, 0), Some(40), "Tab");
        assert_eq!(l.at(2, 1), None, "Tab is wider than its cell");
        assert_eq!(l.at(2, 2), Some(42), "A");
        assert_eq!(l.at(3, 14), Some(74), "Enter");
        assert_eq!(l.at(5, 7), Some(107), "Space");
        assert_eq!(l.at(5, 17), Some(117), "the keypad zero");
        assert_eq!(l.address(0), Some(1), "Esc is the device's key 1");
        assert_eq!(l.address(42), Some(43), "A");
        assert_eq!(l.address(107), Some(108), "Space");
        assert_eq!(l.address(41), None, "the cell Tab leaves behind");
        assert_eq!(l.address(139), None, "past the last key");
        assert_eq!(
            l.keys.iter().filter(|k| k.scancode == NO_SCANCODE).count(),
            3
        );
        assert_eq!(l.lights, Lights::Keys);
    }

    /// Starting a host effect takes the keyboard back from an animating
    /// firmware, the way *Off* does — without it, the animation redraws over
    /// every frame.
    #[test]
    fn a_host_effect_takes_the_laptop_keyboard_back_from_its_firmware() {
        let l = load(KEYBOARD).unwrap();
        let off = l.lighting.firmware_effect(OFF, &[]).unwrap();
        let taken: Vec<Vec<u8>> = l
            .lighting
            .take_over()
            .into_iter()
            .map(|o| o.bytes)
            .collect();
        assert_eq!(taken, vec![off.bytes]);
        assert!(load(ZONES).unwrap().lighting.take_over().is_empty());
    }

    /// *Off* is a kind of this firmware, not a black frame: once it animates,
    /// it redraws over anything the host pushes.
    #[test]
    fn off_is_the_laptop_firmware_s_steady_kind_given_black() {
        let l = load(KEYBOARD).unwrap();
        let off = l.lighting.firmware_effect(OFF, &[]).unwrap();
        assert_eq!(
            shown(&off.bytes),
            "cc 80 01 05 00 00 01 01 01 01 00 00 00 00 00 00 05"
        );
        assert!(
            l.lighting.firmware_effect("hardware:m18-0c", &[]).is_none(),
            "not offered"
        );
    }

    #[test]
    fn a_frame_unchanged_or_too_soon_sends_nothing() {
        let (layout, _, _) = parse(ZONES).unwrap();
        let red = [Rgb::new(255, 0, 0); 3];
        assert!(!layout.lighting.frame(&layout, &red).is_empty());
        assert!(layout.lighting.frame(&layout, &red).is_empty(), "unchanged");
        let blue = [Rgb::new(0, 0, 255); 3];
        assert!(
            layout.lighting.frame(&layout, &blue).is_empty(),
            "within 100 ms"
        );
    }

    fn with(from: &str, to: &str) -> String {
        let json = ZONES;
        assert!(json.contains(from), "{from}");
        json.replacen(from, to, 1)
    }

    #[test]
    fn a_definition_says_what_is_wrong_with_it() {
        let refused = |json: String, says: &str| {
            let e = load(&json).err().expect("refused");
            assert!(e.contains(says), "{e}");
        };
        refused(
            with("{count}", "{counts}"),
            "neither a byte nor a placeholder",
        );
        refused(with("\"length\": 33", "\"length\": 6"), "grows past");
        refused(
            with("\"archUp\"", "\"star\""),
            "not one the simulator draws",
        );
        refused(with("\"address\": 1,", "\"address\": 0,"), "given twice");
        refused(
            with("\"03 21 00 01 ff ff\"", "\"03 21 {r}\""),
            "does not fill",
        );
        refused(
            with("\"wire\": \"output\"", "\"wire\": \"usb\""),
            "“output” or “feature”",
        );
        refused(
            with("\"group\": \"colour\",", ""),
            "through {lights}, or groups them by colour",
        );
    }

    #[test]
    fn a_reference_frame_that_disagrees_is_named() {
        let e = replay(&with("\"03 21 00 03 00 ff\"\n", "\"03 21 00 03 00 fe\"\n")).unwrap_err();
        assert!(e.starts_with("reference frame 1"), "{e}");
    }
}

/// The DeathStalker's survey, checked on its definition
/// (`docs/protocol/deathstalker-v2-pro.md`).
#[cfg(test)]
mod deathstalker {
    use super::*;

    fn deathstalker() -> &'static Layout {
        builtin()[0]
    }

    #[test]
    fn only_the_lighting_interface_is_kept() {
        let l = deathstalker();
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

    #[test]
    fn matrix_dimensions_are_consistent() {
        let l = deathstalker();
        assert_eq!(l.matrix.len(), l.led_count());
        assert_eq!(l.led_count(), 132);
    }

    #[test]
    fn counts_match_device_report() {
        assert_eq!(deathstalker().led_count(), 132, "frame size");
        assert_eq!(deathstalker().lit_count(), 106, "lit keys");
    }

    #[test]
    fn known_positions_resolve() {
        let l = deathstalker();
        assert_eq!(l.at(0, 0), Some(0));
        assert_eq!(l.at(0, 1), None, "gap after Escape");
        assert_eq!(l.at(1, 0), Some(22));
        assert_eq!(l.at(5, 0), Some(110));
    }

    #[test]
    fn rows_have_expected_key_counts() {
        let l = deathstalker();
        let expected = [16, 21, 21, 17, 18, 13];
        assert_eq!(expected.iter().sum::<usize>(), 106);

        for (row, &n) in expected.iter().enumerate() {
            let lit = (0..l.cols)
                .filter(|&c| l.at(row as u8, c).is_some())
                .count();
            assert_eq!(lit, n, "row {row}");
        }
    }

    #[test]
    fn iso_enter_tiles_the_l_shape() {
        let l = deathstalker();
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

    #[test]
    fn space_bar_is_wide_but_single() {
        let l = deathstalker();
        assert_eq!(l.at(5, 6), Some(116));

        let space = l.key(116).expect("space bar");
        assert_eq!(space.scancode, 0x39);
        assert_eq!(space.w, 6.25);
        assert_eq!(l.keys.iter().filter(|k| k.scancode == 0x39).count(), 1);
    }

    #[test]
    fn drawing_fits_a_full_size_iso() {
        let keys = deathstalker().keys;
        let width = keys.iter().fold(0.0f32, |m, k| m.max(k.x + k.w));
        let height = keys.iter().fold(0.0f32, |m, k| m.max(k.y + k.h));
        assert_eq!(width, 22.5, "total width, numeric keypad included");
        assert_eq!(height, 6.5, "total height, function row included");
        assert!(keys.iter().all(|k| k.x >= 0.0 && k.y >= 0.0));
    }
}
