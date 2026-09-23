//! Hardware probes — the executable form of §9 "Reproducing the survey".
//!
//! Each test here answers a question from
//! `docs/protocol/deathstalker-v2-pro.md` by querying a real
//! DeathStalker V2 Pro. All of them are `#[ignore]`: they require the device, so
//! CI never runs them.
//!
//! ```text
//! cargo test -p candeo-desktop sonde -- --ignored --nocapture
//! ```
//!
//! **The results of these probes are already recorded in the survey.** We keep
//! them because they are the way to redo it — on another firmware, on another
//! unit, or to settle one of the questions still open.
//!
//! ⚠️ A costly lesson, set down here: the effect identifier probe first filtered
//! out all-zero replies to discard noise, **hiding exactly the interesting
//! case** (the "normal" mode reads `00 00`); and it set `Static` and
//! `Breathing` without a color, hence in **black** — indistinguishable by eye
//! from a nonexistent effect. Verify by reading back (`0x0f`/`0x82`), never by
//! eye alone.

#![cfg(test)]

use std::time::{Duration, Instant};

use candeo_protocol::{checksum, REPORT_LEN};

const VID: u16 = 0x1532;
const PID: u16 = 0x0292;
/// Lighting goes through this interface of the composite device. Opening the
/// wrong one gives a valid handle on which every write fails without an
/// explicit error.
const INTERFACE: i32 = 3;

const CLASS_LIGHTING: u8 = 0x0f;
const TRANSACTION: u8 = 0x9f;

/// Builds a 90-byte report, checksum included.
fn report(command: u8, args: &[u8]) -> [u8; REPORT_LEN + 1] {
    let mut r = [0u8; REPORT_LEN];
    r[1] = TRANSACTION;
    r[5] = args.len() as u8;
    r[6] = CLASS_LIGHTING;
    r[7] = command;
    r[8..8 + args.len()].copy_from_slice(args);
    r[88] = checksum(&r);

    // The `HidD_SetFeature` buffer is 91 bytes: report identifier, then the 90
    // bytes of the report.
    let mut buf = [0u8; REPORT_LEN + 1];
    buf[1..].copy_from_slice(&r);
    buf
}

fn open_keyboard() -> hidapi::HidDevice {
    let api = hidapi::HidApi::new().expect("HID");
    let info = api
        .device_list()
        .find(|d| {
            d.vendor_id() == VID && d.product_id() == PID && d.interface_number() == INTERFACE
        })
        .expect("DeathStalker V2 Pro not found — plugged in?");
    info.open_device(&api).expect("open")
}

/// Sweeps the effect identifiers, without a color argument.
///
/// Known before this survey: `0x00` Off, `0x03` Spectrum Cycle, `0x04` Wave,
/// `0x08` Direct/custom. The others had never been tried.
#[test]
#[ignore]
fn probe_effect_ids() {
    let dev = open_keyboard();
    let known = |id: u8| match id {
        0x00 => " (known: Off)",
        0x03 => " (known: Spectrum Cycle)",
        0x04 => " (known: Wave)",
        0x08 => " (known: Direct/custom)",
        _ => "",
    };

    // Full brightness first: an effect invisible because the keyboard is dark
    // would read as an identifier with no effect.
    let _ = dev.send_feature_report(&report(0x04, &[0, 0, 0xff]));
    std::thread::sleep(Duration::from_millis(400));

    println!("\n>>> Sweeping effect identifiers, 3 s each.");
    println!(">>> Watch the keyboard and note what each number does.\n");

    for id in 0x00u8..=0x0f {
        println!(">>> identifier 0x{id:02x}{}", known(id));
        match dev.send_feature_report(&report(0x02, &[0, 0, id, 0, 0, 0])) {
            Ok(()) => {}
            Err(e) => println!("    write refused: {e}"),
        }
        std::thread::sleep(Duration::from_secs(3));
    }

    // Leave the keyboard in a visible state rather than on the last attempt.
    let _ = dev.send_feature_report(&report(0x02, &[0, 0, 0x03, 0, 0, 0]));
    println!("\n>>> done — back on Spectrum Cycle.");
}

/// Reads the device's reply after a command.
///
/// `[0]` holds the status: `0x02` = understood, and that is exactly what
/// `reachingKeyboard` cannot tell apart today.
fn read_reply(dev: &hidapi::HidDevice) -> Option<[u8; REPORT_LEN]> {
    let mut buf = [0u8; REPORT_LEN + 1];
    match dev.get_feature_report(&mut buf) {
        Ok(n) if n >= REPORT_LEN => {
            let mut r = [0u8; REPORT_LEN];
            r.copy_from_slice(&buf[1..=REPORT_LEN]);
            Some(r)
        }
        Ok(n) => {
            println!("    truncated reply: {n} bytes");
            None
        }
        Err(e) => {
            println!("    read refused: {e}");
            None
        }
    }
}

