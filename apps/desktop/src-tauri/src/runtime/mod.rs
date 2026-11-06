//! Effects engine: **one render thread per device**, independent of the window.
//!
//! This is the only place where effect code runs. The front end never runs any:
//! it sends the source and receives the frames. The simulator preview is
//! therefore the production output, by construction — not a likeness obtained
//! by running the same code in a second JavaScript engine.
//!
//! # One device, one effect
//!
//! Each device carries its own loop, hence its own frame rate, parameters, error
//! state and output. Nothing is shared between two devices: that is what keeps a
//! failing device from affecting any other — the adoption invariant (issue #25),
//! held this time at the engine level.
//!
//! A loop receives **a layout** and **an output**, never "a keyboard". The day a
//! layout spans several devices, [`DeviceOut`] will split the frame, and the
//! effect code will not change by a single line.
//!
//! # A loop runs parts
//!
//! A device split into regions runs one effect per region, in the same loop
//! (`docs/design/regions.md` §7): each **part** renders on its own layout —
//! the device's, or a region's — then the device's frame takes each position
//! from the part owning it, the follows scale it, and it leaves once. A whole
//! device is one part owning every position, on the same path. One loop rather
//! than one per region: the family keeps a definition's pace by remembering the
//! last frame it sent, and two writers on one device would each find the
//! other's frame too soon or changed.
//!
//! # A target runs a stack
//!
//! Each target — the whole device, or one region — runs a **stack**
//! (`docs/design/composition.md` §3.8): its base, the applied effect, and
//! layers over it, each a part of its own drawn through a mask, a blend mode
//! and an opacity. A layer's effect sees its mask as its device. The frame is
//! composed from the parts in that order, then the follows scale it, then it
//! leaves once. A base, a layer or an order changes between two frames, the
//! other parts keeping their clock. See [`composition`].
//!
//! # And one preview loop, which belongs to no device
//!
//! **Previewing must never interrupt the effect running on the keyboard.**
//! With the engine at one effect per device, previewing Y on a keyboard running
//! X would stop X: browsing the gallery would turn off the current lighting
//! (issue #63). The preview is a **separate** loop, a single one, whose hardware
//! output is [`NoOutput`] — `DeviceOut::present` returning `None` already means
//! "no device open, this is not a failure".
//!
//! It **borrows the layout** of the selected device, to look like what will be
//! obtained, without taking anything else from it: not its loop, not its handle,
//! not its status line.
//!
//! That is why [`EngineReport`] keeps the two in **two separate fields** rather
//! than in a list to filter. See [`PreviewStatus`].
//!
//! # What an effect cannot drag on
//!
//! A `while (true)` in `render` would freeze its thread for good: the `stop`
//! flag is read *between* two frames, so it would never be read again — and
//! since an effect runs with the window closed, closing it would not help. An
//! endless allocation, for its part, would take down the whole process rather
//! than the effect alone.
//!
//! Neither is a matter of malice: they are two ordinary programming errors, and
//! two limits are enough to treat them as such — a compute time per frame
//! ([`FRAME_BUDGET`], and [`LOAD_BUDGET`] for the module body) and a memory per
//! effect ([`MEMORY_BUDGET`]). Exceeding one opens **no new path**: it takes the
//! exception path, which [`MAX_CONSECUTIVE_ERRORS`] turns into a clean stop.
//! What the engine adds is the name of the cause: see [`name_the_cause`].
//!
//! # Lock order
//!
//! Three locks, and the order is the declaration order: **loop table → a
//! device's thread → a loop's shared state**.
//!
//! The table is locked only long enough to read or put an `Arc` in it, never
//! during a start or while waiting for a thread to end. The next two are taken
//! together, in this order, by [`DeviceLoop::start`] and [`DeviceLoop::stop`] —
//! that is what serializes starting and stopping a device, and the lock waited
//! on is **its own**: waiting for one to end holds back no command aimed at the
//! others.
//!
//! The render thread takes only the last one: it knows only its [`Shared`] and
//! its output, never the engine. It therefore has no way to hold back a command,
//! and it never holds a device handle and an engine lock at the same time. See
//! `crate::AppState`. A command changing one region waits for the thread's
//! answer holding its device's thread lock, which the render thread never
//! takes: starts on one device are serialised, and no other device waits.
//!
//! See `docs/design/effects-runtime.md`.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use candeo_device::{Keyboard, Layout, Region};
use candeo_protocol::Rgb;
use rquickjs::function::Args;
use rquickjs::loader::{BuiltinLoader, BuiltinResolver};
use rquickjs::runtime::InterruptHandler;
use rquickjs::{CatchResultExt, Context, Function, Module, Runtime};
use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};

use crate::journal;

pub mod composition;
pub mod dimming;
#[cfg(test)]
mod equivalence;
pub mod presses;
pub mod swatch;

use composition::{Drawn, LayerStart, Look, Refusal, StackStart};
use presses::Presses;

/// Parameters bound to a live value, by parameter: `{"colour": "signal:status"}`
/// (`docs/design/inputs-and-automations.md` §2.3.1).
pub type Bindings = std::collections::BTreeMap<String, String>;

/// The module the host provides, and that the editor describes through its
/// `.d.ts`.
const API_JS: &str = include_str!("api.js");

/// The glue that imports the effect and installs the render function.
const BOOTSTRAP_JS: &str = include_str!("bootstrap.js");

/// An effect's TypeScript to the JavaScript this engine loads, or every
/// problem found, each with its line and column (#381).
///
/// The one contract every caller compiles through: the library recording a
/// file, and the MCP server validating a source to come. Imports are checked
/// against what this engine's `api.js` exports.
pub(crate) fn compile(
    source: &str,
) -> Result<candeo_compiler::Compiled, Vec<candeo_compiler::Diagnostic>> {
    static API: std::sync::OnceLock<candeo_compiler::Api> = std::sync::OnceLock::new();
    candeo_compiler::compile(
        source,
        API.get_or_init(|| candeo_compiler::Api::from_module(API_JS)),
    )
}

/// The stack of a thread loading effect code outside a render loop: what a
/// render loop's thread has, Rust's default.
const ENGINE_STACK: usize = 2 * 1024 * 1024;

/// Runs `f`, which loads effect code, on a thread of its own.
///
/// QuickJS stops a runaway recursion at its own limit, 1 MiB below where its
/// runtime was created, which protects only a thread with more stack than
/// that. Tauri runs sync commands and its setup on the main thread, which has
/// 1 MiB on Windows: 744 nested arrays loaded there abort the whole process
/// (`docs/design/headless-transpile-poc.md` §3.9). The window's TypeScript
/// refused more than 585; Oxc compiles whatever nesting a source has.
pub(crate) fn on_engine_thread<T: Send>(f: impl FnOnce() -> T + Send) -> crate::CmdResult<T> {
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("candeo-compile".into())
            .stack_size(ENGINE_STACK)
            .spawn_scoped(scope, f)
            .map_err(|e| {
                crate::failure::Failure::unexpected(format!("compile thread not started: {e}"))
            })?;
        Ok(worker
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic)))
    })
}

/// 30 frames per second — **measured, not assumed**.
///
/// The frame rate was 60, by analogy with a screen. Timing on the hardware says
/// otherwise: a full update costs **7 control transfers** — 6 rows then the
/// switch to custom mode, see `Keyboard::present` — and the 120 measured give
/// **13.1 ms on average, 14.4 ms at worst**, without a single refused write. The
/// device therefore accepts up to ~76 fps.
///
/// 60 fps therefore *barely* held: the write alone ate **78 % of the 16.7 ms
/// period**, leaving ~3.6 ms to the effect — less than the compute budget it is
/// granted. In other words an effect **well within its limits** already made the
/// deadline slip, and the loop silently fell back to a frame rate it announced
/// nowhere.
///
/// At 33.3 ms, the write drops to 39 % and ~20 ms remain for the effect. What 60
/// promised without keeping, 30 keeps.
///
/// ⚠️ **The bottleneck is the bus, not the JavaScript.** Two leads if the frame
/// rate ever had to go back up: rewrite only the rows that change — partial
/// writes are verified on hardware — and stop re-sending the custom mode report
/// when already in it, which alone accounts for ~1.9 ms of the 13.
const FPS: u32 = 30;

/// Beyond this, stop. An effect that throws on every frame will not recover on
/// its own, and carrying on would only fill the log silently.
const MAX_CONSECUTIVE_ERRORS: u32 = 30;

/// Consecutive failed writes after which the loop **closes** its device.
///
/// An unplugged keyboard leaves a dead handle: every write fails, and plugging
/// it back in creates a new device instance that this handle will never reach.
/// Kept open, it made the window and the tray report the device as open while
/// nothing got through (#72). Closed, every view tells the truth, and
/// reconnecting goes through the existing commands.
///
/// One second at [`FPS`]: a transient failure does not close anything, and a
/// device that is gone is not presented as open for long.
const MAX_DEVICE_WRITE_ERRORS: u32 = FPS;

/// Time granted to compute **one** frame.
///
/// ## What this figure really measures
///
/// Not a performance allowance: **a freeze detector**, with headroom for machine
/// hiccups. An ordinary effect costs **0.23 ms** and a field of five thousand
/// particles with a one-second trail **1.1 ms** — measured in `release` on this
/// engine, for a 132-LED keyboard. No realistic effect lives between 1 and
/// 10 ms.
///
/// What these milliseconds buy is therefore **tolerated preemption**: the
/// deadline is measured in wall-clock time, not compute time, and a thread the
/// scheduler suspends in the middle of a frame consumes its budget without
/// executing anything. At 10 ms, an ordinary effect can be suspended for nearly
/// 10 ms without being accused of freezing.
///
/// ## Where the ceiling is
///
/// The HID write of a full frame costs **13.1 ms on average, 14.4 ms at worst**
/// (§5 of the survey), and it lives in the same 33.3 ms period:
///
/// ```text
///   budget 10 ms + write 14.4 ms = 24.4 ms   →  9 ms of headroom
/// ```
///
/// The threshold where the deadline would start to **slip silently** is around
/// **19 ms** of budget. We are far from it, and that is what makes 10 ms safe
/// where the original figure — half of a 16.7 ms period — was just a proportion,
/// and no longer a measurement.
///
/// ## And far below a freeze
///
/// An effect that does not return is stopped after [`MAX_CONSECUTIVE_ERRORS`]
/// frames, that is **0.3 s** — whereas a budget of one second per frame would
/// have waited half a minute before saying what is wrong.
///
/// Exceeding it once stops nothing: the consecutive error counter goes back to
/// zero at the first rendered frame. It takes thirty in a row.
#[cfg(not(debug_assertions))]
const FRAME_BUDGET: Duration = Duration::from_millis(10);

/// The same budget, at the speed of the engine actually compiled.
///
/// QuickJS is C, compiled at the profile's optimization level. Unoptimized, the
/// **same** frame of the **same** simple effect went from 0.23 ms to 6.3 ms of
/// JavaScript: twenty-five times slower, measured. It was not the effect that
/// changed, it was the interpreter.
///
/// A single budget would therefore have had to pick a side: at 10 ms it would
/// cut flawless effects as soon as the app runs in development; at 200 ms it
/// would allow a six-second freeze in production.
///
/// ⚠️ **This factor of 25 was measured before `[profile.dev.package."*"]` moved
/// dependencies to `opt-level = 2`.** QuickJS comes in through a dependency: it
/// is therefore optimized in debug builds too now, and the gap should have
/// melted. This value remains **a ceiling, not a target** — keeping it wide
/// costs nothing as long as nobody takes it for an up-to-date measurement. To be
/// redone if someone wants to unify the two budgets.
#[cfg(debug_assertions)]
const FRAME_BUDGET: Duration = Duration::from_millis(200);

/// Time granted to **load** an effect, module body included.
///
/// The module body runs once, before the first frame: it therefore escapes the
/// frame budget. Without a limit here, a loop written outside `render` would
/// block [`DeviceLoop::start`] forever — the command waits for the load verdict
/// while holding the device lock, and that keyboard would never start or stop
/// anything again.
///
/// Parsing and evaluating a module takes milliseconds; two seconds is three
/// orders of magnitude above, and that price is paid only once.
const LOAD_BUDGET: Duration = Duration::from_secs(2);

/// Memory granted to an effect's JavaScript engine — **one per device**.
///
/// The frame buffer weighs nothing: `bootstrap.js` reuses it from one frame to
/// the next. But an effect is allowed to keep state, and that is what has to
/// fit. Measured, QuickJS context and loaded modules included: 0.17 MB for a
/// stateless effect, 2.7 MB for two thousand particles keeping one second of
/// frames, 6 MB for five thousand. Thirty-two megabytes therefore leave five
/// times the most outlandish effect we know how to write for 132 LEDs, and
/// nearly two hundred times the ordinary effect — while staying negligible next
/// to the application, even with one effect per keyboard.
///
/// Unlike time, memory does not depend on the build profile: the same objects
/// take the same bytes.
///
/// What is bounded is not an effect's appetite: it is an array that grows on
/// every frame taking down the whole process instead of itself.
const MEMORY_BUDGET: usize = 32 * 1024 * 1024;

/// What a device's loop shares with the rest of the application: the device's
/// half — its output, its frames, its regions. Each part's half is
/// [`PartShared`].
///
/// Everything is behind `Arc`: the render thread outlives the window, so it
/// cannot borrow anything from a command's state.
struct Shared {
    /// The loop ends: set by a command, or by the loop when no part runs any
    /// more after failing.
    stop: AtomicBool,
    /// Keyboard output. Separate from the simulator output — one must be able
    /// to write an effect without owning the keyboard, and to leave it running
    /// without watching the screen.
    to_keyboard: AtomicBool,
    /// Simulator output. `None` while nobody listens: nothing is serialized
    /// then.
    frames: Mutex<Option<Channel<InvokeResponseBody>>>,
    /// Last failed write to the keyboard.
    ///
    /// Distinct from the effect error of each part: these two failures have
    /// neither the same cause nor the same remedy, and mixing them up would
    /// send someone looking in the wrong place. A flawless effect may well
    /// reach no LED.
    device_error: Mutex<Option<Failure>>,
    /// True if the last frame was actually written to a device.
    ///
    /// Without it, starting an effect with no keyboard connected produced **no
    /// sign at all**: the simulator animated, the "Send to keyboard" box stayed
    /// checked, and the keyboard kept its previous frame. A silence that reads
    /// as an engine failure.
    reaching: AtomicBool,
    /// Consecutive failed writes, reset by any successful write. See
    /// [`MAX_DEVICE_WRITE_ERRORS`].
    device_failures: AtomicU32,
    /// The device's layout, which its layers' masks are resolved on.
    layout: &'static Layout,
    /// The device's regions, in its definition's order, while the loop runs
    /// them; `None` while it runs one stack on the whole device.
    regions: Option<&'static [Region]>,
    /// The parts as the commands and the status reach them, in the order they
    /// are composed: each target's base, then its layers, bottom to top. A
    /// part stopped on its own keeps its place, with its error; a target
    /// running nothing has none.
    parts: Mutex<Vec<Arc<PartShared>>>,
    /// What the commands ask of the running thread, read between two frames.
    requests: Mutex<Option<Sender<Request>>>,
}

impl Shared {
    fn new(
        layout: &'static Layout,
        regions: Option<&'static [Region]>,
        requests: Sender<Request>,
    ) -> Self {
        Self {
            stop: AtomicBool::new(false),
            to_keyboard: AtomicBool::new(true),
            frames: Mutex::new(None),
            device_error: Mutex::new(None),
            reaching: AtomicBool::new(false),
            device_failures: AtomicU32::new(0),
            layout,
            regions,
            parts: Mutex::new(Vec::new()),
            requests: Mutex::new(Some(requests)),
        }
    }

    /// The base of this region's stack — the whole device's with `None` — or,
    /// with `layer`, that layer of it.
    fn part(&self, region: Option<&str>, layer: Option<&str>) -> Option<Arc<PartShared>> {
        self.parts
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.is(region, layer))
            .cloned()
    }

    /// A part of the preview's one stack, its base with `None`: the preview is
    /// named by no region.
    fn preview_part(&self, layer: Option<&str>) -> Option<Arc<PartShared>> {
        self.parts
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.layer.as_deref() == layer)
            .cloned()
    }

    /// True while its thread runs the device's regions, ready for a request.
    fn runs_regions(&self) -> bool {
        self.regions.is_some() && !self.stop.load(Ordering::Relaxed)
    }
}

/// What one part shares: its effect's state, as the commands set it and the
/// status reads it.
struct PartShared {
    /// The region it runs on, `None` on the whole device.
    region: Option<&'static str>,
    /// Its layer's id in the region's stack, `None` for the base.
    layer: Option<String>,
    /// How it is drawn over what is under it, re-read on every frame: setting
    /// it does not restart the effect. A base covers its target at full.
    look: Mutex<Look>,
    /// Stopped on its own after [`MAX_CONSECUTIVE_ERRORS`], or a layer that did
    /// not load: it draws nothing, and its error says why.
    stopped: AtomicBool,
    /// Effect parameters, as JSON. Re-read on every frame: setting them does
    /// not restart the loop.
    params: Mutex<String>,
    /// Parameters bound to a signal, by parameter: `{"colour": "signal:status"}`.
    /// Re-read on every frame, like `params`.
    bindings: Mutex<Bindings>,
    /// Name of the running effect, so that the interface knows what to
    /// highlight after the window restarts.
    effect_id: Mutex<Option<String>>,
    /// The effect's last error. Readable even after the window is closed and
    /// reopened, which a one-off event would not allow.
    error: Mutex<Option<String>>,
    /// The effect reads key presses or signals: its error text stays out of the
    /// log and the diagnostic. See [`loggable`].
    reads_private: AtomicBool,
}

impl PartShared {
    /// A base, on its region or on the whole device with `None`.
    fn base(region: Option<&'static str>, start: &EffectStart) -> Self {
        Self::new(region, None, Look::base(), start)
    }

    fn new(
        region: Option<&'static str>,
        layer: Option<String>,
        look: Look,
        start: &EffectStart,
    ) -> Self {
        Self {
            region,
            layer,
            look: Mutex::new(look),
            stopped: AtomicBool::new(false),
            params: Mutex::new(start.params.clone()),
            bindings: Mutex::new(start.bindings.clone()),
            effect_id: Mutex::new(Some(start.effect_id.clone())),
            error: Mutex::new(None),
            reads_private: AtomicBool::new(false),
        }
    }

    /// True for the base of `region`'s stack, or for its `layer`.
    fn is(&self, region: Option<&str>, layer: Option<&str>) -> bool {
        self.region == region && self.layer.as_deref() == layer
    }

    fn look(&self) -> Look {
        self.look.lock().unwrap().clone()
    }

    /// As the status reads it while the loop runs.
    fn layer_status(&self, ended: bool) -> LayerEngineStatus {
        LayerEngineStatus {
            layer: self.layer.clone().unwrap_or_default(),
            running: !ended && self.running(),
            effect_id: self.effect_id(),
            error: self.error(),
            reads_private: self.private(),
        }
    }

    fn effect_id(&self) -> Option<String> {
        self.effect_id.lock().unwrap().clone()
    }

    fn error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }

    fn private(&self) -> bool {
        self.reads_private.load(Ordering::Relaxed)
    }

    fn running(&self) -> bool {
        !self.stopped.load(Ordering::Relaxed)
    }
}

/// An effect to start: which one, its code, and what it starts with.
#[derive(Clone)]
pub struct EffectStart {
    pub effect_id: String,
    pub js: String,
    /// Its parameters, as JSON.
    pub params: String,
    pub bindings: Bindings,
}

/// Why one region's effect did not start on the loop running.
#[derive(Debug, PartialEq, Eq)]
pub enum RegionStart {
    /// The loop does not run the device's regions — a rule interrupts the
    /// device, or nothing runs: the caller starts them all, with
    /// [`Engine::start_regions`].
    NotRunningRegions,
    /// The effect did not load: the region goes on with what it ran.
    Failed(String),
}

/// Why a change to one part of a running loop did not happen: a base, a layer,
/// an order (`docs/design/composition.md` §3.8).
#[derive(Debug, PartialEq, Eq)]
pub enum PartError {
    /// The loop does not run this target's stack — nothing runs, a rule
    /// interrupts the device, or the device is split one way and asked the
    /// other: the caller starts the whole composition.
    NotRunning,
    /// Refused before anything loaded: the stack goes on as it was.
    Refused(Refusal),
    /// The effect did not load: the stack goes on as it was.
    Failed(String),
}

/// How a target's stack started.
#[derive(Debug, PartialEq, Eq)]
pub struct StackVerdict {
    /// The base's, `Ok` when there is none. A target whose base does not load
    /// runs nothing, its layers included, as a device whose effect did not
    /// load ran nothing before layers.
    pub base: Result<(), String>,
    /// Each layer's, by id, in the stack's order. A layer that does not load,
    /// or whose mask lights nothing, runs nothing; the rest of the stack runs
    /// without it.
    pub layers: Vec<(String, Result<(), PartError>)>,
}

impl StackVerdict {
    fn failed(error: String) -> Self {
        Self {
            base: Err(error),
            layers: Vec::new(),
        }
    }
}

/// A layer resolved for its thread: its mask turned into the layout its effect
/// sees and the positions it may touch.
struct LayerLoad {
    id: String,
    start: EffectStart,
    layout: &'static Layout,
    owns: Vec<u16>,
    look: Look,
}

impl LayerLoad {
    fn of(
        device: &'static Layout,
        target: Option<&'static Region>,
        layer: LayerStart,
    ) -> Result<Self, Refusal> {
        let masked = composition::masked(device, target, &layer.mask).map_err(Refusal::Mask)?;
        Ok(Self {
            id: layer.id,
            start: layer.start,
            layout: masked.layout,
            owns: masked.owns,
            look: layer.look,
        })
    }
}

/// What a command asks of a running thread, between two frames: a QuickJS
/// context lives and dies on the thread that runs it, so a new part is built
/// there.
enum Request {
    /// Start this effect as the base of this region's stack — the whole
    /// device's with `None` — in place of what it ran, its layers and the
    /// other targets untouched; the verdict comes back.
    Base {
        region: Option<&'static Region>,
        start: EffectStart,
        verdict: Sender<Result<(), PartError>>,
    },
    /// Start this layer in place of the one with its id, or on top.
    Layer {
        region: Option<&'static str>,
        layer: LayerLoad,
        verdict: Sender<Result<(), PartError>>,
    },
    /// The base, with `layer` `None`, or that layer, runs nothing from now on.
    Stop {
        region: Option<&'static str>,
        layer: Option<String>,
        done: Sender<()>,
    },
    /// The target's layers in this order, bottom to top.
    Order {
        region: Option<&'static str>,
        ids: Vec<String>,
        verdict: Sender<Result<(), PartError>>,
    },
}

/// What the log and the diagnostic may say of an effect's error.
///
/// An effect that reads key presses chooses its error text, and could write the
/// presses into it; nothing about key presses reaches the log or the diagnostic
/// (`docs/design/key-input.md` §3, #44). Its text is left out there; the window,
/// which is local, still shows it.
pub(crate) fn loggable(error: &str, private: bool) -> &str {
    if private {
        "(text not logged: the effect reads key presses or signals)"
    } else {
        error
    }
}

/// Engine state for **one** device, as the interface reads it.
///
/// While the loop runs the device's regions, `running` says whether one of them
/// runs, and `effect_id` and `error` are `None`: each region's are in
/// `regions`.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub running: bool,
    pub effect_id: Option<String>,
    /// Error coming from the effect code, already readable: shown as is.
    pub error: Option<String>,
    /// One per region of the definition, in its order, while the loop runs
    /// them; absent while one stack runs on the whole device — a rule's
    /// included, on a split device.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regions: Option<Vec<RegionEngineStatus>>,
    /// The layers over the whole device's base, bottom to top; each region's
    /// are in `regions`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<LayerEngineStatus>,
    /// Failed write to the keyboard — nothing to do with the effect code.
    pub device_error: Option<Failure>,
    /// True if the frames actually reach a keyboard.
    pub reaching_keyboard: bool,
    pub to_keyboard: bool,
    /// For the diagnostic: see [`loggable`].
    #[serde(skip)]
    pub reads_private: bool,
    /// The rule interrupting this device, if one does (#106). Set from the
    /// automations when the status is reported, never by the engine, which
    /// knows nothing of rules.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interruption: Option<crate::automations::InterruptionStatus>,
}

/// One region of a split device, as the interface reads it.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegionEngineStatus {
    pub region: String,
    pub running: bool,
    /// What it runs, or ran until it stopped on its own; `None` for nothing.
    pub effect_id: Option<String>,
    /// Error coming from the region's effect, already readable.
    pub error: Option<String>,
    /// For the diagnostic: see [`loggable`].
    #[serde(skip)]
    pub reads_private: bool,
    /// The layers over its base, bottom to top.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<LayerEngineStatus>,
}

/// One layer of a stack, as the interface reads it: its row says what it
/// runs, and why it stopped (`docs/design/composition.md` §3.8).
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LayerEngineStatus {
    /// Its id in the stack.
    pub layer: String,
    pub running: bool,
    pub effect_id: Option<String>,
    /// Error coming from its effect, already readable: why it did not load,
    /// or why it stopped.
    pub error: Option<String>,
    /// For the diagnostic: see [`loggable`].
    #[serde(skip)]
    pub reads_private: bool,
}

/// A device's state, and which device it belongs to.
///
/// `engine_status()` returns one per targeted device: a global message would
/// force a choice of which one to show, and the next would erase the previous —
/// exactly what the table of open failures already avoids on the adoption side.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceEngineStatus {
    pub device: DeviceRef,
    #[serde(flatten)]
    pub status: EngineStatus,
}

/// What the window **watches**, and which reaches no keyboard.
///
/// # A type of its own, not one more line in the device list
///
/// This is the fourth time in this project that a lying state has cost a round
/// of debugging — the unadopted keyboard, the "accepted" write, the frame frozen
/// after an automatic stop, and now the preview. A flag to filter gets filtered
/// badly: a single caller that forgets it — the tray icon, the log, the gallery
/// — is enough to announce as running on the keyboard an effect that is only
/// being watched. Here there is **nothing to filter**: the preview is not in the
/// list, and a caller cannot find it there by mistake.
///
/// # What it does not carry is deliberate too
///
/// No `toKeyboard`, no `reachingKeyboard`, no `deviceError`. A preview loop has
/// **no** hardware output; those three fields set to false would not describe a
/// preview, they would describe an effect failing to write — that is, a
/// failure, where there is only a choice.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewStatus {
    /// The device whose layout the preview **borrows**.
    ///
    /// It is not controlled, it is not even necessarily plugged in: it is a
    /// geometry, not a destination. Naming it lets the interface say "what it
    /// will look like on that keyboard".
    pub layout_of: DeviceRef,
    /// The region of it whose layout is borrowed, when one is: the frames have
    /// the device's length, black outside the region.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    pub running: bool,
    pub effect_id: Option<String>,
    /// Error coming from the effect code, already readable: shown as is.
    pub error: Option<String>,
    /// For the diagnostic: see [`loggable`].
    #[serde(skip)]
    pub reads_private: bool,
    /// The layers of the stack previewed, over its base, bottom to top.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<LayerEngineStatus>,
}

/// Everything the engine knows, **arranged so that nothing gets confused**.
///
/// Two fields, not a list: `devices` describes what runs on the hardware,
/// `preview` what the window watches. The tray, the log and the gallery read
/// only the first — see [`PreviewStatus`] for why.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineReport {
    pub devices: Vec<DeviceEngineStatus>,
    /// `None` when nothing is being previewed — which is the case as soon as
    /// the window is closed, see [`Engine::stop_preview`].
    pub preview: Option<PreviewStatus>,
    /// Whether the sound playing is being captured, for the gallery to say when
    /// an effect reading it cannot hear anything (#107).
    pub sound: crate::audio::SoundState,
}

// ---------------------------------------------------------------- output

/// A loop's hardware output.
///
/// A trait rather than the [`Keyboard`] itself, for two reasons that matter
/// equally:
///
/// 1. **it is the only place where a loop touches hardware.** The day a layout
///    spans several devices, this is where the frame will be split; neither
///    the loop nor the effect code will have to change;
/// 2. **a test can make a device fail.** Without this seam, "a failing device
///    affects no other" could only be checked with two keyboards plugged in,
///    so never.
pub(crate) trait DeviceOut: Send {
    /// Writes a frame.
    ///
    /// `None` when no device is open. That is not a failure: one writes an
    /// effect without owning the keyboard, and the interface must be able to
    /// say so other than as an error.
    ///
    /// `abandon`: if this write fails too, **drop the device** in the same
    /// critical section as the write. Deciding outside it would race with a
    /// reconnection: the loop could close the keyboard that was just reopened.
    fn present(&self, colors: &[Rgb], abandon: bool) -> Option<Result<(), String>>;

    /// Takes the lights back from the firmware before a host effect's first
    /// frame. Nothing to do without a device.
    fn take_over(&self) {}

    /// Turns the backlight off, when an effect stopped on its own: its last frame,
    /// frozen, would look like an effect still running (#48). Nothing to do
    /// without a device.
    fn turn_off(&self) {}
}

/// A device handle, shared between the commands and its loop.
///
/// `Arc` because the loop outlives the window: it cannot borrow anything from a
/// command's state. `Option` because closing a device — ignored, unplugged —
/// must not stop the loop that fed it: the loop notices on the next frame and
/// reports it through `reachingKeyboard`.
pub(crate) type Handle = Arc<Mutex<Option<Keyboard>>>;

impl DeviceOut for Handle {
    fn present(&self, colors: &[Rgb], abandon: bool) -> Option<Result<(), String>> {
        // The handle lock is released **before** the result is recorded: a
        // loop never holds the handle and an engine lock at the same time.
        let mut guard = self.lock().unwrap();
        let result = guard.as_ref()?.present(colors).map_err(|e| e.to_string());
        if result.is_err() && abandon {
            // Only the keyboard that just failed can be dropped here: a
            // reconnection would have replaced it before this lock was taken.
            *guard = None;
        }
        Some(result)
    }

    fn take_over(&self) {
        let guard = self.lock().unwrap();
        if let Some(Err(e)) = guard.as_ref().map(Keyboard::take_over) {
            tracing::warn!("lights not taken back from the firmware: {e}");
        }
    }

    fn turn_off(&self) {
        let guard = self.lock().unwrap();
        if let Some(Err(e)) = guard.as_ref().map(|kb| kb.set_effect(crate::OFF, &[])) {
            tracing::warn!("backlight not turned off after the effect stopped: {e}");
        }
    }
}

/// The preview's output: **none**.
///
/// Nothing to invent here — `None` already means "no device open, this is not
/// a failure", and that is exactly what a preview is. The loop therefore feeds
/// its frame channel and nothing else, `reachingKeyboard` stays false, and not
/// a single byte goes to a keyboard.
struct NoOutput;

impl DeviceOut for NoOutput {
    fn present(&self, _colors: &[Rgb], _abandon: bool) -> Option<Result<(), String>> {
        None
    }
}

/// Who a render loop belongs to.
///
/// A type, rather than a `bool` next to the [`DeviceRef`]: the two cases are
/// logged neither at the same level nor under the same word, and "the preview's
/// device" does not exist — there is only a borrowed layout. The compiler holds
/// here a distinction that two arguments side by side would let slip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    /// A device's loop: it writes to the hardware.
    Device(DeviceRef),
    /// The preview loop, which borrows this device's layout without
    /// controlling it.
    Preview(DeviceRef),
}

// ---------------------------------------------------------------- engine

/// The running loops, one per device.
///
/// The table holds only `Arc`s: it is locked for the length of a lookup, never
/// for a start or a wait. Stopping one device's loop therefore holds back no
/// command aimed at the others — otherwise a HID write stuck on one would
/// freeze the other, and the adoption invariant would not survive the engine.
#[derive(Default)]
pub struct Engine {
    loops: Mutex<HashMap<DeviceRef, Arc<DeviceLoop>>>,
    /// The preview loop: **a single one**, with no hardware output.
    ///
    /// One per window, and there is only one — the promise "an effect runs with
    /// the window closed" holds only for devices, and a preview that nobody
    /// watches is a QuickJS context kept alive for nothing.
    ///
    /// It lives **next to** the table, never inside: a table entry would be
    /// found by `all()`, hence stopped by `stop_everywhere`, counted by
    /// `status()`, and it would have had to be excluded every time. Keeping it
    /// out of the table makes the type hold what would otherwise be held by
    /// vigilance.
    preview: Arc<DeviceLoop>,
    /// The layout the preview borrows — a device's, or one of its regions' —
    /// written and cleared with the loop.
    preview_layout: Mutex<Option<(DeviceRef, Option<String>)>>,
    /// Key presses, read only while a loop runs an effect declaring them.
    presses: Arc<Presses>,
    /// The signals held, shared with [`crate::signals`]: a loop reads the values
    /// bound to its parameters on every frame.
    signals: crate::signals::SharedStore,
    /// The sound playing, captured while a loop runs an effect declaring it.
    sound: Arc<crate::audio::Sound>,
    /// What the computer is doing, sampled while something reads it (§2.4).
    system: Arc<crate::system::System>,
    /// What each device's brightness follows, and each region's of a split
    /// device (§2.2.2). Here rather than in a loop's [`Shared`]: it belongs to
    /// the lights, and outlives the effect — a rule's effect over a split
    /// device is scaled by its regions' follows.
    dimming: dimming::Table,
    /// Where the loops' effects read the time.
    clock: Clock,
}

/// Where a loop's effects read the time (`docs/design/composition.md` §3.9).
#[derive(Clone, Copy, Debug, Default)]
enum Clock {
    /// The machine's: each part's time counts from its load, and the wall
    /// clock is the system's.
    #[default]
    System,
    /// Frame `n` of every part at `n / FPS` seconds, and the wall clock at
    /// `epoch_ms` plus as much. Two loops then render the same instants, which
    /// the machine's clock never gives them: each starts its parts' time at
    /// load, a few milliseconds apart.
    #[cfg(test)]
    Frames { epoch_ms: f64 },
}

// Only the tests' clock reads the frame's index.
#[cfg_attr(not(test), allow(unused_variables))]
impl Clock {
    /// A part's time, in seconds, and the wall clock, in milliseconds since
    /// the epoch, for its frame `index` rendered at `now`.
    fn at(self, started: Instant, now: Instant, index: u32) -> (f64, f64) {
        match self {
            Clock::System => (now.duration_since(started).as_secs_f64(), wall_clock_ms()),
            #[cfg(test)]
            Clock::Frames { epoch_ms } => {
                let time = f64::from(index) / f64::from(FPS);
                (time, epoch_ms + time * 1000.0)
            }
        }
    }

    /// The wall clock a loop's follows read at its frame `index`.
    fn wall_ms(self, index: u32) -> f64 {
        match self {
            Clock::System => wall_clock_ms(),
            #[cfg(test)]
            Clock::Frames { epoch_ms } => epoch_ms + f64::from(index) * 1000.0 / f64::from(FPS),
        }
    }
}

