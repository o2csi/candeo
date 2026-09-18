//! What we ask a device **once**, when it is opened.
//!
//! # Why this module exists
//!
//! `reachingKeyboard` proves that a write was *accepted* by the system, not that it
//! was *understood* by the device. Should a firmware update move a byte or renumber
//! a command, `hidapi` would still accept the transfer, `present()` would return
//! `Ok`, and the keyboard would silently discard our frames — a green light above a
//! frozen keyboard. The survey established that the device **answers** (§8): this
//! is where we make it say what it is.
//!
//! # What the inspection establishes, and what it does not
//!
//! | Question | Answer | Scope |
//! |---|---|---|
//! | which firmware? | `0x00`/`0x81` | compared with the survey: **warns, does not block** |
//! | which unit? | `0x00`/`0x82` | the USB descriptor carries none |
//! | does this command exist? | status `0x05` or `0x02` | the class / command pair, **nothing more** |
//!
//! ⚠️ **The status byte never validates an argument.** Setting effect `0x05`, which
//! this keyboard refuses, returns `0x02` "understood". A [`Verdict::Understood`]
//! command is therefore a **known** command, and that is all: this module cannot say
//! "my arguments are right", and nothing it returns may suggest otherwise.
//!
//! # Sending without changing anything
//!
//! Checking that a command exists requires **sending** it, and a lighting command
//! that is sent shows. So we only send what we know how to rewrite **identically**:
//! read the current state, rewrite it as is, read it again.
//!
//! - **brightness** — read back through `0x0f`/`0x84`, rewritten as is;
//! - **effect** — read back through `0x0f`/`0x82`, rewritten as is **only** if it is
//!   one whose read-back returns every argument. `Static` and `Breathing` carry a
//!   color whose position in the read-back is not established: rewriting them could
//!   turn them off, so we refrain and say so. Rewriting a running Spectrum Cycle or
//!   Wave shows no visible restart: watched on the keyboard, firmware v1.5, the
//!   first with #35, the Wave on 14/09/2026 (#74);
//! - **row** — **never sent.** No color read-back exists, so no written row is
//!   invisible.
//!
//! The second read-back does not detect an **ignored** command — rewriting the
//! current value to no effect leaves it unchanged. It detects a command
//! **understood differently**: a firmware reading the argument elsewhere would set
//! another value, and that is what we would read back.
//!
//! Without a successful read-back, no write goes out: each rewrite depends on the
//! read that precedes it.
//!
//! # Once, never in the loop
//!
//! Reading back costs a USB round trip, and the render loop is tight: writing a full
//! frame takes 13 to 14 ms in a 33.3 ms period. The verdict is therefore reached on
//! open and **kept**: [`crate::Keyboard`] then refuses to send a command answered
//! `0x05`, without reading anything back.

use std::time::Duration;

use candeo_protocol::{
    CommandId, Effect, Firmware, Report, Response, Status, SET_BRIGHTNESS, SET_EFFECT, WRITE_ROW,
};

use crate::Layout;

/// Re-reads granted to a "busy" response.
///
/// Never observed during the survey — immediate reads returned their status
/// straight away — but provided for by the protocol. Bounded: an open must not hang
/// on a device that would claim to be busy indefinitely.
const RELECTURES: u32 = 5;

/// Wait between two "busy" re-reads. Five times ten milliseconds stays under a frame
/// period, and is only paid once per open.
const BUSY_DELAY: Duration = Duration::from_millis(10);

/// What an inspection needs from a device.
///
/// A seam rather than a hard-wired `hidapi::HidDevice`, for the only reason that
/// counts: it is what makes the sequence verifiable **without hardware** — a device
/// that refuses, that answers busy, that understands an argument the wrong way.
/// Without it, these cases could only be checked with the firmware that produces
/// them, so never before it ships.
pub(crate) trait Transport {
    fn send(&self, data: &[u8]) -> Result<(), String>;
    fn receive(&self, buf: &mut [u8]) -> Result<usize, String>;
}

impl Transport for hidapi::HidDevice {
    fn send(&self, data: &[u8]) -> Result<(), String> {
        hidapi::HidDevice::send_feature_report(self, data).map_err(|e| e.to_string())
    }

