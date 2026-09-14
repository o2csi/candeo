//! Builds the reports of the Razer lighting protocol, and reads the
//! responses.
//!
//! Structure established by capturing the USB bus — see `docs/protocol/`.
//! No third-party code was consulted: this module derives only from frames
//! observed on the hardware.

/// Report length, excluding the HID report id.
///
/// ⚠️ **`pub` for the probes, and on purpose.** The only consumer outside this
/// crate is `apps/desktop/src-tauri/src/sonde.rs`, which is declared
/// `#[cfg(test)] mod sonde;` and whose probes are all `#[ignore]`: this constant
/// therefore never ships in the released binary, and a dead-code sweep will
/// always report it as restrictable.
///
/// Do not restrict it. These probes are kept to **replay the survey** on
/// another firmware or another unit — that is what established the protocol,
/// and it is the only way to re-establish it the day a device answers
/// differently. Cutting them to save a few characters of visibility would cost
/// that ability, and the connection would not be spotted again.
pub const REPORT_LEN: usize = 90;

/// Size of the buffer passed to `HidD_SetFeature`: report id + data.
pub(crate) const FEATURE_BUF_LEN: usize = REPORT_LEN + 1;

/// "Lighting" command class.
const CLASS_LIGHTING: u8 = 0x0f;

/// "Information" command class: version, serial number, mode.
///
/// ⚠️ **We only read from it.** The same class carries the write of the device
/// mode (`0x04`), which would make the firmware stop handling some keys — a
/// decision recorded in §8 of the survey never to touch it. No constructor in
/// this module can therefore form a command of this class below `0x80`, and
/// that is the only guarantee that does not depend on a reviewer's vigilance.
const CLASS_INFO: u8 = 0x00;

/// Transaction id observed on every captured frame.
/// No variation was seen; the device does not seem to check it, but we
/// reproduce the original value out of caution.
const TRANSACTION_ID: u8 = 0x9f;

/// A command, without its arguments: the class / command pair.
///
/// This is **exactly** what the device validates, and nothing more: an unknown
/// command returns status `0x05`, but an absurd argument value on a known
/// command still returns `0x02` (§8 of the survey). The type therefore carries
/// only what a status byte can confirm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommandId {
    pub class: u8,
    pub command: u8,
}

impl std::fmt::Display for CommandId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#04x}/{:#04x}", self.class, self.command)
    }
}

/// Set the effect. Command `0x0f` / `0x02`.
pub const SET_EFFECT: CommandId = CommandId {
    class: CLASS_LIGHTING,
    command: 0x02,
};
/// Write a row segment. Command `0x0f` / `0x03`.
pub const WRITE_ROW: CommandId = CommandId {
    class: CLASS_LIGHTING,
    command: 0x03,
};
/// Set the brightness. Command `0x0f` / `0x04`.
pub const SET_BRIGHTNESS: CommandId = CommandId {
    class: CLASS_LIGHTING,
    command: 0x04,
};

/// Read the current effect. `0x0f` / `0x82`, length `0x03` as in the survey.
const READ_EFFECT: CommandId = CommandId {
    class: CLASS_LIGHTING,
    command: 0x82,
};
/// Read the current brightness. `0x0f` / `0x84`, length `0x03` as in the survey.
const READ_BRIGHTNESS: CommandId = CommandId {
    class: CLASS_LIGHTING,
    command: 0x84,
};
/// Read the firmware version. `0x00` / `0x81`.
const READ_FIRMWARE: CommandId = CommandId {
    class: CLASS_INFO,
    command: 0x81,
};
/// Read the serial number. `0x00` / `0x82`.
const READ_SERIAL: CommandId = CommandId {
    class: CLASS_INFO,
    command: 0x82,
};

/// Length announced for lighting reads: the one from the survey.
const READ_LIGHTING_LEN: u8 = 0x03;

/// Length announced for class `0x00` reads: the one used by the probe that
/// established the table in §8. The serial number fits in it — 15 characters.
const READ_INFO_LEN: u8 = 0x16;

