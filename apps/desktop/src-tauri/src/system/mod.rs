//! What the computer is doing, for effects that declare `inputs: ['system']` and
//! for settings and brightnesses following a `system:` source (#241, #242,
//! `docs/design/inputs-and-automations.md` §2.4).
//!
//! **Read without a driver and without administrator rights**, or not at all:
//! a source this computer does not offer that way is `None`. One sampler for the
//! whole application, **leased** as the sound capture is: the first reader starts
//! it, the last one to go stops it. It reads once a second, and a reader eases
//! each value between two samples so a setting following it does not step.
//!
//! Privacy: numbers only. No process, adapter or device name leaves the reader.

#[cfg(target_os = "linux")]
mod linux;
pub mod parse;
#[cfg(windows)]
mod windows;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

/// How often the sampler reads: rates need two samples this far apart, and a
/// temperature or a load moves no faster.
pub const PERIOD: Duration = Duration::from_secs(1);

/// How often the sampler checks whether it should stop while it waits.
const STOP_CHECK: Duration = Duration::from_millis(100);

/// The ranges temperatures are mapped over, from a cool computer to one close
/// to throttling (§2.4).
const CPU_COOL: f32 = 40.0;
const CPU_HOT: f32 = 95.0;
const GPU_COOL: f32 = 30.0;
const GPU_HOT: f32 = 85.0;

/// The least CPU power the scale starts from: without it, an idle computer
/// would sit at the top of its own range until something heavier ran.
const POWER_FLOOR_W: f32 = 45.0;

/// A value a setting or a brightness can follow, as `system:<name>` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Cpu,
    Memory,
    Gpu,
    CpuTemp,
    GpuTemp,
    CpuPower,
}

impl Source {
    pub const ALL: [Source; 6] = [
        Source::Cpu,
        Source::Memory,
        Source::Gpu,
        Source::CpuTemp,
        Source::GpuTemp,
        Source::CpuPower,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Source::Cpu => "cpu",
            Source::Memory => "memory",
            Source::Gpu => "gpu",
            Source::CpuTemp => "cpuTemp",
            Source::GpuTemp => "gpuTemp",
            Source::CpuPower => "cpuPower",
        }
    }

    /// `system:cpu` and the others; `None` for anything else.
    pub fn parse(bound: &str) -> Option<Source> {
        let name = bound.strip_prefix("system:")?;
        Source::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// One sample, in the units read: loads and memory in 0..1, degrees Celsius,
/// watts. `None` where this computer does not offer the value, or not yet.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Reading {
    pub cpu: Option<f32>,
    pub memory: Option<f32>,
    pub gpu: Option<f32>,
    pub cpu_celsius: Option<f32>,
    pub gpu_celsius: Option<f32>,
    pub cpu_watts: Option<f32>,
}

impl Reading {
    /// Each value from `from` to `to`, `t` of the way: a value missing on one
    /// side is the other side's.
    pub fn eased(from: &Reading, to: &Reading, t: f32) -> Reading {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: Option<f32>, b: Option<f32>| match (a, b) {
            (Some(a), Some(b)) => Some(a + (b - a) * t),
            (_, b) => b,
        };
        Reading {
            cpu: mix(from.cpu, to.cpu),
            memory: mix(from.memory, to.memory),
            gpu: mix(from.gpu, to.gpu),
            cpu_celsius: mix(from.cpu_celsius, to.cpu_celsius),
            gpu_celsius: mix(from.gpu_celsius, to.gpu_celsius),
            cpu_watts: mix(from.cpu_watts, to.cpu_watts),
        }
    }