fn status_name(code: u8) -> &'static str {
    match code {
        0x00 => "none",
        0x01 => "busy",
        0x02 => "understood",
        0x03 => "failure",
        0x04 => "timeout",
        0x05 => "not supported",
        _ => "?",
    }
}

/// Builds a report for an arbitrary class, not only lighting.
fn class_report(class: u8, command: u8, size: u8) -> [u8; REPORT_LEN + 1] {
    let mut r = [0u8; REPORT_LEN];
    r[1] = TRANSACTION;
    r[5] = size;
    r[6] = class;
    r[7] = command;
    r[88] = checksum(&r);
    let mut buf = [0u8; REPORT_LEN + 1];
    buf[1..].copy_from_slice(&r);
    buf
}

/// **Does the device reply, and what does it say?**
///
/// Answers the `GET_REPORT` entry of §9 of the survey, and gates #35: without
/// reading, no firmware version is reachable — `release_number` only gives
/// `bcdDevice`, fixed at 0x0200 while the firmware reports itself as v1.5.
#[test]
#[ignore]
fn probe_read_reply() {
    let dev = open_keyboard();

    // First a command we KNOW has an effect: if that one does not read back,
    // it is the read path that is missing, not the probed command.
    println!("\n>>> Control: brightness (command validated on write).");
    dev.send_feature_report(&report(0x04, &[0, 0, 0xff]))
        .expect("control write");
    std::thread::sleep(Duration::from_millis(60));
    match read_reply(&dev) {
        Some(r) => println!(
            "    status 0x{:02x} ({}) · class 0x{:02x} · command 0x{:02x} · args {:02x?}",
            r[0],
            status_name(r[0]),
            r[6],
            r[7],
            &r[8..16]
        ),
        None => println!("    no reply — the rest of this probe will be worthless."),
    }

    // Then look for what returns NON-zero bytes: a version, a name, a serial
    // number — the keyboard declares none of them over USB.
    println!("\n>>> Sweeping the commands of class 0x00 (information).");
    for command in 0x80u8..=0x8f {
        for size in [0x02u8, 0x04, 0x10, 0x16] {
            dev.send_feature_report(&class_report(0x00, command, size))
                .ok();
            std::thread::sleep(Duration::from_millis(40));
            let Some(r) = read_reply(&dev) else { continue };
            let payload = &r[8..8 + size as usize];
            if r[0] == 0x02 && payload.iter().any(|&b| b != 0) {
                println!(
                    "    class 0x00 · command 0x{command:02x} · size 0x{size:02x} → {payload:02x?}  {:?}",
                    String::from_utf8_lossy(payload)
                );
            }
        }
    }

    println!("\n>>> done.");
}

/// Reads class `0x00` again **without filtering out zero bytes** — the filter of
/// the first pass had hidden `0x84`, whose expected reply is `00 00`.
///
/// The hypotheses to confirm on OUR hardware:
/// `0x85` = polling rate (`01` → 1000 Hz),
/// `0x86` = keyboard locale layout (`04` → fr_FR).
#[test]
#[ignore]
fn probe_information_class() {
    let dev = open_keyboard();

    for command in 0x80u8..=0x8f {
        dev.send_feature_report(&class_report(0x00, command, 0x16))
            .ok();
        std::thread::sleep(Duration::from_millis(40));
        let Some(r) = read_reply(&dev) else { continue };
        println!(
            "0x{command:02x} → status 0x{:02x} ({:<18}) · echo class 0x{:02x}/cmd 0x{:02x} · {:02x?}",
            r[0],
            status_name(r[0]),
            r[6],
            r[7],
            &r[8..24]
        );
    }
}

/// **Does the status byte mean anything?**
///
/// A reply always at `0x02` would prove nothing. We need to see the device
/// *refuse*: that refusal is what would make protocol verification something
/// other than wishful thinking for #35.
#[test]
#[ignore]
fn probe_status_on_invalid_command() {
    let dev = open_keyboard();

    let cases: [(&str, [u8; REPORT_LEN + 1]); 4] = [
        ("control: brightness, valid", report(0x04, &[0, 0, 0x80])),
        ("nonexistent class 0xee", class_report(0xee, 0x01, 0x02)),
        (
            "lighting class, command 0xee",
            class_report(0x0f, 0xee, 0x02),
        ),
        (
            "absurd size on a valid command",
            class_report(0x0f, 0x04, 0x50),
        ),
    ];

    for (name, mut packet) in cases {
        // The checksum must stay correct: we are testing the refusal of a
        // command, not that of a corrupted packet.
        let mut r = [0u8; REPORT_LEN];
        r.copy_from_slice(&packet[1..]);
        r[88] = checksum(&r);
        packet[1..].copy_from_slice(&r);

        print!(">>> {name:<40} ");
        match dev.send_feature_report(&packet) {
            Ok(()) => match read_reply(&dev) {
                Some(reply) => println!("→ 0x{:02x} ({})", reply[0], status_name(reply[0])),
                None => println!("→ no reply"),
            },
            Err(e) => println!("→ write refused: {e}"),
        }
        std::thread::sleep(Duration::from_millis(120));
    }

    let _ = dev.send_feature_report(&report(0x04, &[0, 0, 0xff]));
}