/// Effects supported by the firmware.
///
/// Variants other than [`Effect::Custom`] are animated by the device itself:
/// they survive the host software shutting down and cost no CPU time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Off,
    SpectrumCycle,
    /// `direction` and `speed` observed at 0x02 and 0x28 respectively.
    Wave {
        direction: u8,
        speed: u8,
    },
    /// Host-driven mode: the device shows only what it is pushed.
    Custom,
}

impl Effect {
    fn args(self) -> [u8; 6] {
        match self {
            Effect::Off => [0, 0, 0x00, 0, 0, 0],
            Effect::SpectrumCycle => [0, 0, 0x03, 0, 0, 0],
            Effect::Wave { direction, speed } => [0, 0, 0x04, direction, speed, 0],
            Effect::Custom => [0, 0, 0x08, 0, 0, 0],
        }
    }
}

/// A 90-byte report ready to be sent.
#[derive(Debug, Clone)]
pub struct Report(pub [u8; REPORT_LEN]);

impl Report {
    fn new(id: CommandId, args: &[u8]) -> Self {
        assert!(args.len() <= REPORT_LEN - 10, "arguments too long");
        let mut r = Self::header(id, args.len() as u8);
        r[8..8 + args.len()].copy_from_slice(args);
        r[88] = checksum(&r);
        Self(r)
    }

    /// A read request: no arguments, only the length expected in return.
    ///
    /// Separate from [`Self::new`] because the length here does not describe
    /// the bytes sent — there are none — but the response. The lengths are the
    /// survey's, and on purpose: that is the form the device answered to, and
    /// nothing established that it would answer the same to another one.
    fn query(id: CommandId, len: u8) -> Self {
        let mut r = Self::header(id, len);
        r[88] = checksum(&r);
        Self(r)
    }

    fn header(id: CommandId, len: u8) -> [u8; REPORT_LEN] {
        let mut r = [0u8; REPORT_LEN];
        r[1] = TRANSACTION_ID;
        r[5] = len;
        r[6] = id.class;
        r[7] = id.command;
        r
    }

    /// The class / command pair this report carries.
    pub fn id(&self) -> CommandId {
        CommandId {
            class: self.0[6],
            command: self.0[7],
        }
    }

    /// Selects an effect. Command `0x0f` / `0x02`.
    pub fn set_effect(effect: Effect) -> Self {
        Self::new(SET_EFFECT, &effect.args())
    }

    /// Sets the global brightness. Command `0x0f` / `0x04`.
    pub fn set_brightness(level: u8) -> Self {
        Self::new(SET_BRIGHTNESS, &[0, 0, level])
    }

    /// Requests the firmware version. See [`Response::firmware`].
    pub fn read_firmware() -> Self {
        Self::query(READ_FIRMWARE, READ_INFO_LEN)
    }

    /// Requests the serial number. See [`Response::serial`].
    pub fn read_serial() -> Self {
        Self::query(READ_SERIAL, READ_INFO_LEN)
    }

    /// Requests the current effect. See [`Response::effect`].
    pub fn read_effect() -> Self {
        Self::query(READ_EFFECT, READ_LIGHTING_LEN)
    }

    /// Requests the current brightness. See [`Response::brightness`].
    pub fn read_brightness() -> Self {
        Self::query(READ_BRIGHTNESS, READ_LIGHTING_LEN)
    }

    /// Writes a row segment. Command `0x0f` / `0x03`.
    ///
    /// Partial writes are supported: `col_start` and `col_end` bound the
    /// segment, which was verified on hardware.
    pub fn write_row(row: u8, col_start: u8, colors: &[Rgb]) -> Self {
        assert!(!colors.is_empty(), "empty segment");
        let col_end = col_start + colors.len() as u8 - 1;
        let mut args = Vec::with_capacity(5 + colors.len() * 3);
        args.extend_from_slice(&[0, 0, row, col_start, col_end]);
        for c in colors {
            args.extend_from_slice(&[c.r, c.g, c.b]);
        }
        Self::new(WRITE_ROW, &args)
    }

    /// Buffer ready for `HidD_SetFeature`: byte 0 = report id.
    pub fn to_feature_buffer(&self) -> [u8; FEATURE_BUF_LEN] {
        let mut buf = [0u8; FEATURE_BUF_LEN];
        buf[1..].copy_from_slice(&self.0);
        buf
    }
}