    /// A source in 0..1: loads as they are, temperatures over their range, the
    /// CPU's power against the most it has drawn since Candeo started.
    pub fn level(&self, source: Source, most_watts: f32) -> Option<f32> {
        let over = |v: f32, low: f32, high: f32| ((v - low) / (high - low)).clamp(0.0, 1.0);
        match source {
            Source::Cpu => self.cpu.map(|v| v.clamp(0.0, 1.0)),
            Source::Memory => self.memory.map(|v| v.clamp(0.0, 1.0)),
            Source::Gpu => self.gpu.map(|v| v.clamp(0.0, 1.0)),
            Source::CpuTemp => self.cpu_celsius.map(|c| over(c, CPU_COOL, CPU_HOT)),
            Source::GpuTemp => self.gpu_celsius.map(|c| over(c, GPU_COOL, GPU_HOT)),
            Source::CpuPower => self
                .cpu_watts
                .map(|w| (w / most_watts.max(POWER_FLOOR_W)).clamp(0.0, 1.0)),
        }
    }
}

/// Which sources this computer offers, found by reading once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Offers {
    pub cpu: bool,
    pub memory: bool,
    pub gpu: bool,
    pub cpu_temp: bool,
    pub gpu_temp: bool,
    pub cpu_power: bool,
}

/// What reads the values on this system. A rate — a load, the CPU's power —
/// needs two reads apart: its first read is `None`.
trait Read {
    fn read(&mut self) -> Reading;
}

/// How long the first question about what this computer offers waits between
/// its two reads: enough for a rate to have a value.
const PROBE_GAP: Duration = Duration::from_millis(250);

impl Offers {
    /// What a second read found.
    fn of(r: &Reading) -> Offers {
        Offers {
            cpu: r.cpu.is_some(),
            memory: r.memory.is_some(),
            gpu: r.gpu.is_some(),
            cpu_temp: r.cpu_celsius.is_some(),
            gpu_temp: r.gpu_celsius.is_some(),
            cpu_power: r.cpu_watts.is_some(),
        }
    }
}

#[cfg(windows)]
fn reader() -> Box<dyn Read> {
    Box::new(windows::Reader::open())
}

#[cfg(target_os = "linux")]
fn reader() -> Box<dyn Read> {
    Box::new(linux::Reader::open())
}

#[cfg(not(any(windows, target_os = "linux")))]
fn reader() -> Box<dyn Read> {
    struct Nothing;
    impl Read for Nothing {
        fn read(&mut self) -> Reading {
            Reading::default()
        }
    }
    Box::new(Nothing)
}

/// The two latest samples, and when the newer one was read.
#[derive(Clone, Copy, Default)]
struct Samples {
    previous: Reading,
    latest: Reading,
    at: Option<Instant>,
    /// How many samples since the sampler started: the loads need two.
    count: u32,
}

/// The sampler, and who holds it.
#[derive(Default)]
pub struct System {
    inner: Mutex<Inner>,
    samples: Arc<Mutex<Samples>>,
    /// The most the CPU drew since Candeo started, in watts.
    most_watts: Arc<Mutex<f32>>,
    offers: Arc<OnceLock<Offers>>,
    generation: Arc<AtomicU64>,
}

#[derive(Default)]
struct Inner {
    leases: usize,
    stop: Option<Arc<AtomicBool>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// Holds the sampler on while it lives.
pub struct Lease {
    system: Arc<System>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.system.release();
    }
}

impl System {
    /// Starts the sampler if nobody held it yet, and holds it.
    pub fn lease(self: &Arc<Self>) -> Lease {
        let mut inner = self.inner.lock().unwrap();
        inner.leases += 1;
        if inner.leases == 1 {
            let stop = Arc::new(AtomicBool::new(false));
            let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            let thread = {
                let stop = Arc::clone(&stop);
                let samples = Arc::clone(&self.samples);
                let most = Arc::clone(&self.most_watts);
                let offers = Arc::clone(&self.offers);
                let current = Arc::clone(&self.generation);
                std::thread::Builder::new()
                    .name("system".into())
                    .spawn(move || sample(&stop, &samples, &most, &offers, &current, generation))
                    .ok()
            };
            inner.stop = Some(stop);
            inner.thread = thread;
            tracing::info!("system sampling started");
        }
        Lease {
            system: Arc::clone(self),
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
            tracing::info!("system sampling stopped");
        }
    }

