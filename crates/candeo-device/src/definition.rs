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

use candeo_protocol::Rgb;
use serde::Deserialize;

use crate::layout::{
    FirmwareEffect, Key, Layout, Lights, Outline, Port, Shape, EMPTY, NO_SCANCODE,
};
use crate::lighting::{Lighting, Outgoing, Wire};

/// The built-in definitions, by file name, as the repository holds them.
pub const BUILTIN: &[(&str, &str)] = &[
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
        .any(|t| !matches!(t, Token::Byte(_)) && !allowed.contains(t))
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
}

const NOTHING: Fill<'static> = Fill {
    colour: BLACK,
    colour2: BLACK,
    lights: &[],
    level: 0,
    kind: 0,
};

/// A firmware's effects: the report, *Off*'s kind, and each effect's id and
/// kind.
type Firmware = (Vec<Token>, Option<u8>, Vec<(String, u8)>);

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
    brightness: Option<Vec<Vec<Token>>>,
    firmware: Option<Firmware>,
    take_over: Vec<Vec<Token>>,
    pace: Option<(Duration, bool)>,
    /// The last image sent and when, for `pace`.
    last: Mutex<(Vec<Rgb>, Option<Instant>)>,
}

impl Template {
    fn put(&self, bytes: &mut Vec<u8>, tokens: &[Token], fill: Fill) {
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
                        self.put(bytes, &self.light, one);
                    }
                }
                Token::Level => bytes.push(fill.level),
                Token::Kind => bytes.push(fill.kind),
            }
        }
    }

    fn render(&self, tokens: &[Token], fill: Fill) -> Outgoing {
        let mut bytes = self.prefix.clone();
        self.put(&mut bytes, tokens, fill);
        // Checked at load: the longest a report can grow still fits.
        bytes.resize(self.prefix.len() + self.length, 0);
        Outgoing {
            bytes,
            wire: self.wire,
            command: None,
        }
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

    fn firmware_effect(&self, id: &str, colours: &[Rgb]) -> Option<Outgoing> {
        let (send, off, effects) = self.firmware.as_ref()?;
        let kind = if id == OFF {
            (*off)?
        } else {
            effects.iter().find(|(known, _)| known == id)?.1
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

    // Nothing is asked of the device on open: a definition carries no read yet.
    fn inspect(
        &self,
        _device: &hidapi::HidDevice,
        accept: &mut dyn FnMut(Option<&str>) -> bool,
    ) -> Option<crate::Inspection> {
        accept(None).then(crate::Inspection::unread)
    }
}

// ---------------------------------------------------------------- loading

/// A definition read, checked and turned into a layout, or why not, in a
/// sentence for whoever wrote it.
///
/// The layout lives as long as the process: devices are listed once, and every
/// part of the application holds them as `&'static`.
pub fn load(json: &str) -> Result<&'static Layout, String> {
    let (layout, _, _) = parse(json)?;
    Ok(Box::leak(Box::new(layout)))
}

fn family(d: &Definition) -> Result<Template, String> {
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
    let chunk = d.frame.each.chunk.unwrap_or(1);
    if chunk == 0 {
        return Err("chunk 0: a report carries at least one light".into());
    }
    let fixed = |list: &[String], place: &str| -> Result<Vec<Vec<Token>>, String> {
        list.iter().map(|t| template(t, &[], place)).collect()
    };
    let light = match &d.frame.each.light {
        Some(text) => template(text, &[Address, R, G, B], "a light's entry")?,
        None => Vec::new(),
    };
    let mut in_send = vec![R, G, B, Address, Count, Addresses];
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
    let firmware = match &d.firmware {
        None => None,
        Some(f) => Some((
            template(&f.send, &[Kind, R, G, B, R2, G2, B2], "a firmware effect")?,
            f.off.as_deref().map(|k| hex_u8(k, "kind")).transpose()?,
            f.effects
                .iter()
                .map(|e| Ok((e.id.clone(), hex_u8(&e.kind, "kind")?)))
                .collect::<Result<Vec<_>, String>>()?,
        )),
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
        .chain(t.firmware.iter().map(|f| &f.0));
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
    let template = family(&d)?;

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
        surveyed_firmware: None,
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
        let fresh = family(&d)?;
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

    const ZONES: &str = BUILTIN[1].1;
    const KEYBOARD: &str = BUILTIN[0].1;

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
            let mut codes = std::collections::BTreeMap::<u16, usize>::new();
            for key in l.keys.iter().filter(|k| k.scancode != NO_SCANCODE) {
                *codes.entry(key.scancode).or_default() += 1;
            }
            assert!(
                codes.values().all(|n| *n == 1),
                "{name}: a scancode names two keys"
            );
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