    fn receive(&self, buf: &mut [u8]) -> Result<usize, String> {
        hidapi::HidDevice::get_feature_report(self, buf).map_err(|e| e.to_string())
    }
}

/// What the device said about itself when it was opened.
///
/// Each field carries **its own** failure reason rather than a silent `Option`:
/// "not read" without saying why is exactly the kind of silence that sends someone
/// hunting for a device fault where there is only a missing permission.
#[derive(Clone, PartialEq, Eq)]
pub struct Inspection {
    /// The version, or why it was not read.
    pub firmware: Result<Firmware, String>,
    /// The serial number, or why it was not read.
    ///
    /// ⚠️ **It identifies one specific unit.** Nothing that gets logged or copied
    /// into a bug report may carry it in clear.
    pub serial: Result<String, String>,
    /// One check per write command the lighting depends on.
    pub checks: Vec<Check>,
}

/// Written by hand, for a single reason: **the serial is not in it.**
///
/// A `{:?}` slipped into a `tracing::debug!` the day someone chases a fault is the
/// shortest path to a serial number pasted into an issue. The stable fingerprint
/// that replaces it in the log lives on the application side; here, we simply
/// disclose nothing.
impl std::fmt::Debug for Inspection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let serial = match &self.serial {
            Ok(s) => format!("Ok(<hidden, {} characters>)", s.chars().count()),
            Err(e) => format!("Err({e:?})"),
        };
        f.debug_struct("Inspection")
            .field("firmware", &self.firmware)
            .field("serial", &format_args!("{serial}"))
            .field("checks", &self.checks)
            .finish()
    }
}

/// The verdict on **one** command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub command: CommandId,
    /// The command's name for a person, `brightness`: the application translates
    /// it.
    pub name: &'static str,
    pub verdict: Verdict,
}

/// What the device answered to a command sent on open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Rewritten identically, answered `0x02`, and read back unchanged.
    ///
    /// **Known**, and nothing more: the status byte validates no argument.
    Understood,
    /// The device answered `0x05`: this firmware does not know the command.
    /// [`crate::Keyboard`] will not send it any more.
    Unsupported,
    /// Answered `0x02`, but the read-back does not return what was rewritten.
    ///
    /// The only case where the device says "understood" and shows the opposite: it
    /// interprets the argument differently than during the survey.
    ReadBackDiffers { wrote: String, read: String },
    /// Not sent, or no conclusion possible — and why.
    Unverified(String),
}

impl Inspection {
    /// What is known about a device of a family whose protocol has no read yet:
    /// nothing — and it says so rather than pretending to have asked.
    ///
    /// No check either: [`Self::refuses`] then refuses nothing, which is right.
    /// A device that has never answered has never declined anything.
    pub fn unread() -> Self {
        Self {
            firmware: Err("no known command reads a version from this device".into()),
            serial: Err("no known command reads a serial from this device".into()),
            checks: Vec::new(),
        }
    }

    /// True if the device declared it does not know this command.
    ///
    /// Only [`Verdict::Unsupported`] refuses: an unverified command is not a refused
    /// command, and blocking it would silence the device over a question we simply
    /// could not answer.
    pub fn refuses(&self, command: CommandId) -> bool {
        self.checks
            .iter()
            .any(|c| c.command == command && c.verdict == Verdict::Unsupported)
    }

    /// What deserves to be **seen**.
    ///
    /// An empty list means "nothing to report", **not** "compatible": an unverified
    /// command is not listed, because it is not an anomaly — it is the limit of what
    /// can be asked without changing anything on the keyboard.
    ///
    /// No warning blocks anything. Blocking on a different version would make the
    /// application useless after a routine update, when the protocol will most
    /// likely not have changed; the warning turns a silent failure into a stated
    /// suspicion.
    pub fn warnings(&self, layout: &Layout) -> Vec<Warning> {
        let mut out = Vec::new();
        // A layout surveyed against no version has nothing to compare: warning
        // about a difference would be inventing one.
        if let Some(surveyed) = layout.surveyed_firmware {
            match &self.firmware {
                Ok(read) if *read != surveyed => out.push(Warning::FirmwareDiffers {
                    read: *read,
                    surveyed,
                }),
                Ok(_) => {}
                Err(reason) => out.push(Warning::FirmwareNotRead {
                    reason: reason.clone(),
                    surveyed,
                }),
            }
        }
        for c in &self.checks {
            match &c.verdict {
                Verdict::Unsupported => out.push(Warning::Unsupported {
                    name: c.name,
                    command: c.command,
                }),
                Verdict::ReadBackDiffers { wrote, read } => out.push(Warning::ReadBackDiffers {
                    name: c.name,
                    command: c.command,
                    wrote: wrote.clone(),
                    read: read.clone(),
                }),
                Verdict::Understood | Verdict::Unverified(_) => {}
            }
        }
        out
    }
}