    /// The values now, eased from the previous sample to the latest over the
    /// period after it was read.
    pub fn now(&self) -> Reading {
        let samples = *self.samples.lock().unwrap();
        match samples.at {
            None => Reading::default(),
            Some(at) => {
                let t = at.elapsed().as_secs_f32() / PERIOD.as_secs_f32();
                Reading::eased(&samples.previous, &samples.latest, t)
            }
        }
    }

    /// A source in 0..1 now, `None` when it cannot be read.
    pub fn level(&self, source: Source) -> Option<f32> {
        self.now().level(source, *self.most_watts.lock().unwrap())
    }

    /// How many samples the running sampler has read.
    pub fn samples(&self) -> u32 {
        self.samples.lock().unwrap().count
    }

    /// Which sources this computer offers: found once, the first time anyone
    /// asks or by the sampler's second read.
    pub fn offers(&self) -> Offers {
        *self.offers.get_or_init(|| {
            let mut reader = reader();
            reader.read();
            std::thread::sleep(PROBE_GAP);
            Offers::of(&reader.read())
        })
    }

    /// Now, as the bootstrap hands it to an effect: each source in 0..1 or
    /// `null`, and the degrees and watts for an author who wants their own range.
    pub fn to_json(&self) -> String {
        let now = self.now();
        let most = *self.most_watts.lock().unwrap();
        let mut out = serde_json::Map::new();
        for source in Source::ALL {
            out.insert(source.name().into(), json(now.level(source, most)));
        }
        out.insert("cpuCelsius".into(), json(now.cpu_celsius));
        out.insert("gpuCelsius".into(), json(now.gpu_celsius));
        out.insert("cpuWatts".into(), json(now.cpu_watts));
        serde_json::Value::Object(out).to_string()
    }
}

fn json(v: Option<f32>) -> serde_json::Value {
    v.map_or(serde_json::Value::Null, |v| {
        serde_json::json!((v as f64 * 1000.0).round() / 1000.0)
    })
}

/// The sampler thread: reads once a period until told to stop.
fn sample(
    stop: &AtomicBool,
    samples: &Mutex<Samples>,
    most: &Mutex<f32>,
    offers: &OnceLock<Offers>,
    current: &AtomicU64,
    generation: u64,
) {
    let mut reader = reader();
    let mut reads = 0u32;
    *samples.lock().unwrap() = Samples::default();
    while !stop.load(Ordering::Relaxed) {
        let reading = reader.read();
        reads += 1;
        if reads == 2 {
            let _ = offers.set(Offers::of(&reading));
        }
        if current.load(Ordering::SeqCst) != generation {
            return;
        }
        if let Some(w) = reading.cpu_watts {
            let mut most = most.lock().unwrap();
            *most = most.max(w);
        }
        {
            let mut s = samples.lock().unwrap();
            // The value shown now becomes where the next easing starts, so a
            // sample arriving early or late does not make it jump.
            let shown = match s.at {
                Some(at) => Reading::eased(
                    &s.previous,
                    &s.latest,
                    at.elapsed().as_secs_f32() / PERIOD.as_secs_f32(),
                ),
                None => reading,
            };
            s.previous = shown;
            s.latest = reading;
            s.at = Some(Instant::now());
            s.count = s.count.saturating_add(1);
        }
        let until = Instant::now() + PERIOD;
        while Instant::now() < until && !stop.load(Ordering::Relaxed) {
            std::thread::sleep(STOP_CHECK);
        }
    }
}

/// The sampler held for Settings' readout while it is shown.
#[derive(Default)]
pub struct Watch(Mutex<Option<Lease>>);

/// What Settings shows: each source's value in its unit, `None` when it cannot
/// be read, and whether the sampler has read enough to say so.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemNow {
    pub offers: Offers,
    /// The loads need two samples: before that, a missing value means "not yet".
    pub settled: bool,
    pub cpu: Option<f32>,
    pub memory: Option<f32>,
    pub gpu: Option<f32>,
    pub cpu_celsius: Option<f32>,
    pub gpu_celsius: Option<f32>,
    pub cpu_watts: Option<f32>,
}