/// One target's stack to load in a starting thread.
struct StackLoad {
    /// The region it runs on, `None` for the whole device.
    region: Option<&'static Region>,
    base: Option<EffectStart>,
    /// Bottom to top: each layer's id and effect, and the layer resolved on
    /// its device, or refused with why.
    layers: Vec<(String, String, Result<LayerLoad, Refusal>)>,
}

impl StackLoad {
    /// `stack` on `region` of `device`, each layer's mask resolved.
    fn of(device: &'static Layout, region: Option<&'static Region>, stack: StackStart) -> Self {
        Self {
            region,
            base: stack.base,
            layers: stack
                .layers
                .into_iter()
                .map(|l| {
                    let (id, effect) = (l.id.clone(), l.start.effect_id.clone());
                    (id, effect, LayerLoad::of(device, region, l))
                })
                .collect(),
        }
    }
}

/// The layout a target's base runs on, and the positions it owns in the
/// device's frame: a region's, or every one for the whole device.
fn target_of(
    device: &'static Layout,
    region: Option<&'static Region>,
) -> (&'static Layout, Option<Vec<u16>>) {
    match region {
        Some(region) => (region.layout, Some(region.positions.to_vec())),
        None => (device, None),
    }
}

/// Whose follow scales a loop's frame (`docs/design/regions.md` §4).
#[derive(Clone)]
enum Follows {
    /// One over the whole frame: the device's, or a region's for its preview,
    /// whose frame is black outside it.
    Whole(Option<String>),
    /// Each region's over its own lights, whatever draws them: a split device,
    /// its regions' effects or a rule's over all of it.
    Regions(&'static [Region]),
}

impl Follows {
    /// Each follow's key in the table — `None` for the device's own — and the
    /// positions it scales, `None` for the whole frame.
    fn scopes(&self) -> Vec<(Option<&str>, Option<&'static [u16]>)> {
        match self {
            Follows::Whole(region) => vec![(region.as_deref(), None)],
            Follows::Regions(regions) => regions
                .iter()
                .map(|r| (Some(r.id), Some(r.positions)))
                .collect(),
        }
    }
}

/// The loop of **one** device.
///
/// Two locks, and the order between them is fixed: `thread` then `shared`.
/// `thread` serializes start and stop, and a region's start or stop, which
/// waits for the thread's answer holding it; `shared` is taken only long
/// enough to clone or replace an `Arc`, never during a wait. That is what
/// allows reading a device's state while another one starts — and even while
/// this one starts.
#[derive(Default)]
struct DeviceLoop {
    thread: Mutex<Option<JoinHandle<()>>>,
    shared: Mutex<Option<Arc<Shared>>>,
}

impl DeviceLoop {
    fn current(&self) -> Option<Arc<Shared>> {
        self.shared.lock().unwrap().clone()
    }

    /// True if **this** effect is one the loop is running, on the whole device
    /// or on a region.
    ///
    /// The id is the one requested from `start`, not a property of the loaded
    /// code: the JavaScript is read once at start and then lives in memory, so
    /// there is nothing to re-read to know it — and that is precisely why
    /// deleting an effect does not get noticed on its own.
    fn runs(&self, effect: &str) -> bool {
        self.current().is_some_and(|s| {
            s.parts
                .lock()
                .unwrap()
                .iter()
                .any(|p| p.effect_id().as_deref() == Some(effect))
        })
    }

    fn status(&self) -> EngineStatus {
        let Some(s) = self.current() else {
            return EngineStatus::default();
        };
        let parts = s.parts.lock().unwrap().clone();
        let ended = s.stop.load(Ordering::Relaxed);
        let layers = |region: Option<&str>| -> Vec<LayerEngineStatus> {
            parts
                .iter()
                .filter(|p| p.region == region && p.layer.is_some())
                .map(|p| p.layer_status(ended))
                .collect()
        };
        let device = |running, regions: Option<Vec<RegionEngineStatus>>| EngineStatus {
            running,
            effect_id: None,
            error: None,
            device_error: s.device_error.lock().unwrap().clone(),
            reaching_keyboard: s.reaching.load(Ordering::Relaxed),
            to_keyboard: s.to_keyboard.load(Ordering::Relaxed),
            reads_private: false,
            // The engine knows nothing of rules: see [`engine_status`].
            interruption: None,
            regions,
            layers: Vec::new(),
        };
        match s.regions {
            None => {
                let base = parts.iter().find(|p| p.is(None, None));
                EngineStatus {
                    effect_id: base.and_then(|p| p.effect_id()),
                    error: base.and_then(|p| p.error()),
                    reads_private: base.is_some_and(|p| p.private()),
                    layers: layers(None),
                    ..device(!ended, None)
                }
            }
            Some(regions) => {
                let regions: Vec<RegionEngineStatus> = regions
                    .iter()
                    .map(|r| {
                        let base = parts.iter().find(|p| p.is(Some(r.id), None));
                        RegionEngineStatus {
                            region: r.id.to_owned(),
                            running: !ended && base.is_some_and(|p| p.running()),
                            effect_id: base.and_then(|p| p.effect_id()),
                            error: base.and_then(|p| p.error()),
                            reads_private: base.is_some_and(|p| p.private()),
                            layers: layers(Some(r.id)),
                        }
                    })
                    .collect();
                // The loop draws while any part runs: a region's base, or a
                // layer over a region with none.
                let running = !ended && parts.iter().any(|p| p.running());
                EngineStatus {
                    reads_private: regions.iter().any(|r| r.reads_private),
                    ..device(running, Some(regions))
                }
            }
        }
    }

    /// Stops the loop and **waits** for it to end.
    ///
    /// The wait is not a detail: without it, starting an effect right after
    /// stopping one would let two loops write to the same device until the
    /// first one notices it must stop. The reasoning holds per device, and so
    /// does the lock waited on.
    ///
    /// `target` is only for the log: a loop does not know its device — it
    /// receives a layout and an output — and "effect stopped" without saying
    /// which one would be worthless with two keyboards plugged in.
    fn stop(&self, target: Target) {
        let mut thread = self.thread.lock().unwrap();
        let was_running = self.shared.lock().unwrap().take().inspect(|s| {
            s.stop.store(true, Ordering::Relaxed);
        });
        if let Some(h) = thread.take() {
            let _ = h.join();
        }
        // Only if something was running: stopping an idle device is the most
        // common action of all — every effect deletion goes through it — and
        // tells nobody anything.
        if let Some(s) = was_running {
            let effect = s.part(None, None).and_then(|p| p.effect_id());
            match (target, s.regions) {
                (Target::Device(d), None) => tracing::info!(device = %d, effect, "effect stopped"),
                (Target::Device(d), Some(_)) => tracing::info!(device = %d, "regions stopped"),
                // `debug`: see [`DeviceLoop::start`]. The "lifecycle" level
                // describes what the keyboard does, and a preview does not touch it.
                (Target::Preview(d), _) => tracing::debug!(layout = %d, effect, "preview stopped"),
            }
        }
    }

    /// Starts stacks on this target — one on the whole layout, or one per
    /// region of the device — and replaces whatever ran. Each stack's verdict
    /// comes back in order; `Err` when the thread itself did not start.
    ///
    /// A whole device whose base does not load runs nothing, as before regions
    /// and layers; nor does one left with no part at all. A region whose base
    /// does not load runs nothing, and the others start all the same.
    #[allow(clippy::too_many_arguments)]
    fn start(
        &self,
        target: Target,
        layout: &'static Layout,
        out: Box<dyn DeviceOut>,
        inputs: Inputs,
        stacks: Vec<StackLoad>,
        regions: Option<&'static [Region]>,
        follows: Follows,
    ) -> Result<Vec<StackVerdict>, String> {
        // Held from start to finish: this lock is what forbids two loops from
        // overlapping on this device. It is taken only here, in [`Self::stop`]
        // and by a region's start or stop, and the render thread does not know
        // it.
        let mut thread = self.thread.lock().unwrap();
        let previous = self.shared.lock().unwrap().take();
        if let Some(s) = &previous {
            s.stop.store(true, Ordering::Relaxed);
        }
        if let Some(h) = thread.take() {
            let _ = h.join();
        }

        let (requests, requested) = std::sync::mpsc::channel::<Request>();
        let shared = Arc::new(Shared::new(layout, regions, requests));
        // The simulator listens to a target, not to an effect: whoever subscribed
        // keeps the frames when another effect starts there. An automation
        // restarts a device's loop without the window knowing — a rule's effect,
        // then the one it gives back — and the simulator must go on showing what
        // the device shows (#106). Taken once the old loop has ended, so no frame
        // of it is racing for the channel.
        if let Some(previous) = &previous {
            *shared.frames.lock().unwrap() = previous.frames.lock().unwrap().take();
        }

        // What was asked, for the log once the verdicts are in: each stack's
        // region and base, and its layers' ids and effects.
        type Asked = (Option<&'static str>, Option<String>, Vec<(String, String)>);
        let asked: Vec<Asked> = stacks
            .iter()
            .map(|s| {
                let layers = s
                    .layers
                    .iter()
                    .map(|(id, effect, _)| (id.clone(), effect.clone()))
                    .collect();
                let base = s.base.as_ref().map(|b| b.effect_id.clone());
                (s.region.map(|r| r.id), base, layers)
            })
            .collect();

        // The JavaScript contexts are built **inside** the thread and never
        // leave it: QuickJS types do not cross threads, and confining them
        // here is safer than making them shareable.
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Loaded>();
        let s = Arc::clone(&shared);
        let handle = std::thread::Builder::new()
            .name("candeo-effect".into())
            .spawn(move || {
                render_loop(
                    target, s, layout, out, inputs, stacks, follows, ready_tx, requested,
                )
            })
            .map_err(|e| format!("render thread not started: {e}"))?;

        // Wait for the load verdicts: a syntax error must surface to the call,
        // not be discovered in a status later.
        let Ok(Loaded { verdicts, runs }) = ready_rx.recv() else {
            let _ = handle.join();
            let e = "the render thread stopped before loading the effect".to_string();
            tracing::error!(target = ?target, "{e}");
            return Err(e);
        };
        for ((region, base, layers), verdict) in asked.iter().zip(&verdicts) {
            if let Some(effect) = base {
                log_start(target, *region, None, effect, &verdict.base);
            }
            for ((id, effect), (_, verdict)) in layers.iter().zip(&verdict.layers) {
                let verdict = verdict.as_ref().map(|_| ()).map_err(ToString::to_string);
                log_start(target, *region, Some(id), effect, &verdict);
            }
        }
        if !runs {
            let _ = handle.join();
            return Ok(verdicts);
        }
        *self.shared.lock().unwrap() = Some(shared);
        *thread = Some(handle);
        Ok(verdicts)
    }

    /// Starts `start` as the base of one target's stack, its layers and the
    /// other targets untouched (`docs/design/regions.md` §7,
    /// `docs/design/composition.md` §3.8). The thread builds the context
    /// between two frames; on failure the target goes on with what it ran.
    fn start_base(
        &self,
        device: DeviceRef,
        region: Option<&str>,
        start: EffectStart,
    ) -> Result<(), PartError> {
        // Held while the thread answers, as `start` holds it: starts on one
        // device are serialised, and the thread never takes it.
        let _thread = self.thread.lock().unwrap();
        let (shared, target) = self.running_target(region)?;
        let effect = start.effect_id.clone();
        let (verdict, answer) = std::sync::mpsc::channel();
        shared.request(Request::Base {
            region: target,
            start,
            verdict,
        })?;
        // No answer: the thread ended meanwhile, on its own.
        let verdict = answer.recv().map_err(|_| PartError::NotRunning)?;
        let logged = verdict.as_ref().map(|_| ()).map_err(ToString::to_string);
        log_start(Target::Device(device), region, None, &effect, &logged);
        verdict
    }

    /// Starts `layer` on one target's stack, in place of the layer with its id
    /// or on top, the rest of the composition untouched.
    fn start_layer(
        &self,
        device: DeviceRef,
        region: Option<&str>,
        layer: LayerStart,
    ) -> Result<(), PartError> {
        let _thread = self.thread.lock().unwrap();
        let (shared, target) = self.running_target(region)?;
        let (id, effect) = (layer.id.clone(), layer.start.effect_id.clone());
        let verdict = match LayerLoad::of(shared.layout, target, layer) {
            Err(refused) => Err(PartError::Refused(refused)),
            Ok(layer) => {
                let (verdict, answer) = std::sync::mpsc::channel();
                shared.request(Request::Layer {
                    region: target.map(|r| r.id),
                    layer,
                    verdict,
                })?;
                answer.recv().map_err(|_| PartError::NotRunning)?
            }
        };
        let logged = verdict.as_ref().map(|_| ()).map_err(ToString::to_string);
        log_start(Target::Device(device), region, Some(&id), &effect, &logged);
        verdict
    }

    /// Puts one target's layers in this order, bottom to top, each keeping its
    /// clock.
    fn order_layers(&self, region: Option<&str>, ids: Vec<String>) -> Result<(), PartError> {
        let _thread = self.thread.lock().unwrap();
        let (shared, target) = self.running_target(region)?;
        let (verdict, answer) = std::sync::mpsc::channel();
        shared.request(Request::Order {
            region: target.map(|r| r.id),
            ids,
            verdict,
        })?;
        answer.recv().map_err(|_| PartError::NotRunning)?
    }

    /// The base of one target's stack, with `layer` `None`, or that layer,
    /// runs nothing from now on; the rest goes on. A base gone leaves its
    /// layers over black, and a region with nothing left is black. The loop
    /// goes on too, even with no part left: see [`render_loop`].
    fn stop_part(&self, device: DeviceRef, region: Option<&str>, layer: Option<&str>) {
        let _thread = self.thread.lock().unwrap();
        let Some(shared) = self.current() else {
            return;
        };
        let stopped = shared.part(region, layer).and_then(|p| p.effect_id());
        match self.running_target(region) {
            Ok((shared, target)) => {
                let (done, answer) = std::sync::mpsc::channel();
                let request = Request::Stop {
                    region: target.map(|r| r.id),
                    layer: layer.map(str::to_owned),
                    done,
                };
                if shared.request(request).is_ok() {
                    let _ = answer.recv();
                }
            }
            // No thread to race with: a loop ended on its own still shows the
            // part's error, which goes with the effect.
            Err(_) => shared
                .parts
                .lock()
                .unwrap()
                .retain(|p| !p.is(region, layer)),
        }
        let Some(effect) = stopped else { return };
        match (region, layer) {
            (None, None) => tracing::info!(device = %device, effect, "effect stopped"),
            (Some(region), None) => {
                tracing::info!(device = %device, region, effect, "effect stopped")
            }
            (None, Some(layer)) => tracing::info!(device = %device, layer, effect, "layer stopped"),
            (Some(region), Some(layer)) => {
                tracing::info!(device = %device, region, layer, effect, "layer stopped")
            }
        }
    }

    /// The loop's shared state and the target named — a region, or the whole
    /// device with `None` — while its thread runs that target's stack: a
    /// region of a split device, or the whole device of one left whole.
    fn running_target(
        &self,
        region: Option<&str>,
    ) -> Result<(Arc<Shared>, Option<&'static Region>), PartError> {
        let shared = self
            .current()
            .filter(|s| !s.stop.load(Ordering::Relaxed))
            .ok_or(PartError::NotRunning)?;
        let target = match (region, shared.regions) {
            (None, None) => None,
            (Some(id), Some(regions)) => Some(
                regions
                    .iter()
                    .find(|r| r.id == id)
                    .ok_or(PartError::NotRunning)?,
            ),
            _ => return Err(PartError::NotRunning),
        };
        Ok((shared, target))
    }

    /// Stops `effect` wherever this loop runs it — the loop on a whole device
    /// running nothing else, the bases and layers running it otherwise — and
    /// says whether it ran here.
    fn stop_effect(&self, device: DeviceRef, effect: &str) -> bool {
        let Some(s) = self.current() else {
            return false;
        };
        let parts = s.parts.lock().unwrap().clone();
        let running: Vec<(Option<&'static str>, Option<String>)> = parts
            .iter()
            .filter(|p| p.effect_id().as_deref() == Some(effect))
            .map(|p| (p.region, p.layer.clone()))
            .collect();
        if running.is_empty() {
            return false;
        }
        // As before layers: a whole device left with nothing stops.
        if s.regions.is_none() && running.len() == parts.len() {
            self.stop(Target::Device(device));
        } else {
            for (region, layer) in running {
                self.stop_part(device, region, layer.as_deref());
            }
        }
        true
    }
}

impl Shared {
    /// Hands a request to the thread; `NotRunning` when it is gone.
    fn request(&self, request: Request) -> Result<(), PartError> {
        let sender = self.requests.lock().unwrap().clone();
        sender
            .ok_or(PartError::NotRunning)?
            .send(request)
            .map_err(|_| PartError::NotRunning)
    }
}

impl std::fmt::Display for PartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PartError::NotRunning => write!(f, "the loop does not run this target's stack"),
            PartError::Refused(refused) => refused.fmt(f),
            PartError::Failed(e) => f.write_str(e),
        }
    }
}

/// A start's verdict, in the log: a base's, or with `layer` a layer's.
fn log_start(
    target: Target,
    region: Option<&str>,
    layer: Option<&str>,
    effect: &str,
    verdict: &Result<(), String>,
) {
    match (target, verdict) {
        (Target::Device(d), Ok(())) => match (region, layer) {
            (None, None) => tracing::info!(device = %d, effect, "effect started"),
            (Some(region), None) => tracing::info!(device = %d, region, effect, "effect started"),
            (None, Some(layer)) => tracing::info!(device = %d, layer, effect, "layer started"),
            (Some(region), Some(layer)) => {
                tracing::info!(device = %d, region, layer, effect, "layer started")
            }
        },
        // `error`: the requested effect will not run, so the lighting is not
        // the one requested. The caller gets the same message — the log is for
        // whoever reads it afterwards, and for whoever does not have the window
        // in front of them.
        (Target::Device(d), Err(e)) => match (region, layer) {
            (None, None) => tracing::error!(device = %d, effect, "effect not started: {e}"),
            (Some(region), None) => {
                tracing::error!(device = %d, region, effect, "effect not started: {e}")
            }
            (None, Some(layer)) => {
                tracing::error!(device = %d, layer, effect, "layer not started: {e}")
            }
            (Some(region), Some(layer)) => {
                tracing::error!(device = %d, region, layer, effect, "layer not started: {e}")
            }
        },
        // `debug` and not `info`, and this is not shyness: "lifecycle" is the
        // level that describes **what the keyboard does**, and a preview does
        // not touch it. Browsing the gallery would otherwise fill the log with
        // lines that match nothing lit.
        (Target::Preview(d), Ok(())) => match layer {
            None => tracing::debug!(layout = %d, effect, "preview started"),
            Some(layer) => tracing::debug!(layout = %d, layer, effect, "preview layer started"),
        },
        // `warn`: nothing is broken on the hardware, but the screen will not
        // show what was requested — and that is precisely the first place where
        // a freshly written effect breaks.
        (Target::Preview(d), Err(e)) => match layer {
            None => tracing::warn!(layout = %d, effect, "preview not started: {e}"),
            Some(layer) => {
                tracing::warn!(layout = %d, layer, effect, "preview layer not started: {e}")
            }
        },
    }
}

/// The verdict of a start of one stack on the whole device or the preview.
fn single(verdicts: Vec<StackVerdict>) -> StackVerdict {
    verdicts
        .into_iter()
        .next()
        .unwrap_or_else(|| StackVerdict::failed("nothing was started".into()))
}

impl Engine {
    /// This device's loop, created stopped if it did not exist.
    ///
    /// Only a start creates one. The entry is then never removed: a device that
    /// has carried an effect keeps its line in `engine_status()`, stopped rather
    /// than absent. "This device is doing nothing" and "I know nothing about
    /// this device" are not the same statement.
    fn device_loop(&self, device: DeviceRef) -> Arc<DeviceLoop> {
        Arc::clone(self.loops.lock().unwrap().entry(device).or_default())
    }

    /// This device's loop, **without creating one**.
    ///
    /// Setting or stopping a device that never started anything does nothing,
    /// and above all must not invent a status line for it.
    fn existing(&self, device: DeviceRef) -> Option<Arc<DeviceLoop>> {
        self.loops.lock().unwrap().get(&device).map(Arc::clone)
    }

    /// All loops, with the table unlocked.
    ///
    /// The copy is not a detail: acting on a loop means waiting for a thread to
    /// end, which we refuse to do while holding the table lock.
    fn all(&self) -> Vec<(DeviceRef, Arc<DeviceLoop>)> {
        let mut all: Vec<_> = self
            .loops
            .lock()
            .unwrap()
            .iter()
            .map(|(d, l)| (*d, Arc::clone(l)))
            .collect();
        // A hash table orders nothing, and a list that reorders itself on every
        // query is unreadable in the interface.
        all.sort_by_key(|(d, _)| (d.vid, d.pid));
        all
    }

    /// State of every device targeted since the application started.
    ///
    /// **The preview is not in it, and cannot be**: it does not live in the
    /// table. This is what the tray icon and the diagnostic read — the two
    /// places that describe the hardware.
    pub fn device_status(&self) -> Vec<DeviceEngineStatus> {
        self.all()
            .into_iter()
            .map(|(device, l)| DeviceEngineStatus {
                device,
                status: l.status(),
            })
            .collect()
    }

    /// The current preview, if there is one.
    ///
    /// `None` as soon as the loop is stopped by [`Self::stop_preview`]: a
    /// preview is transient, and "the last effect you watched" is information
    /// for nobody. A preview that cut out **on its own** — thirty failed frames
    /// — does stay visible, with `running` false and the error along with it:
    /// that is the only way to know why the screen froze.
    pub fn preview_status(&self) -> Option<PreviewStatus> {
        let (layout_of, region) = self.preview_layout.lock().unwrap().clone()?;
        let s = self.preview.current()?;
        let part = s.preview_part(None);
        let ended = s.stop.load(Ordering::Relaxed);
        let layers = s
            .parts
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.layer.is_some())
            .map(|p| p.layer_status(ended))
            .collect();
        let status = PreviewStatus {
            layout_of,
            region,
            running: !ended,
            effect_id: part.as_ref().and_then(|p| p.effect_id()),
            error: part.as_ref().and_then(|p| p.error()),
            reads_private: part.is_some_and(|p| p.private()),
            layers,
        };
        Some(status)
    }

    /// Everything the engine knows, devices and preview **kept apart**.
    pub fn report(&self) -> EngineReport {
        EngineReport {
            devices: self.device_status(),
            preview: self.preview_status(),
            sound: self.sound.state(),
        }
    }

    /// The shared state of the loop running on this device, if there is one.
    fn shared(&self, device: DeviceRef) -> Option<Arc<Shared>> {
        self.existing(device).and_then(|l| l.current())
    }

    pub fn stop(&self, device: DeviceRef) {
        if let Some(l) = self.existing(device) {
            l.stop(Target::Device(device));
        }
    }

    /// Stops **all** loops, preview included, and waits for them to end.
    ///
    /// Used at process exit as well as when resetting the configuration: in
    /// both cases we start again from a known state, and leaving running loops
    /// that nothing refers to any more would be exactly the opposite. The
    /// preview is part of it — it writes to no keyboard, but it keeps a QuickJS
    /// context and a thread alive.
    pub fn stop_all(&self) {
        self.stop_preview();
        for (device, l) in self.all() {
            l.stop(Target::Device(device));
        }
    }

    // ------------------------------------------------------------ preview

    /// Starts — or replaces — the preview of one effect, **without touching
    /// any device**: [`Self::start_preview_stack`] with a base alone.
    pub fn start_preview(
        &self,
        layout_of: DeviceRef,
        region: Option<&'static Region>,
        start: EffectStart,
        layout: &'static Layout,
    ) -> Result<(), String> {
        match self.start_preview_stack(layout_of, region, StackStart::base(Some(start)), layout) {
            Ok(verdict) => verdict.base,
            Err(refused) => Err(refused.to_string()),
        }
    }

    /// Starts — or replaces — the preview of a stack, **without touching any
    /// device** (`docs/design/composition.md` §3.9).
    ///
    /// `layout_of` names the device whose layout is borrowed; it is neither
    /// open, nor controlled, nor even necessarily plugged in, and `layout` is
    /// its. With `region`, the stack runs on that region: its effects see the
    /// region alone, the frames keep the device's length, black outside it,
    /// and its follow is the region's. The output is [`NoOutput`]: not a single
    /// byte goes to a keyboard, whatever happens. The frames are composed by
    /// the function the device loops compose with, so the same stack at the
    /// same instants gives the same bytes in both.
    ///
    /// Replacing costs a QuickJS context destroyed and another one built per
    /// part. That is not free, and that is why the rate is bounded **on the
    /// gesture side** — the window waits for the selection to settle before
    /// calling. Bounding it here would have forced a choice between making the
    /// last selection wait and losing it, and the window would have had to
    /// reconcile what it believed it had requested with what is running.
    pub fn start_preview_stack(
        &self,
        layout_of: DeviceRef,
        region: Option<&'static Region>,
        stack: StackStart,
        layout: &'static Layout,
    ) -> Result<StackVerdict, Refusal> {
        stack.check()?;
        // Written **before** the start: if the start fails, the loop is empty
        // and `preview_status` returns `None` anyway — whereas a layout set
        // afterwards would be missing for the whole load.
        *self.preview_layout.lock().unwrap() = Some((layout_of, region.map(|r| r.id.to_owned())));
        let started = self.preview.start(
            Target::Preview(layout_of),
            layout,
            Box::new(NoOutput),
            self.inputs(),
            vec![StackLoad::of(layout, region, stack)],
            None,
            Follows::Whole(region.map(|r| r.id.to_owned())),
        );
        Ok(started.map_or_else(StackVerdict::failed, single))
    }

    /// Stops the preview. No device effect is touched.
    pub fn stop_preview(&self) {
        // The layout is taken before the stop, so that the log line names the
        // one that was borrowed rather than nothing.
        let borrowed = self.preview_layout.lock().unwrap().take();
        if let Some((device, _)) = borrowed {
            self.preview.stop(Target::Preview(device));
        }
    }

    /// The parameters of the preview's base, or of its `layer`.
    pub fn set_preview_params(&self, layer: Option<&str>, params: String) {
        if let Some(p) = self.preview.current().and_then(|s| s.preview_part(layer)) {
            *p.params.lock().unwrap() = params;
        }
    }

    /// The bindings of the preview's base, or of its `layer`.
    pub fn set_preview_bindings(&self, layer: Option<&str>, bindings: Bindings) {
        if let Some(p) = self.preview.current().and_then(|s| s.preview_part(layer)) {
            *p.bindings.lock().unwrap() = bindings;
        }
    }

    /// How one layer of the preview is drawn, from its next frame.
    pub fn set_preview_look(&self, layer: &str, look: Look) {
        if let Some(p) = self
            .preview
            .current()
            .and_then(|s| s.preview_part(Some(layer)))
        {
            *p.look.lock().unwrap() = look;
        }
    }

    pub fn set_preview_channel(&self, channel: Option<Channel<InvokeResponseBody>>) {
        if let Some(s) = self.preview.current() {
            *s.frames.lock().unwrap() = channel;
        }
    }

    /// Stops this effect **wherever it runs**, and returns the devices affected.
    ///
    /// Called before an effect is deleted: the loop runs an `effect.js` loaded
    /// into memory at start, so it would carry on without the slightest error
    /// while its folder no longer exists — a device controlled by an effect
    /// missing from the library.
    ///
    /// All devices, not just the one being looked at: the same effect can be
    /// started on as many keyboards as one likes, and missing one would leave it
    /// in that invisible state. On a split device, the regions running it stop
    /// and the others go on.
    ///
    /// The device's status line does not disappear, but it stops naming the
    /// effect — that is what [`DeviceLoop::stop`] does, and it is exactly what
    /// is wanted here: the id no longer refers to anything.
    pub fn stop_everywhere(&self, effect: &str) -> Vec<DeviceRef> {
        // **The preview too**, and for exactly the same reason: it runs the
        // same `effect.js` loaded into memory, and leaving it running would give
        // a screen animating an effect missing from the library. It is not in
        // the returned list — no device was touched.
        if self.preview.runs(effect) {
            self.stop_preview();
        }

        let mut stopped = Vec::new();
        for (device, l) in self.all() {
            // The table lock is already released — `all` copied the pointers.
            // A stop waits for a thread to end, and a command aimed at another
            // device is never made to wait.
            if l.stop_effect(device, effect) {
                stopped.push(device);
            }
        }
        stopped
    }

    /// Renames this effect wherever it runs, preview and regions included.
    ///
    /// The loops keep the JavaScript they loaded at start: only the id they
    /// report changes. That is what lets renaming an effect leave the lighting as
    /// it is, while every status line names the effect the way the library does.
    pub fn rename_everywhere(&self, from: &str, to: &str) {
        let loops = self
            .all()
            .into_iter()
            .map(|(_, l)| l)
            .chain(std::iter::once(Arc::clone(&self.preview)));
        for l in loops {
            let Some(s) = l.current() else { continue };
            for part in s.parts.lock().unwrap().iter() {
                let mut id = part.effect_id.lock().unwrap();
                if id.as_deref() == Some(from) {
                    *id = Some(to.to_owned());
                }
            }
        }
    }

    /// The parameters of the base on `region` — the whole device's with `None`
    /// — or of its `layer`, re-read from its next frame. Nothing when that part
    /// does not run — a region during a rule's interruption among others.
    pub fn set_params(
        &self,
        device: DeviceRef,
        region: Option<&str>,
        layer: Option<&str>,
        params: String,
    ) {
        if let Some(p) = self.shared(device).and_then(|s| s.part(region, layer)) {
            *p.params.lock().unwrap() = params;
        }
    }

    /// The bindings of a base or a layer, as [`Self::set_params`].
    pub fn set_bindings(
        &self,
        device: DeviceRef,
        region: Option<&str>,
        layer: Option<&str>,
        bindings: Bindings,
    ) {
        if let Some(p) = self.shared(device).and_then(|s| s.part(region, layer)) {
            *p.bindings.lock().unwrap() = bindings;
        }
    }

    /// How a layer is drawn — its blend, its opacity, what that follows — from
    /// its next frame, its effect going on. A mask changed is another layout
    /// for the effect: that is [`Self::start_layer`].
    pub fn set_look(&self, device: DeviceRef, region: Option<&str>, layer: &str, look: Look) {
        if let Some(p) = self
            .shared(device)
            .and_then(|s| s.part(region, Some(layer)))
        {
            *p.look.lock().unwrap() = look;
        }
    }

    /// The sound playing, for Settings to tune and to meter.
    pub fn sound(&self) -> Arc<crate::audio::Sound> {
        Arc::clone(&self.sound)
    }

    /// What the computer is doing, for Settings to show.
    pub fn system(&self) -> Arc<crate::system::System> {
        Arc::clone(&self.system)
    }

    /// The signal store the loops read, for [`crate::signals`] to hold.
    pub fn signals(&self) -> crate::signals::SharedStore {
        Arc::clone(&self.signals)
    }

    fn inputs(&self) -> Inputs {
        Inputs {
            presses: Arc::clone(&self.presses),
            signals: Arc::clone(&self.signals),
            sound: Arc::clone(&self.sound),
            system: Arc::clone(&self.system),
            dimming: Arc::clone(&self.dimming),
            clock: self.clock,
        }
    }

    /// What this device's brightness follows — or one of its regions', with
    /// `region` — `None` for nothing: its loop and a preview on its layout read
    /// it from the next frame.
    pub fn set_dimming(
        &self,
        device: DeviceRef,
        region: Option<&str>,
        dimming: Option<dimming::Dimming>,
    ) {
        let key = (device, region.map(str::to_owned));
        let mut table = self.dimming.lock().unwrap();
        match dimming {
            Some(d) => table.insert(key, d),
            None => table.remove(&key),
        };
    }

    /// Every device's and region's at once, as the file keeps them: at
    /// startup, and when the configuration is reset.
    pub fn set_dimmings(&self, all: dimming::Dimmings) {
        *self.dimming.lock().unwrap() = all;
    }

    pub fn set_to_keyboard(&self, device: DeviceRef, on: bool) {
        if let Some(s) = self.shared(device) {
            s.to_keyboard.store(on, Ordering::Relaxed);
        }
    }

    pub fn set_channel(&self, device: DeviceRef, channel: Option<Channel<InvokeResponseBody>>) {
        if let Some(s) = self.shared(device) {
            *s.frames.lock().unwrap() = channel;
        }
    }

    /// Starts an effect on a whole device, with no layer: [`Self::start_stack`]
    /// with a base alone, as a rule's interruption runs. Replaces whatever ran
    /// there, its regions included.
    pub fn start(
        &self,
        device: DeviceRef,
        start: EffectStart,
        layout: &'static Layout,
        out: Box<dyn DeviceOut>,
        split: bool,
    ) -> Result<(), String> {
        match self.start_stack(device, layout, out, StackStart::base(Some(start)), split) {
            Ok(verdict) => verdict.base,
            Err(refused) => Err(refused.to_string()),
        }
    }

    /// Starts a stack on a whole device — its base, and its layers over it —
    /// replacing whatever ran there, its regions included
    /// (`docs/design/composition.md` §3.8).
    ///
    /// Refused, with what ran left running, when the stack holds too many
    /// effects or an id twice. A base that does not load leaves the device
    /// running nothing, as before layers; a layer that does not load, or whose
    /// mask lights nothing here, runs nothing, and its verdict says why.
    ///
    /// `split` says whose follows scale the frame: the regions' on a split
    /// device a rule interrupts, the device's otherwise.
    ///
    /// The other devices are not touched — not their loop, not their frame
    /// rate, not their error state.
    pub fn start_stack(
        &self,
        device: DeviceRef,
        layout: &'static Layout,
        out: Box<dyn DeviceOut>,
        stack: StackStart,
        split: bool,
    ) -> Result<StackVerdict, Refusal> {
        stack.check()?;
        let follows = if split && !layout.regions.is_empty() {
            Follows::Regions(layout.regions)
        } else {
            Follows::Whole(None)
        };
        let started = self.device_loop(device).start(
            Target::Device(device),
            layout,
            out,
            self.inputs(),
            vec![StackLoad::of(layout, None, stack)],
            None,
            follows,
        );
        Ok(started.map_or_else(StackVerdict::failed, single))
    }

    /// Starts the device's regions, each with its stack — a base or none, and
    /// layers — replacing whatever ran; a region not given one runs nothing.
    /// Refused as a whole, before anything stops, when one stack is. A region
    /// whose base does not load runs nothing, and its verdict says why; the
    /// loop runs even when no region runs anything, keeping their lights black.
    /// The verdicts come in the definition's order.
    pub fn start_regions(
        &self,
        device: DeviceRef,
        layout: &'static Layout,
        out: Box<dyn DeviceOut>,
        mut regions: Vec<(&'static Region, StackStart)>,
    ) -> Result<Vec<(&'static str, StackVerdict)>, Refusal> {
        for (_, stack) in &regions {
            stack.check()?;
        }
        let ids: Vec<&'static str> = layout.regions.iter().map(|r| r.id).collect();
        let stacks = layout
            .regions
            .iter()
            .map(|region| {
                let given = regions.iter().position(|(r, _)| r.id == region.id);
                let stack = given.map_or_else(StackStart::default, |at| regions.swap_remove(at).1);
                StackLoad::of(layout, Some(region), stack)
            })
            .collect();
        let started = self.device_loop(device).start(
            Target::Device(device),
            layout,
            out,
            self.inputs(),
            stacks,
            Some(layout.regions),
            Follows::Regions(layout.regions),
        );
        Ok(match started {
            Ok(verdicts) => ids.into_iter().zip(verdicts).collect(),
            Err(e) => ids
                .into_iter()
                .map(|id| (id, StackVerdict::failed(e.clone())))
                .collect(),
        })
    }

    /// Starts an effect on one region, the others untouched: their clocks go
    /// on. `Err(NotRunningRegions)` when the loop does not run the device's
    /// regions: the caller then calls [`Self::start_regions`].
    pub fn start_region(
        &self,
        device: DeviceRef,
        region: &str,
        start: EffectStart,
    ) -> Result<(), RegionStart> {
        match self.start_base(device, Some(region), start) {
            Ok(()) => Ok(()),
            Err(PartError::NotRunning) => Err(RegionStart::NotRunningRegions),
            Err(e) => Err(RegionStart::Failed(e.to_string())),
        }
    }

    /// The region runs nothing, its lights black; the others go on.
    pub fn stop_region(&self, device: DeviceRef, region: &str) {
        self.stop_base(device, Some(region));
    }

    /// Starts an effect as the base of one target's stack — a region's, or
    /// the whole device's with `None` — between two frames: its layers and the
    /// other targets keep their clocks. What the tray and *Apply* do to a
    /// device with layers. `Err(NotRunning)` when the loop does not run that
    /// target's stack: the caller starts the whole composition.
    pub fn start_base(
        &self,
        device: DeviceRef,
        region: Option<&str>,
        start: EffectStart,
    ) -> Result<(), PartError> {
        match self.existing(device) {
            Some(l) => l.start_base(device, region, start),
            None => Err(PartError::NotRunning),
        }
    }

    /// The target's base runs nothing; its layers go on, over black.
    pub fn stop_base(&self, device: DeviceRef, region: Option<&str>) {
        if let Some(l) = self.existing(device) {
            l.stop_part(device, region, None);
        }
    }

    /// Starts a layer on one target's stack between two frames, in place of the
    /// layer with its id — its new effect or its new mask — or on top. The
    /// other parts keep their clocks. A layer that does not load, a mask that
    /// lights nothing, a ninth effect: the stack goes on as it was, and the
    /// error says why.
    pub fn start_layer(
        &self,
        device: DeviceRef,
        region: Option<&str>,
        layer: LayerStart,
    ) -> Result<(), PartError> {
        match self.existing(device) {
            Some(l) => l.start_layer(device, region, layer),
            None => Err(PartError::NotRunning),
        }
    }

    /// The layer runs nothing from now on; the rest of the stack goes on.
    pub fn stop_layer(&self, device: DeviceRef, region: Option<&str>, layer: &str) {
        if let Some(l) = self.existing(device) {
            l.stop_part(device, region, Some(layer));
        }
    }

    /// Puts one target's layers in this order, bottom to top: `ids` names each
    /// of them once, or the order is refused. Each keeps its clock.
    pub fn order_layers(
        &self,
        device: DeviceRef,
        region: Option<&str>,
        ids: Vec<String>,
    ) -> Result<(), PartError> {
        match self.existing(device) {
            Some(l) => l.order_layers(region, ids),
            None => Err(PartError::NotRunning),
        }
    }

    /// The ids of a target's layers, bottom to top — those that did not load
    /// or stopped on their own included — while its loop runs that target's
    /// stack: a region's of a split device, or the whole device's with `None`.
    /// `None` while it does not. The engine knows no rules: a rule's effect
    /// over a whole device is that device's stack here, and the caller tells
    /// them apart.
    pub fn layers_of(&self, device: DeviceRef, region: Option<&str>) -> Option<Vec<String>> {
        let (shared, _) = self.existing(device)?.running_target(region).ok()?;
        let parts = shared.parts.lock().unwrap();
        Some(
            parts
                .iter()
                .filter(|p| p.region == region)
                .filter_map(|p| p.layer.clone())
                .collect(),
        )
    }

    /// True while the device's loop runs its regions: a rule's interruption is
    /// not them.
    pub fn runs_regions(&self, device: DeviceRef) -> bool {
        self.shared(device).is_some_and(|s| s.runs_regions())
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop_all();
    }
}