/// Looks for a **readback** in the lighting class.
///
/// If the device can report the current effect, the effect identifiers are
/// confirmed **without relying on the eye** — which is precisely what the first
/// sweep lacked, judged "too fast to tell what I saw".
#[test]
#[ignore]
fn probe_lighting_readback() {
    let dev = open_keyboard();

    println!("\n>>> Readable commands of class 0x0f.");
    for command in 0x80u8..=0x8f {
        dev.send_feature_report(&class_report(0x0f, command, 0x03))
            .ok();
        std::thread::sleep(Duration::from_millis(40));
        let Some(r) = read_reply(&dev) else { continue };
        if r[0] == 0x02 {
            println!(
                "0x{command:02x} → status understood · echo 0x{:02x}/0x{:02x} · {:02x?}",
                r[6],
                r[7],
                &r[8..14]
            );
        }
    }

    // Then: set an effect, read it back. If the readback follows, the
    // identifiers are established objectively.
    println!("\n>>> Setting an effect, then reading it back.");
    for (name, id) in [
        ("Off", 0x00u8),
        ("Static", 0x01),
        ("Breathing", 0x02),
        ("Spectrum", 0x03),
        ("Wave", 0x04),
        ("Reactive", 0x05),
        ("Starlight", 0x07),
        ("Direct/custom", 0x08),
    ] {
        // Static and Breathing want a color: without it, the first sweep set
        // them in BLACK — hence "nothing happens" to the eye.
        let args: Vec<u8> = match id {
            0x01 | 0x02 => vec![0, 0, id, 0, 0, 0x01, 0xff, 0x00, 0x00],
            0x04 => vec![0, 0, id, 0x01, 0x28, 0],
            _ => vec![0, 0, id, 0, 0, 0],
        };
        dev.send_feature_report(&report(0x02, &args)).ok();
        std::thread::sleep(Duration::from_millis(150));

        dev.send_feature_report(&class_report(0x0f, 0x82, 0x03))
            .ok();
        std::thread::sleep(Duration::from_millis(60));
        match read_reply(&dev) {
            Some(r) => println!(
                "{name:<14} set 0x{id:02x} → read back status 0x{:02x} · {:02x?}",
                r[0],
                &r[8..14]
            ),
            None => println!("{name:<14} set 0x{id:02x} → no readback"),
        }
    }

    let _ = dev.send_feature_report(&report(0x02, &[0, 0, 0x03, 0, 0, 0]));
    println!("\n>>> back on Spectrum Cycle.");
}

/// Details the replies of `0x0f`/`0x80` and `0x81`, which look like
/// descriptors: one holds `06 16` — exactly our 6 rows × 22 columns — the other
/// a run `00 01 02 03 04` that could enumerate the effects actually supported.
#[test]
#[ignore]
fn probe_lighting_descriptors() {
    let dev = open_keyboard();
    for command in [0x80u8, 0x81, 0x86] {
        for size in [0x03u8, 0x16] {
            dev.send_feature_report(&class_report(0x0f, command, size))
                .ok();
            std::thread::sleep(Duration::from_millis(50));
            let Some(r) = read_reply(&dev) else { continue };
            println!(
                "0x{command:02x} size 0x{size:02x} → status 0x{:02x} · {:02x?}",
                r[0],
                &r[8..40]
            );
        }
    }
}

