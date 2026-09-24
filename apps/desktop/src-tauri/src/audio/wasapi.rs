//! WASAPI loopback: what the default output plays, captured in shared mode.
//!
//! The mix format is taken as it is — 32-bit float as a rule since Windows 7's
//! audio engine, 16-bit PCM accepted too — and folded to mono: an analysis of
//! what plays has no use for where it plays.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::core::GUID;
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator,
    MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED,
};

use super::analysis::WINDOW;
use super::Ended;

const WAVE_FORMAT_PCM: u16 = 1;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
const SUBTYPE_PCM: GUID = GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);
const SUBTYPE_IEEE_FLOAT: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

/// How often packets are collected. A shared-mode device period is 10 ms.
const POLL: Duration = Duration::from_millis(10);
/// The buffer asked of WASAPI, in 100 ns units: 200 ms, so a late poll loses
/// nothing.
const BUFFER_100NS: i64 = 2_000_000;
/// Loopback hands over nothing while nothing plays. Past this without a packet,
/// the time is counted as silence; shorter gaps are packets not due yet.
const GAP: Duration = Duration::from_millis(50);
/// How often the default output is checked, to follow it when it changes.
const DEFAULT_CHECK: Duration = Duration::from_secs(2);

#[derive(Clone, Copy)]
enum Sample {
    F32,
    I16,
}

struct Format {
    channels: usize,
    rate: u32,
    sample: Sample,
}

/// Captures the default output until `stop`, or until the default output
/// changes.
pub(super) fn listen(
    stop: &AtomicBool,
    heard: &mut dyn FnMut(&[f32], u32),
) -> Result<Ended, String> {
    let _com = Com::init()?;
    // SAFETY: COM is initialised on this thread for as long as `_com` lives, and
    // every pointer below comes from WASAPI and is used within its contract.
    unsafe { capture(stop, heard) }
}

/// COM on the capture thread, for the time of one capture.
struct Com;

impl Com {
    fn init() -> Result<Self, String> {
        // SAFETY: paired with `CoUninitialize` in `drop`, on the same thread.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|e| format!("COM could not start: {e}"))?;
        Ok(Com)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: `init` succeeded on this thread.
        unsafe { CoUninitialize() }
    }
}

unsafe fn capture(stop: &AtomicBool, heard: &mut dyn FnMut(&[f32], u32)) -> Result<Ended, String> {
    let enumerator: IMMDeviceEnumerator =
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
            .map_err(|e| format!("no audio device enumerator: {e}"))?;
    let device = enumerator
        .GetDefaultAudioEndpoint(eRender, eConsole)
        .map_err(|e| format!("no default output: {e}"))?;
    let id = device_id(&device)?;
    let client: IAudioClient = device
        .Activate(CLSCTX_ALL, None)
        .map_err(|e| format!("the default output refused an audio client: {e}"))?;

    let mix = client
        .GetMixFormat()
        .map_err(|e| format!("the default output has no mix format: {e}"))?;
    let format = read_format(mix);
    let initialised = client.Initialize(
        AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_LOOPBACK,
        BUFFER_100NS,
        0,
        mix,
        None,
    );
    CoTaskMemFree(Some(mix as *const _));
    let format = format?;
    initialised.map_err(|e| format!("loopback refused on the default output: {e}"))?;

    let reader: IAudioCaptureClient = client
        .GetService()
        .map_err(|e| format!("no capture client: {e}"))?;
    client
        .Start()
        .map_err(|e| format!("capture would not start: {e}"))?;

    let result = read(stop, heard, &enumerator, &id, &reader, &format);
    let _ = client.Stop();
    result
}