/// A finding of the inspection, for the application to say in its language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    FirmwareDiffers {
        read: Firmware,
        surveyed: Firmware,
    },
    FirmwareNotRead {
        reason: String,
        surveyed: Firmware,
    },
    Unsupported {
        name: &'static str,
        command: CommandId,
    },
    ReadBackDiffers {
        name: &'static str,
        command: CommandId,
        wrote: String,
        read: String,
    },
}

/// In English, for the log and the diagnostic.
impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FirmwareDiffers { read, surveyed } => write!(
                f,
                "firmware {read}, while this layout was surveyed on {surveyed}"
            ),
            Self::FirmwareNotRead { reason, surveyed } => write!(
                f,
                "firmware version not read ({reason}), cannot compare with the surveyed \
                 {surveyed}"
            ),
            Self::Unsupported { name, command } => write!(
                f,
                "the firmware does not know the {name} command ({command}), no longer sent"
            ),
            Self::ReadBackDiffers {
                name,
                command,
                wrote,
                read,
            } => write!(
                f,
                "{name} command ({command}) accepted, but reads back {read} after {wrote} was \
                 rewritten"
            ),
        }
    }
}

/// Inspects a device that was just opened, whatever unit it is.
#[cfg(test)]
pub(crate) fn inspect(t: &impl Transport) -> Inspection {
    inspect_if(t, |_| true).expect("a unit nobody refuses is inspected")
}

/// Inspects a device that was just opened. **Cannot fail**: every unanswered
/// question becomes a reason, never an open error.
///
/// The identity comes first, in class `0x00`, which is read only. `accept` is
/// asked about the serial then, and a refused unit gets no rewrite: `None`
/// (#74).
pub(crate) fn inspect_if(
    t: &impl Transport,
    accept: impl FnOnce(Option<&str>) -> bool,
) -> Option<Inspection> {
    let firmware = read(t, &Report::read_firmware())
        .and_then(|r| r.firmware().ok_or_else(|| "empty response".to_string()));
    let serial = read(t, &Report::read_serial())
        .and_then(|r| r.serial().ok_or_else(|| "unreadable response".to_string()));
    if !accept(serial.as_deref().ok()) {
        return None;
    }

    let checks = vec![
        Check {
            command: SET_BRIGHTNESS,
            name: "brightness",
            verdict: check_brightness(t),
        },
        Check {
            command: SET_EFFECT,
            name: "effect",
            verdict: check_effect(t),
        },
        Check {
            command: WRITE_ROW,
            name: "row",
            verdict: Verdict::Unverified(
                "never sent on open: no color read-back exists, any written row would show on \
                 the keyboard"
                    .into(),
            ),
        },
    ];

    Some(Inspection {
        firmware,
        serial,
        checks,
    })
}

fn check_brightness(t: &impl Transport) -> Verdict {
    let level = match read(t, &Report::read_brightness()) {
        Ok(r) => match r.brightness() {
            Some(n) => n,
            None => return Verdict::Unverified("brightness read back in an unknown form".into()),
        },
        Err(e) => {
            return Verdict::Unverified(format!("brightness not read back, nothing sent: {e}"))
        }
    };
    rewrite(
        t,
        &Report::set_brightness(level),
        &Report::read_brightness(),
        level,
        Response::brightness,
        |n| n.to_string(),
    )
}

fn check_effect(t: &impl Transport) -> Verdict {
    let effect = match read(t, &Report::read_effect()) {
        Ok(r) => match r.effect() {
            Some(e) => e,
            None => {
                return Verdict::Unverified(
                    "the current effect cannot be rewritten identically (color or shape not \
                     established by the survey), nothing sent"
                        .into(),
                )
            }
        },
        Err(e) => return Verdict::Unverified(format!("effect not read back, nothing sent: {e}")),
    };
    rewrite(
        t,
        &Report::set_effect(effect),
        &Report::read_effect(),
        effect,
        Response::effect,
        effect_name,
    )
}