/// **What frame rate does the device actually sustain?**
///
/// Answers the "maximum rate accepted before dropping out" of §9, and settled a
/// design question: the loop then aimed at 60 frames per second, yet a full
/// update costs **7 control transfers** — 6 rows then the switch to custom
/// mode. At 60 Hz that made 420 transfers per second on a single interface.
///
/// This probe is what answered no, and `runtime::FPS` has been 30 since. The
/// tense is past for that reason, not out of carelessness: keeping the original
/// wording is what lets you replay the measurement and compare it.
///
/// What is at stake is not comfort: if the device cannot keep up, half of our
/// frames are dropped by it, and **the simulator is then smoother than the
/// keyboard** — which contradicts "the preview IS production".
#[test]
#[ignore]
fn probe_sustainable_frame_rate() {
    let dev = open_keyboard();

    // A full frame: 6 rows of 22 colors, then custom mode.
    let frame = |hue: u8| -> Vec<[u8; REPORT_LEN + 1]> {
        let mut packets = Vec::with_capacity(7);
        for row in 0u8..6 {
            let mut args = vec![0u8, 0, row, 0, 21];
            for _ in 0..22 {
                args.extend_from_slice(&[hue, 0, 255 - hue]);
            }
            packets.push(report(0x03, &args));
        }
        packets.push(report(0x02, &[0, 0, 0x08, 0, 0, 0]));
        packets
    };

    // Warm-up: the first write after opening pays costs we do not want to
    // count as throughput.
    for t in frame(0) {
        let _ = dev.send_feature_report(&t);
    }
    std::thread::sleep(Duration::from_millis(100));

    println!("\n>>> Cost of a full update (7 packets), as fast as possible.");
    const N: u32 = 120;
    let mut failures = 0u32;
    let mut worst = Duration::ZERO;
    let start = Instant::now();

    for i in 0..N {
        let t0 = Instant::now();
        for t in frame((i * 2) as u8) {
            if dev.send_feature_report(&t).is_err() {
                failures += 1;
            }
        }
        let d = t0.elapsed();
        if d > worst {
            worst = d;
        }
    }

    let total = start.elapsed();
    let mean = total / N;
    println!("    {N} updates in {total:?}");
    println!("    mean {mean:?} · worst {worst:?} · writes refused: {failures}");
    println!(
        "    i.e. {:.1} fps at most, {:.1} fps in the worst case",
        1.0 / mean.as_secs_f64(),
        1.0 / worst.as_secs_f64()
    );
    println!("    (target period at 60 fps: 16.7 ms · at 30 fps: 33.3 ms)");

    // Does the device still reply? A rate "sustained" that leaves the device
    // deaf would be worthless.
    dev.send_feature_report(&class_report(0x0f, 0x82, 0x03))
        .ok();
    std::thread::sleep(Duration::from_millis(60));
    match read_reply(&dev) {
        Some(r) => println!(
            "    after the burst: status 0x{:02x} ({}), effect read back 0x{:02x}",
            r[0],
            status_name(r[0]),
            r[10]
        ),
        None => println!("    after the burst: NO MORE REPLIES — dropped out"),
    }

    let _ = dev.send_feature_report(&report(0x02, &[0, 0, 0x03, 0, 0, 0]));
    println!(">>> back on Spectrum Cycle.");
}

/// What HID enumeration gives **without any protocol**, on each interface.
///
/// Opens nothing: this is a read of the list the system keeps. The path is
/// printed for the `interface -1` entry of §10 — it is what tells through which
/// bus, and so through which driver, that entry arrives.
#[test]
#[ignore]
fn probe_usb_descriptor() {
    let api = hidapi::HidApi::new().expect("HID");
    for d in api
        .device_list()
        .filter(|d| d.vendor_id() == VID && d.product_id() == PID)
    {
        let r = d.release_number();
        println!(
            "interface {:>2} | release_number = 0x{r:04x} (i.e. {}.{:02}) | serial {:?} | product {:?} | page 0x{:04x}/0x{:04x}\n             path {}",
            d.interface_number(),
            r >> 8,
            r & 0xff,
            d.serial_number().unwrap_or("—"),
            d.product_string().unwrap_or("—"),
            d.usage_page(),
            d.usage(),
            d.path().to_string_lossy(),
        );
    }
}

/// **The production inspection**, as the application runs it on every open —
/// and not a hand-written rewrite of what it is supposed to do.
///
/// This is the probe to replay before trusting the inspection on another
/// firmware: it sends exactly what `Keyboard::open` sends, brightness and
/// effect **rewritten identically**. If the keyboard changes appearance while
/// it runs, the hypothesis that allows the inspection to write is wrong — and
/// this is where we want to learn it, not on a user's machine.
///
/// ⚠️ **Application closed.** A render loop writing to the same interface would
/// interleave its commands with ours: the echo filters them out, but the
/// verdicts would become "unverified".
#[test]
#[ignore]
fn probe_inspection_on_open() {
    let api = hidapi::HidApi::new().expect("HID");
    let layout = &candeo_device::DEATHSTALKER_V2_PRO;
    let kb = candeo_device::Keyboard::open(&api, layout).expect("open");
    let i = kb.inspection();

    println!(
        "\nfirmware: {:?} (surveyed on {})",
        i.firmware.as_ref().map(ToString::to_string),
        layout
            .surveyed_firmware
            .map_or_else(|| "nothing".to_string(), |f| f.to_string())
    );
    // The fingerprint, not the serial: the output of this probe ends up pasted
    // into an issue as often as a log does.
    println!(
        "serial: {}",
        crate::journal::fingerprint_of(i.serial.as_deref().ok())
    );
    for c in &i.checks {
        println!("{:<10} {} → {:?}", c.name, c.command, c.verdict);
    }
    for a in i.warnings(layout) {
        println!("WARNING: {a}");
    }
}