/// Checksum: XOR of bytes 2 to 87 inclusive, stored in byte 88.
///
/// Verified on every captured frame, across all commands, without exception.
///
/// ⚠️ **`pub` for the probes**, like [`REPORT_LEN`] and for the same reason:
/// the internal constructor of [`Report`] already calls it for everything the
/// application sends, and the only external caller is the `sonde.rs` of
/// `candeo-desktop`, compiled only in tests. A probe builds its frames **by
/// hand**, without going through [`Report`] — that is the whole point: it
/// queries commands the constructor cannot form, including ones that may not
/// exist. Taking the checksum away from it would make it send frames the
/// device refuses, and the survey could no longer be replayed.
pub fn checksum(report: &[u8; REPORT_LEN]) -> u8 {
    report[2..88].iter().fold(0u8, |acc, b| acc ^ b)
}

// ---------------------------------------------------------------- responses

/// The status byte of a response (offset 0) — §8 of the survey.
///
/// ⚠️ **`Understood` means "this command exists", not "it did what I
/// wanted".** Measured: setting effect `0x05`, which this keyboard refuses,
/// returns `0x02` — the device validates the class / command pair, never the
/// value of an argument. No status byte replaces a read-back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// `0x00` — no response set.
    Empty,
    /// `0x01` — the device has not finished.
    Busy,
    /// `0x02` — the class / command pair is known.
    Understood,
    /// `0x03`
    Failed,
    /// `0x04`
    TimedOut,
    /// `0x05` — class or command unknown to this firmware. **Verified to
    /// discriminate**: a nonexistent class and a nonexistent command both
    /// return it, where a valid command returns `0x02`.
    Unsupported,
    /// No surveyed value is written this way.
    Other(u8),
}

impl Status {
    fn from_byte(b: u8) -> Self {
        match b {
            0x00 => Self::Empty,
            0x01 => Self::Busy,
            0x02 => Self::Understood,
            0x03 => Self::Failed,
            0x04 => Self::TimedOut,
            0x05 => Self::Unsupported,
            other => Self::Other(other),
        }
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("0x00 (none)"),
            Self::Busy => f.write_str("0x01 (busy)"),
            Self::Understood => f.write_str("0x02 (understood)"),
            Self::Failed => f.write_str("0x03 (failed)"),
            Self::TimedOut => f.write_str("0x04 (timed out)"),
            Self::Unsupported => f.write_str("0x05 (unsupported)"),
            Self::Other(b) => write!(f, "{b:#04x} (unknown)"),
        }
    }
}

/// Firmware version, as `0x00`/`0x81` returns it.
///
/// Two bytes kept as two numbers rather than a string: `01 05` is v1.5, and the
/// survey once also wrote it "1.05". Comparing strings would turn a difference
/// in notation into a difference in version.
///
/// **This is not `release_number`.** HID enumeration returns `0x0200` across
/// the whole composite: that is the `bcdDevice`, a frozen hardware revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Firmware {
    pub major: u8,
    pub minor: u8,
}

/// As the device reports itself: `v1.5`.
impl std::fmt::Display for Firmware {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "v{}.{}", self.major, self.minor)
    }
}

/// The device's response to the last command it received.
///
/// Same structure as [`Report`] (§3), with two differences that make it
/// useful: byte 0 carries a [`Status`], and bytes 6 and 7 **echo** the class
/// and the command. The echo is not decorative — it is what tells whether we
/// are reading back the expected response and not that of a command sent in
/// the meantime by someone else.
///
/// The device sets a correct checksum in its responses: every response to an
/// open's inspection carried one, firmware v1.5, 14/09/2026 (#74). See
/// [`Response::checksum_matches`].
#[derive(Debug, Clone)]
pub struct Response([u8; REPORT_LEN]);

impl Response {
    /// The buffer `HidD_GetFeature` expects: report id included.
    pub const fn buffer() -> [u8; FEATURE_BUF_LEN] {
        [0u8; FEATURE_BUF_LEN]
    }