/// Rewrites a value that was read back, then reads it again.
fn rewrite<T: PartialEq + Copy>(
    t: &impl Transport,
    write: &Report,
    reread: &Report,
    before: T,
    decode: impl Fn(&Response) -> Option<T>,
    render: impl Fn(T) -> String,
) -> Verdict {
    let response = match exchange(t, write) {
        Ok(r) => r,
        Err(e) => return Verdict::Unverified(format!("rewrite without a response: {e}")),
    };
    match response.status() {
        Status::Understood => {}
        Status::Unsupported => return Verdict::Unsupported,
        other => return Verdict::Unverified(format!("rewrite answered {other}")),
    }
    match read(t, reread) {
        Ok(r) => match decode(&r) {
            Some(after) if after == before => Verdict::Understood,
            Some(after) => Verdict::ReadBackDiffers {
                wrote: render(before),
                read: render(after),
            },
            None => Verdict::ReadBackDiffers {
                wrote: render(before),
                read: "an unknown form".into(),
            },
        },
        Err(e) => Verdict::Unverified(format!("rewrite understood, but not read back: {e}")),
    }
}

/// An effect, as named to a person — not the name of a Rust variant.
fn effect_name(effect: Effect) -> String {
    match effect {
        Effect::Off => "off".into(),
        Effect::SpectrumCycle => "spectrum".into(),
        Effect::Wave { direction, speed } => {
            format!("wave (direction {direction}, speed {speed})")
        }
        Effect::Custom => "host-controlled".into(),
    }
}

/// The effect the firmware runs now, read back through `0x0f`/`0x82` — the read
/// [`check_effect`] makes on opening, and nothing more. `None` for an effect this
/// crate cannot describe: Static and Breathing carry a colour whose place in the
/// read-back is not established.
pub(crate) fn read_effect(t: &impl Transport) -> Result<Option<Effect>, String> {
    read(t, &Report::read_effect()).map(|r| r.effect())
}

/// A read, which is only worth anything once understood.
fn read(t: &impl Transport, request: &Report) -> Result<Response, String> {
    let r = exchange(t, request)?;
    match r.status() {
        Status::Understood => Ok(r),
        other => Err(format!("{} answers status {other}", request.id())),
    }
}