/// One part as its thread runs it: an effect, its context, its clock.
struct Part {
    shared: Arc<PartShared>,
    /// The positions it owns in the device's frame, `None` for every one.
    owns: Option<Vec<u16>>,
    /// Its region and effect, for what it logs; none on the whole device, whose
    /// loop's span already names its effect.
    span: tracing::Span,
    /// The deadline its interrupt handler checks.
    budget: Rc<Budget>,
    // The context before its runtime: fields drop in declaration order, and the
    // runtime carries the module resolver the context uses until it goes.
    ctx: Context,
    _rt: Runtime,
    /// Where its `time` counts from: replacing one region's effect restarts
    /// that region's clock alone.
    started: Instant,
    frame_index: u32,
    consecutive_errors: u32,
    reads_signals: bool,
    reads_audio: bool,
    reads_system: bool,
    /// The sound capture, held while the effect declares it or a setting
    /// follows the sound (§2.2).
    hearing: Option<crate::audio::Lease>,
    /// The system sampler, held the same way (§2.4).
    watching: Option<crate::system::Lease>,
    keys: Option<(presses::Reading, presses::Positions)>,
    /// Its last frame rendered: what its lights show while it fails for a
    /// moment, as a whole device keeps its last frame.
    last: Option<Vec<u8>>,
}

/// What one frame did to a part.
enum Outcome {
    Rendered,
    Failed,
    /// Failed [`MAX_CONSECUTIVE_ERRORS`] times in a row: it stops, with this
    /// error.
    Stopped(String),
}

impl Part {
    /// Loads `js` for the part `shared` describes, on its layout, under
    /// [`LOAD_BUDGET`].
    fn load(
        shared: Arc<PartShared>,
        layout: &'static Layout,
        owns: Option<Vec<u16>>,
        js: &str,
        inputs: &Inputs,
    ) -> Result<Self, String> {
        let effect = shared.effect_id().unwrap_or_default();
        let span = match (shared.region, shared.layer.as_deref()) {
            (Some(region), None) => tracing::info_span!("region", region, effect = %effect),
            (None, Some(layer)) => tracing::info_span!("layer", layer, effect = %effect),
            (Some(region), Some(layer)) => {
                tracing::info_span!("layer", region, layer, effect = %effect)
            }
            (None, None) => tracing::Span::none(),
        };

        // The budget is born here and never leaves the thread: the interrupt
        // handler crosses no boundary, and the deadline is written only by this
        // loop, just before each run of effect code.
        let budget = Rc::new(Budget::default());
        // Loading gets its own: the module body runs once, before the first
        // frame, hence outside any frame budget.
        budget.grant(LOAD_BUDGET);
        // The runtime must live as long as the context: it carries the module
        // resolver. Dropping it would make every `import` unresolvable on the
        // first frame.
        let (rt, ctx) = prepare_budgeted(js, layout, &budget).map_err(|e| {
            name_the_cause(
                e,
                &budget,
                "loading the effect",
                &format!("{} s", LOAD_BUDGET.as_secs()),
            )
        })?;

        // Presses are read only for an effect that declares them, and for as
        // long as this part runs it: the guard ends with it
        // (`docs/design/key-input.md` §3). A device's loop reads its own
        // keyboard; the preview draws a layout, not a device, and reads them
        // all.
        //
        // `started` first: a press that arrives once the guard exists is never
        // older than the effect's clock.
        let started = Instant::now();
        let declares = |name: &str| {
            ctx.with(|ctx| ctx.globals().get::<_, bool>(name))
                .unwrap_or(false)
        };
        let reads_keys = declares("__candeo_reads_keys");
        // The bag of signals, for an effect declaring it (§2.3.1). Bound values
        // need no declaration: they reach any effect through its parameters.
        let reads_signals = declares("__candeo_reads_signals");
        // The sound playing, for an effect declaring it: the capture runs while
        // at least one reader holds it (§2.2).
        let reads_audio = declares("__candeo_reads_audio");
        // What the computer is doing, for an effect declaring it: sampled while
        // at least one reader holds it, and held the same way (§2.4).
        let reads_system = declares("__candeo_reads_system");
        shared
            .reads_private
            .store(reads_keys || reads_signals, Ordering::Relaxed);
        let keys = reads_keys.then(|| (inputs.presses.read(), presses::positions(layout)));
        Ok(Self {
            shared,
            owns,
            span,
            budget,
            ctx,
            _rt: rt,
            started,
            frame_index: 0,
            consecutive_errors: 0,
            reads_signals,
            reads_audio,
            reads_system,
            hearing: reads_audio.then(|| inputs.sound.lease()),
            watching: reads_system.then(|| inputs.system.lease()),
            keys,
            last: None,
        })
    }

    /// One frame of its effect, counted: three outcomes, and the transitions
    /// logged once, not once per frame.
    fn frame(
        &mut self,
        inputs: &Inputs,
        keyboard: Option<(u16, u16)>,
        frame_len: usize,
    ) -> Outcome {
        let span = self.span.clone();
        let _entered = span.enter();
        match self.render(inputs, keyboard, frame_len) {
            Ok(()) => {
                self.consecutive_errors = 0;
                // The effect recovered: clear the error, otherwise the
                // interface would show a stale one indefinitely.
                let before = self.shared.error.lock().unwrap().take();
                if journal::transition(before.as_deref(), None) == journal::Transition::Recovered {
                    tracing::info!("effect recovered");
                }
                Outcome::Rendered
            }
            Err(e) => {
                // Exceeding a limit opens no new path: it is a frame error like
                // any other, which the counter below eventually turns into a
                // clean stop.
                self.consecutive_errors += 1;
                // **One line when the failure starts, not one per frame.** At 30
                // frames per second, logging every failure would produce thirty
                // lines per second and bury the one that names the cause.
                // [`journal::transition`] holds the rule.
                let before = self.shared.error.lock().unwrap().replace(e.clone());
                if journal::transition(before.as_deref(), Some(&e)) == journal::Transition::Started
                {
                    tracing::warn!(
                        "effect started failing: {}",
                        loggable(&e, self.shared.private())
                    );
                }
                if self.consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                    self.shared.stopped.store(true, Ordering::Relaxed);
                    Outcome::Stopped(e)
                } else {
                    Outcome::Failed
                }
            }
        }
    }

    /// Renders one frame on its layout, which it keeps as its last.
    fn render(
        &mut self,
        inputs: &Inputs,
        keyboard: Option<(u16, u16)>,
        frame_len: usize,
    ) -> Result<(), String> {
        let params = self.shared.params.lock().unwrap().clone();
        let bindings = self.shared.bindings.lock().unwrap().clone();
        let now = Instant::now();
        let (time, clock_ms) = inputs.clock.at(self.started, now, self.frame_index);
        // The store is touched only when something reads it: most effects bind
        // nothing and declare nothing.
        let (mut bound, held) = if bindings.is_empty() && !self.reads_signals {
            (String::new(), String::new())
        } else {
            inputs.signals.lock().unwrap().frame_inputs(
                &bindings,
                self.reads_signals,
                clock_ms as i64,
            )
        };
        let follows_sound = bindings
            .values()
            .any(|s| crate::audio::analysis::Source::parse(s).is_some());
        // Held while the effect declares it or a setting follows the sound; the
        // bindings change while the loop runs, so it is taken and let go per
        // frame.
        match (self.reads_audio || follows_sound, self.hearing.is_some()) {
            (true, false) => self.hearing = Some(inputs.sound.lease()),
            (false, true) => self.hearing = None,
            _ => {}
        }
        if follows_sound {
            bound = with_levels(&bound, sound_bound(&bindings, &inputs.sound.frame()));
        }
        let follows_system = bindings
            .values()
            .any(|s| crate::system::Source::parse(s).is_some());
        match (self.reads_system || follows_system, self.watching.is_some()) {
            (true, false) => self.watching = Some(inputs.system.lease()),
            (false, true) => self.watching = None,
            _ => {}
        }
        if follows_system {
            bound = with_levels(&bound, system_bound(&bindings, &inputs.system));
        }
        let heard = if self.reads_audio {
            inputs.sound.latest()
        } else {
            String::new()
        };
        let busy = if self.reads_system {
            inputs.system.to_json()
        } else {
            String::new()
        };
        let pressed = match &self.keys {
            Some((reading, positions)) => {
                positions.to_json(&reading.since(self.started, now, keyboard), now, time)
            }
            None => String::new(),
        };

        // The deadline is renewed before **every** frame: that is the whole
        // point of the shared cell. The handler, for its part, was set once and
        // for all on the `Runtime`.
        self.budget.grant(FRAME_BUDGET);

        let rendered = render_with(
            &self.ctx,
            time,
            self.frame_index,
            &FrameInputs {
                params: &params,
                presses: &pressed,
                clock_ms,
                bound: &bound,
                signals: &held,
                audio: &heard,
                system: &busy,
            },
            frame_len,
        );
        self.frame_index = self.frame_index.wrapping_add(1);
        match rendered {
            Ok(bytes) => {
                self.last = Some(bytes);
                Ok(())
            }
            Err(e) => Err(name_the_cause(
                e,
                &self.budget,
                "the effect",
                &format!("{} ms per frame", FRAME_BUDGET.as_millis()),
            )),
        }
    }
}

/// The factor a follow gives this frame, from its floor to 1 (§2.2.2): the
/// sound, the computer, or a signal. A value that cannot be read leaves the
/// brightness the slider sets.
fn follow_factor(d: &dimming::Dimming, inputs: &Inputs, clock_ms: f64) -> f32 {
    if d.sound().is_some() {
        d.sound_factor(&inputs.sound.frame())
    } else if let Some(source) = d.system() {
        d.level_factor(inputs.system.level(source))
    } else {
        let store = inputs.signals.lock().unwrap();
        d.signal_factor(
            d.signal()
                .and_then(|name| store.value(name, clock_ms as i64)),
        )
    }
}

/// Takes `lease` while `wanted`, and lets it go once not.
fn hold<T>(lease: &mut Option<T>, wanted: bool, take: impl FnOnce() -> T) {
    match (wanted, lease.is_some()) {
        (true, false) => *lease = Some(take()),
        (false, true) => *lease = None,
        _ => {}
    }
}

/// What a device's loop draws, in the order it composes it
/// (`docs/design/composition.md` §3.2): one stack per target, each its base,
/// then its layers, bottom to top.
struct Composition {
    stacks: Vec<Stack>,
}

/// One target's stack, as its thread runs it.
struct Stack {
    /// The region it runs on, `None` for the whole device.
    region: Option<&'static Region>,
    base: Option<Slot>,
    layers: Vec<Slot>,
}

/// A place in a stack, and what runs there.
struct Slot {
    shared: Arc<PartShared>,
    /// `None` once its effect stopped on its own, or for a layer that did not
    /// load: its context and what it held are gone, its place and its error
    /// stay, for its row to say why.
    part: Option<Part>,
}

impl Slot {
    fn running(part: Part) -> Self {
        Self {
            shared: Arc::clone(&part.shared),
            part: Some(part),
        }
    }

    /// A layer that did not load, kept with why.
    fn failed(shared: Arc<PartShared>, error: String) -> Self {
        shared.stopped.store(true, Ordering::Relaxed);
        *shared.error.lock().unwrap() = Some(error);
        Self { shared, part: None }
    }
}

impl Composition {
    fn slots(&self) -> impl Iterator<Item = &Slot> {
        self.stacks
            .iter()
            .flat_map(|s| s.base.iter().chain(s.layers.iter()))
    }

    fn slots_mut(&mut self) -> impl Iterator<Item = &mut Slot> {
        self.stacks
            .iter_mut()
            .flat_map(|s| s.base.iter_mut().chain(s.layers.iter_mut()))
    }

    /// How many parts still run.
    fn running(&self) -> usize {
        self.slots().filter(|s| s.part.is_some()).count()
    }

    fn stack(&mut self, region: Option<&str>) -> Option<&mut Stack> {
        self.stacks
            .iter_mut()
            .find(|s| s.region.map(|r| r.id) == region)
    }

    /// The parts as the commands and the status reach them, in this order.
    fn publish(&self, shared: &Shared) {
        *shared.parts.lock().unwrap() = self.slots().map(|s| Arc::clone(&s.shared)).collect();
    }
}

impl Stack {
    /// Loads `load` for its thread, and says how it went. `named` is what its
    /// parts answer to: the region, except in the preview, which is named by
    /// none.
    fn load(
        load: StackLoad,
        named: Option<&'static str>,
        device: &'static Layout,
        inputs: &Inputs,
    ) -> (Self, StackVerdict) {
        let mut stack = Self {
            region: load.region,
            base: None,
            layers: Vec::new(),
        };
        let mut verdict = StackVerdict {
            base: Ok(()),
            layers: Vec::new(),
        };
        if let Some(start) = load.base {
            let (layout, owns) = target_of(device, load.region);
            let shared = Arc::new(PartShared::base(named, &start));
            match Part::load(shared, layout, owns, &start.js, inputs) {
                Ok(part) => stack.base = Some(Slot::running(part)),
                // Nothing runs on a target whose base did not load.
                Err(e) => return (stack, StackVerdict::failed(e)),
            }
        }
        for (id, _, layer) in load.layers {
            // A mask lighting nothing here is the settings' to say, from the
            // verdict: the layer takes no place.
            let layer = match layer {
                Ok(layer) => layer,
                Err(refused) => {
                    verdict.layers.push((id, Err(PartError::Refused(refused))));
                    continue;
                }
            };
            let part_shared = Arc::new(PartShared::new(
                named,
                Some(id.clone()),
                layer.look,
                &layer.start,
            ));
            let loaded = Part::load(
                Arc::clone(&part_shared),
                layer.layout,
                Some(layer.owns),
                &layer.start.js,
                inputs,
            );
            match loaded {
                Ok(part) => {
                    stack.layers.push(Slot::running(part));
                    verdict.layers.push((id, Ok(())));
                }
                // An effect that does not load keeps its place, for its row
                // to say why.
                Err(e) => {
                    stack.layers.push(Slot::failed(part_shared, e.clone()));
                    verdict.layers.push((id, Err(PartError::Failed(e))));
                }
            }
        }
        (stack, verdict)
    }
}

/// Serves a command's request between two frames. A part replaced goes with
/// its context, on this thread; one that does not load leaves what ran.
fn serve(
    request: Request,
    composition: &mut Composition,
    shared: &Shared,
    device: &'static Layout,
    inputs: &Inputs,
) {
    match request {
        Request::Base {
            region,
            start,
            verdict,
        } => {
            let result = (|| {
                let stack = composition
                    .stack(region.map(|r| r.id))
                    .ok_or(PartError::NotRunning)?;
                if stack.base.is_none() {
                    composition::within_limits(true, stack.layers.len())
                        .map_err(PartError::Refused)?;
                }
                let (layout, owns) = target_of(device, region);
                let part_shared = Arc::new(PartShared::base(region.map(|r| r.id), &start));
                let part = Part::load(part_shared, layout, owns, &start.js, inputs)
                    .map_err(PartError::Failed)?;
                stack.base = Some(Slot::running(part));
                Ok(())
            })();
            if result.is_ok() {
                composition.publish(shared);
            }
            let _ = verdict.send(result);
        }
        Request::Layer {
            region,
            layer,
            verdict,
        } => {
            let result = (|| {
                let stack = composition.stack(region).ok_or(PartError::NotRunning)?;
                let at = stack
                    .layers
                    .iter()
                    .position(|s| s.shared.layer.as_deref() == Some(layer.id.as_str()));
                if at.is_none() {
                    composition::within_limits(stack.base.is_some(), stack.layers.len() + 1)
                        .map_err(PartError::Refused)?;
                }
                let part_shared = PartShared::new(region, Some(layer.id), layer.look, &layer.start);
                let part = Part::load(
                    Arc::new(part_shared),
                    layer.layout,
                    Some(layer.owns),
                    &layer.start.js,
                    inputs,
                )
                .map_err(PartError::Failed)?;
                match at {
                    Some(at) => stack.layers[at] = Slot::running(part),
                    None => stack.layers.push(Slot::running(part)),
                }
                Ok(())
            })();
            if result.is_ok() {
                composition.publish(shared);
            }
            let _ = verdict.send(result);
        }
        Request::Stop {
            region,
            layer,
            done,
        } => {
            if let Some(stack) = composition.stack(region) {
                match &layer {
                    None => stack.base = None,
                    Some(id) => stack
                        .layers
                        .retain(|s| s.shared.layer.as_deref() != Some(id.as_str())),
                }
            }
            composition.publish(shared);
            let _ = done.send(());
        }
        Request::Order {
            region,
            ids,
            verdict,
        } => {
            let result = (|| {
                let stack = composition.stack(region).ok_or(PartError::NotRunning)?;
                let place = |slot: &Slot| {
                    ids.iter()
                        .position(|id| slot.shared.layer.as_deref() == Some(id.as_str()))
                };
                let each_once = ids.len() == stack.layers.len()
                    && stack.layers.iter().all(|s| place(s).is_some())
                    && ids.iter().enumerate().all(|(n, id)| !ids[..n].contains(id));
                if !each_once {
                    return Err(PartError::Refused(Refusal::Order));
                }
                stack.layers.sort_by_key(place);
                Ok(())
            })();
            if result.is_ok() {
                composition.publish(shared);
            }
            let _ = verdict.send(result);
        }
    }
}

/// What a starting thread answers: each stack's verdict, and whether the loop
/// runs — a whole device whose base did not load, or left with no part, does
/// not.
struct Loaded {
    verdicts: Vec<StackVerdict>,
    runs: bool,
}

/// Loads the stacks, then runs until stopped.
///
/// A loop on a split device goes on with no part left once its last region is
/// stopped, writing black: a family's pace drops a frame sent too soon after the
/// last (`Template::frame` in `definition.rs`), so a single black frame on the
/// way out would be lost three times out of four on the Alienware zones. An
/// unchanged frame costs a comparison there. A whole device's loop does the
/// same once its base and layers are stopped one by one.
#[allow(clippy::too_many_arguments)]
fn render_loop(
    target: Target,
    shared: Arc<Shared>,
    layout: &'static Layout,
    out: Box<dyn DeviceOut>,
    inputs: Inputs,
    stacks: Vec<StackLoad>,
    follows: Follows,
    ready: Sender<Loaded>,
    requests: Receiver<Request>,
) {
    let whole = shared.regions.is_none();
    let effect_id = match stacks.as_slice() {
        [only] => only
            .base
            .as_ref()
            .map_or_else(String::new, |s| s.effect_id.clone()),
        _ => String::new(),
    };
    // **The span, and it is the reason `tracing` was chosen.** There is one
    // loop per device: "write refused" is useless without knowing which one.
    // Opened here, it carries the device and the effect until the thread ends,
    // and everything logged below — including in [`emit`] — carries them too,
    // without a single call having to pass them. On a split device each part's
    // own span names its region and effect.
    //
    // Two names, not a field to read: in a log read afterwards, "render" and
    // "preview" must be told apart at a glance — an effect error in one did not
    // turn off the keyboard, in the other it did.
    let span = match (target, &follows) {
        (Target::Device(d), _) if whole => {
            tracing::info_span!("render", device = %d, effect = %effect_id)
        }
        (Target::Device(d), _) => tracing::info_span!("render", device = %d),
        (Target::Preview(d), Follows::Whole(Some(region))) => {
            tracing::info_span!("preview", layout = %d, region = %region, effect = %effect_id)
        }
        (Target::Preview(d), _) => {
            tracing::info_span!("preview", layout = %d, effect = %effect_id)
        }
    };
    let _entered = span.enter();

    let frame_len = layout.led_count();
    let mut composition = Composition { stacks: Vec::new() };
    let mut verdicts = Vec::new();
    for load in stacks {
        let named = match target {
            Target::Device(_) => load.region.map(|r| r.id),
            Target::Preview(_) => None,
        };
        let (stack, verdict) = Stack::load(load, named, layout, &inputs);
        composition.stacks.push(stack);
        verdicts.push(verdict);
    }
    composition.publish(&shared);
    // A whole device whose base does not load runs nothing — its stack holds
    // no part then — and the command says why.
    let runs = !whole || composition.running() > 0;
    let _ = ready.send(Loaded { verdicts, runs });
    if !runs {
        return;
    }

    let keyboard = match target {
        Target::Device(d) => Some((d.vid, d.pid)),
        Target::Preview(_) => None,
    };
    // The preview borrows the device's brightness with its layout: it shows
    // what applying the effect there would give (§2.2.2).
    let (Target::Device(device) | Target::Preview(device)) = target;

    // A firmware that animates goes on redrawing over every frame until it is
    // told to stop: the Alienware keyboard needed *Off* in between otherwise.
    out.take_over();

    let period = Duration::from_nanos(1_000_000_000 / u64::from(FPS));
    let mut deadline = Instant::now();
    // What the follows read, held by the loop rather than by a part: they
    // belong to the device, its regions and its layers' opacity, not to an
    // effect.
    let mut hearing = None;
    let mut watching = None;
    // The loop's own frames, which the follows read the clock at.
    let mut frame: u32 = 0;

    while !shared.stop.load(Ordering::Relaxed) {
        for request in requests.try_iter() {
            serve(request, &mut composition, &shared, layout, &inputs);
        }

        let mut rendered = false;
        let mut stopped: Vec<(Part, String)> = Vec::new();
        for slot in composition.slots_mut() {
            let Some(part) = slot.part.as_mut() else {
                continue;
            };
            match part.frame(&inputs, keyboard, frame_len) {
                Outcome::Rendered => rendered = true,
                Outcome::Failed => {}
                // Its context and what it held go with it; its place and its
                // status stay, with the error that stopped it.
                Outcome::Stopped(e) => stopped.extend(slot.part.take().map(|p| (p, e))),
            }
        }
        if !stopped.is_empty() {
            let left = composition.running();
            for (part, e) in stopped {
                let _in = part.span.enter();
                let error = loggable(&e, part.shared.private());
                if left == 0 {
                    tracing::error!(
                        failures = MAX_CONSECUTIVE_ERRORS,
                        "effect stopped after {MAX_CONSECUTIVE_ERRORS} consecutive failures, \
                         backlight turned off: {error}"
                    );
                } else if part.shared.layer.is_some() {
                    tracing::error!(
                        failures = MAX_CONSECUTIVE_ERRORS,
                        "layer stopped after {MAX_CONSECUTIVE_ERRORS} consecutive failures, \
                         the rest of its stack going on: {error}"
                    );
                } else {
                    tracing::error!(
                        failures = MAX_CONSECUTIVE_ERRORS,
                        "effect stopped after {MAX_CONSECUTIVE_ERRORS} consecutive failures, \
                         its lights black: {error}"
                    );
                }
            }
            // The device is turned off only when no part runs any more: a
            // frozen last frame reads as an effect still running; off says
            // nothing runs, and the gallery says why (#48).
            if left == 0 {
                shared.stop.store(true, Ordering::Relaxed);
                out.turn_off();
                break;
            }
        }

        // A whole device whose parts all failed this frame keeps its last
        // frame, as before regions; a split one goes on composing, so that a
        // region stopping goes black whatever the others do.
        if rendered || !whole {
            let dims: Vec<(Option<&'static [u16]>, Option<dimming::Dimming>)> = {
                let table = inputs.dimming.lock().unwrap();
                follows
                    .scopes()
                    .into_iter()
                    .map(|(region, positions)| {
                        let key = (device, region.map(str::to_owned));
                        (positions, table.get(&key).cloned())
                    })
                    .collect()
            };
            // Each running part's look, read once this frame.
            let looks: Vec<Option<Look>> = composition
                .slots()
                .map(|s| s.part.as_ref().map(|_| s.shared.look()))
                .collect();
            let by = |what: fn(&dimming::Dimming) -> bool| {
                dims.iter().any(|(_, d)| d.as_ref().is_some_and(what))
                    || looks
                        .iter()
                        .flatten()
                        .any(|l| l.follows.as_ref().is_some_and(what))
            };
            hold(&mut hearing, by(|d| d.sound().is_some()), || {
                inputs.sound.lease()
            });
            hold(&mut watching, by(|d| d.system().is_some()), || {
                inputs.system.lease()
            });
            let clock_ms = inputs.clock.wall_ms(frame);
            let drawn = composition.slots().zip(&looks).filter_map(|(slot, look)| {
                let (part, look) = (slot.part.as_ref()?, look.as_ref()?);
                let factor = look
                    .follows
                    .as_ref()
                    .map_or(1.0, |d| follow_factor(d, &inputs, clock_ms));
                Some(Drawn {
                    owns: part.owns.as_deref(),
                    bytes: part.last.as_deref(),
                    blend: look.blend,
                    alpha: look.alpha(factor),
                })
            });
            let mut bytes = composition::compose(frame_len * 3, drawn);
            for (positions, d) in &dims {
                let Some(d) = d else { continue };
                let factor = follow_factor(d, &inputs, clock_ms);
                match positions {
                    None => dimming::dim(&mut bytes, factor),
                    Some(positions) => dimming::dim_positions(&mut bytes, positions, factor),
                }
            }
            emit(&shared, out.as_ref(), &bytes);
        }
        frame = frame.wrapping_add(1);

        // Absolute deadline rather than `sleep(period)`: a slow frame must not
        // shift all the following ones. If we fell behind, start again from now
        // instead of trying to catch up at high speed.
        deadline += period;
        let now = Instant::now();
        if deadline > now {
            std::thread::sleep(deadline - now);
        } else {
            deadline = now;
        }
    }
}

/// The deadline the interrupt handler checks — and what it did about it.
///
/// A shared cell, not a captured deadline: the handler is set on the `Runtime`
/// **once**, whereas the deadline changes on **every frame**.
/// [`prepare_bounded`], which grants only one for a whole installation, can
/// make do with capturing it; the render loop cannot.
///
/// The flag is there to **name the cause**. QuickJS throws the same
/// "InternalError: interrupted" whatever the reason for an interruption, and it
/// is the handler — and only the handler — that knows the deadline made it
/// throw.
#[derive(Default)]
struct Budget {
    deadline: Cell<Option<Instant>>,
    exceeded: Cell<bool>,
}

impl Budget {
    /// Grants `duration` to the next run, with the flag lowered.
    fn grant(&self, duration: Duration) {
        self.deadline.set(Some(Instant::now() + duration));
        self.exceeded.set(false);
    }

    /// The handler itself: `true` cuts the current run.
    ///
    /// QuickJS calls it every 10,000 instructions — an `Instant::now()` at that
    /// frequency is not measurable.
    fn expire(&self) -> bool {
        match self.deadline.get() {
            Some(end) if Instant::now() >= end => {
                self.exceeded.set(true);
                true
            }
            _ => false,
        }
    }
}

/// Loads an effect once and returns the manifest its module declares, as JSON.
///
/// Bounded like swatch sampling: this is effect code being run so the library
/// can list it, and a module that loops at the top level must fail here with an
/// error, not freeze the command that records it.
pub(crate) fn declared_manifest(js: &str) -> Result<String, String> {
    let deadline = Instant::now() + swatch::BUDGET;
    let (_rt, ctx) = prepare_bounded(js, crate::default_layout(), Some(deadline))?;
    ctx.with(|ctx| ctx.globals().get::<_, String>("__candeo_manifest"))
        .map_err(|e| format!("unreadable manifest: {e}"))
}

/// JavaScript context ready to render, **with no time limit**.
///
/// Reserved for tests. Since the render loop bounds every frame and swatch
/// sampling bounds its installation, no production caller prepares a context
/// that a `while (true)` could freeze any more — that is the whole point of the
/// two budgets. What remains are the tests that are not about the limits, to
/// which a deadline would only add machine-dependent randomness.
///
/// The `Runtime` is returned with the context, not kept here: it carries the
/// module resolver, so it must live just as long.
#[cfg(test)]
fn prepare(js: &str, layout: &'static Layout) -> Result<(Runtime, Context), String> {
    prepare_bounded(js, layout, None)
}

/// Like [`prepare_with`], with a **single** deadline, captured by value.
///
/// It covers everything the caller will do with the context, from loading to
/// the last frame. That is what swatch sampling needs — a few frames, a single
/// limit, on a command's thread where a `while (true)` would prevent an
/// installation from completing. The render loop, for its part, changes it on
/// every frame: see [`Budget`].
///
/// The deadline is set **before** any evaluation, to cover the module body as
/// well.
fn prepare_bounded(
    js: &str,
    layout: &'static Layout,
    deadline: Option<Instant>,
) -> Result<(Runtime, Context), String> {
    let interrupt =
        deadline.map(|end| -> InterruptHandler { Box::new(move || Instant::now() >= end) });
    prepare_with(js, layout, interrupt)
}

/// Like [`prepare_with`], but bounded **call by call**: the handler reads a
/// budget that the caller renews before each run.
fn prepare_budgeted(
    js: &str,
    layout: &'static Layout,
    budget: &Rc<Budget>,
) -> Result<(Runtime, Context), String> {
    let budget = Rc::clone(budget);
    prepare_with(js, layout, Some(Box::new(move || budget.expire())))
}

/// The common trunk, for a layout from this repository: it serializes, then
/// delegates.
fn prepare_with(
    js: &str,
    layout: &'static Layout,
    interrupt: Option<InterruptHandler>,
) -> Result<(Runtime, Context), String> {
    prepare_with_layout(js, layout.led_count(), layout_json(layout), interrupt)
}

