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

use crate::layout::{Key, Layout, Lights, Outline, Port, Shape, EMPTY, NO_SCANCODE};
use crate::lighting::{Lighting, Outgoing, Wire};

/// The built-in definitions, by file name, as the repository holds them.
pub const BUILTIN: &[(&str, &str)] = &[(
    "alienware-m18-r1-zones.json",
    include_str!("../devices/alienware-m18-r1-zones.json"),
)];

// ---------------------------------------------------------------- the file

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    name: String,
    #[serde(rename = "match")]
    matching: Match,
    report: ReportSpec,
    frame: FrameSpec,
    lights: LightsSpec,
    #[serde(default)]
    outline: Vec<PartSpec>,
    #[serde(default)]
    examples: Vec<Example>,
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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LightsSpec {
    kind: String,
    items: Vec<ItemSpec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemSpec {
    address: u16,
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
/// frame fills in.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Token {
    Byte(u8),
    R,
    G,
    B,
    Address,
    Count,
    /// The group's addresses, one byte each: as many as the group holds.
    Addresses,
}

fn tokens(text: &str) -> Result<Vec<Token>, String> {
    text.split_whitespace()
        .map(|word| match word {
            "{r}" => Ok(Token::R),
            "{g}" => Ok(Token::G),
            "{b}" => Ok(Token::B),
            "{address}" => Ok(Token::Address),
            "{count}" => Ok(Token::Count),
            "{addresses}" => Ok(Token::Addresses),
            _ if word.len() == 2 => u8::from_str_radix(word, 16)
                .map(Token::Byte)
                .map_err(|_| format!("“{word}” is neither a byte nor a placeholder")),
            _ => Err(format!("“{word}” is neither a byte nor a placeholder")),
        })
        .collect()
}

fn hex_bytes(text: &str) -> Result<Vec<u8>, String> {
    tokens(text)?
        .into_iter()
        .map(|t| match t {
            Token::Byte(b) => Ok(b),
            _ => Err(format!("“{text}” holds a placeholder where only bytes go")),
        })
        .collect()
}

fn hex_u16(text: &str, what: &str) -> Result<u16, String> {
    u16::from_str_radix(text, 16).map_err(|_| format!("{what} “{text}” is not hexadecimal"))
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
    close: Vec<Vec<Token>>,
    pace: Option<(Duration, bool)>,
    /// The last image sent and when, for `pace`.
    last: Mutex<(Vec<Rgb>, Option<Instant>)>,
}

impl Template {
    fn render(&self, tokens: &[Token], colour: Rgb, addresses: &[u8]) -> Outgoing {
        let mut bytes = self.prefix.clone();
        for token in tokens {
            match token {
                Token::Byte(b) => bytes.push(*b),
                Token::R => bytes.push(colour.r),
                Token::G => bytes.push(colour.g),
                Token::B => bytes.push(colour.b),
                Token::Address => bytes.push(addresses.first().copied().unwrap_or(0)),
                Token::Count => bytes.push(addresses.len() as u8),
                Token::Addresses => bytes.extend_from_slice(addresses),
            }
        }
        // Checked at load: the longest a report can grow still fits.
        bytes.resize(self.prefix.len() + self.length, 0);
        Outgoing {
            bytes,
            wire: self.wire,
            command: None,
        }
    }

    /// The most bytes one template can grow to, a whole chunk of addresses in it.
    fn widest(&self, tokens: &[Token]) -> usize {
        tokens
            .iter()
            .map(|t| {
                if *t == Token::Addresses {
                    self.chunk
                } else {
                    1
                }
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

        let none = Rgb::new(0, 0, 0);
        let mut out: Vec<Outgoing> = self
            .open
            .iter()
            .map(|t| self.render(t, none, &[]))
            .collect();

        let lit = layout
            .matrix
            .iter()
            .zip(frame)
            .filter(|(address, _)| **address != EMPTY)
            .map(|(&address, &colour)| (address as u8, colour));
        // Lights sharing a colour go in one selection when the device takes it
        // that way, in the order they first appear.
        let mut groups: Vec<(Rgb, Vec<u8>)> = Vec::new();
        for (address, colour) in lit {
            match groups
                .iter_mut()
                .find(|(seen, _)| self.per_colour && *seen == colour)
            {
                Some((_, addresses)) => addresses.push(address),
                None => groups.push((colour, vec![address])),
            }
        }
        for (colour, addresses) in groups {
            for chunk in addresses.chunks(self.chunk) {
                out.extend(self.send.iter().map(|t| self.render(t, colour, chunk)));
            }
        }

        out.extend(self.close.iter().map(|t| self.render(t, none, &[])));
        out
    }

    // A definition carries no firmware effect yet: *Off* is a black frame.
    fn firmware_effect(&self, _id: &str, _colours: &[Rgb]) -> Option<Outgoing> {
        None
    }

    // Nor any read: nothing is asked of the device on open.
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
    let (layout, _) = parse(json)?;
    Ok(Box::leak(Box::new(layout)))
}

fn parse(json: &str) -> Result<(Layout, Vec<Example>), String> {
    let d: Definition = serde_json::from_str(json).map_err(|e| e.to_string())?;

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
    let fixed = |list: &[String]| -> Result<Vec<Vec<Token>>, String> {
        list.iter()
            .map(|text| {
                let t = tokens(text)?;
                if t.iter().any(|t| !matches!(t, Token::Byte(_))) {
                    return Err(format!(
                        "“{text}” opens or closes a frame: it takes no placeholder"
                    ));
                }
                Ok(t)
            })
            .collect()
    };
    let template = Template {
        wire,
        length: d.report.length,
        prefix: hex_bytes(&d.report.prefix)?,
        open: fixed(&d.frame.open)?,
        per_colour,
        chunk,
        send: d
            .frame
            .each
            .send
            .iter()
            .map(|t| tokens(t))
            .collect::<Result<_, _>>()?,
        close: fixed(&d.frame.close)?,
        pace: d
            .frame
            .pace
            .map(|p| (Duration::from_millis(p.at_least_ms), p.skip_unchanged)),
        last: Mutex::new((Vec::new(), None)),
    };
    for t in template
        .open
        .iter()
        .chain(&template.send)
        .chain(&template.close)
    {
        if template.widest(t) > template.length {
            return Err(format!("a report grows past its {} bytes", template.length));
        }
    }
    if !per_colour
        && template
            .send
            .iter()
            .flatten()
            .any(|t| *t == Token::Addresses)
    {
        return Err("{addresses} needs lights grouped by colour".into());
    }

    let lights = match d.lights.kind.as_str() {
        "zones" => Lights::Zones,
        "keys" => Lights::Keys,
        other => return Err(format!("lights “{other}”: “keys” or “zones”")),
    };
    let mut keys = Vec::with_capacity(d.lights.items.len());
    let mut matrix = Vec::with_capacity(d.lights.items.len());
    for (position, item) in d.lights.items.iter().enumerate() {
        if matrix.contains(&item.address) {
            return Err(format!("address {} is given twice", item.address));
        }
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
            scancode: NO_SCANCODE,
            x,
            y,
            w,
            h,
            shape,
        });
        matrix.push(item.address);
    }
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
        firmware_effects: &[],
        lights,
        rows: 1,
        cols: u8::try_from(keys.len()).map_err(|_| "more than 255 lights in one row")?,
        matrix: Box::leak(matrix.into_boxed_slice()),
        keys: Box::leak(keys.into_boxed_slice()),
        outline: Box::leak(outline.into_boxed_slice()),
    };
    Ok((layout, d.examples))
}