    /// The response held in a buffer filled by `get_feature_report`.
    ///
    /// `read` is the count returned by the call. The threshold is the one of
    /// the probes that established the return direction — at least the 90
    /// bytes of the report — and not a stricter threshold never tried: a read
    /// refused here for one missing id byte would be a failure of our own
    /// making.
    pub fn from_feature_buffer(buf: &[u8; FEATURE_BUF_LEN], read: usize) -> Option<Self> {
        if read < REPORT_LEN {
            return None;
        }
        let mut r = [0u8; REPORT_LEN];
        r.copy_from_slice(&buf[1..]);
        Some(Self(r))
    }

    pub fn status(&self) -> Status {
        Status::from_byte(self.0[0])
    }

    /// The echoed pair.
    pub fn id(&self) -> CommandId {
        CommandId {
            class: self.0[6],
            command: self.0[7],
        }
    }

    /// True if this response is the one to `request`, according to the echo.
    pub fn answers(&self, request: &Report) -> bool {
        self.id() == request.id()
    }

    /// True if byte 88 holds the checksum of the rest, as in a [`Report`].
    pub fn checksum_matches(&self) -> bool {
        self.0[88] == checksum(&self.0)
    }

    fn args(&self) -> &[u8] {
        &self.0[8..88]
    }

    /// The version, read from the response to [`Report::read_firmware`].
    ///
    /// `None` for `00 00`: that is what a buffer left empty returns, not a
    /// firmware — and announcing "v0.0" would compare against the survey a
    /// version that does not exist.
    pub fn firmware(&self) -> Option<Firmware> {
        let a = self.args();
        let version = Firmware {
            major: a[0],
            minor: a[1],
        };
        (version != Firmware { major: 0, minor: 0 }).then_some(version)
    }

    /// The serial number, read from the response to [`Report::read_serial`].
    ///
    /// ASCII up to the first NUL byte, and printable from end to end: 15
    /// characters in the survey. Anything that does not look like that returns
    /// `None` rather than an approximate string — a serial is used to
    /// **match**, and two reads damaged differently would unmatch the same
    /// unit.
    pub fn serial(&self) -> Option<String> {
        let a = &self.args()[..READ_INFO_LEN as usize];
        let end = a.iter().position(|&b| b == 0).unwrap_or(a.len());
        let serial = &a[..end];
        if serial.is_empty() || !serial.iter().all(|b| b.is_ascii_graphic()) {
            return None;
        }
        Some(serial.iter().map(|&b| char::from(b)).collect())
    }

    /// The current effect, read from the response to [`Report::read_effect`].
    ///
    /// The read-back places the id and its two parameters **at the same
    /// positions** as the write (`00 00 <effect> <p1> <p2>`) — that is what
    /// made it possible to read back the Wave "identically, parameters
    /// included".
    ///
    /// `None` for anything [`Effect`] cannot rewrite identically: `Static` and
    /// `Breathing` carry a color whose place in the read-back is not
    /// established, and an unexpected byte where the survey saw only zeros
    /// means we do not understand what we are reading. In both cases, better
    /// to conclude nothing than to rewrite something other than what is
    /// displayed.
    pub fn effect(&self) -> Option<Effect> {
        let a = self.args();
        if a[0] != 0 || a[1] != 0 || a[5] != 0 {
            return None;
        }
        match (a[2], a[3], a[4]) {
            (0x00, 0, 0) => Some(Effect::Off),
            (0x03, 0, 0) => Some(Effect::SpectrumCycle),
            // Direction bounded to `00`–`02` in the survey; beyond that, it is
            // no longer a Wave we know how to describe.
            (0x04, direction @ 0..=2, speed) => Some(Effect::Wave { direction, speed }),
            (0x08, 0, 0) => Some(Effect::Custom),
            _ => None,
        }
    }

    /// The current brightness, read from the response to [`Report::read_brightness`].
    ///
    /// Surveyed in the form `00 00 <level>`, like the write.
    pub fn brightness(&self) -> Option<u8> {
        let a = self.args();
        (a[0] == 0 && a[1] == 0).then_some(a[2])
    }
}

