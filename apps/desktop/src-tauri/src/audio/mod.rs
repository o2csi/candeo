//! The sound the computer plays, for effects that declare `inputs: ['audio']`
//! (#107, `docs/design/inputs-and-automations.md` §2.2).
//!
//! One capture for the whole application, **leased**: the first loop running an
//! effect that reads sound starts it, the last one to stop ends it. It runs on a
//! thread of its own, which also analyses what it hears 60 times a second; a
//! loop takes the latest analysis at each frame, however many loops there are.
//!
//! Privacy: samples go from the sound card to the analysis and nowhere else.
//! Nothing is recorded, and the log says only when capture starts and stops.

pub mod analysis;
#[cfg(windows)]
mod wasapi;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use analysis::{Analyzer, Frame, WINDOW};
use serde::Serialize;

/// How often the capture thread analyses what it heard: about twice an effect's
/// frame rate, so a loop always finds a fresh one.
const ANALYSIS_PERIOD: Duration = Duration::from_millis(16);

/// How long a failed capture waits before trying again: the output device may
/// come back, a driver may be restarting.
const RETRY_AFTER: Duration = Duration::from_secs(2);

/// Where capture stands, for the gallery to say when sound cannot be read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SoundState {
    /// No effect reads sound: nothing is captured.
    #[default]
    Idle,
    Capturing,
    /// An effect reads sound and it cannot be captured: no output device, a
    /// refused format, or a system this capture does not cover yet.
    Unavailable,
}

/// The capture, and who holds it.
#[derive(Default)]
pub struct Sound {
    inner: Mutex<Inner>,
    /// The latest analysis, as the bootstrap reads it; empty when none.
    latest: Arc<Mutex<String>>,
    state: Arc<Mutex<SoundState>>,
    /// Which capture thread may still write `latest` and `state`: one stopping
    /// must not overwrite what the next one, already started, reports.
    generation: Arc<AtomicU64>,
}

#[derive(Default)]
struct Inner {
    leases: usize,
    stop: Option<Arc<AtomicBool>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// Holds the capture on while it lives.
pub struct Lease {
    sound: Arc<Sound>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.sound.release();
    }
}

impl Sound {
    /// Starts the capture if nobody held it yet, and holds it.
    pub fn lease(self: &Arc<Self>) -> Lease {
        let mut inner = self.inner.lock().unwrap();
        inner.leases += 1;
        if inner.leases == 1 {
            let stop = Arc::new(AtomicBool::new(false));
            let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            let thread = {
                let stop = Arc::clone(&stop);
                let latest = Arc::clone(&self.latest);
                let state = Arc::clone(&self.state);
                let current = Arc::clone(&self.generation);
                std::thread::Builder::new()
                    .name("sound".into())
                    .spawn(move || capture(&stop, &latest, &state, &current, generation))
                    .ok()
            };
            inner.stop = Some(stop);
            inner.thread = thread;
        }
        Lease {
            sound: Arc::clone(self),
        }
    }

    fn release(&self) {
        let ending = {
            let mut inner = self.inner.lock().unwrap();
            inner.leases = inner.leases.saturating_sub(1);
            if inner.leases > 0 {
                return;
            }
            if let Some(stop) = inner.stop.take() {
                stop.store(true, Ordering::Relaxed);
            }
            inner.thread.take()
        };
        // Outside the lock: a lease taken meanwhile starts a thread of its own.
        if let Some(thread) = ending {
            let _ = thread.join();
        }
    }

    /// The latest analysis, as the bootstrap reads it.
    pub fn latest(&self) -> String {
        let latest = self.latest.lock().unwrap();
        if latest.is_empty() {
            Frame::SILENT.to_json()
        } else {
            latest.clone()
        }
    }

    pub fn state(&self) -> SoundState {
        *self.state.lock().unwrap()
    }
}