/// Replays a definition's reference frames: the reports it gives must be the
/// ones its contributor saw, byte for byte (§8 of the design). Each is written
/// without the zeros that pad it to its length.
pub fn replay(json: &str) -> Result<(), String> {
    let (layout, examples) = parse(json)?;
    if examples.is_empty() {
        return Err("no reference frame to replay".into());
    }
    let d: Definition = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let prefix = hex_bytes(&d.report.prefix)?.len();
    for (n, example) in examples.iter().enumerate() {
        let colours = example
            .colours
            .iter()
            .map(|c| colour(c))
            .collect::<Result<Vec<_>, _>>()?;
        // A fresh family per example: pacing belongs to a device, not a test.
        let (fresh, _) = parse(json)?;
        let sent: Vec<Vec<u8>> = fresh
            .lighting
            .frame(&layout, &colours)
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

    /// §8: every built-in definition loads, and gives the reports its
    /// contributor saw.
    #[test]
    fn built_in_definitions_replay_their_reference_frames() {
        for (name, json) in BUILTIN {
            load(json).unwrap_or_else(|e| panic!("{name}: {e}"));
            replay(json).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }

    /// The zones' definition gives, byte for byte, what the Rust family it
    /// replaces gave — whatever the colours, repeated or not.
    #[test]
    fn the_zones_definition_gives_what_the_zones_family_gave() {
        let json = BUILTIN[0].1;
        let frames = [
            [
                Rgb::new(255, 0, 0),
                Rgb::new(0, 255, 0),
                Rgb::new(0, 0, 255),
            ],
            [Rgb::new(9, 9, 9); 3],
            [Rgb::new(1, 2, 3), Rgb::new(1, 2, 3), Rgb::new(0, 0, 0)],
            [Rgb::new(0, 0, 0), Rgb::new(200, 100, 50), Rgb::new(0, 0, 0)],
        ];
        for frame in frames {
            let (layout, _) = parse(json).unwrap();
            let family = crate::lighting::AlienwareZones::new();
            let theirs: Vec<Vec<u8>> = family
                .frame(&crate::ALIENWARE_M18_R1_ZONES, &frame)
                .into_iter()
                .map(|o| o.bytes)
                .collect();
            let ours: Vec<Vec<u8>> = layout
                .lighting
                .frame(&layout, &frame)
                .into_iter()
                .map(|o| o.bytes)
                .collect();
            assert_eq!(ours, theirs, "{frame:?}");
        }
    }

    #[test]
    fn a_frame_unchanged_or_too_soon_sends_nothing() {
        let (layout, _) = parse(BUILTIN[0].1).unwrap();
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
        let json = BUILTIN[0].1;
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
            "takes no placeholder",
        );
        refused(
            with("\"wire\": \"output\"", "\"wire\": \"usb\""),
            "“output” or “feature”",
        );
    }

    #[test]
    fn a_reference_frame_that_disagrees_is_named() {
        let e = replay(&with("\"03 21 00 03 00 ff\"\n", "\"03 21 00 03 00 fe\"\n")).unwrap_err();
        assert!(e.starts_with("reference frame 1"), "{e}");
    }
}