/// Color, in protocol order: R, G, B.
///
/// Beware: Razer's Chroma SDK uses `0x00BBGGRR`. The device protocol itself is
/// indeed RGB — do not infer one from the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Frame actually captured: row 0 entirely red.
    /// The checksum observed on the bus was `0x5e`.
    #[test]
    fn checksum_matches_captured_frame() {
        let colors = [Rgb::new(0xff, 0, 0); 22];
        let report = Report::write_row(0, 0, &colors);
        assert_eq!(report.0[88], 0x5e);
    }

    #[test]
    fn header_layout_matches_capture() {
        let colors = [Rgb::new(0xff, 0, 0); 22];
        let r = Report::write_row(0, 0, &colors).0;
        assert_eq!(r[0], 0x00, "status");
        assert_eq!(r[1], 0x9f, "transaction id");
        assert_eq!(r[5], 0x47, "argument length: 5 + 22*3");
        assert_eq!(r[6], 0x0f, "class");
        assert_eq!(r[7], 0x03, "command id");
        assert_eq!(r[12], 0x15, "end column = 21");
        assert_eq!(&r[13..16], &[0xff, 0x00, 0x00], "RGB order");
    }

    #[test]
    fn partial_row_sets_boundaries() {
        let colors = [Rgb::new(255, 255, 255); 6];
        let r = Report::write_row(2, 5, &colors).0;
        assert_eq!(r[10], 2, "row");
        assert_eq!(r[11], 5, "start column");
        assert_eq!(r[12], 10, "end column");
        assert_eq!(r[5], 5 + 6 * 3, "argument length");
    }

    #[test]
    fn effect_ids_match_capture() {
        assert_eq!(Report::set_effect(Effect::Off).0[10], 0x00);
        assert_eq!(Report::set_effect(Effect::SpectrumCycle).0[10], 0x03);
        assert_eq!(Report::set_effect(Effect::Custom).0[10], 0x08);
        let w = Report::set_effect(Effect::Wave {
            direction: 0x02,
            speed: 0x28,
        })
        .0;
        assert_eq!(&w[10..13], &[0x04, 0x02, 0x28]);
    }

    #[test]
    fn feature_buffer_has_leading_report_id() {
        let buf = Report::set_brightness(0xff).to_feature_buffer();
        assert_eq!(buf.len(), 91);
        assert_eq!(buf[0], 0x00, "HID report id");
        assert_eq!(buf[7], 0x0f, "class, shifted by one byte");
    }

    // -------------------------------------------------------- reads

    /// A response as `get_feature_report` fills it: report id, status, echo,
    /// arguments.
    fn response(status_byte: u8, id: CommandId, args: &[u8]) -> Response {
        let mut buf = Response::buffer();
        buf[1] = status_byte;
        buf[1 + 6] = id.class;
        buf[1 + 7] = id.command;
        buf[1 + 8..1 + 8 + args.len()].copy_from_slice(args);
        Response::from_feature_buffer(&buf, buf.len()).expect("full buffer")
    }

    /// Requests are formed the way the probe that established §8 formed them:
    /// no arguments, the expected length in byte 5, and a correct checksum —
    /// otherwise the device would refuse the frame rather than the command.
    #[test]
    fn read_request_is_formed_as_in_survey() {
        let r = Report::read_firmware().0;
        assert_eq!(r[1], 0x9f, "transaction id");
        assert_eq!(r[5], 0x16, "expected length");
        assert_eq!((r[6], r[7]), (0x00, 0x81));
        assert!(r[8..88].iter().all(|&b| b == 0), "a read has no arguments");
        assert_eq!(r[88], checksum(&r));

        let e = Report::read_effect().0;
        assert_eq!((e[5], e[6], e[7]), (0x03, 0x0f, 0x82));
        let l = Report::read_brightness().0;
        assert_eq!((l[5], l[6], l[7]), (0x03, 0x0f, 0x84));
    }

    /// **Driver mode is not written from here.** In class `0x00`, only reads
    /// (`0x80` and above) can be formed; `0x00`/`0x04` would make the firmware
    /// stop handling some keys.
    #[test]
    fn no_constructor_writes_to_info_class() {
        let all = [
            Report::read_firmware(),
            Report::read_serial(),
            Report::read_effect(),
            Report::read_brightness(),
            Report::set_effect(Effect::Custom),
            Report::set_brightness(0x80),
            Report::write_row(0, 0, &[Rgb::default()]),
        ];
        for r in all {
            let id = r.id();
            assert!(
                id.class != 0x00 || id.command >= 0x80,
                "{id} writes to the information class"
            );
        }
    }

    #[test]
    fn status_decodes_as_in_survey() {
        let s = |b| response(b, SET_BRIGHTNESS, &[]).status();
        assert_eq!(s(0x02), Status::Understood);
        assert_eq!(s(0x05), Status::Unsupported);
        assert_eq!(s(0x01), Status::Busy);
        assert_eq!(s(0x07), Status::Other(0x07));
    }

    /// The echo names the command being answered: a brightness response is not
    /// read as a version, even if its bytes would lend themselves to it.
    #[test]
    fn echo_names_answered_command() {
        let r = response(0x02, READ_FIRMWARE, &[0x01, 0x05]);
        assert!(r.answers(&Report::read_firmware()));
        assert!(!r.answers(&Report::read_serial()));
        assert!(!r.answers(&Report::read_brightness()));
    }

    /// `01 05`, surveyed on 12/09/2026 — what the device reports elsewhere as
    /// **v1.5**. The match served as validation.
    #[test]
    fn version_01_05_reads_as_v1_5() {
        let v = response(0x02, READ_FIRMWARE, &[0x01, 0x05])
            .firmware()
            .expect("version");
        assert_eq!(v, Firmware { major: 1, minor: 5 });
        assert_eq!(v.to_string(), "v1.5");
        assert!(Firmware { major: 1, minor: 6 } > v);
    }

    #[test]
    fn empty_buffer_is_not_a_version() {
        assert_eq!(response(0x02, READ_FIRMWARE, &[0, 0]).firmware(), None);
    }

    /// Made-up serial, in the surveyed form: 15 ASCII characters.
    #[test]
    fn serial_number_stops_at_first_nul_byte() {
        let r = response(0x02, READ_SERIAL, b"XY24ABCDEFG0001\0\0\0");
        assert_eq!(r.serial().as_deref(), Some("XY24ABCDEFG0001"));
    }

    /// A serial is used to match: a damaged read must not invent a second one
    /// for the same unit.
    #[test]
    fn unreadable_serial_is_not_guessed() {
        assert_eq!(response(0x02, READ_SERIAL, &[]).serial(), None);
        assert_eq!(response(0x02, READ_SERIAL, b"XY24\x07BCD").serial(), None);
        assert_eq!(response(0x02, READ_SERIAL, b"XY 24").serial(), None);
    }

    /// The read-back places the effect at the write positions: an effect that
    /// was set reads back as is. That is the condition for being able to
    /// **rewrite it identically** without changing anything the keyboard shows.
    #[test]
    fn read_back_effect_rewrites_identically() {
        for effect in [
            Effect::Off,
            Effect::SpectrumCycle,
            Effect::Wave {
                direction: 0x01,
                speed: 0x28,
            },
            Effect::Custom,
        ] {
            let written = Report::set_effect(effect).0;
            let read_back = response(0x02, READ_EFFECT, &written[8..14]).effect();
            assert_eq!(read_back, Some(effect));
        }
    }

    /// `Static` carries a color whose place in the read-back is not
    /// established: rewriting it "identically" could turn it off.
    #[test]
    fn colored_effect_is_not_read_back() {
        let static_args = [0, 0, 0x01, 0, 0, 0x01, 0xff, 0x00, 0x00];
        assert_eq!(response(0x02, READ_EFFECT, &static_args).effect(), None);
        // Non-zero LED id: no longer the surveyed form.
        assert_eq!(response(0x02, READ_EFFECT, &[0x05, 0, 0x03]).effect(), None);
    }

    #[test]
    fn brightness_reads_back_as_written() {
        let written = Report::set_brightness(0x80).0;
        let read_back = response(0x02, READ_BRIGHTNESS, &written[8..11]).brightness();
        assert_eq!(read_back, Some(0x80));
        assert_eq!(
            response(0x02, READ_BRIGHTNESS, &[0x05, 0, 0x80]).brightness(),
            None
        );
    }

    /// The threshold is the probes' one: at least 90 bytes.
    #[test]
    fn truncated_response_is_refused() {
        let buf = Response::buffer();
        assert!(Response::from_feature_buffer(&buf, 89).is_none());
        assert!(Response::from_feature_buffer(&buf, 90).is_some());
    }
}