unsafe fn read(
    stop: &AtomicBool,
    heard: &mut dyn FnMut(&[f32], u32),
    enumerator: &IMMDeviceEnumerator,
    id: &str,
    reader: &IAudioCaptureClient,
    format: &Format,
) -> Result<Ended, String> {
    let mut mono: Vec<f32> = Vec::new();
    let mut last_packet = Instant::now();
    let mut silence_until = Instant::now();
    let mut last_check = Instant::now();
    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(Ended::Stopped);
        }
        std::thread::sleep(POLL);

        let mut got = false;
        loop {
            let size = reader
                .GetNextPacketSize()
                .map_err(|e| format!("capture stopped: {e}"))?;
            if size == 0 {
                break;
            }
            let mut data = std::ptr::null_mut();
            let mut frames = 0u32;
            let mut flags = 0u32;
            reader
                .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                .map_err(|e| format!("capture stopped: {e}"))?;
            mono.clear();
            if data.is_null() || flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                mono.resize(frames as usize, 0.0);
            } else {
                downmix(data, frames as usize, format, &mut mono);
            }
            reader
                .ReleaseBuffer(frames)
                .map_err(|e| format!("capture stopped: {e}"))?;
            heard(&mono, format.rate);
            got = true;
        }

        let now = Instant::now();
        if got {
            last_packet = now;
            silence_until = now;
        } else if now.duration_since(last_packet) > GAP {
            let from = silence_until.max(last_packet);
            let missing = (now.duration_since(from).as_secs_f32() * format.rate as f32) as usize;
            mono.clear();
            mono.resize(missing.min(WINDOW), 0.0);
            heard(&mono, format.rate);
            silence_until = now;
        }

        if now.duration_since(last_check) >= DEFAULT_CHECK {
            last_check = now;
            let current = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|e| format!("no default output: {e}"))?;
            if device_id(&current)? != id {
                return Ok(Ended::DeviceChanged);
            }
        }
    }
}

unsafe fn device_id(device: &IMMDevice) -> Result<String, String> {
    let raw = device
        .GetId()
        .map_err(|e| format!("an output without an id: {e}"))?;
    let id = raw
        .to_string()
        .map_err(|e| format!("an output id that is not text: {e}"));
    CoTaskMemFree(Some(raw.0 as *const _));
    id
}

/// The mix format, if it is one this capture reads.
unsafe fn read_format(format: *const WAVEFORMATEX) -> Result<Format, String> {
    // The structure is packed: read it whole rather than through a reference.
    let base = std::ptr::read_unaligned(format);
    let tag = base.wFormatTag;
    let bits = base.wBitsPerSample;
    let channels = base.nChannels as usize;
    let rate = base.nSamplesPerSec;
    let sub = (tag == WAVE_FORMAT_EXTENSIBLE)
        .then(|| std::ptr::read_unaligned(format as *const WAVEFORMATEXTENSIBLE).SubFormat);
    let float = tag == WAVE_FORMAT_IEEE_FLOAT || sub == Some(SUBTYPE_IEEE_FLOAT);
    let pcm = tag == WAVE_FORMAT_PCM || sub == Some(SUBTYPE_PCM);
    let sample = match (float, pcm, bits) {
        (true, _, 32) => Sample::F32,
        (_, true, 16) => Sample::I16,
        _ => {
            return Err(format!(
                "an output mix format this capture does not read: tag {tag}, {bits} bits"
            ))
        }
    };
    if channels == 0 || rate == 0 {
        return Err("an output mix format without channels or rate".into());
    }
    Ok(Format {
        channels,
        rate,
        sample,
    })
}

/// Interleaved frames folded to mono, appended to `out`.
unsafe fn downmix(data: *const u8, frames: usize, format: &Format, out: &mut Vec<f32>) {
    let channels = format.channels;
    match format.sample {
        Sample::F32 => {
            let samples = std::slice::from_raw_parts(data as *const f32, frames * channels);
            out.extend(
                samples
                    .chunks_exact(channels)
                    .map(|f| f.iter().sum::<f32>() / channels as f32),
            );
        }
        Sample::I16 => {
            let samples = std::slice::from_raw_parts(data as *const i16, frames * channels);
            out.extend(samples.chunks_exact(channels).map(|f| {
                f.iter().map(|&s| f32::from(s) / 32_768.0).sum::<f32>() / channels as f32
            }));
        }
    }
}