/// Holds the sampler on while Settings shows its readout, or lets it go.
#[tauri::command]
pub fn watch_system(
    state: tauri::State<'_, crate::AppState>,
    watch: tauri::State<'_, Watch>,
    on: bool,
) {
    let lease = on.then(|| state.engine.system().lease());
    *watch.0.lock().unwrap() = lease;
}

#[tauri::command]
pub fn system_now(state: tauri::State<'_, crate::AppState>) -> SystemNow {
    let system = state.engine.system();
    let now = system.now();
    SystemNow {
        offers: system.offers(),
        settled: system.samples() >= 2,
        cpu: now.cpu,
        memory: now.memory,
        gpu: now.gpu,
        cpu_celsius: now.cpu_celsius,
        gpu_celsius: now.gpu_celsius,
        cpu_watts: now.cpu_watts,
    }
}

/// Which sources this computer offers, for the lists a setting picks from.
#[tauri::command]
pub fn system_offers(state: tauri::State<'_, crate::AppState>) -> Offers {
    state.engine.system().offers()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_is_named_as_a_setting_binds_it() {
        for source in Source::ALL {
            assert_eq!(
                Source::parse(&format!("system:{}", source.name())),
                Some(source)
            );
        }
        assert_eq!(Source::parse("system:network"), None);
        assert_eq!(Source::parse("sound:bass"), None);
        assert_eq!(Source::parse("cpu"), None);
    }

    /// Temperatures over their range, loads as they are, power against the most
    /// drawn but never less than the floor.
    #[test]
    fn each_source_maps_to_its_range() {
        let r = Reading {
            cpu: Some(0.25),
            memory: Some(1.2),
            gpu: Some(0.5),
            cpu_celsius: Some(67.5),
            gpu_celsius: Some(20.0),
            cpu_watts: Some(30.0),
        };
        assert_eq!(r.level(Source::Cpu, 0.0), Some(0.25));
        assert_eq!(r.level(Source::Memory, 0.0), Some(1.0));
        assert_eq!(r.level(Source::CpuTemp, 0.0), Some(0.5));
        assert_eq!(r.level(Source::GpuTemp, 0.0), Some(0.0));
        assert_eq!(r.level(Source::CpuPower, 10.0), Some(30.0 / 45.0));
        assert_eq!(r.level(Source::CpuPower, 120.0), Some(0.25));
        assert_eq!(Reading::default().level(Source::GpuTemp, 0.0), None);
    }

    #[test]
    fn values_ease_between_two_samples() {
        let from = Reading {
            cpu: Some(0.0),
            gpu_celsius: Some(50.0),
            ..Reading::default()
        };
        let to = Reading {
            cpu: Some(1.0),
            gpu_celsius: None,
            memory: Some(0.5),
            ..Reading::default()
        };
        let half = Reading::eased(&from, &to, 0.5);
        assert_eq!(half.cpu, Some(0.5));
        assert_eq!(half.memory, Some(0.5), "new on the latest: taken as it is");
        assert_eq!(half.gpu_celsius, None, "gone on the latest: gone");
        assert_eq!(Reading::eased(&from, &to, 3.0).cpu, Some(1.0));
    }

    #[test]
    fn nobody_sampling_is_nothing_read() {
        let system = System::default();
        assert_eq!(system.now(), Reading::default());
        assert_eq!(system.level(Source::Cpu), None);
        let json: serde_json::Value = serde_json::from_str(&system.to_json()).unwrap();
        assert!(json["cpu"].is_null() && json["cpuCelsius"].is_null());
        assert_eq!(json.as_object().unwrap().len(), 9);
    }

    /// What this computer offers, read twice a period apart. Hardware-dependent,
    /// so run by hand: `cargo test -p candeo-desktop reads_this_computer -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn reads_this_computer() {
        let mut r = reader();
        let first = r.read();
        std::thread::sleep(PERIOD);
        let second = r.read();
        println!("first: {first:?}");
        println!("offers: {:?}", Offers::of(&second));
        println!("reading: {second:?}");
    }
}