/// The capture thread: captures, analyses and publishes until told to stop,
/// trying again after a failure.
fn capture(
    stop: &AtomicBool,
    latest: &Mutex<String>,
    state: &Mutex<SoundState>,
    current: &AtomicU64,
    generation: u64,
) {
    let mine = || current.load(Ordering::SeqCst) == generation;
    let mut listener = Listener::default();
    let mut said_unavailable = false;
    while !stop.load(Ordering::Relaxed) {
        let mut heard = |samples: &[f32], rate: u32| {
            if let Some(frame) = listener.heard(samples, rate) {
                if mine() {
                    *latest.lock().unwrap() = frame.to_json();
                    *state.lock().unwrap() = SoundState::Capturing;
                }
            }
        };
        match listen(stop, &mut heard) {
            Ok(Ended::Stopped) => break,
            // The default output changed: listen to the new one at once.
            Ok(Ended::DeviceChanged) => {
                tracing::info!("sound capture follows the new default output");
            }
            Err(e) => {
                if !said_unavailable {
                    tracing::warn!("sound capture unavailable: {e}");
                    said_unavailable = true;
                }
                if mine() {
                    latest.lock().unwrap().clear();
                    *state.lock().unwrap() = SoundState::Unavailable;
                }
                let until = Instant::now() + RETRY_AFTER;
                while Instant::now() < until && !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }
    if mine() {
        latest.lock().unwrap().clear();
        *state.lock().unwrap() = SoundState::Idle;
    }
    if listener.started {
        tracing::info!("sound capture stopped");
    }
}

/// How a capture ended without failing. Only Windows captures yet.
#[cfg_attr(not(windows), allow(dead_code))]
enum Ended {
    Stopped,
    DeviceChanged,
}

/// Captures until `stop`, handing mono samples and their rate to `heard`.
#[cfg(windows)]
fn listen(stop: &AtomicBool, heard: &mut dyn FnMut(&[f32], u32)) -> Result<Ended, String> {
    wasapi::listen(stop, heard)
}

/// Linux is its own issue: the default sink's monitor, through PipeWire or
/// PulseAudio.
#[cfg(not(windows))]
fn listen(_stop: &AtomicBool, _heard: &mut dyn FnMut(&[f32], u32)) -> Result<Ended, String> {
    Err("capturing the sound playing is not supported on this system yet".into())
}

/// What the capture thread keeps between packets: the latest window of
/// samples, and when it last analysed them.
#[derive(Default)]
struct Listener {
    samples: VecDeque<f32>,
    analyzer: Option<(Analyzer, u32)>,
    last: Option<Instant>,
    started: bool,
}

impl Listener {
    /// Takes samples as they come, and analyses them every [`ANALYSIS_PERIOD`]:
    /// the analysis when one is due.
    fn heard(&mut self, samples: &[f32], rate: u32) -> Option<Frame> {
        if !self.started {
            self.started = true;
            tracing::info!(rate, "sound capture started");
        }
        if self.analyzer.as_ref().is_none_or(|(_, r)| *r != rate) {
            self.analyzer = Some((Analyzer::new(rate), rate));
        }
        self.samples.extend(samples);
        while self.samples.len() > WINDOW {
            self.samples.pop_front();
        }
        let now = Instant::now();
        let dt = self.last.map_or(ANALYSIS_PERIOD, |t| now.duration_since(t));
        if dt < ANALYSIS_PERIOD {
            return None;
        }
        self.last = Some(now);
        let (analyzer, _) = self.analyzer.as_mut()?;
        let window: Vec<f32> = self.samples.iter().copied().collect();
        Some(analyzer.analyse(&window, dt.as_secs_f32()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A listener fed a tone publishes an analysis that hears it, at the pace
    /// set, not at every packet.
    #[test]
    fn the_listener_analyses_at_its_own_pace() {
        let mut listener = Listener::default();
        let mut published = Vec::new();
        let tone: Vec<f32> = (0..480)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 48_000.0).sin())
            .collect();
        for _ in 0..10 {
            published.extend(listener.heard(&tone, 48_000));
        }
        assert_eq!(
            published.len(),
            1,
            "ten packets in a moment make one analysis"
        );
        assert!(
            published[0].level > 0.5,
            "it hears the tone: {}",
            published[0].level
        );
        std::thread::sleep(ANALYSIS_PERIOD);
        published.extend(listener.heard(&tone, 48_000));
        assert_eq!(published.len(), 2, "and another once the period has passed");
    }

    #[test]
    fn nobody_reading_is_silence() {
        let sound = Sound::default();
        assert_eq!(sound.latest(), Frame::SILENT.to_json());
        assert_eq!(sound.state(), SoundState::Idle);
    }
}