/// The trunk: limits set, modules resolved, `__candeo_render` installed, for an
/// **already serialized** layout.
///
/// Both limits are set **before** any evaluation — the module body is effect
/// code like any other.
///
/// The layout arrives as JSON rather than as a [`Layout`] for a reason that only
/// shows in the tests: `candeo_device::Key` carries a **mandatory** rectangle,
/// so no layout in this repository can describe a device without a surveyed
/// geometry — and yet that is the case an effect measuring physical distances
/// must refuse, saying so. Writing it by hand is the only way to check that
/// refusal until #34 makes the rectangle optional.
fn prepare_with_layout(
    js: &str,
    frame_len: usize,
    layout_json: String,
    interrupt: Option<InterruptHandler>,
) -> Result<(Runtime, Context), String> {
    let rt = Runtime::new().map_err(|e| format!("QuickJS : {e}"))?;

    rt.set_interrupt_handler(interrupt);

    // The memory limit, for its part, is not renewed and applies to every
    // caller: an endless allocation takes down the whole process, whether it
    // happens in a render loop or during an effect installation.
    rt.set_memory_limit(MEMORY_BUDGET);

    // `@candeo/effects-api` is **built in**. That is what allows writing a
    // normal `import` without a bundler, without path resolution and without
    // `node_modules`.
    let resolver = BuiltinResolver::default()
        .with_module("@candeo/effects-api")
        .with_module("effect");
    let loader = BuiltinLoader::default()
        .with_module("@candeo/effects-api", API_JS)
        .with_module("effect", js);
    rt.set_loader(resolver, loader);

    let ctx = Context::full(&rt).map_err(|e| format!("QuickJS : {e}"))?;

    ctx.with(|ctx| -> Result<(), String> {
        let g = ctx.globals();
        g.set("__candeo_frame_len", frame_len as u32)
            .map_err(js_error)?;
        g.set("__candeo_layout", layout_json).map_err(js_error)?;

        Module::evaluate(ctx.clone(), "bootstrap", BOOTSTRAP_JS)
            .catch(&ctx)
            .map_err(|e| format!("loading the effect: {e}"))?
            .finish::<()>()
            .catch(&ctx)
            .map_err(|e| format!("loading the effect: {e}"))?;
        Ok(())
    })?;

    Ok((rt, ctx))
}

/// The wall clock, in milliseconds since the epoch.
///
/// Read here rather than in the effect so that both loops see the same instant
/// and a test can hand one in. A clock set before 1970 gives zero, which the
/// bootstrap already means as "no clock": nothing to draw is better than a
/// negative date.
///
/// `f64` is what `Date` takes, and it counts milliseconds exactly for another
/// 280 000 years.
fn wall_clock_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |since| since.as_millis() as f64)
}

/// One frame with neutral inputs: swatches, tests, and effects reading nothing.
///
/// A swatch is sampled this way on purpose — no presses and a zeroed clock — so
/// it shows what the effect draws at rest (`docs/design/inputs-and-automations.md`
/// §1).
fn render_once(
    ctx: &Context,
    time: f64,
    frame_index: u32,
    params: &str,
    frame_len: usize,
) -> Result<Vec<u8>, String> {
    render_with_inputs(ctx, time, frame_index, params, "", 0.0, frame_len)
}

/// One frame, with `presses` as `presses::to_json` writes them (empty: none) and
/// `clock_ms` the wall clock in milliseconds since the epoch.
///
/// The host reads the clock rather than the effect: a test then renders any
/// instant, and the device loop and the preview see the same one.
fn render_with_inputs(
    ctx: &Context,
    time: f64,
    frame_index: u32,
    params: &str,
    presses: &str,
    clock_ms: f64,
    frame_len: usize,
) -> Result<Vec<u8>, String> {
    render_with(
        ctx,
        time,
        frame_index,
        &FrameInputs {
            params,
            presses,
            clock_ms,
            bound: "",
            signals: "",
            audio: "",
            system: "",
        },
        frame_len,
    )
}

/// What one frame hands the effect besides the time, each as the bootstrap
/// reads it: JSON, or an empty string for none.
struct FrameInputs<'a> {
    params: &'a str,
    presses: &'a str,
    clock_ms: f64,
    /// Raw values bound to parameters, by parameter: converted by the bootstrap
    /// against each parameter's spec.
    bound: &'a str,
    /// Every signal held, for an effect declaring the `signals` input.
    signals: &'a str,
    /// The latest analysis of the sound playing, for an effect declaring `audio`.
    audio: &'a str,
    /// What the computer is doing now, for an effect declaring `system`.
    system: &'a str,
}

/// What a loop reads besides its parameters: key presses and signals, shared
/// with the rest of the application.
struct Inputs {
    presses: Arc<Presses>,
    signals: crate::signals::SharedStore,
    sound: Arc<crate::audio::Sound>,
    system: Arc<crate::system::System>,
    dimming: dimming::Table,
    clock: Clock,
}

fn render_with(
    ctx: &Context,
    time: f64,
    frame_index: u32,
    inputs: &FrameInputs,
    frame_len: usize,
) -> Result<Vec<u8>, String> {
    ctx.with(|ctx| {
        let render: Function = ctx
            .globals()
            .get("__candeo_render")
            .map_err(|_| "the render function is gone from the context".to_string())?;

        // A list rather than a tuple: rquickjs takes tuples of seven at most,
        // and the sound made eight (#107), the system nine (§2.4).
        let mut args = Args::new(ctx.clone(), 9);
        let pushed = (|| {
            args.push_arg(time)?;
            args.push_arg(frame_index)?;
            args.push_arg(inputs.params)?;
            args.push_arg(inputs.presses)?;
            args.push_arg(inputs.clock_ms)?;
            args.push_arg(inputs.bound)?;
            args.push_arg(inputs.signals)?;
            args.push_arg(inputs.audio)?;
            args.push_arg(inputs.system)
        })();
        pushed.map_err(|e| format!("the frame's inputs could not be handed over: {e}"))?;
        let out: Vec<u8> = render
            .call_arg(args)
            .catch(&ctx)
            .map_err(|e| format!("{e}"))?;

        if out.len() != frame_len * 3 {
            return Err(format!(
                "the effect rendered {} bytes, {} expected",
                out.len(),
                frame_len * 3
            ));
        }
        Ok(out)
    })
}

/// The text QuickJS throws when the memory limit is reached, and the only sign
/// it gives of it (`JS_ThrowOutOfMemory`).
const QUICKJS_OOM: &str = "out of memory";

/// Turns a failure of a bounded run into a cause the effect author can relate
/// to **their** code.
///
/// QuickJS names neither limit: an interruption surfaces as
/// "InternalError: interrupted", which points at nothing, and a memory overrun
/// as "out of memory", which says neither whose nor by how much. Both limits
/// are set here; so it is here, and nowhere else, that they can be explained.
///
/// Time is read from the budget flag, never from the message: the handler did
/// the cutting, and it alone knows so for sure. Memory has only QuickJS's text
/// — hence the comparison, and the fallback to the raw error when it says
/// nothing: misnaming a cause would be worse than not naming it.
///
/// `subject` tells the two bounded places apart — a frame, a load. The loop that
/// never ends is not at the same place in the file, and the time `granted` is not
/// the same.
///
/// English, like the TypeScript diagnostics next to it: the reader is the effect's
/// author.
fn name_the_cause(error: String, budget: &Budget, subject: &str, granted: &str) -> String {
    if budget.exceeded.get() {
        return format!(
            "{subject} exceeded its computing time ({granted}): \
             a loop that never ends, or a computation too heavy."
        );
    }
    if error.contains(QUICKJS_OOM) {
        return format!(
            "{subject} exceeded the memory granted to it ({} MB): \
             a state that grows on every frame, or an oversized allocation.",
            MEMORY_BUDGET / (1024 * 1024)
        );
    }
    error
}

/// The two outputs, independent: either one may be absent.
fn emit(shared: &Shared, out: &dyn DeviceOut, bytes: &[u8]) {
    if !shared.to_keyboard.load(Ordering::Relaxed) {
        // Output deliberately turned off: this is not a fault, but the frames
        // reach no keyboard and the interface must be able to say so.
        shared.reaching.store(false, Ordering::Relaxed);
    } else {
        let colors: Vec<Rgb> = bytes
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&[r, g, b]| Rgb::new(r, g, b))
            .collect();
        // A keyboard unplugged midway does not stop the effect: the simulator
        // carries on, and reconnecting goes through the existing commands. But
        // the failure is **recorded**, not swallowed — hiding it gave a loop
        // that claims to be healthy while not a single byte reaches the device.
        // And this device's failure says nothing about the others: each loop
        // writes into its own state.
        let failures = shared.device_failures.load(Ordering::Relaxed);
        let abandon = failures + 1 >= MAX_DEVICE_WRITE_ERRORS;
        match out.present(&colors, abandon) {
            // **No device open.** Without this report, starting an effect with
            // no keyboard connected produced no sign at all: the simulator
            // animated, the "Send to keyboard" box stayed checked, and the
            // keyboard kept its previous frame. It read as "only the first frame
            // got through".
            // Nothing to log: writing an effect **without owning the keyboard**
            // is an intended use, not a failure. Saying so on every frame would
            // drown the log, and saying it once would pass off as an incident
            // what `reachingKeyboard` already makes visible on screen.
            None => {
                shared.device_failures.store(0, Ordering::Relaxed);
                shared.reaching.store(false, Ordering::Relaxed);
            }
            Some(Ok(())) => {
                shared.device_failures.store(0, Ordering::Relaxed);
                let before = shared.device_error.lock().unwrap().take();
                if journal::transition(before.as_ref(), None) == journal::Transition::Recovered {
                    tracing::info!("writing to the device recovered");
                }
                shared.reaching.store(true, Ordering::Relaxed);
            }
            Some(Err(e)) if abandon => {
                // The device was dropped under its own lock (see
                // [`DeviceOut::present`]). The message kept for the window says
                // what to do, a closed device is reopened by the existing
                // commands; the technical cause goes to the log only.
                shared.device_failures.store(0, Ordering::Relaxed);
                *shared.device_error.lock().unwrap() = Some(Failure::new("deviceClosed"));
                tracing::warn!(
                    "device closed after {MAX_DEVICE_WRITE_ERRORS} consecutive failed writes: {e}"
                );
                shared.reaching.store(false, Ordering::Relaxed);
            }
            Some(Err(e)) => {
                // Same rule as for the effect error: the start of the failure,
                // and nothing else. A keyboard unplugged midway would fail on
                // every frame until it is plugged back in.
                shared
                    .device_failures
                    .store(failures + 1, Ordering::Relaxed);
                let failure = Failure::new("deviceWrite").with("detail", &e);
                let before = shared.device_error.lock().unwrap().replace(failure.clone());
                if journal::transition(before.as_ref(), Some(&failure))
                    == journal::Transition::Started
                {
                    tracing::warn!("writing to the device started failing: {e}");
                }
                shared.reaching.store(false, Ordering::Relaxed);
            }
        }
    }

    // Raw binary: serializing as a JSON array of integers would take a 396-byte
    // frame to more than 1.5 KB, thirty times per second.
    if let Some(ch) = shared.frames.lock().unwrap().as_ref() {
        let _ = ch.send(InvokeResponseBody::Raw(bytes.to_vec()));
    }
}