/// Sends a command and reads back **its** response, whatever its status.
///
/// The echo is checked: without it, a command sent in the meantime on the same
/// interface — a render loop holding another handle — would have its response read
/// as ours. Better to conclude nothing than to conclude on someone else's response.
fn exchange(t: &impl Transport, request: &Report) -> Result<Response, String> {
    t.send(&request.to_feature_buffer())
        .map_err(|e| format!("write refused: {e}"))?;
    for _ in 0..RELECTURES {
        let mut buf = Response::buffer();
        let received = t
            .receive(&mut buf)
            .map_err(|e| format!("read refused: {e}"))?;
        let r = Response::from_feature_buffer(&buf, received)
            .ok_or_else(|| format!("truncated response: {received} bytes"))?;
        if r.status() == Status::Busy {
            std::thread::sleep(BUSY_DELAY);
            continue;
        }
        if !r.answers(request) {
            return Err(format!(
                "response to {} read instead of {}",
                r.id(),
                request.id()
            ));
        }
        // The device sets a correct checksum in its responses: 8 of 8 on an open,
        // firmware v1.5, 14/09/2026 (#74). A mismatch is a garbled reply.
        if !r.checksum_matches() {
            return Err(format!("response to {} has a wrong checksum", r.id()));
        }
        return Ok(r);
    }
    Err(format!("still busy after {RELECTURES} reads"))
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::DEATHSTALKER_V2_PRO;

    /// A simulated device that answers as the survey described — except where a
    /// test asks it not to.
    struct Fake {
        state: RefCell<State>,
    }

    struct State {
        version: [u8; 2],
        serial: &'static [u8],
        /// The six argument bytes of the current effect.
        effect: [u8; 6],
        brightness: u8,
        /// Commands the device answers `0x05` to.
        unknown: Vec<CommandId>,
        /// Reads the brightness argument one byte too early — a firmware that would
        /// have moved the argument.
        brightness_shifted: bool,
        /// "Busy" responses to return before the real one.
        busy: u32,
        /// Every read fails, as without read access.
        read_denied: bool,
        /// Forced echo, as if another command had been sent in the meantime.
        echo: Option<CommandId>,
        /// Every response carries a wrong checksum.
        garbled: bool,
        last: Option<Report>,
        sent: Vec<CommandId>,
    }

    impl Fake {
        fn as_surveyed() -> Self {
            let spectrum = Report::set_effect(Effect::SpectrumCycle).0;
            let mut effect = [0u8; 6];
            effect.copy_from_slice(&spectrum[8..14]);
            Self {
                state: RefCell::new(State {
                    version: [0x01, 0x05],
                    serial: b"XY24ABCDEFG0001",
                    effect,
                    brightness: 0xff,
                    unknown: Vec::new(),
                    brightness_shifted: false,
                    busy: 0,
                    read_denied: false,
                    echo: None,
                    garbled: false,
                    last: None,
                    sent: Vec::new(),
                }),
            }
        }

        fn with(self, f: impl FnOnce(&mut State)) -> Self {
            f(&mut self.state.borrow_mut());
            self
        }

        fn sent(&self) -> Vec<CommandId> {
            self.state.borrow().sent.clone()
        }
    }

    impl Transport for Fake {
        fn send(&self, data: &[u8]) -> Result<(), String> {
            let mut r = [0u8; 90];
            r.copy_from_slice(&data[1..]);
            let report = Report(r);
            let id = report.id();
            let mut e = self.state.borrow_mut();
            e.sent.push(id);
            if !e.unknown.contains(&id) {
                if id == SET_EFFECT {
                    e.effect.copy_from_slice(&r[8..14]);
                }
                if id == SET_BRIGHTNESS {
                    e.brightness = if e.brightness_shifted { r[9] } else { r[10] };
                }
            }
            e.last = Some(report);
            Ok(())
        }

        fn receive(&self, buf: &mut [u8]) -> Result<usize, String> {
            let mut e = self.state.borrow_mut();
            if e.read_denied {
                return Err("read access denied".into());
            }
            let request = e.last.as_ref().expect("read without a command").id();
            buf.fill(0);
            if e.busy > 0 {
                e.busy -= 1;
                buf[1] = 0x01;
            } else {
                let echo = e.echo.unwrap_or(request);
                buf[1 + 6] = echo.class;
                buf[1 + 7] = echo.command;
                if e.unknown.contains(&request) {
                    buf[1] = 0x05;
                } else {
                    buf[1] = 0x02;
                    let args = &mut buf[1 + 8..];
                    match (request.class, request.command) {
                        (0x00, 0x81) => args[..2].copy_from_slice(&e.version),
                        (0x00, 0x82) => args[..e.serial.len()].copy_from_slice(e.serial),
                        (0x0f, 0x82) => args[..6].copy_from_slice(&e.effect),
                        (0x0f, 0x84) => args[2] = e.brightness,
                        _ => {}
                    }
                }
            }
            // As the device does (#74), unless the test garbles it.
            let mut report = [0u8; 90];
            report.copy_from_slice(&buf[1..91]);
            buf[1 + 88] = candeo_protocol::checksum(&report) ^ u8::from(e.garbled);
            Ok(buf.len())
        }
    }

    fn verdict(i: &Inspection, command: CommandId) -> &Verdict {
        &i.checks
            .iter()
            .find(|c| c.command == command)
            .expect("command not checked")
            .verdict
    }

    /// The survey case: v1.5, serial read, brightness and effect known.
    #[test]
    fn a_device_matching_the_survey_raises_nothing() {
        let fake = Fake::as_surveyed();
        let i = inspect(&fake);

        assert_eq!(i.firmware, Ok(Firmware { major: 1, minor: 5 }));
        assert_eq!(i.serial.as_deref(), Ok("XY24ABCDEFG0001"));
        assert_eq!(verdict(&i, SET_BRIGHTNESS), &Verdict::Understood);
        assert_eq!(verdict(&i, SET_EFFECT), &Verdict::Understood);
        assert!(i.warnings(&DEATHSTALKER_V2_PRO).is_empty());
        assert!(!i.refuses(SET_EFFECT));
    }

    /// **The inspection changes nothing in what the keyboard shows.** That is the
    /// condition for being allowed to send on every open.
    #[test]
    fn inspection_leaves_the_keyboard_as_it_found_it() {
        let wave = Report::set_effect(Effect::Wave {
            direction: 0x02,
            speed: 0x28,
        })
        .0;
        let fake = Fake::as_surveyed().with(|e| {
            e.effect.copy_from_slice(&wave[8..14]);
            e.brightness = 0x40;
        });
        let i = inspect(&fake);

        assert_eq!(verdict(&i, SET_EFFECT), &Verdict::Understood);
        let e = fake.state.borrow();
        assert_eq!(e.effect[..], wave[8..14], "the effect changed");
        assert_eq!(e.brightness, 0x40, "the brightness changed");
    }

    /// The effect an automation gives back is the one read here: a Wave comes back
    /// with its direction and speed, and a colored effect reads as unknown rather
    /// than as something else.
    #[test]
    fn the_current_effect_reads_back() {
        let wave = Effect::Wave {
            direction: 0x01,
            speed: 0x28,
        };
        let wave_bytes = Report::set_effect(wave).0;
        let fake = Fake::as_surveyed().with(|e| e.effect.copy_from_slice(&wave_bytes[8..14]));
        assert_eq!(read_effect(&fake), Ok(Some(wave)));

        let breathing = Fake::as_surveyed().with(|e| e.effect = [0, 0, 0x02, 0, 0, 0x01]);
        assert_eq!(read_effect(&breathing), Ok(None));
    }

    /// Another version **warns**, and nothing else: no command is refused because of
    /// it.
    #[test]
    fn another_version_warns_without_blocking() {
        let fake = Fake::as_surveyed().with(|e| e.version = [0x01, 0x06]);
        let i = inspect(&fake);

        assert_eq!(
            i.warnings(&DEATHSTALKER_V2_PRO),
            [Warning::FirmwareDiffers {
                read: Firmware { major: 1, minor: 6 },
                surveyed: Firmware { major: 1, minor: 5 },
            }]
        );
        assert!(!i.refuses(SET_EFFECT) && !i.refuses(SET_BRIGHTNESS));
    }

    /// The device says it does not know the command: it is named on screen, and the
    /// keyboard will not send it any more.
    #[test]
    fn an_unknown_command_is_named_and_refused() {
        let fake = Fake::as_surveyed().with(|e| e.unknown.push(SET_EFFECT));
        let i = inspect(&fake);

        assert_eq!(verdict(&i, SET_EFFECT), &Verdict::Unsupported);
        assert!(i.refuses(SET_EFFECT));
        assert!(!i.refuses(SET_BRIGHTNESS));
        assert_eq!(
            i.warnings(&DEATHSTALKER_V2_PRO),
            [Warning::Unsupported {
                name: "effect",
                command: SET_EFFECT,
            }]
        );
    }

    /// A firmware reading the argument elsewhere would answer `0x02` and set
    /// something else. The second read-back sees it — the status byte would know
    /// nothing about it.
    #[test]
    fn an_argument_understood_differently_shows_on_read_back() {
        let fake = Fake::as_surveyed().with(|e| e.brightness_shifted = true);
        let i = inspect(&fake);

        assert!(matches!(
            verdict(&i, SET_BRIGHTNESS),
            Verdict::ReadBackDiffers { .. }
        ));
        // Understood differently is not unknown: we warn, we do not forbid.
        assert!(!i.refuses(SET_BRIGHTNESS));
        assert_eq!(i.warnings(&DEATHSTALKER_V2_PRO).len(), 1);
    }

    /// `Static` carries a color whose position in the read-back is not established:
    /// rewriting it could turn it off. Nothing is sent.
    #[test]
    fn a_colored_effect_is_never_rewritten() {
        let fake = Fake::as_surveyed().with(|e| e.effect = [0, 0, 0x01, 0, 0, 0x01]);
        let i = inspect(&fake);

        assert!(matches!(verdict(&i, SET_EFFECT), Verdict::Unverified(_)));
        assert!(
            !fake.sent().contains(&SET_EFFECT),
            "the effect was rewritten"
        );
        assert!(
            i.warnings(&DEATHSTALKER_V2_PRO).is_empty(),
            "a limit is not an anomaly"
        );
    }

    /// No row is invisible: the inspection never writes one.
    #[test]
    fn no_row_is_sent() {
        let fake = Fake::as_surveyed();
        let i = inspect(&fake);

        assert!(!fake.sent().contains(&WRITE_ROW));
        assert!(matches!(verdict(&i, WRITE_ROW), Verdict::Unverified(_)));
        // And an unverified command is not refused: the keyboard must still be able
        // to receive its frames.
        assert!(!i.refuses(WRITE_ROW));
    }

    /// **Driver mode is not written** (§8 of the survey): in class `0x00`, the
    /// inspection only reads.
    #[test]
    fn inspection_writes_nothing_in_the_information_class() {
        let fake = Fake::as_surveyed();
        inspect(&fake);
        for id in fake.sent() {
            assert!(
                id.class != 0x00 || id.command >= 0x80,
                "{id} writes to the information class"
            );
        }
    }

    /// Without any read — hidraw without permission, a driver that does not relay —
    /// **no write goes out**: each rewrite waits for its read-back.
    #[test]
    fn without_read_back_no_write_goes_out() {
        let fake = Fake::as_surveyed().with(|e| e.read_denied = true);
        let i = inspect(&fake);

        assert!(i.firmware.is_err());
        assert!(i.serial.is_err());
        let sent = fake.sent();
        assert!(!sent.contains(&SET_EFFECT), "{sent:?}");
        assert!(!sent.contains(&SET_BRIGHTNESS), "{sent:?}");
        // The unread version is reported, since it can no longer be compared.
        let warnings = i.warnings(&DEATHSTALKER_V2_PRO);
        assert!(
            matches!(warnings[..], [Warning::FirmwareNotRead { .. }]),
            "{warnings:?}"
        );
    }

    /// The serial number does not leak through a careless `{:?}`.
    #[test]
    fn debug_format_does_not_leak_the_serial() {
        let i = inspect(&Fake::as_surveyed());
        let text = format!("{i:?}");
        assert!(!text.contains("XY24ABCDEFG0001"), "{text}");
        assert!(text.contains("v1.5") || text.contains("major: 1"), "{text}");
    }

    /// Provided for by the protocol, never observed: a "busy" response is read again.
    #[test]
    fn a_busy_response_is_read_again() {
        let fake = Fake::as_surveyed().with(|e| e.busy = 2);
        let i = inspect(&fake);
        assert_eq!(i.firmware, Ok(Firmware { major: 1, minor: 5 }));
    }

    /// The response to another command is not taken for ours, even when its bytes
    /// would fit.
    #[test]
    fn a_response_to_another_command_concludes_nothing() {
        let fake = Fake::as_surveyed().with(|e| e.echo = Some(WRITE_ROW));
        let i = inspect(&fake);

        assert!(i.firmware.is_err());
        assert!(matches!(verdict(&i, SET_EFFECT), Verdict::Unverified(_)));
        assert!(!fake.sent().contains(&SET_EFFECT));
    }

    /// #74: a garbled reply is not read as an answer.
    #[test]
    fn a_response_with_a_wrong_checksum_concludes_nothing() {
        let fake = Fake::as_surveyed().with(|e| e.garbled = true);
        let i = inspect(&fake);

        assert!(i.firmware.is_err() && i.serial.is_err());
        assert!(!fake.sent().contains(&SET_BRIGHTNESS));
        assert!(!fake.sent().contains(&SET_EFFECT));
    }

    /// #74: another unit of the model is refused on its serial, **before** the
    /// inspection rewrites anything: it receives reads only.
    #[test]
    fn a_refused_unit_receives_no_write() {
        let fake = Fake::as_surveyed();
        let mut asked = None;
        let inspection = inspect_if(&fake, |serial| {
            asked = serial.map(str::to_owned);
            false
        });

        assert!(inspection.is_none());
        assert_eq!(asked.as_deref(), Some("XY24ABCDEFG0001"));
        assert!(
            fake.sent().iter().all(|id| id.command >= 0x80),
            "a refused unit was written to: {:?}",
            fake.sent()
        );
    }
}