/// The layout as the effect sees it.
///
/// Serialized by hand: `candeo-device` has no serde, and that is deliberate —
/// its tests run without any system dependency.
///
/// # Two spaces, and both travel
///
/// `row`/`col` place the LED in the matrix; `x`/`y`/`w`/`h` give the keycap
/// rectangle, in keyboard pitch units — the same unit as
/// [`candeo_device::Key`], with no conversion on the way. An effect that talks
/// about distance cannot be right without the second: a matrix cell is one
/// cell, whether the key is 1 u or 6.25 u wide.
///
/// The geometry is **always** emitted here, because `candeo_device::Key` always
/// carries it. The day the rectangle becomes optional (#34), these four fields
/// will disappear for layouts that have not been drawn, and the effects reading
/// them will fail saying so — see `bounds` and `center` in `api.js`.
fn layout_json(l: &'static Layout) -> String {
    let mut keys = String::new();
    for (row, col, k) in keys_in_order(l) {
        if !keys.is_empty() {
            keys.push(',');
        }
        let mut fields = format!(r#""index":{},"row":{row},"col":{col}"#, k.index);
        if k.scancode != candeo_device::NO_SCANCODE {
            fields.push_str(&format!(r#","scancode":{}"#, k.scancode));
        }
        if let Some(label) = crate::keys::label(k.scancode) {
            fields.push_str(&format!(r#","label":{}"#, json_string(&label)));
        }
        keys.push_str(&format!(
            r#"{{{fields},"x":{},"y":{},"w":{},"h":{}}}"#,
            k.x, k.y, k.w, k.h
        ));
    }
    format!(
        r#"{{"name":{},"rows":{},"cols":{},"keys":[{keys}]}}"#,
        json_string(l.name),
        l.rows,
        l.cols
    )
}

/// A layout's keys, row by row: the order of `layout.keys` for effects. The
/// positions presses are given as ([`presses::positions`]) depend on it.
fn keys_in_order(
    l: &'static Layout,
) -> impl Iterator<Item = (u8, u8, &'static candeo_device::Key)> {
    (0..l.rows)
        .flat_map(move |row| (0..l.cols).map(move |col| (row, col)))
        .filter_map(move |(row, col)| Some((row, col, l.key(l.at(row, col)?)?)))
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn js_error(e: rquickjs::Error) -> String {
    format!("QuickJS : {e}")
}

// ---------------------------------------------------------------- commands

use crate::{AppState, CmdResult, DeviceRef, Failure, Scope};
use tauri::{AppHandle, Manager, State};

/// Starts an effect, built-in or installed, **on a device** — or, on a split
/// device, on one of its regions, the others untouched.
///
/// The `id → JavaScript` resolution is the library's, so built-ins come first:
/// see [`crate::storage`]. The engine itself makes no difference — a shipped
/// effect is a module loaded exactly like the one just written.
///
/// The layout comes from **the targeted device**, open or not. That is
/// deliberate: one must be able to write and preview an effect **without owning
/// the keyboard**, and a device's layout does not depend on its presence.
/// Targeting an unplugged device therefore starts the effect, feeds the
/// simulator, and `reachingKeyboard` stays false until the device is opened.
///
/// # This is the gesture that **commits** the keyboard
///
/// Previewing is the other path, and it does not go through here:
/// [`start_preview`] opens no hardware output and writes nothing to disk.
/// "Apply" does both — it sends to the keyboard, and it
/// **remembers** the effect for this device, or this region.
#[tauri::command]
pub fn start_effect(
    app: AppHandle,
    device: DeviceRef,
    region: Option<String>,
    id: String,
    params: serde_json::Value,
    bindings: Option<Bindings>,
) -> CmdResult<()> {
    let bindings = checked_bindings(bindings.unwrap_or_default())?;
    let part = crate::part_of(&app, device, region.as_deref(), false)?;
    match part {
        Scope::Whole => apply(&app, device, effect_start(&app, &id, &params, bindings)?)?,
        Scope::Region(region) => run_region(&app, device, region, || {
            effect_start(&app, &id, &params, bindings.clone())
        })?,
    }
    // A gesture always wins over a rule: an effect applied during an
    // interruption ends it, and becomes the one the device goes back to.
    crate::automations::dismiss(&app, device);

    // **After** the start, never before: only what actually runs is
    // remembered. An effect whose load fails must not leave behind an id that
    // the file presents as applied.
    match part {
        Scope::Whole => remember_active_effect(&app, device, Some(&id)),
        Scope::Region(region) => remember_region_effect(&app, device, region.id, Some(&id)),
    }
    Ok(())
}

/// Starts a rule's effect on a device, alone, **without remembering it as
/// applied**: an interruption must never rewrite `activeEffects` — it would
/// take the place of what the device goes back to
/// (`docs/design/inputs-and-automations.md` §3.1). The device's layers wait
/// under it; drawing over it is an overlay rule's, which step 3 of
/// `docs/design/composition.md` §5.1 adds. On a split device it runs over all
/// of it, its regions' follows still scaling their lights
/// (`docs/design/regions.md` §4).
pub(crate) fn run_effect(
    app: &AppHandle,
    device: DeviceRef,
    id: &str,
    params: &serde_json::Value,
    bindings: Bindings,
) -> CmdResult<()> {
    let layout = crate::find_layout(device)?;
    let start = effect_start(app, id, params, bindings)?;
    let split = crate::is_split(app, device)?;

    let state = app.state::<AppState>();
    // The handle is shared with the loop, not copied: closing the device later
    // — ignored, unplugged — shows on the next frame.
    let out = Box::new(state.handle(device));
    state
        .engine
        .start(device, start, layout, out, split)
        .map_err(|error| not_started(id, error))
}

/// Starts `start` as the effect applied on a whole device: *Apply*, the tray.
///
/// While the loop runs the device's stack, only its base changes, between two
/// frames: its layers keep their clocks (`docs/design/composition.md` §3.8).
/// Otherwise — nothing runs, a firmware effect does, the device ran its regions
/// — the whole stack starts, its layers read from the file. A rule's effect
/// running there is no stack of the device's: it is replaced whole.
pub(crate) fn apply(app: &AppHandle, device: DeviceRef, start: EffectStart) -> CmdResult<()> {
    if !crate::automations::interrupts(app, device) {
        let id = start.effect_id.clone();
        match app
            .state::<AppState>()
            .engine
            .start_base(device, None, start.clone())
        {
            Ok(()) => return Ok(()),
            Err(PartError::NotRunning) => {}
            Err(PartError::Refused(refused)) => return Err(not_started(&id, refused.to_string())),
            Err(PartError::Failed(error)) => return Err(not_started(&id, error)),
        }
    }
    run_stack(app, device, Some(start))
}

/// Starts a whole device's stack — `base`, or none, and the layers kept for it
/// — replacing whatever ran there. A layer that cannot start is said in the
/// log and waits in the file; a base that does not load is the command's
/// error, and the device runs nothing, as before layers.
pub(crate) fn run_stack(
    app: &AppHandle,
    device: DeviceRef,
    base: Option<EffectStart>,
) -> CmdResult<()> {
    let layout = crate::find_layout(device)?;
    let split = crate::is_split(app, device)?;
    let settings = crate::storage::store(app)?.read_settings()?;
    let layers = crate::layers::starts(app, &settings, layout, None);
    let id = base.as_ref().map(|b| b.effect_id.clone());
    let state = app.state::<AppState>();
    let out = Box::new(state.handle(device));
    let verdict = state
        .engine
        .start_stack(device, layout, out, StackStart { base, layers }, split);
    let failed = match verdict {
        Ok(verdict) => verdict.base.err(),
        Err(refused) => Some(refused.to_string()),
    };
    match (failed, id) {
        (None, _) => Ok(()),
        (Some(error), Some(id)) => Err(not_started(&id, error)),
        (Some(error), None) => Err(Failure::unexpected(format!(
            "the layers of {device} did not start: {error}"
        ))),
    }
}

/// Starts an effect on one region of a split device, without remembering it.
///
/// The running loop takes it between two frames, the other regions going on.
/// When the loop does not run the regions — a rule interrupts the device, or
/// nothing runs — they all start, the others with their applied effects: the
/// gesture gives the device back its regions (`docs/design/regions.md` §4).
/// `start` is asked for twice then, the first having gone to the loop.
fn run_region(
    app: &AppHandle,
    device: DeviceRef,
    region: &'static Region,
    start: impl Fn() -> CmdResult<EffectStart>,
) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let first = start()?;
    let id = first.effect_id.clone();
    match state.engine.start_region(device, region.id, first) {
        Ok(()) => return Ok(()),
        Err(RegionStart::Failed(error)) => return Err(not_started(&id, error)),
        Err(RegionStart::NotRunningRegions) => {}
    }
    let layout = crate::find_layout(device)?;
    let settings = crate::storage::store(app)?.read_settings()?;
    let mut starts = saved_region_starts(app, &settings, device, layout);
    let mut this = Some(start()?);
    for (r, s) in &mut starts {
        if r.id == region.id {
            *s = this.take();
        }
    }
    let stacks = region_stacks(app, &settings, layout, starts);
    let out = Box::new(state.handle(device));
    let verdict = state
        .engine
        .start_regions(device, layout, out, stacks)
        .map_err(|refused| refused.to_string())
        .and_then(|verdicts| {
            verdicts
                .into_iter()
                .find(|(r, _)| *r == region.id)
                .map_or(Ok(()), |(_, verdict)| verdict.base)
        });
    verdict.map_err(|error| not_started(&id, error))
}

/// Starts every region of a split device with its applied effect and the
/// settings saved for it there, replacing whatever ran: on splitting it, when a
/// rule gives it back, when it opens. A region whose effect does not start runs
/// nothing; the log says why, and its record stays, as a device's does.
pub(crate) fn run_regions(app: &AppHandle, device: DeviceRef) -> CmdResult<()> {
    let layout = crate::find_layout(device)?;
    let settings = crate::storage::store(app)?.read_settings()?;
    let starts = saved_region_starts(app, &settings, device, layout);
    let stacks = region_stacks(app, &settings, layout, starts);
    let state = app.state::<AppState>();
    let out = Box::new(state.handle(device));
    // Each verdict is in the log already, under the region's name; the
    // layers kept are within every limit (`crate::layers::planned`).
    let _ = state.engine.start_regions(device, layout, out, stacks);
    Ok(())
}

/// Each region with its base and the layers kept for it.
fn region_stacks(
    app: &AppHandle,
    settings: &crate::storage::Settings,
    layout: &'static Layout,
    starts: Vec<(&'static Region, Option<EffectStart>)>,
) -> Vec<(&'static Region, StackStart)> {
    starts
        .into_iter()
        .map(|(region, base)| {
            let layers = crate::layers::starts(app, settings, layout, Some(region));
            (region, StackStart { base, layers })
        })
        .collect()
}

/// Each region of `layout` with its applied effect on `device`, ready to start
/// with the settings saved for it there, as the gallery would start it.
fn saved_region_starts(
    app: &AppHandle,
    settings: &crate::storage::Settings,
    device: DeviceRef,
    layout: &'static Layout,
) -> Vec<(&'static Region, Option<EffectStart>)> {
    let applied = |r: &Region| settings.region_effect(device.vid, device.pid, r.id);
    // Read once, and only when a region has something to start.
    let library = if layout.regions.iter().any(|r| applied(r).is_some()) {
        crate::storage::store(app).and_then(|s| s.list_effects())
    } else {
        Ok(Vec::new())
    };
    layout
        .regions
        .iter()
        .map(|r| {
            let start = applied(r).and_then(|effect| {
                let saved = library.as_ref().map_err(Clone::clone).and_then(|library| {
                    saved_start(
                        app,
                        library,
                        effect,
                        settings.region_effect_params(device.vid, device.pid, r.id, effect),
                        settings.region_effect_bindings(device.vid, device.pid, r.id, effect),
                    )
                });
                saved
                    .inspect_err(|e| {
                        tracing::warn!(device = %device, region = r.id, effect, "applied effect not started: {e}")
                    })
                    .ok()
            });
            (r, start)
        })
        .collect()
}

/// An effect from the library, ready to start: its code, its parameters.
pub(crate) fn effect_start(
    app: &AppHandle,
    id: &str,
    params: &serde_json::Value,
    bindings: Bindings,
) -> CmdResult<EffectStart> {
    let js = crate::storage::store(app)?.effect_js(&crate::storage::EffectKey::parse(id)?)?;
    Ok(EffectStart {
        effect_id: id.to_owned(),
        js,
        params: serialised(params)?,
        bindings,
    })
}

/// `effect` ready to start with `stored` over its declared defaults, as the
/// gallery starts it: see [`crate::storage::starting_params`].
fn saved_start(
    app: &AppHandle,
    library: &[crate::storage::EffectEntry],
    effect: &str,
    stored: Option<&serde_json::Map<String, serde_json::Value>>,
    bindings: Bindings,
) -> CmdResult<EffectStart> {
    let entry = library
        .iter()
        .find(|e| e.id == effect)
        .ok_or_else(|| Failure::new("effectNotFound").with("name", effect))?;
    let params = crate::storage::starting_params(&entry.manifest, stored);
    effect_start(app, effect, &serde_json::Value::Object(params), bindings)
}

/// Starts a rule's `effect` on `device` with `stored` over its declared
/// defaults, alone and without remembering it: see [`run_effect`].
pub(crate) fn run_with(
    app: &AppHandle,
    device: DeviceRef,
    effect: &str,
    stored: Option<&serde_json::Map<String, serde_json::Value>>,
    bindings: Bindings,
) -> CmdResult<()> {
    let entry = crate::storage::store(app)?
        .list_effects()?
        .into_iter()
        .find(|e| e.id == effect)
        .ok_or_else(|| Failure::new("effectNotFound").with("name", effect))?;
    let params = crate::storage::starting_params(&entry.manifest, stored);
    run_effect(
        app,
        device,
        effect,
        &serde_json::Value::Object(params),
        bindings,
    )
}

/// `effect` ready to start with `stored` over its declared defaults.
fn stored_start(
    app: &AppHandle,
    effect: &str,
    stored: Option<&serde_json::Map<String, serde_json::Value>>,
    bindings: Bindings,
) -> CmdResult<EffectStart> {
    let library = crate::storage::store(app)?.list_effects()?;
    saved_start(app, &library, effect, stored, bindings)
}

/// Gives a whole device its applied stack back, whole: the end of a rule's
/// interruption. The rule's effect was no base of the device's: the base and
/// every layer start again from the file, with their settings there.
pub(crate) fn run_applied_with(
    app: &AppHandle,
    device: DeviceRef,
    effect: &str,
    stored: Option<&serde_json::Map<String, serde_json::Value>>,
    bindings: Bindings,
) -> CmdResult<()> {
    let start = stored_start(app, effect, stored, bindings)?;
    run_stack(app, device, Some(start))
}

/// Starts `effect` on `device` with the settings saved for it on that device.
///
/// What the tray and resuming do: neither may start an effect differently from
/// the gallery, and both remember it as applied.
pub(crate) fn start_saved(app: &AppHandle, device: DeviceRef, effect: &str) -> CmdResult<()> {
    let settings = crate::storage::store(app)?.read_settings()?;
    let start = stored_start(
        app,
        effect,
        settings.effect_params(device.vid, device.pid, effect),
        settings.effect_bindings(device.vid, device.pid, effect),
    )?;
    apply(app, device, start)?;
    remember_active_effect(app, device, Some(effect));
    Ok(())
}

/// Starts `effect` on one region of a split device with the settings saved for
/// it there, and remembers it as the region's: the tray's gesture, as *Apply*
/// is the gallery's.
pub(crate) fn start_saved_region(
    app: &AppHandle,
    device: DeviceRef,
    region: &str,
    effect: &str,
) -> CmdResult<()> {
    let Scope::Region(region) = crate::part_of(app, device, Some(region), false)? else {
        return Err(Failure::unexpected(format!(
            "{device} has no region {region}"
        )));
    };
    let start = || {
        let settings = crate::storage::store(app)?.read_settings()?;
        let library = crate::storage::store(app)?.list_effects()?;
        saved_start(
            app,
            &library,
            effect,
            settings.region_effect_params(device.vid, device.pid, region.id, effect),
            settings.region_effect_bindings(device.vid, device.pid, region.id, effect),
        )
    };
    run_region(app, device, region, start)?;
    crate::automations::dismiss(app, device);
    remember_region_effect(app, device, region.id, Some(effect));
    Ok(())
}

/// Gives a device that just opened the effect applied on it (#102): at startup,
/// on adoption, and on replug. A split device gets each region's.
///
/// Nothing happens when the setting is off, when nothing is applied, or when that
/// effect already runs on the device — a replug finds the loop still writing to
/// the reopened handle.
///
/// An effect that cannot start keeps its record: deleted, it is the gallery's
/// "no longer in the folder" notice; edited outside Candeo, it waits for the
/// window to have it compiled, and [`resume_waiting`] tries again then.
pub(crate) fn resume_applied(app: &AppHandle, device: DeviceRef) {
    // An interruption under way keeps the device: the rule gives the applied
    // effect back itself when it ends.
    if crate::automations::interrupts(app, device) {
        return;
    }
    let settings = match crate::storage::store(app).and_then(|s| s.read_settings()) {
        Ok(settings) => settings,
        Err(e) => {
            tracing::warn!(device = %device, "applied effect not resumed: {e}");
            return;
        }
    };
    let status = app
        .state::<AppState>()
        .engine
        .device_status()
        .into_iter()
        .find(|s| s.device == device)
        .map(|s| s.status);
    if crate::find_layout(device).is_ok_and(|l| crate::split(&settings, l)) {
        let applied = applied_regions(&settings, device);
        let running = status.and_then(|s| s.regions);
        if regions_to_resume(&settings, &applied, running.as_deref()) {
            match run_regions(app, device) {
                Ok(()) => tracing::info!(device = %device, "applied regions resumed"),
                Err(e) => tracing::warn!(device = %device, "applied regions not resumed: {e}"),
            }
        }
        return;
    }
    let running = status.filter(|s| s.running).and_then(|s| s.effect_id);
    let Some(effect) = to_resume(&settings, device, running.as_deref()) else {
        return;
    };
    match start_saved(app, device, &effect) {
        Ok(()) => tracing::info!(device = %device, effect, "applied effect resumed"),
        Err(e) if e.code == "effectNotCompiled" => {
            tracing::info!(device = %device, effect, "applied effect waits for compilation")
        }
        Err(e) => tracing::warn!(device = %device, effect, "applied effect not resumed: {e}"),
    }
}

/// The effect to start on `device`, given what runs there now: the applied one,
/// unless the setting is off or it already runs.
fn to_resume(
    settings: &crate::storage::Settings,
    device: DeviceRef,
    running: Option<&str>,
) -> Option<String> {
    if !settings.preferences.resume_effects {
        return None;
    }
    let applied = settings.active_effect(device.vid, device.pid)?;
    (running != Some(applied)).then(|| applied.to_owned())
}

/// Each region of a split device with its applied effect, by id.
fn applied_regions(
    settings: &crate::storage::Settings,
    device: DeviceRef,
) -> Vec<(String, String)> {
    settings
        .split(device.vid, device.pid)
        .map(|split| {
            split
                .regions
                .iter()
                .filter_map(|r| Some((r.region.clone(), r.effect.clone()?)))
                .collect()
        })
        .unwrap_or_default()
}

/// Whether a split device that opened starts its regions again: the setting on,
/// and the loop not running each region's applied effect already — a replug
/// finds it still writing. A split device whose regions run nothing is started
/// too, so that it stays dark rather than showing its firmware's lighting.
fn regions_to_resume(
    settings: &crate::storage::Settings,
    applied: &[(String, String)],
    running: Option<&[RegionEngineStatus]>,
) -> bool {
    if !settings.preferences.resume_effects {
        return false;
    }
    let Some(running) = running else {
        return true;
    };
    !applied.iter().all(|(region, effect)| {
        running
            .iter()
            .any(|r| &r.region == region && r.running && r.effect_id.as_ref() == Some(effect))
    })
}

/// Resumes, on every open device, `effect` if it is the one applied there, or
/// on one of its regions: called once it is compiled.
pub(crate) fn resume_waiting(app: &AppHandle, effect: &str) {
    let Ok(settings) = crate::storage::store(app).and_then(|s| s.read_settings()) else {
        return;
    };
    for device in app.state::<AppState>().open_devices() {
        let on_a_region = applied_regions(&settings, device)
            .iter()
            .any(|(_, applied)| applied == effect);
        if settings.active_effect(device.vid, device.pid) == Some(effect) || on_a_region {
            resume_applied(app, device);
        }
    }
}

/// The parameters as the loop reads them.
pub(crate) fn serialised(params: &serde_json::Value) -> CmdResult<String> {
    serde_json::to_string(params)
        .map_err(|e| Failure::unexpected(format!("parameters not serialisable: {e}")))
}

/// A load that failed: the effect's own error, for its author, in English.
pub(crate) fn not_started(id: &str, error: String) -> Failure {
    Failure::new("effectNotStarted")
        .with("name", id)
        .with("error", error)
}

/// Stops what runs on a device — or, on a split device, on one of its regions,
/// whose lights go black while the others go on — and forgets it as applied.
#[tauri::command]
pub fn stop_effect(
    app: AppHandle,
    state: State<'_, AppState>,
    device: DeviceRef,
    region: Option<String>,
) -> CmdResult<()> {
    match crate::part_of(&app, device, region.as_deref(), false)? {
        Scope::Whole => {
            let interrupted = crate::automations::interrupts(&app, device);
            // Stopping during an interruption ends it too: nothing is applied
            // any more, so there is nothing to give back but the layers.
            crate::automations::dismiss(&app, device);
            remember_active_effect(&app, device, None);
            if interrupted {
                // The rule's effect goes, and the device's layers come back
                // over black; with none, nothing starts.
                run_stack(&app, device, None)?;
            } else if state
                .engine
                .layers_of(device, None)
                .is_some_and(|layers| !layers.is_empty())
            {
                // The layers go on, over black (`docs/design/composition.md`
                // §3.2): stopping the effect is not removing them.
                state.engine.stop_base(device, None);
            } else {
                state.engine.stop(device);
            }
        }
        Scope::Region(region) => {
            let interrupted = crate::automations::interrupts(&app, device);
            crate::automations::dismiss(&app, device);
            remember_region_effect(&app, device, region.id, None);
            if interrupted {
                // The gesture ends the rule: the device goes back to its
                // regions, this one with nothing.
                run_regions(&app, device)?;
            } else {
                state.engine.stop_region(device, region.id);
            }
        }
    }
    Ok(())
}

/// Remembers — or forgets, with `None` — the effect applied on this device.
///
/// # A failure is logged, not propagated
///
/// The effect runs, the keyboard is lit: making "Apply" fail because
/// the disk did not take note would make the lighting pay for an incident that
/// does not concern it. What is lost is bounded and fits in one line — the file
/// will not resume this effect later.
pub(crate) fn remember_active_effect(app: &AppHandle, device: DeviceRef, effect: Option<&str>) {
    let write = || -> CmdResult<()> {
        let store = crate::storage::store(app)?;
        let mut settings = store.read_settings()?;
        // Nothing new: do not go through the temporary file and its rename
        // again. Starting the same effect twice is a double click.
        if settings.set_active_effect(device.vid, device.pid, effect) {
            store.write_settings(&settings)?;
        }
        Ok(())
    };
    if let Err(e) = write() {
        tracing::warn!(device = %device, "applied effect not remembered: {e}");
    }
}

/// Remembers — or forgets — the effect applied on a region, as
/// [`remember_active_effect`] does for a device: a failure is logged.
pub(crate) fn remember_region_effect(
    app: &AppHandle,
    device: DeviceRef,
    region: &str,
    effect: Option<&str>,
) {
    let write = || -> CmdResult<()> {
        let store = crate::storage::store(app)?;
        let mut settings = store.read_settings()?;
        if settings.set_region_effect(device.vid, device.pid, region, effect) {
            store.write_settings(&settings)?;
        }
        Ok(())
    };
    if let Err(e) = write() {
        tracing::warn!(device = %device, region, "applied effect not remembered: {e}");
    }
}

// ---------------------------------------------------------------- preview

/// Starts the preview of an effect, **without touching the keyboard or the
/// disk**.
///
/// It is the exact counterpart of [`start_effect`], minus everything that
/// commits: no hardware output, no write to `settings.json`, and above all **no
/// device loop stopped**. Selecting an effect in the gallery goes through here;
/// the effect running on the keyboard keeps running.
///
/// `device` names the device whose **layout is borrowed**. `None` falls back to
/// the default layout: one previews without owning the keyboard, and without
/// having adopted any — the same reason `get_default_layout` exists.
///
/// With `region`, the region's layout, and its follow: the effect sees the
/// region alone, and the frames have the device's length, black outside it
/// (`docs/design/regions.md` §7). A split device's whole layout may be borrowed
/// too.
#[tauri::command]
pub fn start_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    device: Option<DeviceRef>,
    region: Option<String>,
    id: String,
    params: serde_json::Value,
    bindings: Option<Bindings>,
) -> CmdResult<()> {
    let bindings = checked_bindings(bindings.unwrap_or_default())?;
    let (layout, region) = match (device, region.as_deref()) {
        (None, None) => (crate::default_layout(), None),
        (None, Some(region)) => {
            return Err(Failure::unexpected(format!(
                "the preview of region “{region}” names no device"
            )))
        }
        (Some(d), region) => match crate::part_of(&app, d, region, true)? {
            Scope::Whole => (crate::find_layout(d)?, None),
            Scope::Region(region) => (crate::find_layout(d)?, Some(region)),
        },
    };
    let start = effect_start(&app, &id, &params, bindings)?;

    state
        .engine
        .start_preview(DeviceRef::of(layout), region, start, layout)
        .map_err(|error| not_started(&id, error))
}

#[tauri::command]
pub fn stop_preview(state: State<'_, AppState>) {
    state.engine.stop_preview();
}

/// Adjusts the preview parameters live, as [`set_effect_params`] does for a
/// device. The loop re-reads the JSON on every frame.
#[tauri::command]
pub fn set_preview_params(state: State<'_, AppState>, params: serde_json::Value) -> CmdResult<()> {
    state.engine.set_preview_params(None, serialised(&params)?);
    Ok(())
}

/// Binds the preview's parameters to signals, live, as
/// [`set_effect_bindings`] does for a device.
#[tauri::command]
pub fn set_preview_bindings(state: State<'_, AppState>, bindings: Bindings) -> CmdResult<()> {
    state
        .engine
        .set_preview_bindings(None, checked_bindings(bindings)?);
    Ok(())
}

/// Opens the preview frame stream to the simulator.
///
/// A channel separate from the devices' one, and that is what allows watching
/// an effect while another one runs on the keyboard: both streams exist at the
/// same time, and the window picks which one it shows.
#[tauri::command]
pub fn subscribe_preview_frames(state: State<'_, AppState>, channel: Channel<InvokeResponseBody>) {
    state.engine.set_preview_channel(Some(channel));
}

#[tauri::command]
pub fn unsubscribe_preview_frames(state: State<'_, AppState>) {
    state.engine.set_preview_channel(None);
}

/// Adjusts the parameters live — of the device's effect, or of one region's on
/// a split device. The loop does not restart: it re-reads the JSON on every
/// frame.
#[tauri::command]
pub fn set_effect_params(
    app: AppHandle,
    state: State<'_, AppState>,
    device: DeviceRef,
    region: Option<String>,
    params: serde_json::Value,
) -> CmdResult<()> {
    let part = crate::part_of(&app, device, region.as_deref(), false)?;
    state
        .engine
        .set_params(device, part.region(), None, serialised(&params)?);
    Ok(())
}

/// Binds parameters of the effect running on a device — or on one of its
/// regions — to signals, live: the loop reads the bound values from the next
/// frame, without restarting. Remembering them is
/// [`crate::storage::remember_effect_bindings`].
#[tauri::command]
pub fn set_effect_bindings(
    app: AppHandle,
    state: State<'_, AppState>,
    device: DeviceRef,
    region: Option<String>,
    bindings: Bindings,
) -> CmdResult<()> {
    let part = crate::part_of(&app, device, region.as_deref(), false)?;
    state
        .engine
        .set_bindings(device, part.region(), None, checked_bindings(bindings)?);
    Ok(())
}

/// Bindings as the interface or a rule sends them, checked: each source must
/// name something a setting can follow ([`is_source`]). A parameter the effect does not
/// declare is let through — the bootstrap ignores it, as it ignores a stored
/// value for a parameter an effect no longer has.
pub(crate) fn checked_bindings(bindings: Bindings) -> CmdResult<Bindings> {
    for (param, source) in &bindings {
        if !is_source(source) {
            return Err(Failure::new("bindingInvalid")
                .with("param", param)
                .with("source", source));
        }
    }
    Ok(bindings)
}

/// The bound values the signals gave, with the levels of the sound or the system
/// added.
fn with_levels(bound: &str, sound: serde_json::Map<String, serde_json::Value>) -> String {
    let mut all: serde_json::Map<String, serde_json::Value> = if bound.is_empty() {
        serde_json::Map::new()
    } else {
        serde_json::from_str(bound).unwrap_or_default()
    };
    all.extend(sound);
    serde_json::Value::Object(all).to_string()
}

/// Whether a binding names something a setting can follow: a signal, the
/// sound playing (§2.2.1), or what the computer is doing (§2.4).
pub(crate) fn is_source(source: &str) -> bool {
    crate::signals::store::bound_signal(source).is_some()
        || crate::audio::analysis::Source::parse(source).is_some()
        || crate::system::Source::parse(source).is_some()
}

/// The values of the settings following what the computer is doing, as the
/// bootstrap converts them: `{"hue": {"system": 0.42}}`, each 0..1. A source
/// this computer cannot read is left out, and the setting keeps its value, as
/// one following an absent signal does.
fn system_bound(
    bindings: &Bindings,
    system: &crate::system::System,
) -> serde_json::Map<String, serde_json::Value> {
    bindings
        .iter()
        .filter_map(|(param, source)| {
            let level = system.level(crate::system::Source::parse(source)?)?;
            let value = (f64::from(level) * 1000.0).round() / 1000.0;
            Some((param.clone(), serde_json::json!({ "system": value })))
        })
        .collect()
}

/// The values of the settings following the sound, as the bootstrap converts
/// them: `{"speed": {"sound": 0.42}}`, each 0..1 and mapped by the setting's
/// kind rather than taken as a sender's raw value.
fn sound_bound(
    bindings: &Bindings,
    frame: &crate::audio::analysis::Frame,
) -> serde_json::Map<String, serde_json::Value> {
    bindings
        .iter()
        .filter_map(|(param, source)| {
            let source = crate::audio::analysis::Source::parse(source)?;
            let value = (f64::from(frame.source(source)) * 1000.0).round() / 1000.0;
            Some((param.clone(), serde_json::json!({ "sound": value })))
        })
        .collect()
}

/// Turns a device's keyboard output on or off, without touching the simulator.
#[tauri::command]
pub fn set_output_to_keyboard(state: State<'_, AppState>, device: DeviceRef, on: bool) {
    state.engine.set_to_keyboard(device, on);
}

/// Opens the frame stream of **one device** to the simulator.
///
/// A channel, not a global event: the destination is known, the scope is
/// explicit, and the binary goes through raw. Releasing the channel on the
/// front end, or calling [`unsubscribe_frames`], stops the stream **without
/// stopping the effect**, which keeps feeding the keyboard with the window
/// closed.
///
/// The simulator follows the selected device: changing the selection means
/// subscribing elsewhere, not multiplexing a single stream.
#[tauri::command]
pub fn subscribe_frames(
    state: State<'_, AppState>,
    device: DeviceRef,
    channel: Channel<InvokeResponseBody>,
) {
    state.engine.set_channel(device, Some(channel));
}

#[tauri::command]
pub fn unsubscribe_frames(state: State<'_, AppState>, device: DeviceRef) {
    state.engine.set_channel(device, None);
}

/// Engine state: what runs **on the devices**, and what is **being watched**.
///
/// Polled rather than pushed: an error raised while the window was closed must
/// be readable when it reopens, which a one-off event does not allow.
///
/// `devices` holds one entry per device targeted since startup — not only per
/// open device, nor per running loop: "this device is doing nothing" and "I
/// know nothing about this device" are not the same statement.
///
/// `preview` is **separate**, and the interface cannot confuse the two: see
/// [`PreviewStatus`].
#[tauri::command]
pub fn engine_status(app: AppHandle, state: State<'_, AppState>) -> EngineReport {
    let mut report = state.engine.report();
    crate::automations::annotate(&app, &mut report.devices);
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal effect, written the way a user would write it.
    const EFFECT: &str = r#"
        import { hsv } from '@candeo/effects-api'
        export default {
          name: 'Test',
          render({ layout, time, frame, params }) {
            for (const key of layout.keys) {
              frame.set(key, hsv(time * Number(params.speed ?? 0) + key.col * 10, 1, 1))
            }
          },
        }
    "#;

    fn layout() -> &'static Layout {
        candeo_device::definition::builtin()[0]
    }

    /// `unwrap_err` would require `(Runtime, Context)` to be `Debug`, which
    /// rquickjs does not provide.
    fn load_error(js: &str) -> String {
        match prepare(js, layout()) {
            Ok(_) => panic!("loading should have failed"),
            Err(e) => e,
        }
    }

    // ------------------------------------------------------------ one device

    /// Two made-up devices: the only real layout is unique, and everything that
    /// is "per device" only makes sense from two onwards. They only exist to be
    /// told apart — the geometry stays that of the real layout, so that the
    /// effects render real frames.
    const FIRST: DeviceRef = DeviceRef {
        vid: 0x1532,
        pid: 0x1111,
    };
    const SECOND: DeviceRef = DeviceRef {
        vid: 0x1532,
        pid: 0x2222,
    };

    /// #102: a device that opens gets its applied effect back, unless the setting
    /// is off, nothing is applied there, or that effect already runs.
    #[test]
    fn the_applied_effect_is_resumed_unless_it_runs_or_resuming_is_off() {
        let mut settings = crate::storage::Settings::default();
        settings.set_active_effect(FIRST.vid, FIRST.pid, Some("Rain"));

        assert_eq!(to_resume(&settings, FIRST, None), Some("Rain".into()));
        assert_eq!(
            to_resume(&settings, FIRST, Some("Swirl")),
            Some("Rain".into())
        );
        assert_eq!(to_resume(&settings, FIRST, Some("Rain")), None);
        assert_eq!(to_resume(&settings, SECOND, None), None);

        settings.preferences.resume_effects = false;
        assert_eq!(to_resume(&settings, FIRST, None), None);
    }

    /// A split device that opens gets its regions back, unless resuming is off
    /// or its loop already runs each region's applied effect (`regions.md` §11 d).
    #[test]
    fn a_split_device_s_regions_are_resumed_unless_they_run() {
        let mut settings = crate::storage::Settings::default();
        settings.set_split(FIRST.vid, FIRST.pid, Some(&["keyboard", "lid"]));
        settings.set_region_effect(FIRST.vid, FIRST.pid, "lid", Some("Rain"));
        let applied = applied_regions(&settings, FIRST);
        assert_eq!(applied, vec![("lid".to_owned(), "Rain".to_owned())]);
        let region = |id: &str, running: bool, effect: Option<&str>| RegionEngineStatus {
            region: id.into(),
            running,
            effect_id: effect.map(str::to_owned),
            error: None,
            reads_private: false,
            layers: Vec::new(),
        };

        assert!(regions_to_resume(&settings, &applied, None), "nothing runs");
        let running = [
            region("keyboard", false, None),
            region("lid", true, Some("Rain")),
        ];
        assert!(!regions_to_resume(&settings, &applied, Some(&running)));
        let failed = [
            region("keyboard", false, None),
            region("lid", false, Some("Rain")),
        ];
        assert!(regions_to_resume(&settings, &applied, Some(&failed)));
        let other = [
            region("keyboard", false, None),
            region("lid", true, Some("Swirl")),
        ];
        assert!(regions_to_resume(&settings, &applied, Some(&other)));

        settings.preferences.resume_effects = false;
        assert!(!regions_to_resume(&settings, &applied, None));
    }

    /// Beyond this, the expected condition is considered not to be coming.
    ///
    /// Generous, and on purpose: at 30 frames per second a few frames fit in a
    /// few tens of milliseconds, but the pace of a CI runner is not that of a
    /// development machine. A test that sleeps for a chosen duration would be
    /// either slow or flaky.
    const PATIENCE: Duration = Duration::from_secs(5);

    /// A device output driven from the test.
    ///
    /// That is the whole point of [`DeviceOut`]: making **one** device fail
    /// without plugging any in. With the `Keyboard` hard-wired, the invariant "a
    /// failing device affects no other" could only have been checked with two
    /// keyboards on the desk, so never.
    #[derive(Default)]
    struct Output {
        written: AtomicU32,
        failing: AtomicBool,
        /// Writes still to fail before this output recovers on its own.
        transient_failures: AtomicU32,
        /// Set when the loop dropped the device, as the real handle does.
        closed: AtomicBool,
        /// Set when the loop turned the backlight off.
        turned_off: AtomicBool,
        /// How many times the loop took the lights back.
        taken_over: AtomicU32,
        /// The last frame written.
        last: Mutex<Vec<Rgb>>,
        /// The first frames written, in order.
        first: Mutex<Vec<Vec<Rgb>>>,
    }

    /// Frames [`Output::first`] keeps.
    const FIRST_FRAMES: usize = 64;

    impl DeviceOut for Arc<Output> {
        fn present(&self, colors: &[Rgb], abandon: bool) -> Option<Result<(), String>> {
            if self.closed.load(Ordering::Relaxed) {
                return None;
            }
            let transient = self
                .transient_failures
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1))
                .is_ok();
            if transient || self.failing.load(Ordering::Relaxed) {
                if abandon {
                    self.closed.store(true, Ordering::Relaxed);
                }
                return Some(Err("write refused by the device".into()));
            }
            *self.last.lock().unwrap() = colors.to_vec();
            let mut first = self.first.lock().unwrap();
            if first.len() < FIRST_FRAMES {
                first.push(colors.to_vec());
            }
            self.written.fetch_add(1, Ordering::Relaxed);
            Some(Ok(()))
        }

        fn turn_off(&self) {
            self.turned_off.store(true, Ordering::Relaxed);
        }

        fn take_over(&self) {
            self.taken_over.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// #72: a device whose writes keep failing is **closed** after
    /// [`MAX_DEVICE_WRITE_ERRORS`] frames. An unplugged keyboard leaves a dead
    /// handle, and keeping it made every view report the device as open. The
    /// loop itself goes on — the simulator still animates — and the error kept
    /// for the window says the device was closed.
    #[test]
    fn a_device_that_keeps_failing_is_closed_but_the_loop_goes_on() {
        let engine = Engine::default();
        let broken = Arc::new(Output::default());
        broken.failing.store(true, Ordering::Relaxed);

        start(&engine, FIRST, "broken", Arc::clone(&broken));
        wait_for("the failing device was never closed", || {
            broken.closed.load(Ordering::Relaxed)
        });

        let status = status(&engine, FIRST);
        assert!(status.running, "closing the device stopped the loop");
        assert!(!status.reaching_keyboard);
        assert_eq!(status.device_error, Some(Failure::new("deviceClosed")));

        engine.stop(FIRST);
    }

    /// A few failures below the threshold close nothing: a transient error
    /// must not cost a manual reconnection, and a success clears the count.
    #[test]
    fn frames_follow_the_device_when_another_effect_starts_there() {
        let engine = Engine::default();
        let received = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&received);
        let channel = Channel::new(move |_| {
            counter.fetch_add(1, Ordering::Relaxed);
            Ok(())
        });

        start(&engine, FIRST, "applied", Arc::new(Output::default()));
        engine.set_channel(FIRST, Some(channel));
        wait_for("no frame reached the simulator", || {
            received.load(Ordering::Relaxed) > 0
        });

        // What an automation does: another effect on the same device, with no
        // window to subscribe again.
        start(&engine, FIRST, "rule", Arc::new(Output::default()));
        let before = received.load(Ordering::Relaxed);
        wait_for(
            "the simulator lost the device when its effect changed",
            || received.load(Ordering::Relaxed) > before + 3,
        );
        engine.stop(FIRST);
    }

    /// §2.2.2: a device's brightness following a signal dims the frames its
    /// loop writes, from the next frame, and holds across a change of effect.
    #[test]
    fn a_brightness_following_a_signal_dims_the_frames_written() {
        let engine = Engine::default();
        let output = Arc::new(Output::default());
        let brightest = |out: &Output| {
            let last = out.last.lock().unwrap();
            last.iter()
                .map(|c| c.r.max(c.g).max(c.b))
                .max()
                .unwrap_or(0)
        };
        let now = wall_clock_ms() as i64;
        engine
            .signals()
            .lock()
            .unwrap()
            .apply(&serde_json::json!({ "lux": 50 }), None, now)
            .unwrap();

        start(&engine, FIRST, "applied", Arc::clone(&output));
        wait_for("the effect never lit the device fully", || {
            brightest(&output) == 255
        });

        engine.set_dimming(
            FIRST,
            None,
            Some(dimming::Dimming {
                source: "signal:lux".into(),
                floor: 20,
            }),
        );
        wait_for("the brightness did not follow the signal", || {
            brightest(&output) == 128
        });

        // What a rule does: another effect on the device. The brightness is
        // the device's, not the effect's.
        let next = Arc::new(Output::default());
        start(&engine, FIRST, "rule", Arc::clone(&next));
        wait_for("the brightness was lost with the effect", || {
            brightest(&next) == 128
        });

        engine.set_dimming(FIRST, None, None);
        wait_for("the brightness did not come back", || {
            brightest(&next) == 255
        });
        engine.stop(FIRST);
    }

    /// A host effect starting on a device takes its lights back from the
    /// firmware once, before its first frame, whichever effect ran before.
    #[test]
    fn a_host_effect_takes_the_lights_back_once() {
        let engine = Engine::default();
        let output = Arc::new(Output::default());
        start(&engine, FIRST, "applied", Arc::clone(&output));
        wait_for("no frame was written", || {
            output.written.load(Ordering::Relaxed) >= 3
        });
        assert_eq!(output.taken_over.load(Ordering::Relaxed), 1);
        engine.stop(FIRST);
    }

    #[test]
    fn a_transient_write_failure_does_not_close_the_device() {
        let engine = Engine::default();
        let output = Arc::new(Output::default());
        output.transient_failures.store(3, Ordering::Relaxed);

        start(&engine, FIRST, "hiccup", Arc::clone(&output));
        wait_for("writing never recovered", || {
            output.written.load(Ordering::Relaxed) >= 3
        });

        assert!(
            !output.closed.load(Ordering::Relaxed),
            "closed on a transient failure"
        );
        let status = status(&engine, FIRST);
        assert!(status.reaching_keyboard);
        assert_eq!(status.device_error, None);

        engine.stop(FIRST);
    }

    fn start(engine: &Engine, device: DeviceRef, effect_id: &str, out: Arc<Output>) {
        engine
            .start(
                device,
                effect(effect_id, EFFECT),
                layout(),
                Box::new(out),
                false,
            )
            .expect("start");
    }

    /// `js` to start under the name `effect_id`, with no setting moved.
    fn effect(effect_id: &str, js: &str) -> EffectStart {
        EffectStart {
            effect_id: effect_id.into(),
            js: js.into(),
            params: "{}".into(),
            bindings: Bindings::new(),
        }
    }

    /// A device's status, taken from the list the engine returns.
    fn status(engine: &Engine, device: DeviceRef) -> EngineStatus {
        engine
            .device_status()
            .into_iter()
            .find(|s| s.device == device)
            .unwrap_or_else(|| panic!("no status for {device}"))
            .status
    }

    /// Waits for a condition to come true, or fails. See [`PATIENCE`].
    fn wait_for(what: &str, ready: impl FnMut() -> bool) {
        wait_at_most(PATIENCE, what, ready);
    }

    /// Like [`wait_for`], but with a patience computed from the limit under
    /// test.
    ///
    /// [`PATIENCE`] is a fixed duration, chosen for conditions that come true
    /// within a few frames. A limit, on the other hand, promises a duration:
    /// stopping a looping effect takes [`MAX_CONSECUTIVE_ERRORS`] cut frames,
    /// and a cut frame lasts the whole [`FRAME_BUDGET`] — which is not the same
    /// from one build profile to the other. Waiting for a multiple of what is
    /// under test keeps the test right in both cases.
    fn wait_at_most(patience: Duration, what: &str, mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + patience;
        while Instant::now() < deadline {
            if ready() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("{what}: nothing in {patience:?}");
    }

    /// **The adoption invariant, at the engine level.**
    ///
    /// `a_failing_device_blocks_no_other` checks it at open time;
    /// this one checks it once the loops are running, where the device fails
    /// while in use. A device whose every write fails must take nothing away
    /// from the others: not their loop, not their frames, not their state — and
    /// stopping it must not stop them.
    #[test]
    fn a_broken_device_affects_no_other() {
        let engine = Engine::default();
        let broken = Arc::new(Output::default());
        broken.failing.store(true, Ordering::Relaxed);
        let healthy = Arc::new(Output::default());

        start(&engine, FIRST, "casse", Arc::clone(&broken));
        start(&engine, SECOND, "sain", Arc::clone(&healthy));

        wait_for("the healthy neighbor writes nothing", || {
            healthy.written.load(Ordering::Relaxed) >= 3
        });

        let failed = status(&engine, FIRST);
        assert!(failed.running, "the broken device's loop stopped");
        assert!(
            failed.device_error.is_some(),
            "the write failure was not recorded"
        );
        assert!(!failed.reaching_keyboard);
        assert_eq!(broken.written.load(Ordering::Relaxed), 0);

        let ok = status(&engine, SECOND);
        assert!(ok.running, "the neighbor's loop stopped");
        assert_eq!(ok.device_error, None, "the neighbor's failure spilled over");
        assert!(ok.reaching_keyboard, "the neighbor is no longer reached");
        assert_eq!(
            ok.error, None,
            "effect error on the neighbor: {:?}",
            ok.error
        );

        // Stopping the broken device leaves the other one running: the loops
        // share no thread, no lock and no state.
        let before = healthy.written.load(Ordering::Relaxed);
        engine.stop(FIRST);
        wait_for("the neighbor stopped along with it", || {
            healthy.written.load(Ordering::Relaxed) > before
        });
        assert!(!status(&engine, FIRST).running);
        assert!(status(&engine, SECOND).running);

        engine.stop(SECOND);
    }

    /// Each device carries its own effect and output. Turning one off does not
    /// turn off the other — otherwise "Send to keyboard"
    /// would be a global switch disguised as a device setting.
    #[test]
    fn each_device_carries_its_own_effect_and_output() {
        let engine = Engine::default();
        let a = Arc::new(Output::default());
        let b = Arc::new(Output::default());

        start(&engine, FIRST, "premier", Arc::clone(&a));
        start(&engine, SECOND, "second", Arc::clone(&b));

        assert_eq!(status(&engine, FIRST).effect_id.as_deref(), Some("premier"));
        assert_eq!(status(&engine, SECOND).effect_id.as_deref(), Some("second"));

        engine.set_to_keyboard(SECOND, false);
        wait_for("the second output stays on", || {
            !status(&engine, SECOND).reaching_keyboard
        });

        let frozen = b.written.load(Ordering::Relaxed);
        let before = a.written.load(Ordering::Relaxed);
        wait_for("the first no longer writes", || {
            a.written.load(Ordering::Relaxed) > before + 2
        });

        assert!(status(&engine, FIRST).to_keyboard, "the cut spilled over");
        assert!(status(&engine, FIRST).reaching_keyboard);
        assert_eq!(
            b.written.load(Ordering::Relaxed),
            frozen,
            "the output turned off still writes"
        );

        engine.stop(FIRST);
        engine.stop(SECOND);
    }

    /// A stopped device keeps its line: "this device is doing nothing" and "I
    /// know nothing about this device" are not the same statement, and the
    /// interface must be able to tell them apart.
    #[test]
    fn a_stopped_device_keeps_its_status_line() {
        let engine = Engine::default();
        start(&engine, FIRST, "premier", Arc::new(Output::default()));
        engine.stop(FIRST);

        let s = status(&engine, FIRST);
        assert!(!s.running);
        assert_eq!(s.effect_id, None);
        assert_eq!(engine.device_status().len(), 1);
    }

    /// **What deleting an effect must get from the engine.**
    ///
    /// A deleted effect may be running on several devices, and it must stop on
    /// all of them — a forgotten loop would keep running an `effect.js` loaded
    /// into memory, with no visible error, while its folder no longer exists.
    /// The loops running **something else** are not concerned: deleting an
    /// effect does not turn off the other keyboards.
    #[test]
    fn stopping_an_effect_stops_it_everywhere_and_nowhere_else() {
        let engine = Engine::default();
        let neighbor = Arc::new(Output::default());

        start(&engine, FIRST, "a-supprimer", Arc::new(Output::default()));
        start(&engine, SECOND, "autre", Arc::clone(&neighbor));

        let stopped = engine.stop_everywhere("a-supprimer");
        assert_eq!(stopped, vec![FIRST]);

        let deleted = status(&engine, FIRST);
        assert!(!deleted.running);
        assert_eq!(
            deleted.effect_id, None,
            "the status line still names an effect that no longer exists"
        );

        // The neighbor, for its part, noticed nothing: it is still running, and
        // its frames keep going out.
        let before = neighbor.written.load(Ordering::Relaxed);
        assert!(status(&engine, SECOND).running);
        wait_for("the neighbor stopped along with it", || {
            neighbor.written.load(Ordering::Relaxed) > before
        });

        engine.stop(SECOND);
    }

    /// The same effect on two devices: both loops go.
    #[test]
    fn stopping_an_effect_covers_every_device_running_it() {
        let engine = Engine::default();
        start(&engine, FIRST, "partout", Arc::new(Output::default()));
        start(&engine, SECOND, "partout", Arc::new(Output::default()));

        assert_eq!(engine.stop_everywhere("partout"), vec![FIRST, SECOND]);
        assert!(!status(&engine, FIRST).running);
        assert!(!status(&engine, SECOND).running);
    }

    /// An id that nothing is running stops nothing. That is the common case:
    /// deleting an effect that was never applied.
    #[test]
    fn stopping_an_effect_running_nowhere_touches_nothing() {
        let engine = Engine::default();
        start(&engine, FIRST, "en-cours", Arc::new(Output::default()));

        assert!(engine.stop_everywhere("jamais-lance").is_empty());
        assert!(status(&engine, FIRST).running);

        engine.stop(FIRST);
    }

    /// What resetting the configuration expects from the engine: not a single
    /// loop left, whatever the effect and whatever the device — preview
    /// included, which writes to nothing but keeps a QuickJS context alive.
    #[test]
    fn stopping_everything_leaves_no_loop() {
        let engine = Engine::default();
        start(&engine, FIRST, "premier", Arc::new(Output::default()));
        start(&engine, SECOND, "second", Arc::new(Output::default()));
        preview_effect(&engine, FIRST, "regarde");

        engine.stop_all();

        assert!(engine.device_status().iter().all(|s| !s.status.running));
        // The lines stay: "this device is doing nothing" and "I know nothing
        // about this device" are not the same statement, reset or not.
        assert_eq!(engine.device_status().len(), 2);
        assert!(engine.preview_status().is_none());
    }

    // ------------------------------------------------------------ regions

    /// The 17 R5's zones: keyboard 4–7 and 12, lid 0, 1, 10, 11, base 2, 8, 9,
    /// power 3 — positions and addresses alike.
    fn r5() -> &'static Layout {
        candeo_device::definition::builtin_layout("alienware-17-r5-zones.json").expect("built in")
    }

    const R5: DeviceRef = DeviceRef {
        vid: 0x187c,
        pid: 0x0550,
    };

    /// Every light of its layout in one colour, read from `params.r`.
    const LEVEL: &str = r#"export default {
        render({ layout, frame, params }) {
            for (const key of layout.keys) frame.set(key, { r: Number(params.r ?? 0), g: 0, b: 0 })
        },
    }"#;

    /// Paints the whole frame, past its layout's lights: what it owns is all a
    /// region's part keeps.
    const FILL: &str = r#"export default {
        render({ frame }) { frame.fill({ r: 0, g: 0, b: 255 }) },
    }"#;

    /// Its time, in red: 255 after 0.64 s.
    const RAMP: &str = r#"export default {
        render({ layout, frame, time }) {
            for (const key of layout.keys) frame.set(key, { r: Math.min(255, Math.floor(time * 400)), g: 0, b: 0 })
        },
    }"#;

    const THROWS: &str = "export default { render() { throw new Error('boum') } }";

    fn with_params(effect_id: &str, js: &str, params: &str) -> EffectStart {
        EffectStart {
            params: params.into(),
            ..effect(effect_id, js)
        }
    }

    fn colour(out: &Output, position: usize) -> Rgb {
        out.last
            .lock()
            .unwrap()
            .get(position)
            .copied()
            .unwrap_or_default()
    }

    /// The regions of the 17 R5 with what each runs, in the definition's order.
    fn regions(
        mut starts: Vec<(&str, Option<EffectStart>)>,
    ) -> Vec<(&'static Region, Option<EffectStart>)> {
        r5().regions
            .iter()
            .map(|r| {
                let start = starts
                    .iter()
                    .position(|(id, _)| *id == r.id)
                    .and_then(|i| starts.remove(i).1);
                (r, start)
            })
            .collect()
    }

    /// Each region with its base alone: its stack as before layers.
    fn bases(
        starts: Vec<(&'static Region, Option<EffectStart>)>,
    ) -> Vec<(&'static Region, StackStart)> {
        starts
            .into_iter()
            .map(|(region, start)| (region, StackStart::base(start)))
            .collect()
    }

    fn start_regions(engine: &Engine, out: &Arc<Output>, starts: Vec<(&str, Option<EffectStart>)>) {
        let verdicts = engine
            .start_regions(R5, r5(), Box::new(Arc::clone(out)), bases(regions(starts)))
            .expect("within the limits");
        assert!(verdicts.iter().all(|(_, v)| v.base.is_ok()), "{verdicts:?}");
    }

    fn region_status(engine: &Engine, region: &str) -> RegionEngineStatus {
        status(engine, R5)
            .regions
            .expect("the loop runs the regions")
            .into_iter()
            .find(|r| r.region == region)
            .expect("a region of the definition")
    }

    /// One loop, one frame: each region's lights from its own effect, a region
    /// running nothing black, and an effect painting past its region's lights
    /// owning none of them (`regions.md` §7).
    #[test]
    fn each_region_runs_its_effect_in_the_device_s_frame() {
        let engine = Engine::default();
        let out = Arc::new(Output::default());
        start_regions(
            &engine,
            &out,
            vec![
                ("keyboard", Some(with_params("red", LEVEL, r#"{"r":200}"#))),
                ("lid", Some(effect("blue", FILL))),
            ],
        );
        wait_for("the regions were never drawn", || {
            colour(&out, 4).r == 200 && colour(&out, 0).b == 255
        });
        let last = out.last.lock().unwrap().clone();
        for position in [4, 5, 6, 7, 12] {
            assert_eq!(last[position], Rgb::new(200, 0, 0), "keyboard {position}");
        }
        for position in [0, 1, 10, 11] {
            assert_eq!(last[position], Rgb::new(0, 0, 255), "lid {position}");
        }
        for position in [2, 3, 8, 9] {
            assert_eq!(last[position], Rgb::default(), "{position} runs nothing");
        }

        let device = status(&engine, R5);
        assert!(device.running);
        assert_eq!((device.effect_id, device.error), (None, None));
        let regions = device.regions.unwrap();
        let ids: Vec<&str> = regions.iter().map(|r| r.region.as_str()).collect();
        assert_eq!(
            ids,
            ["keyboard", "lid", "base", "power"],
            "the definition's order"
        );
        assert_eq!(regions[0].effect_id.as_deref(), Some("red"));
        assert!(regions[1].running);
        assert!(!regions[2].running && regions[2].effect_id.is_none());
        assert!(engine.runs_regions(R5));
        engine.stop(R5);
    }

    /// Applying an effect on one region leaves the others running, their clocks
    /// going on; one that does not load leaves the region with what it ran.
    #[test]
    fn one_region_replaced_leaves_the_others_clocks_running() {
        let engine = Engine::default();
        let out = Arc::new(Output::default());
        start_regions(
            &engine,
            &out,
            vec![
                ("keyboard", Some(effect("ramp", RAMP))),
                ("lid", Some(effect("ramp", RAMP))),
            ],
        );
        wait_for("the clocks never reached 0.64 s", || {
            colour(&out, 4).r == 255 && colour(&out, 0).r == 255
        });

        assert_eq!(
            engine.start_region(R5, "lid", effect("again", RAMP)),
            Ok(())
        );
        wait_for("the lid did not start again from zero", || {
            colour(&out, 0).r < 200
        });
        assert_eq!(colour(&out, 4).r, 255, "the keyboard's clock went back");
        assert_eq!(
            region_status(&engine, "lid").effect_id.as_deref(),
            Some("again")
        );

        let refused = engine.start_region(R5, "lid", effect("broken", "not JavaScript {{{"));
        assert!(matches!(refused, Err(RegionStart::Failed(e)) if e.contains("loading the effect")));
        let lid = region_status(&engine, "lid");
        assert!(lid.running);
        assert_eq!(lid.effect_id.as_deref(), Some("again"), "still what it ran");
        engine.stop(R5);
    }

    /// A region's effect failing stops alone: its lights black, its error kept,
    /// the others going on and the device left on (`regions.md` §7).
    #[test]
    fn a_region_s_effect_failing_stops_alone() {
        let engine = Engine::default();
        let out = Arc::new(Output::default());
        start_regions(
            &engine,
            &out,
            vec![
                ("keyboard", Some(with_params("red", LEVEL, r#"{"r":200}"#))),
                ("lid", Some(effect("broken", THROWS))),
            ],
        );
        wait_for("the failing region never stopped", || {
            !region_status(&engine, "lid").running
        });
        let lid = region_status(&engine, "lid");
        assert!(
            lid.error.as_deref().is_some_and(|e| e.contains("boum")),
            "{lid:?}"
        );
        assert_eq!(
            lid.effect_id.as_deref(),
            Some("broken"),
            "it says what failed"
        );
        assert!(status(&engine, R5).running, "the keyboard runs");
        assert!(!out.turned_off.load(Ordering::Relaxed));
        let before = out.written.load(Ordering::Relaxed);
        wait_for("the other region stopped with it", || {
            out.written.load(Ordering::Relaxed) > before + 2
        });
        assert_eq!(colour(&out, 0), Rgb::default());
        assert_eq!(colour(&out, 4), Rgb::new(200, 0, 0));
        engine.stop(R5);
    }

    /// The device is turned off once no part runs any more, as a whole
    /// device's is.
    #[test]
    fn the_last_region_failing_turns_the_device_off() {
        let engine = Engine::default();
        let out = Arc::new(Output::default());
        start_regions(
            &engine,
            &out,
            vec![
                ("keyboard", Some(effect("broken", THROWS))),
                ("lid", Some(effect("broken", THROWS))),
            ],
        );
        wait_for("the loop never stopped", || !status(&engine, R5).running);
        wait_for("the device was not turned off", || {
            out.turned_off.load(Ordering::Relaxed)
        });
        assert!(!engine.runs_regions(R5));
        assert!(region_status(&engine, "keyboard").error.is_some());
    }

    /// A region stopped goes black and the others go on; with none left the
    /// loop keeps the device dark, and a region started then joins it.
    #[test]
    fn a_region_stopped_goes_black() {
        let engine = Engine::default();
        let out = Arc::new(Output::default());
        start_regions(
            &engine,
            &out,
            vec![
                ("keyboard", Some(with_params("red", LEVEL, r#"{"r":200}"#))),
                ("lid", Some(effect("blue", FILL))),
            ],
        );
        wait_for("never drawn", || colour(&out, 0).b == 255);

        engine.stop_region(R5, "lid");
        wait_for("the lid stayed lit", || colour(&out, 0) == Rgb::default());
        assert_eq!(colour(&out, 4).r, 200, "the keyboard went with it");
        let lid = region_status(&engine, "lid");
        assert!(!lid.running && lid.effect_id.is_none() && lid.error.is_none());

        engine.stop_region(R5, "keyboard");
        wait_for("the keyboard stayed lit", || {
            colour(&out, 4) == Rgb::default()
        });
        assert!(!status(&engine, R5).running, "no region runs anything");
        assert!(engine.runs_regions(R5), "the loop keeps them dark");

        assert_eq!(
            engine.start_region(R5, "power", effect("blue", FILL)),
            Ok(())
        );
        wait_for("the power button never lit", || colour(&out, 3).b == 255);
        assert_eq!(
            colour(&out, 0),
            Rgb::default(),
            "the lid is not the power button's"
        );
        engine.stop(R5);
    }

    /// A loop running one effect over the whole device takes no region: the
    /// caller starts them all.
    #[test]
    fn a_whole_device_takes_no_region() {
        let engine = Engine::default();
        assert_eq!(
            engine.start_region(R5, "lid", effect("blue", FILL)),
            Err(RegionStart::NotRunningRegions),
            "nothing runs"
        );
        engine
            .start(
                R5,
                effect("blue", FILL),
                r5(),
                Box::new(Arc::new(Output::default())),
                true,
            )
            .expect("start");
        assert_eq!(
            engine.start_region(R5, "lid", effect("blue", FILL)),
            Err(RegionStart::NotRunningRegions),
            "a rule's effect over the whole device"
        );
        assert!(!engine.runs_regions(R5));
        let device = status(&engine, R5);
        assert!(device.regions.is_none());
        assert_eq!(device.effect_id.as_deref(), Some("blue"));
        engine.stop(R5);
    }

    /// Parameters and follows reach one region's lights alone; a rule's effect
    /// over a split device is scaled by each region's follow (`regions.md` §4).
    #[test]
    fn parameters_and_follows_are_each_region_s() {
        let engine = Engine::default();
        let now = wall_clock_ms() as i64;
        engine
            .signals()
            .lock()
            .unwrap()
            .apply(&serde_json::json!({ "lux": 50 }), None, now)
            .unwrap();
        let out = Arc::new(Output::default());
        start_regions(
            &engine,
            &out,
            vec![
                (
                    "keyboard",
                    Some(with_params("level", LEVEL, r#"{"r":200}"#)),
                ),
                ("lid", Some(with_params("level", LEVEL, r#"{"r":200}"#))),
            ],
        );
        engine.set_params(R5, Some("lid"), None, r#"{"r":100}"#.into());
        wait_for("the lid's parameters were not set", || {
            colour(&out, 0).r == 100
        });
        assert_eq!(colour(&out, 4).r, 200, "the keyboard's moved too");

        let lux = Some(dimming::Dimming {
            source: "signal:lux".into(),
            floor: 20,
        });
        engine.set_dimming(R5, Some("keyboard"), lux.clone());
        wait_for("the keyboard did not follow the signal", || {
            colour(&out, 4).r == 100
        });
        assert_eq!(colour(&out, 0).r, 100, "the lid followed the keyboard's");

        // A rule over the whole device: the keyboard's follow still scales the
        // keyboard's lights, and the device's own scales nothing.
        engine.set_dimming(R5, None, lux);
        let rule = Arc::new(Output::default());
        engine
            .start(
                R5,
                with_params("rule", LEVEL, r#"{"r":200}"#),
                r5(),
                Box::new(Arc::clone(&rule)),
                true,
            )
            .expect("start");
        wait_for("the rule never drew", || colour(&rule, 0).r == 200);
        assert_eq!(colour(&rule, 4).r, 100);
        engine.stop(R5);
    }

    /// Deleting an effect stops the regions running it, and those alone.
    #[test]
    fn stopping_an_effect_everywhere_stops_the_regions_running_it() {
        let engine = Engine::default();
        let out = Arc::new(Output::default());
        start_regions(
            &engine,
            &out,
            vec![
                ("keyboard", Some(effect("gone", FILL))),
                ("lid", Some(effect("kept", FILL))),
                ("base", Some(effect("gone", FILL))),
            ],
        );
        assert_eq!(engine.stop_everywhere("gone"), vec![R5]);
        assert!(region_status(&engine, "keyboard").effect_id.is_none());
        assert!(region_status(&engine, "base").effect_id.is_none());
        assert!(region_status(&engine, "lid").running);

        engine.rename_everywhere("kept", "renamed");
        assert_eq!(
            region_status(&engine, "lid").effect_id.as_deref(),
            Some("renamed")
        );
        engine.stop(R5);
    }

    /// The preview of a region sees the region alone, frames of the device's
    /// length black outside it, and says which region it borrows.
    #[test]
    fn a_region_s_preview_is_black_outside_it() {
        let engine = Engine::default();
        let frames: Arc<Mutex<Vec<u8>>> = Arc::default();
        let kept = Arc::clone(&frames);
        let channel = Channel::new(move |body| {
            if let InvokeResponseBody::Raw(bytes) = body {
                *kept.lock().unwrap() = bytes;
            }
            Ok(())
        });
        let lid = r5().region("lid").unwrap();
        engine
            .start_preview(R5, Some(lid), effect("blue", FILL), r5())
            .expect("preview start");
        engine.set_preview_channel(Some(channel));
        wait_for("no preview frame", || !frames.lock().unwrap().is_empty());

        let frame = frames.lock().unwrap().clone();
        assert_eq!(frame.len(), r5().led_count() * 3, "the device's length");
        for position in 0..r5().led_count() {
            let lit = frame[position * 3 + 2] == 255;
            assert_eq!(
                lit,
                lid.positions.contains(&(position as u16)),
                "{position}"
            );
        }
        let preview = engine.preview_status().unwrap();
        assert_eq!(
            (preview.layout_of, preview.region.as_deref()),
            (R5, Some("lid"))
        );
        assert!(engine.device_status().is_empty());
        engine.stop_preview();
    }

    // The device's frame from its parts', as a pure function, is tested with
    // the blend modes in `composition.rs`.

    // ------------------------------------------------------------ layers

    use composition::{Blend, Mask, MaskError};

    /// Every light of its layout in `params.r`, `params.g` and `params.b`.
    const SOLID: &str = r#"export default {
        render({ layout, frame, params }) {
            const colour = { r: Number(params.r ?? 0), g: Number(params.g ?? 0), b: Number(params.b ?? 0) }
            for (const key of layout.keys) frame.set(key, colour)
        },
    }"#;

    /// Every light of its layout in a red as bright as it has lights: what a
    /// layer's effect counts as its device.
    const COUNT: &str = r#"export default {
        render({ layout, frame }) {
            for (const key of layout.keys) frame.set(key, { r: layout.keys.length, g: 0, b: 0 })
        },
    }"#;

    /// White everywhere: what no stack below draws, to tell a preview's first
    /// frames from those of the one it replaced.
    const WHITE: &str =
        "export default { render({ frame }) { frame.fill({ r: 255, g: 255, b: 255 }) } }";

    fn solid(effect_id: &str, [r, g, b]: [u8; 3]) -> EffectStart {
        with_params(effect_id, SOLID, &format!(r#"{{"r":{r},"g":{g},"b":{b}}}"#))
    }

    fn mask_group(id: &str) -> Mask {
        Mask {
            group: Some(id.into()),
            ..Mask::default()
        }
    }

    fn mask_keys(codes: &[&str]) -> Mask {
        Mask {
            keys: codes.iter().map(|c| (*c).to_owned()).collect(),
            ..Mask::default()
        }
    }

    fn mask_lights(addresses: &[u16]) -> Mask {
        Mask {
            lights: addresses.to_vec(),
            ..Mask::default()
        }
    }

    fn mask_region(id: &str) -> Mask {
        Mask {
            region: Some(id.into()),
            ..Mask::default()
        }
    }

    fn layer_of(id: &str, start: EffectStart, mask: Mask, blend: Blend, opacity: u8) -> LayerStart {
        LayerStart {
            id: id.into(),
            start,
            mask,
            look: Look {
                blend,
                opacity,
                follows: None,
            },
        }
    }

    /// The position of the DeathStalker's key sending `scancode`.
    fn key_at(scancode: u16) -> usize {
        let key = layout().keys.iter().find(|k| k.scancode == scancode);
        usize::from(key.expect("a key of the DeathStalker").index)
    }

    fn start_stack(
        engine: &Engine,
        device: DeviceRef,
        l: &'static Layout,
        out: &Arc<Output>,
        stack: StackStart,
    ) -> StackVerdict {
        engine
            .start_stack(device, l, Box::new(Arc::clone(out)), stack, false)
            .expect("within the limits")
    }

    fn layer_ids(engine: &Engine, device: DeviceRef) -> Vec<String> {
        status(engine, device)
            .layers
            .into_iter()
            .map(|l| l.layer)
            .collect()
    }

    /// The two-layer fixture of `composition.md` §3.11 on a per-key keyboard:
    /// a base; a layer screened on the number row; another covering half-way
    /// the two keys left of it and Escape. Then the two in the other order, and
    /// the top one made invisible, live.
    #[test]
    fn two_layers_compose_in_order_on_a_per_key_keyboard() {
        let engine = Engine::default();
        let out = Arc::new(Output::default());
        let verdict = start_stack(
            &engine,
            FIRST,
            layout(),
            &out,
            StackStart {
                base: Some(solid("base", [100, 50, 0])),
                layers: vec![
                    layer_of(
                        "a3",
                        solid("blue", [0, 0, 200]),
                        mask_group("numberRow"),
                        Blend::Screen,
                        100,
                    ),
                    layer_of(
                        "b7",
                        solid("red", [255, 0, 0]),
                        mask_keys(&["29", "02", "01"]),
                        Blend::Cover,
                        50,
                    ),
                ],
            },
        );
        assert_eq!(
            verdict,
            StackVerdict {
                base: Ok(()),
                layers: vec![("a3".into(), Ok(())), ("b7".into(), Ok(()))],
            }
        );
        let (a, two, one, escape) = (key_at(0x1E), key_at(0x03), key_at(0x02), key_at(0x01));
        // 1: screened to (100, 50, 200), then half-way to red, rounded once.
        wait_for("the stack was never drawn", || {
            colour(&out, one) == Rgb::new(178, 25, 100)
        });
        assert_eq!(colour(&out, a), Rgb::new(100, 50, 0), "under no layer");
        assert_eq!(colour(&out, two), Rgb::new(100, 50, 200), "screened");
        assert_eq!(
            colour(&out, escape),
            Rgb::new(178, 25, 0),
            "covered half-way"
        );
        assert_eq!(layer_ids(&engine, FIRST), ["a3", "b7"], "bottom to top");

        assert_eq!(
            engine.order_layers(FIRST, None, vec!["b7".into(), "a3".into()]),
            Ok(())
        );
        // 1: half-way to red first, (177.5, 25, 0), then screened.
        wait_for("the order did not change", || {
            colour(&out, one) == Rgb::new(178, 25, 200)
        });
        assert_eq!(colour(&out, escape), Rgb::new(178, 25, 0));
        assert_eq!(layer_ids(&engine, FIRST), ["b7", "a3"]);

        engine.set_look(
            FIRST,
            None,
            "b7",
            Look {
                blend: Blend::Cover,
                opacity: 0,
                follows: None,
            },
        );
        wait_for("the opacity was not set live", || {
            colour(&out, escape) == Rgb::new(100, 50, 0)
        });
        assert_eq!(colour(&out, one), Rgb::new(100, 50, 200));
        engine.stop(FIRST);
    }

    /// A layer's effect sees its mask as its device: four lights on WASD.
    /// Without a base, it is drawn over black.
    #[test]
    fn a_layer_s_effect_sees_its_mask_as_its_device() {
        let engine = Engine::default();
        let out = Arc::new(Output::default());
        start_stack(
            &engine,
            FIRST,
            layout(),
            &out,
            StackStart {
                base: None,
                layers: vec![layer_of(
                    "w",
                    effect("count", COUNT),
                    mask_group("wasd"),
                    Blend::Cover,
                    100,
                )],
            },
        );
        wait_for("the layer never drew", || colour(&out, key_at(0x11)).r == 4);
        assert_eq!(colour(&out, key_at(0x20)), Rgb::new(4, 0, 0));
        assert_eq!(
            colour(&out, key_at(0x01)),
            Rgb::default(),
            "black around it"
        );
        assert_eq!(status(&engine, FIRST).effect_id, None, "no base");
        engine.stop(FIRST);
    }

    /// The fixture on a zones device. The 17 R5 split: the Lid region's stack
    /// with a layer added on its logo, and a Keyboard layer masked to the lid,
    /// which lights nothing there. Then the 17 R5 whole, with a layer on its
    /// Lid region only.
    #[test]
    fn two_layers_compose_on_a_zones_device() {
        let engine = Engine::default();
        let out = Arc::new(Output::default());
        let lid = r5().region("lid").unwrap();
        let keyboard = r5().region("keyboard").unwrap();
        let logo = 1u16;
        let verdicts = engine
            .start_regions(
                R5,
                r5(),
                Box::new(Arc::clone(&out)),
                vec![
                    (
                        lid,
                        StackStart {
                            base: Some(solid("blue", [0, 0, 100])),
                            layers: vec![layer_of(
                                "t",
                                solid("amber", [200, 100, 0]),
                                mask_lights(&[logo]),
                                Blend::Add,
                                100,
                            )],
                        },
                    ),
                    (
                        keyboard,
                        StackStart {
                            base: Some(solid("dim", [50, 0, 0])),
                            layers: vec![layer_of(
                                "k",
                                solid("blue", [0, 0, 255]),
                                mask_region("lid"),
                                Blend::Cover,
                                100,
                            )],
                        },
                    ),
                ],
            )
            .expect("within the limits");
        let ids: Vec<&str> = verdicts.iter().map(|(r, _)| *r).collect();
        assert_eq!(
            ids,
            ["keyboard", "lid", "base", "power"],
            "the definition's order"
        );
        assert_eq!(
            verdicts[0].1.layers,
            [(
                "k".to_owned(),
                Err(PartError::Refused(Refusal::Mask(MaskError::NothingToLight)))
            )]
        );
        assert_eq!(verdicts[1].1.layers, [("t".to_owned(), Ok(()))]);
        // Positions and addresses alike on the 17 R5.
        wait_for("the logo never lit", || {
            colour(&out, usize::from(logo)) == Rgb::new(200, 100, 100)
        });
        for position in [0, 10, 11] {
            assert_eq!(
                colour(&out, position),
                Rgb::new(0, 0, 100),
                "lid {position}"
            );
        }
        for position in [4, 5, 6, 7, 12] {
            assert_eq!(
                colour(&out, position),
                Rgb::new(50, 0, 0),
                "keyboard {position}"
            );
        }
        assert_eq!(region_status(&engine, "lid").layers.len(), 1);
        assert!(
            region_status(&engine, "keyboard").layers.is_empty(),
            "a mask lighting nothing takes no row"
        );

        let whole = Arc::new(Output::default());
        start_stack(
            &engine,
            R5,
            r5(),
            &whole,
            StackStart {
                base: Some(solid("grey", [100, 100, 100])),
                layers: vec![layer_of(
                    "red lid",
                    solid("red", [255, 0, 0]),
                    mask_region("lid"),
                    Blend::Multiply,
                    100,
                )],
            },
        );
        wait_for("the lid was never tinted", || {
            colour(&whole, 0) == Rgb::new(100, 0, 0)
        });
        for position in 0..r5().led_count() {
            let want = if lid.positions.contains(&(position as u16)) {
                Rgb::new(100, 0, 0)
            } else {
                Rgb::new(100, 100, 100)
            };
            assert_eq!(colour(&whole, position), want, "{position}");
        }
        engine.stop(R5);
    }

    /// The stack both loops run in
    /// [`the_preview_and_the_device_give_the_same_frames`]: everything moves
    /// from frame to frame.
    fn moving_stack() -> StackStart {
        StackStart {
            base: Some(with_params("hues", EFFECT, r#"{"speed":90}"#)),
            layers: vec![
                layer_of(
                    "w",
                    effect("ramp", RAMP),
                    mask_group("wasd"),
                    Blend::Screen,
                    70,
                ),
                layer_of(
                    "f",
                    with_params("hues", EFFECT, r#"{"speed":300}"#),
                    mask_group("functionRow"),
                    Blend::Multiply,
                    100,
                ),
            ],
        }
    }

    /// `composition.md` §3.9: the same stack, given the same instants, renders
    /// the same bytes in a preview loop and in a device loop. The clocks are
    /// injected: the machine's would start each loop's parts a few
    /// milliseconds apart.
    #[test]
    fn the_preview_and_the_device_give_the_same_frames() {
        const FRAMES: usize = 8;
        let mut engine = Engine::default();
        engine.clock = Clock::Frames {
            epoch_ms: 1_800_000_000_000.0,
        };
        let out = Arc::new(Output::default());
        start_stack(&engine, FIRST, layout(), &out, moving_stack());

        let previewed: Arc<Mutex<Vec<Vec<u8>>>> = Arc::default();
        let kept = Arc::clone(&previewed);
        let channel = Channel::new(move |body| {
            if let InvokeResponseBody::Raw(bytes) = body {
                kept.lock().unwrap().push(bytes);
            }
            Ok(())
        });
        // The channel passes to the next preview with the loop it replaces,
        // before that one's first frame.
        engine
            .start_preview(FIRST, None, effect("white", WHITE), layout())
            .expect("preview start");
        engine.set_preview_channel(Some(channel));
        let verdict = engine
            .start_preview_stack(FIRST, None, moving_stack(), layout())
            .expect("within the limits");
        assert!(verdict.base.is_ok() && verdict.layers.iter().all(|(_, v)| v.is_ok()));
        let stack_frames = || -> Vec<Vec<u8>> {
            let all = previewed.lock().unwrap();
            all.iter()
                .filter(|f| f.iter().any(|&b| b != 255))
                .cloned()
                .collect()
        };
        wait_for("too few frames", || {
            stack_frames().len() >= FRAMES && out.first.lock().unwrap().len() >= FRAMES
        });

        let device: Vec<Vec<u8>> = out.first.lock().unwrap()[..FRAMES]
            .iter()
            .map(|f| f.iter().flat_map(|c| [c.r, c.g, c.b]).collect())
            .collect();
        assert_ne!(device[0], device[FRAMES - 1], "the frames move");
        assert_eq!(
            stack_frames()[..FRAMES],
            device[..],
            "the same instants give the same bytes"
        );
        let layers: Vec<String> = engine
            .preview_status()
            .unwrap()
            .layers
            .into_iter()
            .map(|l| l.layer)
            .collect();
        assert_eq!(layers, ["w", "f"]);
        engine.stop_preview();
        engine.stop(FIRST);
    }

    /// The preview's layers take their look and their settings live, as a
    /// device's do, and nothing reaches a device.
    #[test]
    fn the_preview_s_layers_change_live() {
        let engine = Engine::default();
        let last: Arc<Mutex<Vec<u8>>> = Arc::default();
        let kept = Arc::clone(&last);
        let channel = Channel::new(move |body| {
            if let InvokeResponseBody::Raw(bytes) = body {
                *kept.lock().unwrap() = bytes;
            }
            Ok(())
        });
        let stack = StackStart {
            base: Some(solid("base", [100, 0, 0])),
            layers: vec![layer_of(
                "e",
                solid("blue", [0, 0, 200]),
                mask_group("escape"),
                Blend::Cover,
                100,
            )],
        };
        engine
            .start_preview_stack(FIRST, None, stack, layout())
            .expect("within the limits");
        engine.set_preview_channel(Some(channel));
        let escape = key_at(0x01) * 3;
        let at = |i: usize| last.lock().unwrap().get(i..i + 3).map(<[u8]>::to_vec);
        wait_for("the preview never drew its layer", || {
            at(escape) == Some(vec![0, 0, 200])
        });
        engine.set_preview_params(Some("e"), r#"{"g":200}"#.into());
        wait_for("the layer's parameters were not set", || {
            at(escape) == Some(vec![0, 200, 0])
        });
        engine.set_preview_look(
            "e",
            Look {
                blend: Blend::Cover,
                opacity: 0,
                follows: None,
            },
        );
        wait_for("the layer's look was not set", || {
            at(escape) == Some(vec![100, 0, 0])
        });
        assert!(engine.device_status().is_empty(), "no device touched");
        engine.stop_preview();
    }

    /// A layer failing thirty frames in a row stops alone: it draws nothing
    /// more, its row says why, the base goes on and the device stays on.
    #[test]
    fn a_layer_failing_stops_alone() {
        let engine = Engine::default();
        let out = Arc::new(Output::default());
        start_stack(
            &engine,
            FIRST,
            layout(),
            &out,
            StackStart {
                base: Some(solid("base", [100, 0, 0])),
                layers: vec![layer_of(
                    "x",
                    effect("broken", THROWS),
                    mask_group("wasd"),
                    Blend::Cover,
                    100,
                )],
            },
        );
        wait_for("the failing layer never stopped", || {
            status(&engine, FIRST)
                .layers
                .first()
                .is_some_and(|l| !l.running)
        });
        let x = status(&engine, FIRST).layers.remove(0);
        assert!(
            x.error.as_deref().is_some_and(|e| e.contains("boum")),
            "{x:?}"
        );
        assert_eq!(
            x.effect_id.as_deref(),
            Some("broken"),
            "it says what failed"
        );
        let device = status(&engine, FIRST);
        assert!(device.running);
        assert_eq!(device.error, None, "the base's own");
        assert!(!out.turned_off.load(Ordering::Relaxed));
        let before = out.written.load(Ordering::Relaxed);
        wait_for("the base stopped with it", || {
            out.written.load(Ordering::Relaxed) > before + 2
        });
        assert_eq!(
            colour(&out, key_at(0x11)),
            Rgb::new(100, 0, 0),
            "nothing drawn"
        );
        engine.stop(FIRST);
    }

    /// A base failing stops alone too: its layers go on, over black.
    #[test]
    fn a_base_failing_leaves_its_layers() {
        let engine = Engine::default();
        let out = Arc::new(Output::default());
        start_stack(
            &engine,
            FIRST,
            layout(),
            &out,
            StackStart {
                base: Some(effect("broken", THROWS)),
                layers: vec![layer_of(
                    "w",
                    solid("blue", [0, 0, 200]),
                    mask_group("wasd"),
                    Blend::Cover,
                    100,
                )],
            },
        );
        let base = || {
            engine
                .shared(FIRST)
                .and_then(|s| s.part(None, None))
                .unwrap()
        };
        wait_for("the failing base never stopped", || !base().running());
        assert!(base().error().is_some_and(|e| e.contains("boum")));
        let before = out.written.load(Ordering::Relaxed);
        wait_for("the layer stopped with it", || {
            out.written.load(Ordering::Relaxed) > before + 2
        });
        assert_eq!(colour(&out, key_at(0x11)), Rgb::new(0, 0, 200));
        assert_eq!(colour(&out, key_at(0x01)), Rgb::default());
        assert!(status(&engine, FIRST).running);
        assert!(!out.turned_off.load(Ordering::Relaxed));
        engine.stop(FIRST);
    }

    /// A layer that does not load keeps its row, with why, and takes its
    /// place again when started under its id; one whose mask lights nothing on
    /// this keyboard takes none, and its verdict says so.
    #[test]
    fn a_layer_that_does_not_start_says_why() {
        let engine = Engine::default();
        let out = Arc::new(Output::default());
        let verdict = start_stack(
            &engine,
            FIRST,
            layout(),
            &out,
            StackStart {
                base: Some(solid("base", [0, 100, 0])),
                layers: vec![
                    layer_of(
                        "bad",
                        effect("bad", "not JavaScript {{{"),
                        Mask::default(),
                        Blend::Cover,
                        100,
                    ),
                    // No right Windows key on the DeathStalker.
                    layer_of(
                        "win",
                        solid("red", [255, 0, 0]),
                        mask_keys(&["e05c"]),
                        Blend::Cover,
                        100,
                    ),
                ],
            },
        );
        assert!(
            matches!(&verdict.layers[0], (id, Err(PartError::Failed(e))) if id == "bad" && e.contains("loading the effect")),
            "{verdict:?}"
        );
        assert_eq!(
            verdict.layers[1],
            (
                "win".to_owned(),
                Err(PartError::Refused(Refusal::Mask(MaskError::NothingToLight)))
            )
        );
        let rows = status(&engine, FIRST).layers;
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert!(rows[0].layer == "bad" && !rows[0].running && rows[0].error.is_some());
        wait_for("the base never drew", || {
            colour(&out, key_at(0x01)) == Rgb::new(0, 100, 0)
        });

        let again = layer_of(
            "bad",
            solid("blue", [0, 0, 200]),
            mask_group("escape"),
            Blend::Cover,
            100,
        );
        assert_eq!(engine.start_layer(FIRST, None, again), Ok(()));
        wait_for("the layer never drew", || {
            colour(&out, key_at(0x01)) == Rgb::new(0, 0, 200)
        });
        let rows = status(&engine, FIRST).layers;
        assert!(rows.len() == 1 && rows[0].running && rows[0].error.is_none());
        engine.stop(FIRST);
    }

    /// Eight effects per target, the base included: a ninth is refused,
    /// whether in a stack or added live, and what ran goes on. Replacing a
    /// layer is no ninth.
    #[test]
    fn the_limits_are_refused() {
        let engine = Engine::default();
        let out = Arc::new(Output::default());
        let dot = |id: &str| {
            layer_of(
                id,
                solid("dot", [0, 0, 1]),
                mask_group("escape"),
                Blend::Add,
                100,
            )
        };
        let layers = |n: usize| (0..n).map(|i| dot(&format!("l{i}"))).collect::<Vec<_>>();
        start(&engine, FIRST, "applied", Arc::clone(&out));
        let nine = StackStart {
            base: Some(solid("base", [1, 1, 1])),
            layers: layers(8),
        };
        let refused = engine.start_stack(FIRST, layout(), Box::new(Arc::clone(&out)), nine, false);
        assert_eq!(refused, Err(Refusal::TooManyEffects));
        let twice = StackStart {
            base: None,
            layers: vec![dot("a"), dot("a")],
        };
        let refused = engine.start_stack(FIRST, layout(), Box::new(Arc::clone(&out)), twice, false);
        assert_eq!(refused, Err(Refusal::LayerTwice("a".into())));
        let device = status(&engine, FIRST);
        assert!(device.running);
        assert_eq!(
            device.effect_id.as_deref(),
            Some("applied"),
            "what ran goes on"
        );

        start_stack(
            &engine,
            FIRST,
            layout(),
            &out,
            StackStart {
                base: Some(solid("base", [1, 1, 1])),
                layers: layers(7),
            },
        );
        assert_eq!(layer_ids(&engine, FIRST).len(), 7);
        assert_eq!(
            engine.start_layer(FIRST, None, dot("ninth")),
            Err(PartError::Refused(Refusal::TooManyEffects))
        );
        assert_eq!(
            engine.start_layer(FIRST, None, dot("l3")),
            Ok(()),
            "a replacement"
        );
        engine.stop_base(FIRST, None);
        assert_eq!(
            engine.start_layer(FIRST, None, dot("eighth")),
            Ok(()),
            "eight without a base"
        );
        assert_eq!(
            engine.start_base(FIRST, None, solid("base", [1, 1, 1])),
            Err(PartError::Refused(Refusal::TooManyEffects))
        );
        assert_eq!(
            engine.order_layers(FIRST, None, vec!["l0".into()]),
            Err(PartError::Refused(Refusal::Order))
        );
        assert_eq!(layer_ids(&engine, FIRST).len(), 8);
        engine.stop(FIRST);
    }

    /// Adding a layer or changing the base happens between two frames: the
    /// other parts keep their clocks.
    #[test]
    fn a_layer_or_a_base_started_leaves_the_others_clocks() {
        let engine = Engine::default();
        let out = Arc::new(Output::default());
        let (w, escape) = (key_at(0x11), key_at(0x01));
        start_stack(
            &engine,
            FIRST,
            layout(),
            &out,
            StackStart::base(Some(effect("ramp", RAMP))),
        );
        wait_for("the base's clock never reached 0.64 s", || {
            colour(&out, escape).r == 255
        });
        let ramp = layer_of(
            "w",
            effect("ramp", RAMP),
            mask_group("wasd"),
            Blend::Cover,
            100,
        );
        assert_eq!(engine.start_layer(FIRST, None, ramp), Ok(()));
        wait_for("the layer did not start from zero", || {
            colour(&out, w).r < 200
        });
        assert_eq!(colour(&out, escape).r, 255, "the base's clock went back");
        wait_for("the layer's clock never reached 0.64 s", || {
            colour(&out, w).r == 255
        });

        assert_eq!(
            engine.start_base(FIRST, None, effect("again", RAMP)),
            Ok(())
        );
        wait_for("the base did not start again from zero", || {
            colour(&out, escape).r < 200
        });
        assert_eq!(colour(&out, w).r, 255, "the layer's clock went back");
        assert_eq!(status(&engine, FIRST).effect_id.as_deref(), Some("again"));

        engine.stop_layer(FIRST, None, "w");
        assert!(layer_ids(&engine, FIRST).is_empty());
        wait_for("the layer stayed drawn", || {
            let last = out.last.lock().unwrap();
            last[w] == last[escape]
        });
        engine.stop(FIRST);
    }

    /// A layer goes to a stack the loop runs, or the caller starts the whole
    /// composition: nothing running, a split device asked for its whole
    /// stack, a region it does not declare, a rule over the whole device. A
    /// mask lighting nothing changes nothing.
    #[test]
    fn a_layer_goes_to_a_stack_the_loop_runs() {
        let engine = Engine::default();
        let fill = || {
            layer_of(
                "x",
                effect("blue", FILL),
                Mask::default(),
                Blend::Cover,
                100,
            )
        };
        assert_eq!(
            engine.start_layer(R5, None, fill()),
            Err(PartError::NotRunning),
            "nothing runs"
        );
        let out = Arc::new(Output::default());
        start_regions(&engine, &out, vec![("lid", Some(effect("blue", FILL)))]);
        assert_eq!(
            engine.start_layer(R5, None, fill()),
            Err(PartError::NotRunning)
        );
        assert_eq!(
            engine.start_layer(R5, Some("screen"), fill()),
            Err(PartError::NotRunning)
        );
        let wasd = layer_of(
            "x",
            effect("blue", FILL),
            mask_group("wasd"),
            Blend::Cover,
            100,
        );
        assert_eq!(
            engine.start_layer(R5, Some("lid"), wasd),
            Err(PartError::Refused(Refusal::Mask(MaskError::NothingToLight)))
        );
        assert_eq!(
            engine.start_layer(R5, Some("base"), fill()),
            Ok(()),
            "a region running nothing takes a layer, over black"
        );
        wait_for("the base region never lit", || colour(&out, 8).b == 255);
        assert_eq!(colour(&out, 4), Rgb::default(), "the keyboard runs nothing");
        assert_eq!(region_status(&engine, "base").layers.len(), 1);

        engine
            .start(
                R5,
                effect("rule", FILL),
                r5(),
                Box::new(Arc::clone(&out)),
                true,
            )
            .expect("start");
        assert_eq!(
            engine.start_layer(R5, Some("lid"), fill()),
            Err(PartError::NotRunning),
            "a rule over the whole device"
        );
        engine.stop(R5);
    }

    /// A layer's opacity follows a signal as a brightness does, and its
    /// settings are its own.
    #[test]
    fn a_layer_s_opacity_follows_and_its_settings_are_its_own() {
        let engine = Engine::default();
        let now = wall_clock_ms() as i64;
        engine
            .signals()
            .lock()
            .unwrap()
            .apply(&serde_json::json!({ "lux": 50 }), None, now)
            .unwrap();
        let out = Arc::new(Output::default());
        let following = LayerStart {
            look: Look {
                blend: Blend::Cover,
                opacity: 100,
                follows: Some(dimming::Dimming {
                    source: "signal:lux".into(),
                    floor: 0,
                }),
            },
            ..layer_of(
                "half",
                solid("red", [200, 0, 0]),
                mask_group("escape"),
                Blend::Cover,
                100,
            )
        };
        start_stack(
            &engine,
            FIRST,
            layout(),
            &out,
            StackStart {
                base: Some(solid("black", [0, 0, 0])),
                layers: vec![
                    following,
                    layer_of(
                        "space",
                        solid("red", [200, 0, 0]),
                        mask_group("space"),
                        Blend::Cover,
                        100,
                    ),
                ],
            },
        );
        wait_for("the opacity did not follow the signal", || {
            colour(&out, key_at(0x01)).r == 100
        });
        engine.set_params(FIRST, None, Some("space"), r#"{"b":40}"#.into());
        wait_for("the layer's parameters were not set", || {
            colour(&out, key_at(0x39)) == Rgb::new(0, 0, 40)
        });
        assert_eq!(colour(&out, key_at(0x01)).r, 100, "the other layer's moved");
        engine.stop(FIRST);
    }

    /// Deleting an effect stops the layers running it, and those alone; a
    /// whole device running nothing else stops, as before layers.
    #[test]
    fn deleting_an_effect_stops_the_layers_running_it() {
        let engine = Engine::default();
        let out = Arc::new(Output::default());
        let gone = |id: &str| {
            layer_of(
                id,
                effect("gone", FILL),
                mask_group("wasd"),
                Blend::Cover,
                100,
            )
        };
        start_stack(
            &engine,
            FIRST,
            layout(),
            &out,
            StackStart {
                base: Some(solid("kept", [1, 1, 1])),
                layers: vec![gone("g")],
            },
        );
        assert_eq!(engine.stop_everywhere("gone"), vec![FIRST]);
        assert!(layer_ids(&engine, FIRST).is_empty());
        let device = status(&engine, FIRST);
        assert!(device.running);
        assert_eq!(device.effect_id.as_deref(), Some("kept"));

        start_stack(
            &engine,
            FIRST,
            layout(),
            &out,
            StackStart {
                base: Some(effect("gone", FILL)),
                layers: vec![gone("g")],
            },
        );
        assert_eq!(engine.stop_everywhere("gone"), vec![FIRST]);
        assert!(!status(&engine, FIRST).running);
    }

    // ------------------------------------------------------------ preview

    fn preview_effect(engine: &Engine, layout_of: DeviceRef, effect_id: &str) {
        engine
            .start_preview(layout_of, None, effect(effect_id, EFFECT), layout())
            .expect("preview start");
    }

    /// **The heart of issue #63.** Previewing Y while X runs on the keyboard
    /// must stop nothing: otherwise browsing the gallery would turn off the
    /// current lighting, and it would only show once shipped.
    #[test]
    fn the_preview_does_not_interrupt_the_device_effect() {
        let engine = Engine::default();
        let output = Arc::new(Output::default());
        start(&engine, FIRST, "applique", Arc::clone(&output));

        // Three previews in a row, like browsing the gallery.
        for effect in ["regarde-1", "regarde-2", "regarde-3"] {
            preview_effect(&engine, FIRST, effect);
        }

        let device = status(&engine, FIRST);
        assert!(device.running, "the preview stopped the device's loop");
        assert_eq!(device.effect_id.as_deref(), Some("applique"));

        // And frames keep going out to the keyboard during the preview.
        let before = output.written.load(Ordering::Relaxed);
        wait_for("the keyboard is no longer fed", || {
            output.written.load(Ordering::Relaxed) > before + 2
        });

        engine.stop_all();
    }

    /// **Not a single byte goes to a keyboard from a preview.** That is what
    /// [`NoOutput`] guarantees, and it is the only guarantee that matters: an
    /// output opened by accident would turn the preview into an application.
    #[test]
    fn the_preview_writes_to_no_device() {
        let engine = Engine::default();
        let output = Arc::new(Output::default());
        start(&engine, FIRST, "applique", Arc::clone(&output));
        engine.stop(FIRST);

        let frozen = output.written.load(Ordering::Relaxed);
        preview_effect(&engine, FIRST, "regarde");
        wait_for("the preview rendered no frame", || {
            engine
                .preview_status()
                .is_some_and(|p| p.running && p.error.is_none())
        });
        // A few frames go by while the test sleeps.
        std::thread::sleep(Duration::from_millis(120));

        assert_eq!(
            output.written.load(Ordering::Relaxed),
            frozen,
            "the preview wrote to the device's output"
        );
        engine.stop_all();
    }

    /// The preview is not mistaken for any device: it adds no line to the list
    /// that the tray icon and the diagnostic read.
    #[test]
    fn the_preview_is_not_in_the_device_list() {
        let engine = Engine::default();
        preview_effect(&engine, FIRST, "regarde");

        assert!(
            engine.device_status().is_empty(),
            "the preview slipped into the device list"
        );

        let report = engine.report();
        assert!(report.devices.is_empty());
        let preview = report.preview.expect("no preview");
        assert_eq!(preview.effect_id.as_deref(), Some("regarde"));
        assert_eq!(
            preview.layout_of, FIRST,
            "the borrowed layout is not the one requested"
        );

        engine.stop_preview();
        assert!(
            engine.preview_status().is_none(),
            "a stopped preview is still announced"
        );
    }

    /// Deleting an effect also stops the **preview** running it: the
    /// JavaScript is loaded into memory, and the screen would keep animating an
    /// effect missing from the library. And it does not appear in the list of
    /// stopped devices — no device was touched.
    #[test]
    fn deleting_an_effect_also_stops_its_preview() {
        let engine = Engine::default();
        start(&engine, FIRST, "autre", Arc::new(Output::default()));
        preview_effect(&engine, FIRST, "a-supprimer");

        assert!(engine.stop_everywhere("a-supprimer").is_empty());
        assert!(engine.preview_status().is_none());
        assert!(
            status(&engine, FIRST).running,
            "the device's loop was stopped along the way"
        );

        engine.stop_all();
    }

    /// Two selections in a row: the second **replaces** the first, it does not
    /// add to it. There is only one preview loop, hence only one QuickJS context
    /// at a time.
    #[test]
    fn changing_the_selection_replaces_the_preview() {
        let engine = Engine::default();
        preview_effect(&engine, FIRST, "premier-regarde");
        preview_effect(&engine, SECOND, "second-regarde");

        let preview = engine.preview_status().expect("no preview");
        assert_eq!(preview.effect_id.as_deref(), Some("second-regarde"));
        assert_eq!(preview.layout_of, SECOND);
        assert!(engine.device_status().is_empty());

        engine.stop_preview();
    }

    // ------------------------------------------------------------ key presses

    /// Lights every key pressed, white.
    const READS_KEYS: &str = r#"
        export default {
          inputs: ['keys'],
          render({ presses, frame }) {
            for (const { key } of presses) frame.set(key, { r: 255, g: 255, b: 255 })
          },
        }
    "#;

    /// An engine whose presses come from the test, not from the keyboard.
    fn engine_with(presses: &Arc<Presses>) -> Engine {
        let mut engine = Engine::default();
        engine.presses = Arc::clone(presses);
        engine
    }

    fn lit(output: &Output, index: u16) -> bool {
        output
            .last
            .lock()
            .unwrap()
            .get(usize::from(index))
            .is_some_and(|c| c.r == 255)
    }

    /// A press lights its key on the keyboard it was typed on, and on no other:
    /// 0x11 is LED 46, 0x1E is LED 67.
    #[test]
    fn a_device_sees_the_presses_of_its_own_keyboard() {
        let presses = Presses::manual();
        let engine = engine_with(&presses);
        let output = Arc::new(Output::default());
        start_js(&engine, FIRST, READS_KEYS, Arc::clone(&output)).expect("start");
        wait_for("the loop never read presses", || presses.readers() == 1);

        presses.push(presses::Press {
            at: Instant::now(),
            scancode: 0x11,
            device: Some((FIRST.vid, FIRST.pid)),
        });
        presses.push(presses::Press {
            at: Instant::now(),
            scancode: 0x1E,
            device: Some((SECOND.vid, SECOND.pid)),
        });

        wait_for("the pressed key never lit", || lit(&output, 46));
        assert!(
            !lit(&output, 67),
            "a press from another keyboard lit this one"
        );
        engine.stop(FIRST);
    }

    /// An effect writing what the clock says onto the first key, so a test can
    /// read the fields back out of a frame.
    const CLOCK_EFFECT: &str = "export default {
        inputs: ['clock'],
        render({ layout, clock, frame }) {
            frame.set(layout.keys[0], {
                r: clock.seconds,
                g: clock.hours,
                b: Math.floor(clock.ms / 4),
            })
        },
    }";

    /// The instant the host hands over reaches the effect.
    ///
    /// Asserted on **seconds and milliseconds**: hours and minutes are local, so
    /// this machine and a runner in another time zone would not agree on them —
    /// seconds are the same everywhere, offsets of half an hour included.
    #[test]
    fn an_effect_declaring_the_clock_is_given_the_instant_the_host_reads() {
        let (_rt, ctx) = prepare(CLOCK_EFFECT, layout()).expect("load");
        let len = layout().led_count();

        // 1 600 000 007 250 ms since the epoch: 47 seconds and 250 ms past a
        // minute, wherever the machine stands.
        let frame =
            render_with_inputs(&ctx, 0.0, 0, "{}", "", 1_600_000_007_250.0, len).expect("render");

        assert_eq!(frame[0], 47, "seconds");
        assert_eq!(frame[2], 250 / 4, "milliseconds, in quarters");
        assert!(frame[1] <= 23, "hours of a local day, got {}", frame[1]);
    }

    /// An effect that asks for nothing is given a zeroed clock, whatever the host
    /// passes — the rule that also makes a swatch show an effect at rest.
    #[test]
    fn an_effect_declaring_nothing_sees_no_clock() {
        const SILENT: &str = "export default {
            render({ layout, clock, frame }) {
                frame.set(layout.keys[0], { r: clock.seconds, g: clock.hours, b: clock.ms })
            },
        }";
        let (_rt, ctx) = prepare(SILENT, layout()).expect("load");
        let len = layout().led_count();

        let frame =
            render_with_inputs(&ctx, 0.0, 0, "{}", "", 1_600_000_007_250.0, len).expect("render");

        assert_eq!(&frame[0..3], &[0, 0, 0], "a clock nobody asked for");
    }

    /// The shipped Clock draws the time as a banner crossing the keyboard: over a
    /// lap it lights a digit's worth of keys, never on the bottom row, and what
    /// it lights moves.
    ///
    /// Asserted on shapes, not on which digits: the hour is local, so the text
    /// depends on the time zone the test runs in.
    #[test]
    fn the_clock_effect_scrolls_the_time_across_the_keyboard() {
        let (_rt, ctx) = prepare(crate::shipped::source("Clock"), layout()).expect("load");
        let len = layout().led_count();
        const BACKGROUND: [u8; 3] = [4, 6, 12];

        // LED indices, not ranks in `keys`: a frame covers the 132 matrix
        // positions, of which 106 carry a key.
        let lit_at = |time: f64| -> Vec<usize> {
            let frame = render_with_inputs(&ctx, time, 0, "{}", "", 1_600_000_007_250.0, len)
                .expect("render");
            layout()
                .keys
                .iter()
                .map(|k| k.index as usize)
                .filter(|&i| frame[i * 3..i * 3 + 3] != BACKGROUND)
                .collect()
        };
        // The last row of the matrix, the space bar's, holes left out.
        let cols = usize::from(layout().cols);
        let bottom_row: Vec<usize> = layout().matrix[layout().matrix.len() - cols..]
            .iter()
            .filter(|&&index| index != u16::MAX)
            .map(|&index| usize::from(index))
            .collect();

        // Ten seconds at the default speed is more than a lap.
        let frames: Vec<Vec<usize>> = (0..40).map(|i| lit_at(f64::from(i) * 0.25)).collect();

        let most = frames.iter().map(Vec::len).max().unwrap_or(0);
        assert!(
            most >= 15,
            "a banner of four digits lights keys: at most {most}"
        );
        assert!(
            frames
                .iter()
                .all(|lit| lit.iter().all(|i| !bottom_row.contains(i))),
            "the bottom row stays dark"
        );
        assert!(
            frames.windows(2).any(|pair| pair[0] != pair[1]),
            "the text moves"
        );
    }

    /// Scrolling text draws its words as Clock draws the time: keys lit over a
    /// lap, never on the bottom row, more for a longer text, none for none.
    #[test]
    fn the_scrolling_text_effect_scrolls_its_text() {
        let (_rt, ctx) = prepare(crate::shipped::source("Scrolling text"), layout()).expect("load");
        let len = layout().led_count();
        const BACKGROUND: [u8; 3] = [4, 6, 12];
        let cols = usize::from(layout().cols);
        let bottom_row: Vec<usize> = layout().matrix[layout().matrix.len() - cols..]
            .iter()
            .filter(|&&index| index != u16::MAX)
            .map(|&index| usize::from(index))
            .collect();

        let lit_over_a_lap = |params: &str| -> Vec<usize> {
            (0..40)
                .flat_map(|i| {
                    let frame =
                        render_once(&ctx, f64::from(i) * 0.25, 0, params, len).expect("render");
                    layout()
                        .keys
                        .iter()
                        .map(|k| k.index as usize)
                        .filter(|&i| frame[i * 3..i * 3 + 3] != BACKGROUND)
                        .collect::<Vec<_>>()
                })
                .collect()
        };

        let short = lit_over_a_lap(r#"{"text":"HI"}"#);
        let long = lit_over_a_lap(r#"{"text":"HELLO WORLD"}"#);
        assert!(!short.is_empty(), "HI lights keys");
        assert!(long.len() > short.len(), "a longer text lights more");
        assert!(
            long.iter().all(|i| !bottom_row.contains(i)),
            "the bottom row stays dark"
        );
        assert!(
            lit_over_a_lap(r#"{"text":""}"#).is_empty(),
            "no text, nothing lit"
        );
    }

    /// Still, text that fits stays put, and longer text comes a page of words
    /// at a time: the same keys from one frame to the next within a page,
    /// others on the next page.
    #[test]
    fn the_scrolling_text_effect_holds_still_text_page_by_page() {
        let (_rt, ctx) = prepare(crate::shipped::source("Scrolling text"), layout()).expect("load");
        let len = layout().led_count();
        let still = |text: &str, time: f64| {
            let params = format!(r#"{{"text":"{text}","display":"still","hold":1}}"#);
            render_once(&ctx, time, 0, &params, len).expect("render")
        };

        assert_eq!(
            still("OK", 0.0),
            still("OK", 3.7),
            "a text that fits does not move"
        );
        assert_ne!(still("OK", 0.0), still("", 0.0), "and it is drawn");
        let long = "BUILD FAILED AGAIN";
        assert_eq!(still(long, 0.1), still(long, 0.9), "one page for a second");
        assert_ne!(still(long, 0.5), still(long, 1.5), "then the next");
    }

    /// With the seconds on, the banner carries two more digits and a colon, so
    /// over the same ten seconds it lights more keys than without.
    #[test]
    fn the_clock_effect_shows_the_seconds_when_asked() {
        let (_rt, ctx) = prepare(crate::shipped::source("Clock"), layout()).expect("load");
        let len = layout().led_count();
        const BACKGROUND: [u8; 3] = [4, 6, 12];

        let lit_over_ten_seconds = |params: &str| -> usize {
            (0..40)
                .map(|i| {
                    let frame = render_with_inputs(
                        &ctx,
                        f64::from(i) * 0.25,
                        0,
                        params,
                        "",
                        1_600_000_007_250.0,
                        len,
                    )
                    .expect("render");
                    layout()
                        .keys
                        .iter()
                        .filter(|k| {
                            let i = k.index as usize * 3;
                            frame[i..i + 3] != BACKGROUND
                        })
                        .count()
                })
                .sum()
        };

        let without = lit_over_ten_seconds("{}");
        let with = lit_over_ten_seconds(r#"{"seconds":true}"#);
        assert!(with > without, "seconds on lit {with} keys, off {without}");
    }

    /// Still, the time stays put while it fits: the same keys from one frame to
    /// the next within a minute, the colon held; scrolling, they move.
    #[test]
    fn the_clock_effect_holds_the_time_still_when_asked() {
        let (_rt, ctx) = prepare(crate::shipped::source("Clock"), layout()).expect("load");
        let len = layout().led_count();
        let at = |params: &str, time: f64| {
            render_with_inputs(&ctx, time, 0, params, "", 1_600_000_007_250.0, len).expect("render")
        };
        let still = r#"{"display":"still","blink":false}"#;
        assert_eq!(at(still, 0.0), at(still, 2.5), "the time does not move");
        let scrolling = r#"{"blink":false}"#;
        assert_ne!(at(scrolling, 0.0), at(scrolling, 2.5), "unless it scrolls");
    }

    /// Ripples draws its ring from the pressed key: at the instant of the press,
    /// the key itself takes the ring color; at rest, the background.
    #[test]
    fn ripples_starts_its_ring_on_the_pressed_key() {
        let (_rt, ctx) = prepare(crate::shipped::source("Ripples"), layout()).expect("load");
        let len = layout().led_count();
        let rest = render_once(&ctx, 1.0, 0, "{}", len).expect("render");

        let now = Instant::now();
        let press = presses::Press {
            at: now,
            scancode: 0x11,
            device: None,
        };
        let pressed = presses::positions(layout()).to_json(&[press], now, 1.0);
        let lit = render_with_inputs(&ctx, 1.0, 1, "{}", &pressed, 0.0, len).expect("render");

        let led = 46 * 3;
        assert_eq!(&rest[led..led + 3], &[6, 12, 28], "background at rest");
        assert_eq!(
            &lit[led..led + 3],
            &[64, 200, 255],
            "ring color on the pressed key"
        );
        assert_eq!(&lit[0..3], &[6, 12, 28], "Escape, far away, untouched yet");
    }

    /// Presses are read only while a loop runs an effect declaring them, the
    /// preview included, and no longer once the last one stops.
    #[test]
    fn presses_are_read_only_while_an_effect_declares_them() {
        let presses = Presses::manual();
        let engine = engine_with(&presses);

        start_js(&engine, FIRST, EFFECT, Arc::new(Output::default())).expect("start");
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(presses.readers(), 0, "an effect that reads no keys");

        start_js(&engine, SECOND, READS_KEYS, Arc::new(Output::default())).expect("start");
        engine
            .start_preview(FIRST, None, effect("keys", READS_KEYS), layout())
            .expect("preview start");
        wait_for("both loops read presses", || presses.readers() == 2);

        engine.stop(SECOND);
        assert_eq!(presses.readers(), 1);
        engine.stop_preview();
        assert_eq!(presses.readers(), 0);
        engine.stop(FIRST);
    }

    /// END TO END — writes to the REAL keyboard. `#[ignore]` by default.
    ///
    /// `cargo test -p candeo-desktop end_to_end -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn end_to_end_on_the_real_keyboard() {
        let api = hidapi::HidApi::new().expect("HID");
        let l = layout();
        let kb = match Keyboard::open(&api, l) {
            Ok(kb) => kb,
            Err(e) => panic!("cannot open: {e}"),
        };
        println!("keyboard open: {}", l.name);

        let device = DeviceRef {
            vid: l.vid,
            pid: l.pid,
        };
        let handle: Handle = Arc::new(Mutex::new(Some(kb)));
        let engine = Engine::default();
        let js = crate::shipped::source("Radial wave");

        engine
            .start(
                device,
                effect("onde-radiale", js),
                l,
                Box::new(Arc::clone(&handle)),
                false,
            )
            .expect("start");
        println!("engine started — 3 s of radial wave on the keyboard");
        std::thread::sleep(Duration::from_secs(3));

        let s = status(&engine, device);
        println!("status: running={} error={:?}", s.running, s.error);
        assert!(s.running, "the loop stopped");
        assert!(s.error.is_none(), "error while rendering: {:?}", s.error);
        assert!(
            s.reaching_keyboard,
            "no frame reaches the keyboard: {:?}",
            s.device_error
        );

        engine.stop(device);
        println!("stopped cleanly");
    }

    #[test]
    fn an_effect_renders_a_full_frame() {
        let (_rt, ctx) = prepare(EFFECT, layout()).expect("load");
        let bytes = render_once(&ctx, 0.0, 0, "{}", layout().led_count()).expect("render");

        // 132 positions, not 106: a frame covers the whole matrix.
        assert_eq!(bytes.len(), 132 * 3);
    }

    #[test]
    fn positions_without_an_led_stay_black() {
        let (_rt, ctx) = prepare(EFFECT, layout()).expect("load");
        let bytes = render_once(&ctx, 1.0, 0, "{}", layout().led_count()).expect("render");

        // (0, 1) is a hole in the matrix — the effect iterates over
        // `layout.keys`, so it cannot reach it.
        let hole = 1usize;
        assert_eq!(&bytes[hole * 3..hole * 3 + 3], &[0, 0, 0]);

        // Escape, on the other hand, is lit.
        assert_ne!(&bytes[0..3], &[0, 0, 0]);
    }

    #[test]
    fn a_syntax_error_surfaces_at_load() {
        let err = load_error("this is not JavaScript {{{");
        assert!(
            err.contains("loading the effect"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn a_module_without_a_default_export_is_refused() {
        let err = load_error("export const x = 1");
        assert!(err.contains("default export"), "unexpected message: {err}");
    }

    /// An exception at run time must not bring the engine down: it surfaces as
    /// `Err`, and the loop counts it and carries on.
    #[test]
    fn a_runtime_exception_is_caught() {
        let js = "export default { name: 'X', render() { throw new Error('boum') } }";
        let (_rt, ctx) = prepare(js, layout()).expect("load");
        let err = render_once(&ctx, 0.0, 0, "{}", layout().led_count()).unwrap_err();
        assert!(err.contains("boum"), "unexpected message: {err}");
    }

    /// An out-of-range color must become a valid byte, not make the conversion
    /// fail far from its cause.
    #[test]
    fn out_of_range_colors_are_clamped() {
        let js = r#"
            export default {
              name: 'X',
              render({ layout, frame }) {
                for (const key of layout.keys) frame.set(key, { r: 999, g: -5, b: NaN })
              },
            }
        "#;
        let (_rt, ctx) = prepare(js, layout()).expect("load");
        let bytes = render_once(&ctx, 0.0, 0, "{}", layout().led_count()).expect("render");
        assert_eq!(&bytes[0..3], &[255, 0, 0]);
    }

    /// `api.js` and `packages/effects-api/src/index.ts` describe the same API.
    /// If a name disappears from here, the editor would promise a missing
    /// function.
    #[test]
    fn api_js_exports_match_the_typescript_surface() {
        let js = r#"
            import * as api from '@candeo/effects-api'
            export default {
              name: 'X',
              render({ frame }) {
                const missing = ['rgb','hsv','mix','lerp','BLACK','defineEffect','center','bounds','banner'].filter(n => api[n] === undefined)
                if (missing.length) throw new Error('missing from api.js: ' + missing.join(', '))
                frame.fill(api.BLACK)
              },
            }
        "#;
        let (_rt, ctx) = prepare(js, layout()).expect("load");
        render_once(&ctx, 0.0, 0, "{}", layout().led_count()).expect("render");
    }

    /// The font's lines between the markers, as written.
    fn font_table(source: &str) -> Vec<&str> {
        source
            .lines()
            .skip_while(|line| !line.contains("font:begin"))
            .take_while(|line| !line.contains("font:end"))
            .collect()
    }

    /// The editor draws with `index.ts`, the engine with `api.js`: a glyph
    /// changed in one only would preview one way and light the keys another.
    #[test]
    fn the_font_is_the_same_in_both_api_files() {
        let typescript = font_table(include_str!(
            "../../../../../packages/effects-api/src/index.ts"
        ));
        let engine = font_table(API_JS);
        assert!(typescript.len() > 50, "the TypeScript font has its glyphs");
        assert_eq!(typescript, engine);
    }

    /// Upper case, accents dropped, a space for what the font cannot draw,
    /// one dark column between glyphs: in QuickJS, which has to know
    /// `normalize` for the accents.
    #[test]
    fn banner_draws_text_in_the_engine() {
        let js = r#"
            import { banner } from '@candeo/effects-api'
            const same = (a, b) => JSON.stringify(a) === JSON.stringify(b)
            export default {
              render({ frame }) {
                const hi = ['#.#.###', '#.#..#.', '###..#.', '#.#..#.', '#.#.###']
                if (!same(banner('HI'), hi)) throw new Error('HI: ' + banner('HI'))
                if (!same(banner('hi'), hi)) throw new Error('lower case: ' + banner('hi'))
                if (!same(banner('É'), banner('E'))) throw new Error('accent: ' + banner('É'))
                if (!same(banner('~'), banner(' '))) throw new Error('unknown: ' + banner('~'))
                if (!same(banner(''), ['', '', '', '', ''])) throw new Error('empty')
                frame.fill({ r: 1, g: 1, b: 1 })
              },
            }
        "#;
        let (_rt, ctx) = prepare(js, layout()).expect("load");
        render_once(&ctx, 0.0, 0, "{}", layout().led_count()).expect("render");
    }

    // ---------------------------------------------------------------- geometry

    /// The key rectangle makes it all the way to the effect, in the Rust unit.
    ///
    /// The space bar is the case that sums up the subject: **one** matrix cell,
    /// **6.25 u** of keycap. Without the rectangle, an effect cannot tell it
    /// apart from a letter key.
    #[test]
    fn the_layout_given_to_the_effect_carries_the_geometry() {
        let json: serde_json::Value =
            serde_json::from_str(&layout_json(layout())).expect("layout JSON");
        let keys = json["keys"].as_array().expect("keys");

        let space = keys
            .iter()
            .find(|k| k["scancode"] == 0x39)
            .expect("the space bar");
        assert_eq!(space["col"], 6, "a single matrix cell");
        assert_eq!(space["w"], 6.25, "and 6.25 u of keycap");
        assert_eq!(space["x"], 3.75);
        assert_eq!(space["y"], 5.5);
        assert_eq!(space["h"], 1.0);

        assert!(
            keys.iter().all(|k| k["w"].as_f64().unwrap_or(0.0) > 0.0),
            "a key without a width cannot be told apart from a key without geometry"
        );
    }

    /// An effect finds a key by its scancode, whatever the legend: 0x11 is the key
    /// right of the one right of Tab's, engraved Z on this AZERTY keyboard. Fn
    /// sends nothing, and carries no scancode rather than a false one.
    #[test]
    fn the_layout_given_to_the_effect_carries_scancodes() {
        let json: serde_json::Value =
            serde_json::from_str(&layout_json(layout())).expect("layout JSON");
        let keys = json["keys"].as_array().expect("keys");

        let z = keys.iter().find(|k| k["scancode"] == 0x11).expect("0x11");
        assert_eq!((z["row"].as_u64(), z["col"].as_u64()), (Some(2), Some(2)));
        let fn_key = keys.iter().find(|k| k["index"] == 121).expect("Fn");
        assert!(fn_key.get("scancode").is_none() && fn_key.get("label").is_none());
    }

    /// A layout whose arrangement nobody has drawn.
    ///
    /// There is none in this repository — `candeo_device::Key` carries a
    /// mandatory rectangle — but that is what a device contributed without the
    /// `geometry` capability of `docs/design/device-sdk.md` §3.2 will be. Two
    /// positions are enough: what is tested is the absence of the fields.
    const NO_GEOMETRY: &str = r#"{"name":"Undrawn layout","rows":1,"cols":2,"keys":[{"index":0,"row":0,"col":0,"label":"A"},{"index":1,"row":0,"col":1,"label":"B"}]}"#;

    /// **The pattern we are fighting.** Without geometry, `key.x` is
    /// `undefined`, the distance `NaN`, and the color would be clamped to zero:
    /// a black keyboard, without a single error, and a round of debugging to
    /// understand why. The effect must fail by naming what is missing.
    #[test]
    fn the_radial_wave_refuses_a_layout_without_geometry() {
        let js = crate::shipped::source("Radial wave");
        let (_rt, ctx) = prepare_with_layout(js, 2, NO_GEOMETRY.to_string(), None).expect("load");

        let err = render_once(&ctx, 0.0, 0, "{}", 2).unwrap_err();
        assert!(err.contains("geometry"), "unexpected message: {err}");
        assert!(
            err.contains('A') || err.contains('B'),
            "the message must name the offending key: {err}"
        );
    }

    /// And the diagonal wave, for its part, does run there: that is the whole
    /// point of having kept it rather than fixed it. A layout that has not been
    /// drawn keeps an effect.
    #[test]
    fn the_diagonal_wave_runs_without_geometry() {
        let js = crate::shipped::source("Diagonal wave");
        let (_rt, ctx) = prepare_with_layout(js, 2, NO_GEOMETRY.to_string(), None).expect("load");

        let bytes = render_once(&ctx, 0.0, 0, "{}", 2).expect("render");
        assert!(
            bytes.iter().any(|&c| c != 0),
            "the diagonal wave only needs `row` and `col`"
        );
    }

    // ------------------------------------------------------- execution limits

    /// A loop that never gives control back, written the way it gets written by
    /// accident: an exit condition that never comes.
    const ENDLESS_RENDER: &str = r#"
        export default {
          name: 'Sans fin',
          render() {
            let i = 0
            while (i >= 0) i += 1
          },
        }
    "#;

    /// A state that grows on every frame, and that nothing frees.
    const ENDLESS_ALLOCATION: &str = r#"
        const kept = []
        export default {
          name: 'Fuite',
          render() {
            kept.push(new Uint8Array(4 * 1024 * 1024))
          },
        }
    "#;

    /// Like [`start`], but with a given effect, and without requiring it to
    /// start.
    fn start_js(
        engine: &Engine,
        device: DeviceRef,
        js: &str,
        out: Arc<Output>,
    ) -> Result<(), String> {
        engine.start(device, effect("borne", js), layout(), Box::new(out), false)
    }

    /// Runs `f` on the side, and returns its result — or fails if it does not
    /// come back.
    ///
    /// Calling a function suspected of never returning directly gives a test
    /// that cannot fail: it hangs, and it is CI that ends up killing it, hours
    /// later. Here, the test decides. The thread left behind would spin in the
    /// void, but it prevents nothing from finishing — and a test that has
    /// already failed has nothing left to protect.
    fn without_hanging<T: Send + 'static>(
        patience: Duration,
        what: &str,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(patience)
            .unwrap_or_else(|_| panic!("{what}: nothing in {patience:?}"))
    }

    /// **An effect that never gives control back no longer freezes its
    /// thread.**
    ///
    /// This is the failure the limits exist to handle: without them, the `stop`
    /// flag would never be read again, and closing the window would not help —
    /// an effect runs with the window closed. The stop must therefore come from
    /// the engine, through the ordinary error path.
    #[test]
    fn an_effect_looping_on_every_frame_is_stopped_cleanly() {
        let engine = Engine::default();
        let out = Arc::new(Output::default());
        start_js(&engine, FIRST, ENDLESS_RENDER, Arc::clone(&out)).expect("start");

        // The test's safeguard: if nothing stopped the loop, this is what would
        // fail, rather than CI timing out hours later. Three times what the
        // limit promises — thirty cut frames, each taking its whole budget.
        wait_at_most(
            FRAME_BUDGET * MAX_CONSECUTIVE_ERRORS * 3,
            "the loop did not stop",
            || !status(&engine, FIRST).running,
        );

        let error = status(&engine, FIRST).error.expect("no error recorded");
        assert!(
            error.contains("computing time"),
            "the cause is not named: {error}"
        );
        assert_eq!(
            out.written.load(Ordering::Relaxed),
            0,
            "a frame came out of an effect that never finished one"
        );
        // #48: stopped on its own, the effect does not leave a frame that looks
        // like it still runs.
        assert!(out.turned_off.load(Ordering::Relaxed), "backlight left on");

        // The loop did give its thread back: otherwise this is where the test
        // would stop forever, waiting for it to end.
        engine.stop(FIRST);
    }

    /// **An effect that allocates endlessly now takes down only itself.**
    ///
    /// The first frames go through — the effect is allowed to keep state —,
    /// then the limit hits and exceeding it becomes a frame error like any
    /// other.
    #[test]
    fn an_effect_allocating_endlessly_is_stopped_cleanly() {
        let engine = Engine::default();
        let out = Arc::new(Output::default());
        start_js(&engine, FIRST, ENDLESS_ALLOCATION, Arc::clone(&out)).expect("start");

        wait_for("the loop did not stop", || !status(&engine, FIRST).running);

        let error = status(&engine, FIRST).error.expect("no error recorded");
        assert!(error.contains("memory"), "the cause is not named: {error}");

        engine.stop(FIRST);
    }

    /// **A loop outside `render` no longer blocks the start.**
    ///
    /// The module body runs at load time, outside any frame, and `start` waits
    /// for its verdict while holding the device lock: without a limit, that
    /// keyboard would never start or stop anything again.
    #[test]
    fn an_effect_looping_at_load_gives_control_back() {
        let error = without_hanging(LOAD_BUDGET * 3, "starting never returned", || {
            let engine = Engine::default();
            start_js(
                &engine,
                FIRST,
                "while (true) {}\nexport default { name: 'X', render() {} }",
                Arc::new(Output::default()),
            )
            .expect_err("loading should have been interrupted")
        });

        assert!(
            error.contains("computing time"),
            "the cause is not named: {error}"
        );
    }

    /// An ordinary exception keeps its message: the limits only explain what
    /// they cut, and a `throw` from the effect already speaks for itself.
    #[test]
    fn an_ordinary_exception_keeps_its_message() {
        let engine = Engine::default();
        let js = "export default { name: 'X', render() { throw new Error('boum') } }";
        start_js(&engine, FIRST, js, Arc::new(Output::default())).expect("start");

        wait_for("no error recorded", || {
            status(&engine, FIRST).error.is_some()
        });
        let error = status(&engine, FIRST).error.unwrap();
        assert!(error.contains("boum"), "message rewritten: {error}");
        assert!(!status(&engine, FIRST).reads_private);

        engine.stop(FIRST);
    }

    /// #44: an effect reading key presses could write them into its error; the
    /// window still shows the text, the log and the diagnostic do not.
    #[test]
    fn the_error_of_an_effect_reading_keys_stays_out_of_the_log() {
        let engine = Engine::default();
        let js = "export default { name: 'X', inputs: ['keys'], render() { throw new Error('A pressed') } }";
        start_js(&engine, FIRST, js, Arc::new(Output::default())).expect("start");

        wait_for("no error recorded", || {
            status(&engine, FIRST).error.is_some()
        });
        let s = status(&engine, FIRST);
        assert!(s.reads_private);
        let error = s.error.unwrap();
        assert!(
            error.contains("A pressed"),
            "the window lost the text: {error}"
        );
        assert!(!loggable(&error, s.reads_private).contains("A pressed"));
        assert_eq!(loggable("boum", false), "boum");

        engine.stop(FIRST);
    }

    /// **The limits must strangle no honest effect.**
    ///
    /// The frame buffer costs nothing — `bootstrap.js` reuses it — but an effect
    /// is allowed to keep state and to keep it alive. Two thousand particles and
    /// a trail of one second of frames, for a keyboard with 132 LEDs, is
    /// outlandish on purpose: if the limits let that one through, they let
    /// through anything anyone will write.
    ///
    /// Rendering goes beyond the depth of the trail, to its steady state: a
    /// state that is only freed at the sixtieth frame does not show over thirty.
    ///
    /// The margin is checked, not only the success: an effect that passed by a
    /// hair would no longer pass on the machine next door.
    #[test]
    fn the_limits_let_a_stateful_effect_through() {
        let js = r#"
            const particles = Array.from({ length: 2000 }, (_, i) => ({
              x: (i * 7) % 20,
              y: (i * 3) % 6,
              vx: 0.11,
              vy: 0.07,
            }))
            const trail = []

            export default {
              name: 'Particules',
              render({ layout, frame }) {
                for (const p of particles) {
                  p.x = (p.x + p.vx) % layout.cols
                  p.y = (p.y + p.vy) % layout.rows
                }
                // One second of frames kept, the oldest one freed.
                trail.push(particles.map((p) => (p.x + p.y) | 0))
                if (trail.length > 60) trail.shift()

                for (const key of layout.keys) {
                  frame.set(key, { r: (key.col * 8) % 256, g: (key.row * 40) % 256, b: 60 })
                }
              },
            }
        "#;

        let budget = Rc::new(Budget::default());
        budget.grant(LOAD_BUDGET);
        let (rt, ctx) = prepare_budgeted(js, layout(), &budget).expect("load");

        for frame in 0..90u32 {
            budget.grant(FRAME_BUDGET);
            render_once(
                &ctx,
                f64::from(frame) / f64::from(FPS),
                frame,
                "{}",
                layout().led_count(),
            )
            .unwrap_or_else(|e| panic!("frame {frame}: {e}"));
        }

        let used = rt.memory_usage().malloc_size as usize;
        assert!(
            used * 4 < MEMORY_BUDGET,
            "a stateful effect is close to the memory limit: {used} bytes of {MEMORY_BUDGET}"
        );
    }

    // ------------------------------------------------------------ shipped effects
    //
    // Shipped effects go through the same engine as the user's, hence through
    // the same tests. A broken shipped effect must not be discovered at run
    // time, by whoever opens it first.

    /// Sampling times. Several, and not only zero: a division by a cycle's
    /// duration or an overrun past the last row only shows once the animation
    /// has started.
    const SAMPLE_TIMES: [f64; 4] = [0.0, 0.4, 1.3, 2.7];

    #[test]
    fn every_shipped_effect_renders_a_full_frame() {
        for b in &crate::shipped::ALL {
            let (_rt, ctx) =
                prepare(b.source, layout()).unwrap_or_else(|e| panic!("{}: {e}", b.name));

            for (i, time) in SAMPLE_TIMES.iter().enumerate() {
                let bytes = render_once(&ctx, *time, i as u32, "{}", layout().led_count())
                    .unwrap_or_else(|e| panic!("{} at t={time}: {e}", b.name));

                assert_eq!(bytes.len(), 132 * 3, "{} at t={time}", b.name);
                // (0, 1) is a hole in the matrix. An effect that reaches it
                // does not iterate over `layout.keys`: it works on the 132
                // cells instead of the 106 lit positions.
                assert_eq!(
                    &bytes[3..6],
                    &[0, 0, 0],
                    "{} writes to a position without an LED",
                    b.name
                );
            }
        }
    }

    /// A shipped effect must be visible from its first frame: a black frame at
    /// start looks like an effect that did not start.
    #[test]
    fn every_shipped_effect_lights_something_from_the_first_frame() {
        for b in &crate::shipped::ALL {
            let (_rt, ctx) =
                prepare(b.source, layout()).unwrap_or_else(|e| panic!("{}: {e}", b.name));
            let bytes = render_once(&ctx, 0.0, 0, "{}", layout().led_count()).expect("render");

            assert!(
                bytes.iter().any(|&c| c != 0),
                "{} renders an entirely black frame",
                b.name
            );
        }
    }

    // ------------------------------------------------- the two waves, side by side
    //
    // Two pairs of keys from the **real** layout, chosen so that each one tells
    // the two spaces apart. Nothing is simulated here: the geometry comes from
    // the DeathStalker's definition, and that is what makes the check possible
    // without a keyboard.

    /// The color of an LED in a rendered frame.
    fn color_at(frame: &[u8], index: usize) -> &[u8] {
        &frame[index * 3..index * 3 + 3]
    }

    /// The first frame of a shipped effect, on the default layout.
    fn first_frame(name: &str) -> Vec<u8> {
        let js = crate::shipped::source(name);
        let (_rt, ctx) = prepare(js, layout()).unwrap_or_else(|e| panic!("{name}: {e}"));
        render_once(&ctx, 0.0, 0, "{}", layout().led_count()).expect("render")
    }

    /// "L" (index 75) and "ù" (index 77) are at the **same physical distance**
    /// from the center of the drawing — 1 u on either side, 0.75 u lower.
    ///
    /// The radial wave must therefore paint them the same color. That is the
    /// definition of "radial", checked rather than announced.
    #[test]
    fn the_radial_wave_measures_physical_distance() {
        let radial = first_frame("Radial wave");
        assert_eq!(
            color_at(&radial, 75),
            color_at(&radial, 77),
            "two keys at equal physical distance must have the same color"
        );
    }

    /// "L" (index 75) and "*" (index 78) are at 1.25 u and 2.14 u from the center
    /// of the drawing, because the row is staggered and the L-shaped Enter key
    /// does not fall on the grid: the radial wave tells them apart.
    #[test]
    fn the_radial_wave_follows_the_staggered_rows() {
        let radial = first_frame("Radial wave");
        assert_ne!(
            color_at(&radial, 75),
            color_at(&radial, 78),
            "physically, they are not at equal distance"
        );
    }

    /// **The diagonal wave leaves the top-left corner.** Any wave
    /// measured from the center looked like the radial one on a keyboard 22 cells
    /// wide and 6 high: both became near-vertical bands. Counting steps from a
    /// corner changes the motion itself.
    ///
    /// Checked on every pair of keys of the real layout: the same diagonal
    /// (column + row) gives the same color, and mirrored keys, at equal distance
    /// from the center, no longer do.
    #[test]
    fn the_diagonal_wave_starts_from_the_corner() {
        let diagonal = first_frame("Diagonal wave");
        let l = layout();
        let keys: Vec<(u16, u8, u8)> = (0..l.rows)
            .flat_map(|row| (0..l.cols).map(move |col| (row, col)))
            .filter_map(|(row, col)| l.at(row, col).map(|index| (index, row, col)))
            .collect();

        for (i, &(a, a_row, a_col)) in keys.iter().enumerate() {
            for &(b, b_row, b_col) in &keys[i + 1..] {
                if a_row + a_col == b_row + b_col {
                    assert_eq!(
                        color_at(&diagonal, a.into()),
                        color_at(&diagonal, b.into()),
                        "LEDs {a} and {b} are on the same diagonal"
                    );
                }
            }
        }

        // "L" (row 3, column 9) and "*" (row 3, column 12) mirror each other
        // around the center: a centered wave painted them alike.
        assert_ne!(
            color_at(&diagonal, 75),
            color_at(&diagonal, 78),
            "the wave must not be symmetric around the center any more"
        );
    }

    /// The bands move **away** from the corner. With speed and scale equal, one
    /// second moves them by exactly one step: "L" (row 3, column 9) then shows
    /// the color "K" (row 3, column 8) had a second earlier.
    #[test]
    fn the_diagonal_wave_moves_away_from_the_corner() {
        let js = crate::shipped::source("Diagonal wave");
        let (_rt, ctx) = prepare(js, layout()).expect("load");
        let params = r#"{"speed":18,"scale":18}"#;
        let len = layout().led_count();
        let before = render_once(&ctx, 0.0, 0, params, len).expect("render");
        let after = render_once(&ctx, 1.0, 1, params, len).expect("render");

        assert_eq!(
            color_at(&after, 75),
            color_at(&before, 74),
            "one second later, the color one step closer to the corner has moved on"
        );
    }

    /// The space bar is painted according to **the middle of its keycap**.
    ///
    /// One matrix cell, 6.25 u wide: its center is at 6.875 u, not at the left
    /// edge (3.75 u) nor at column 6. The key of the row above whose keycap is
    /// centered at the same place — "B", at 6.75 u — must therefore be at almost
    /// the same distance from the center, although nothing in the matrix says
    /// so.
    #[test]
    fn the_radial_wave_places_the_space_bar_at_the_middle_of_its_keycap() {
        let radial = first_frame("Radial wave");

        // Distances to the center of the drawing (11.25; 3.25): "Space"
        // at 5.17 u, "B" at 4.83 u — a 0.34 u gap, hence neighboring hues.
        // Measuring from the left edge of the keycap (3.75 u) would widen the
        // gap to 2.7 u, and the two colors would have nothing in common.
        let space = color_at(&radial, 116);
        let key_b = color_at(&radial, 94);
        let gap = space
            .iter()
            .zip(key_b)
            .map(|(e, t)| e.abs_diff(*t) as u32)
            .max()
            .expect("three components");

        assert!(
            gap < 60,
            "Space and B are physically close, their colors should be too: {space:?} vs {key_b:?}"
        );
    }
}

/// Signals reaching an effect: bound to a parameter, converted by the bootstrap
/// against its spec, or handed whole to an effect declaring them (§2.3.1).
#[cfg(test)]
mod signal_tests {
    use super::*;

    /// Paints its parameters on the first two keys and one signal on the third,
    /// to read them back out of a frame.
    const PAINTS_ITS_PARAMETERS: &str = "export default {
        params: {
            colour: { kind: 'color', label: 'Colour', default: { r: 1, g: 2, b: 3 } },
            level: { kind: 'number', label: 'Level', min: 0, max: 100, default: 50 },
            on: { kind: 'boolean', label: 'On', default: false },
            mode: { kind: 'choice', label: 'Mode', options: ['a', { value: 'b', label: 'B' }], default: 'a' },
        },
        inputs: ['signals'],
        render({ layout, params, signals, frame }) {
            frame.set(layout.keys[0], params.colour)
            frame.set(layout.keys[1], {
                r: params.level,
                g: params.on ? 255 : 0,
                b: params.mode === 'b' ? 255 : 0,
            })
            frame.set(layout.keys[2], {
                r: typeof signals.count === 'number' ? signals.count : 0,
                g: Object.isFrozen(signals) ? 1 : 0,
                b: 0,
            })
        },
    }";

    /// The same, declaring nothing.
    const DECLARES_NOTHING: &str = "export default {
        render({ layout, signals, frame }) {
            frame.set(layout.keys[2], { r: Object.keys(signals).length, g: 0, b: 0 })
        },
    }";

    const CONFIGURED: &str = r#"{"colour":{"r":1,"g":2,"b":3},"level":50,"on":false,"mode":"a"}"#;

    fn frame_of(js: &str, bound: &str, signals: &str) -> Vec<u8> {
        let layout = crate::default_layout();
        let (_rt, ctx) = prepare(js, layout).expect("load");
        render_with(
            &ctx,
            0.0,
            0,
            &FrameInputs {
                params: CONFIGURED,
                presses: "",
                clock_ms: 0.0,
                bound,
                signals,
                audio: "",
                system: "",
            },
            layout.led_count(),
        )
        .expect("render")
    }

    fn frame(bound: &str, signals: &str) -> Vec<u8> {
        frame_of(PAINTS_ITS_PARAMETERS, bound, signals)
    }

    fn key(frame: &[u8], i: usize) -> [u8; 3] {
        let at = usize::from(crate::default_layout().keys[i].index) * 3;
        [frame[at], frame[at + 1], frame[at + 2]]
    }

    #[test]
    fn without_a_binding_the_effect_draws_its_configured_values() {
        let drawn = frame("", "");
        assert_eq!(key(&drawn, 0), [1, 2, 3]);
        assert_eq!(key(&drawn, 1), [50, 0, 0]);
    }

    #[test]
    fn a_bound_colour_replaces_the_configured_one() {
        assert_eq!(
            key(&frame(r##"{"colour":"#ff8000"}"##, ""), 0),
            [255, 128, 0]
        );
        assert_eq!(
            key(&frame(r#"{"colour":"0f0"}"#, ""), 0),
            [0, 255, 0],
            "#rgb, # optional"
        );
    }

    #[test]
    fn a_bound_number_is_clamped_and_text_reads_as_sent() {
        let drawn = frame(r#"{"level":"250","on":"true","mode":"b"}"#, "");
        assert_eq!(key(&drawn, 1), [100, 255, 255]);
        let drawn = frame(r#"{"level":-4,"on":1}"#, "");
        assert_eq!(key(&drawn, 1), [0, 255, 0]);
    }

    /// A setting following the sound takes it across what it accepts: a number
    /// over its range, a flag past half, a colour's hue turned from the one set.
    #[test]
    fn a_setting_following_the_sound_takes_it_by_its_kind() {
        let drawn = frame(
            r#"{"level":{"sound":0.25},"on":{"sound":0.6},"colour":{"sound":0.5},"mode":{"sound":1}}"#,
            "",
        );
        assert_eq!(
            key(&drawn, 1),
            [25, 255, 0],
            "a quarter of 0..100, on past half, the mode kept"
        );
        let still = frame(r#"{"colour":{"sound":0}}"#, "");
        assert_eq!(
            key(&still, 0),
            [1, 2, 3],
            "silence leaves the colour as set"
        );
        let [r, g, b] = key(&drawn, 0);
        assert!(
            r > b && g > b,
            "half a turn from blue is yellow: {:?}",
            [r, g, b]
        );
    }

    /// A setting following the system takes it as it takes the sound (§2.4).
    #[test]
    fn a_setting_following_the_system_takes_it_by_its_kind() {
        let drawn = frame(
            r#"{"level":{"system":0.25},"on":{"system":0.6},"mode":{"system":1}}"#,
            "",
        );
        assert_eq!(key(&drawn, 1), [25, 255, 0]);
        assert!(is_source("system:gpuTemp") && !is_source("system:network"));
    }

    /// A system source nobody has read yet is left out: the setting keeps its
    /// value, as one following an absent signal does.
    #[test]
    fn a_system_source_not_read_leaves_the_setting_as_set() {
        let bindings: Bindings = [("speed".to_string(), "system:cpu".to_string())].into();
        let system = crate::system::System::default();
        assert!(system_bound(&bindings, &system).is_empty());
    }

    /// The loop turns a sound binding into its value, beside the signals'.
    #[test]
    fn sound_bindings_become_values_beside_the_signals() {
        let mut frame = crate::audio::analysis::Frame::SILENT;
        frame.bass = 0.5;
        frame.pulse = 1.0;
        let bindings: Bindings = [
            ("speed".to_string(), "sound:bass".to_string()),
            ("flash".to_string(), "sound:beat".to_string()),
            ("colour".to_string(), "signal:status".to_string()),
        ]
        .into();
        let merged = with_levels(r##"{"colour":"#ff0000"}"##, sound_bound(&bindings, &frame));
        let json: serde_json::Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(json["speed"]["sound"], 0.5);
        assert_eq!(json["flash"]["sound"], 1.0);
        assert_eq!(json["colour"], "#ff0000");
        assert!(is_source("sound:tone") && is_source("signal:x") && !is_source("sound:pitch"));
    }

    #[test]
    fn a_bound_value_that_does_not_fit_leaves_the_configured_one() {
        let drawn = frame(
            r#"{"colour":"red","level":"loud","on":"maybe","mode":"c","unknown":1}"#,
            "",
        );
        assert_eq!(key(&drawn, 0), [1, 2, 3]);
        assert_eq!(key(&drawn, 1), [50, 0, 0]);
    }

    /// A `text` setting takes any scalar as its text, cut at its `maxLength`.
    #[test]
    fn a_bound_text_takes_any_value_as_text_cut_at_its_length() {
        let js = "export default {
            params: { words: { kind: 'text', label: 'Words', maxLength: 4, default: 'hi' } },
            render({ layout, params, frame }) {
                const w = params.words
                frame.set(layout.keys[0], {
                    r: w.length,
                    g: w === 'abcd' ? 1 : w === 'hi' ? 2 : 0,
                    b: w === '1' ? 1 : w === 'true' ? 2 : 0,
                })
            },
        }";
        let layout = crate::default_layout();
        let (_rt, ctx) = prepare(js, layout).expect("load");
        let drawn = |bound: &str| {
            let frame = render_with(
                &ctx,
                0.0,
                0,
                &FrameInputs {
                    params: r#"{"words":"hi"}"#,
                    presses: "",
                    clock_ms: 0.0,
                    bound,
                    signals: "",
                    audio: "",
                    system: "",
                },
                layout.led_count(),
            )
            .expect("render");
            key(&frame, 0)
        };
        assert_eq!(drawn(""), [2, 2, 0], "configured");
        assert_eq!(
            drawn(r#"{"words":"abcdef"}"#),
            [4, 1, 0],
            "cut at maxLength"
        );
        assert_eq!(drawn(r#"{"words":1}"#), [1, 0, 1], "a number as its text");
        assert_eq!(drawn(r#"{"words":true}"#), [4, 0, 2], "a flag as its text");
    }

    #[test]
    fn an_effect_declaring_signals_is_handed_them_frozen() {
        assert_eq!(key(&frame("", r#"{"count":7}"#), 2), [7, 1, 0]);
        assert_eq!(
            key(&frame("", ""), 2),
            [0, 1, 0],
            "an empty bag, frozen too"
        );
    }

    /// The shipped dashboard: one key of the top row per signal, in the order of
    /// their names, coloured by what each says.
    #[test]
    fn status_row_lights_one_key_per_signal_in_name_order() {
        let layout = crate::default_layout();
        let (_rt, ctx) = prepare(crate::shipped::source("Status row"), layout).expect("load");
        // The top row as effects see it: the matrix's first line, keys only.
        let keyed: std::collections::HashSet<u16> = layout.keys.iter().map(|k| k.index).collect();
        let row: Vec<u16> = layout.matrix[..usize::from(layout.cols)]
            .iter()
            .copied()
            .filter(|index| keyed.contains(index))
            .collect();
        let at = |frame: &[u8], index: u16| {
            let i = usize::from(index) * 3;
            [frame[i], frame[i + 1], frame[i + 2]]
        };

        let frame = render_with(
            &ctx,
            0.25,
            0,
            &FrameInputs {
                params: "{}",
                presses: "",
                clock_ms: 0.0,
                bound: "",
                signals: r#"{"tests":"running","build":"failed","deploy":"ok","cpu":1}"#,
                audio: "",
                system: "",
            },
            layout.led_count(),
        )
        .expect("render");

        // build, cpu, deploy, tests: alphabetical, whatever order they came in.
        assert_eq!(at(&frame, row[0]), [235, 24, 24], "build failed: red");
        assert_eq!(at(&frame, row[1]), [235, 24, 24], "cpu at its worst: red");
        assert_eq!(at(&frame, row[2]), [32, 200, 64], "deploy ok: green");
        let tests = at(&frame, row[3]);
        assert!(
            tests[0] > 200 && tests[2] == 0,
            "tests running: amber, {tests:?}"
        );
        assert_eq!(at(&frame, row[4]), [0, 0, 0], "no fifth signal");

        // With a prefix, only the names starting with it take a key.
        let frame = render_with(
            &ctx,
            0.25,
            0,
            &FrameInputs {
                params: r#"{"prefix":"ci."}"#,
                presses: "",
                clock_ms: 0.0,
                bound: "",
                signals: r#"{"build":"failed","ci.tests":"ok","ci.build":"failed"}"#,
                audio: "",
                system: "",
            },
            layout.led_count(),
        )
        .expect("render");
        assert_eq!(at(&frame, row[0]), [235, 24, 24], "ci.build failed: red");
        assert_eq!(at(&frame, row[1]), [32, 200, 64], "ci.tests ok: green");
        assert_eq!(at(&frame, row[2]), [0, 0, 0], "build is not under ci.");
    }

    /// A frame of a shipped effect with this analysis of the sound playing.
    fn heard(ctx: &Context, time: f64, audio: &str) -> Vec<u8> {
        let layout = crate::default_layout();
        render_with(
            ctx,
            time,
            0,
            &FrameInputs {
                params: "{}",
                presses: "",
                clock_ms: 0.0,
                bound: "",
                signals: "",
                audio,
                system: "",
            },
            layout.led_count(),
        )
        .expect("render")
    }

    fn analysis(level: f64, bands: &[f64], beat: bool) -> String {
        let bands: Vec<String> = bands.iter().map(f64::to_string).collect();
        format!(
            r#"{{"level":{level},"peak":{level},"bands":[{}],"beat":{beat},"onset":{}}}"#,
            bands.join(","),
            if beat { 1.0 } else { 0.0 }
        )
    }

    /// An effect declaring `audio` reads the analysis; one that does not gets
    /// silence, whatever plays.
    #[test]
    fn only_an_effect_declaring_audio_hears_the_sound() {
        let paints = |declares: bool| {
            format!(
                "export default {{
                    {}
                    render({{ layout, audio, frame }}) {{
                        frame.set(layout.keys[0], {{ r: audio.level * 100, g: audio.bands[3] * 100, b: audio.beat ? 255 : 0 }})
                    }},
                }}",
                if declares { "inputs: ['audio']," } else { "" }
            )
        };
        let loud = analysis(0.5, &[0.25; 16], true);
        let (_rt, ctx) = prepare(&paints(true), crate::default_layout()).expect("load");
        assert_eq!(key(&heard(&ctx, 0.0, &loud), 0), [50, 25, 255]);
        let (_rt, ctx) = prepare(&paints(false), crate::default_layout()).expect("load");
        assert_eq!(key(&heard(&ctx, 0.0, &loud), 0), [0, 0, 0]);
    }

    /// An effect declaring `system` reads what the computer is doing; one that
    /// does not gets nothing read.
    #[test]
    fn only_an_effect_declaring_system_is_given_it() {
        let paints = |declares: bool| {
            format!(
                "export default {{
                    {}
                    render({{ layout, system, frame }}) {{
                        frame.set(layout.keys[0], {{ r: (system.cpu ?? 0) * 100, g: system.gpuCelsius ?? 0, b: system.cpuTemp === null ? 255 : 0 }})
                    }},
                }}",
                if declares { "inputs: ['system']," } else { "" }
            )
        };
        let busy = r#"{"cpu":0.5,"gpuCelsius":51,"cpuTemp":null}"#;
        let layout = crate::default_layout();
        let drawn = |js: &str| {
            let (_rt, ctx) = prepare(js, layout).expect("load");
            let bytes = render_with(
                &ctx,
                0.0,
                0,
                &FrameInputs {
                    params: "{}",
                    presses: "",
                    clock_ms: 0.0,
                    bound: "",
                    signals: "",
                    audio: "",
                    system: busy,
                },
                layout.led_count(),
            )
            .expect("render");
            key(&bytes, 0)
        };
        assert_eq!(drawn(&paints(true)), [50, 51, 255]);
        assert_eq!(drawn(&paints(false)), [0, 0, 255]);
    }

    /// The shipped Temperature: the cool colour when cool, the hot one when hot;
    /// the processor's power where its temperature cannot be read; the cool
    /// colour dimmed when nothing is read, rather than black.
    #[test]
    fn the_temperature_goes_from_cool_to_hot() {
        let layout = crate::default_layout();
        let (_rt, ctx) = prepare(crate::shipped::source("Temperature"), layout).expect("load");
        let drawn = |params: &str, system: &str| {
            let bytes = render_with(
                &ctx,
                0.0,
                0,
                &FrameInputs {
                    params,
                    presses: "",
                    clock_ms: 0.0,
                    bound: "",
                    signals: "",
                    audio: "",
                    system,
                },
                layout.led_count(),
            )
            .expect("render");
            key(&bytes, 0)
        };
        let gpu = r#"{"follows":"gpu","cool":{"r":0,"g":0,"b":200},"hot":{"r":200,"g":0,"b":0}}"#;
        assert_eq!(drawn(gpu, r#"{"gpuTemp":0}"#), [0, 0, 200]);
        assert_eq!(drawn(gpu, r#"{"gpuTemp":1}"#), [200, 0, 0]);
        assert_eq!(drawn(gpu, r#"{"gpuTemp":0.5}"#), [100, 0, 100]);
        assert_eq!(
            drawn(gpu, ""),
            [0, 0, 60],
            "nothing read: the cool colour, dimmed"
        );
        let cpu = gpu.replace("\"gpu\"", "\"cpu\"");
        assert_eq!(
            drawn(&cpu, r#"{"cpuTemp":null,"cpuPower":1}"#),
            [200, 0, 0],
            "the power stands in for the temperature"
        );
    }

    /// The shipped Equalizer: loud everywhere lights the keyboard up to the
    /// top; only the lowest band lights the left side only.
    #[test]
    fn the_equalizer_draws_the_bands_as_bars() {
        let layout = crate::default_layout();
        let (_rt, ctx) = prepare(crate::shipped::source("Equalizer"), layout).expect("load");
        let background = [4u8, 8, 14];
        // The horizontal position of each key lit, in pitch units.
        let lit = |frame: &[u8]| -> Vec<f32> {
            layout
                .keys
                .iter()
                .filter(|k| {
                    let i = usize::from(k.index) * 3;
                    frame[i..i + 3] != background
                })
                .map(|k| k.x)
                .collect()
        };

        let silent = heard(&ctx, 0.0, &analysis(0.0, &[0.0; 16], false));
        assert!(lit(&silent).is_empty(), "silence shows the background only");

        let loud = heard(&ctx, 0.0, &analysis(1.0, &[1.0; 16], false));
        assert_eq!(
            lit(&loud).len(),
            layout.keys.len(),
            "every band full lights every key"
        );

        let mut bass = [0.0; 16];
        bass[0] = 1.0;
        let frame = heard(&ctx, 0.0, &analysis(0.5, &bass, false));
        let middle = layout.keys.iter().map(|k| k.x).fold(0.0, f32::max) / 2.0;
        let bars = lit(&frame);
        assert!(!bars.is_empty(), "the lowest band shows");
        assert!(bars.iter().all(|&x| x < middle), "and only on the left");
    }

    /// In rows, a band per row: the bass alone lights the bottom row only,
    /// from the left.
    #[test]
    fn the_equalizer_draws_a_band_per_row_when_asked() {
        let layout = crate::default_layout();
        let (_rt, ctx) = prepare(crate::shipped::source("Equalizer"), layout).expect("load");
        let background = [4u8, 8, 14];
        let rows_params = r#"{"bars":"rows"}"#;
        let frame = |audio: &str| {
            render_with(
                &ctx,
                0.0,
                0,
                &FrameInputs {
                    params: rows_params,
                    presses: "",
                    clock_ms: 0.0,
                    bound: "",
                    signals: "",
                    audio,
                    system: "",
                },
                layout.led_count(),
            )
            .expect("render")
        };
        // The top edge of each key lit: keys of a row share it.
        let lit_tops = |frame: &[u8]| -> Vec<f32> {
            layout
                .keys
                .iter()
                .filter(|k| {
                    let i = usize::from(k.index) * 3;
                    frame[i..i + 3] != background
                })
                .map(|k| k.y)
                .collect()
        };
        let bottom = layout.keys.iter().map(|k| k.y).fold(0.0, f32::max);

        let loud = frame(&analysis(1.0, &[1.0; 16], false));
        assert_eq!(lit_tops(&loud).len(), layout.keys.len(), "every row full");

        let mut bass = [0.0; 16];
        bass[0] = 1.0;
        let only_bass = lit_tops(&frame(&analysis(0.5, &bass, false)));
        assert!(!only_bass.is_empty(), "the bass shows");
        assert!(
            only_bass.iter().all(|&y| y >= bottom - 0.5),
            "on the bottom row only: {only_bass:?}"
        );
    }

    /// A frame of a shipped effect with these settings and this analysis.
    fn heard_with(ctx: &Context, time: f64, params: &str, audio: &str) -> Vec<u8> {
        render_with(
            ctx,
            time,
            0,
            &FrameInputs {
                params,
                presses: "",
                clock_ms: 0.0,
                bound: "",
                signals: "",
                audio,
                system: "",
            },
            crate::default_layout().led_count(),
        )
        .expect("render")
    }

    /// The horizontal positions of the keys a frame lights over its background.
    fn lit_x(frame: &[u8], background: [u8; 3]) -> Vec<f32> {
        crate::default_layout()
            .keys
            .iter()
            .filter(|k| {
                let i = usize::from(k.index) * 3;
                frame[i..i + 3] != background
            })
            .map(|k| k.x)
            .collect()
    }

    /// Mirrored, the lowest band lights the middle of the keyboard, not an edge.
    #[test]
    fn the_mirrored_equalizer_puts_low_notes_in_the_middle() {
        let layout = crate::default_layout();
        let (_rt, ctx) = prepare(crate::shipped::source("Equalizer"), layout).expect("load");
        let mut bass = [0.0; 16];
        bass[0] = 1.0;
        let frame = heard_with(
            &ctx,
            0.0,
            r#"{"bars":"mirror"}"#,
            &analysis(0.5, &bass, false),
        );
        let width = layout.keys.iter().map(|k| k.x + k.w).fold(0.0, f32::max);
        let lit = lit_x(&frame, [4, 8, 14]);
        assert!(!lit.is_empty(), "the bass shows");
        assert!(
            lit.iter().all(|&x| (x - width / 2.0).abs() < width / 4.0),
            "in the middle: {lit:?}"
        );
    }

    /// Waterfall: a sound comes in on the right, then slides left.
    #[test]
    fn the_waterfall_slides_the_sound_from_right_to_left() {
        let layout = crate::default_layout();
        let (_rt, ctx) = prepare(crate::shipped::source("Waterfall"), layout).expect("load");
        let background = [2, 4, 12];
        let silent = analysis(0.0, &[0.0; 16], false);
        let loud = analysis(1.0, &[1.0; 16], false);
        let width = layout.keys.iter().map(|k| k.x + k.w).fold(0.0, f32::max);

        assert!(lit_x(&heard_with(&ctx, 0.0, "{}", &silent), background).is_empty());
        let now = lit_x(&heard_with(&ctx, 0.1, "{}", &loud), background);
        assert!(!now.is_empty(), "the sound shows at once");
        assert!(
            now.iter().all(|&x| x > width / 2.0),
            "on the right: {now:?}"
        );
        // One second of silence later, at ten keys a second, it is ten keys on.
        let later = lit_x(&heard_with(&ctx, 1.1, "{}", &silent), background);
        let leftmost = |xs: &[f32]| xs.iter().copied().fold(f32::MAX, f32::min);
        assert!(!later.is_empty(), "it is still on its way");
        assert!(
            leftmost(&later) < leftmost(&now) - 5.0,
            "further left: {later:?}"
        );
    }

    /// Beat ripples: a beat starts a ring at the middle, which moves out.
    #[test]
    fn the_beat_ripples_spread_from_the_middle() {
        let layout = crate::default_layout();
        let (_rt, ctx) = prepare(crate::shipped::source("Beat ripples"), layout).expect("load");
        let background = [4, 6, 16];
        let silent = analysis(0.0, &[0.0; 16], false);
        let width = layout.keys.iter().map(|k| k.x + k.w).fold(0.0, f32::max);
        let spread = |xs: &[f32]| {
            xs.iter()
                .map(|x| (x - width / 2.0).abs())
                .fold(0.0, f32::max)
        };

        assert!(lit_x(&heard_with(&ctx, 0.0, "{}", &silent), background).is_empty());
        let _ = heard_with(&ctx, 1.0, "{}", &analysis(0.8, &[0.8; 16], true));
        let soon = lit_x(&heard_with(&ctx, 1.1, "{}", &silent), background);
        let later = lit_x(&heard_with(&ctx, 1.4, "{}", &silent), background);
        assert!(!soon.is_empty() && !later.is_empty(), "the ring shows");
        assert!(spread(&later) > spread(&soon), "and moves out");
    }

    /// Fireworks: a beat sets off bursts, which are gone within a second.
    #[test]
    fn the_fireworks_burst_on_a_beat_and_fade() {
        let layout = crate::default_layout();
        let (_rt, ctx) = prepare(crate::shipped::source("Fireworks"), layout).expect("load");
        let background = [3, 3, 10];
        let silent = analysis(0.0, &[0.0; 16], false);

        assert!(lit_x(&heard_with(&ctx, 0.0, "{}", &silent), background).is_empty());
        let burst = lit_x(
            &heard_with(&ctx, 1.0, "{}", &analysis(1.0, &[1.0; 16], true)),
            background,
        );
        assert!(!burst.is_empty(), "a beat sets off bursts");
        let gone = lit_x(&heard_with(&ctx, 2.0, "{}", &silent), background);
        assert!(gone.is_empty(), "gone a second later: {} keys", gone.len());
    }

    /// Aurora: dim at rest, brighter with the bass, the low colour for low
    /// music and the high one for high music.
    #[test]
    fn the_aurora_follows_the_bass_and_where_the_music_sits() {
        let layout = crate::default_layout();
        // Summed over every key: the curtains leave some keys dark.
        let totals = |frame: &[u8]| -> [u32; 3] {
            let mut sum = [0u32; 3];
            for k in layout.keys {
                let i = usize::from(k.index) * 3;
                for c in 0..3 {
                    sum[c] += u32::from(frame[i + c]);
                }
            }
            sum
        };
        let music = |bands: &[usize]| {
            let mut b = [0.0; 16];
            for &i in bands {
                b[i] = 1.0;
            }
            analysis(0.8, &b, false)
        };
        let played = |audio: &str| {
            let (_rt, ctx) = prepare(crate::shipped::source("Aurora"), layout).expect("load");
            let mut last = Vec::new();
            for i in 0..=60 {
                last = heard_with(&ctx, f64::from(i) * 0.1, "{}", audio);
            }
            totals(&last)
        };

        let rest = played(&analysis(0.0, &[0.0; 16], false));
        assert!(rest.iter().sum::<u32>() > 0, "it drifts dimly at rest");
        let low = played(&music(&[0, 1]));
        assert!(
            low.iter().sum::<u32>() > rest.iter().sum::<u32>() * 3 / 2,
            "music brightens it: {low:?} against {rest:?}"
        );
        assert!(
            low[1] > low[0],
            "low music takes the low colour, green: {low:?}"
        );
        let high = played(&music(&[14, 15]));
        assert!(
            high[2] > high[1],
            "high music takes the high colour, violet: {high:?}"
        );
    }

    /// Aurora moves on the beat: frames change more just after a beat than
    /// the same music without one.
    #[test]
    fn the_aurora_surges_on_the_beat() {
        let layout = crate::default_layout();
        let music = analysis(0.6, &[0.6; 16], false);
        let beat = analysis(0.6, &[0.6; 16], true);
        // How much the frame moves over a tenth of a second after `at`.
        let change = |beats: bool| -> u32 {
            let (_rt, ctx) = prepare(crate::shipped::source("Aurora"), layout).expect("load");
            for i in 0..20 {
                let _ = heard_with(&ctx, f64::from(i) * 0.05, "{}", &music);
            }
            let before = heard_with(&ctx, 1.0, "{}", if beats { &beat } else { &music });
            let after = heard_with(&ctx, 1.1, "{}", &music);
            before
                .iter()
                .zip(&after)
                .map(|(a, b)| u32::from(a.abs_diff(*b)))
                .sum()
        };
        let calm = change(false);
        let surged = change(true);
        assert!(
            surged > calm * 3 / 2,
            "a beat moves it more: {surged} against {calm}"
        );
    }

    /// The shipped Beat pulse: a beat flashes the keyboard, which fades after.
    #[test]
    fn the_beat_pulse_flashes_on_a_beat_and_fades() {
        let layout = crate::default_layout();
        let (_rt, ctx) = prepare(crate::shipped::source("Beat pulse"), layout).expect("load");
        let brightness =
            |frame: &[u8]| -> u32 { key(frame, 0).iter().map(|&c| u32::from(c)).sum() };

        let rest = brightness(&heard(&ctx, 0.0, &analysis(0.0, &[0.0; 16], false)));
        assert!(rest > 0, "at rest the colour glows dimly");
        let beat = brightness(&heard(&ctx, 1.0, &analysis(0.5, &[0.5; 16], true)));
        assert!(beat > rest * 4, "a beat flashes: {beat} against {rest}");
        let after = brightness(&heard(&ctx, 2.0, &analysis(0.0, &[0.0; 16], false)));
        assert!(after < beat / 2, "and fades: {after} after {beat}");
    }

    #[test]
    fn an_effect_declaring_nothing_is_handed_no_signal() {
        let drawn = frame_of(DECLARES_NOTHING, "", r#"{"count":7}"#);
        assert_eq!(key(&drawn, 2), [0, 0, 0]);
    }
}
