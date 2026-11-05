//! The engine's load and render path, as `apps/desktop/src-tauri/src/runtime`
//! runs it, to run both transpilers' output the way Candeo does.
//!
//! A copy of `prepare_with_layout`, `render_with` and `layout_json`, kept
//! short, rather than a call into them: those are private to the application
//! crate, and the scope of #364 is to change nothing there. What makes the
//! copy faithful is what it does not copy: `api.js` and `bootstrap.js` are the
//! application's own files, included from where they live, and rquickjs is the
//! same version with the same feature.
//!
//! Left out on purpose: key labels in the layout (they come from the system's
//! keyboard layout, and both outputs get the same layout anyway) and dimming,
//! which applies after the effect, outside its code.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use candeo_device::Layout;
use rquickjs::function::Args;
use rquickjs::loader::{BuiltinLoader, BuiltinResolver};
use rquickjs::{CatchResultExt, Context, Function, Module, Runtime};

/// The module the host provides.
pub const API_JS: &str = include_str!("../../../apps/desktop/src-tauri/src/runtime/api.js");
/// The glue that imports the effect and installs the render function.
const BOOTSTRAP_JS: &str = include_str!("../../../apps/desktop/src-tauri/src/runtime/bootstrap.js");

/// `MEMORY_BUDGET` in `runtime/mod.rs`.
const MEMORY_BUDGET: usize = 32 * 1024 * 1024;
/// `swatch::BUDGET`, which `declared_manifest` loads a module under.
const LOAD_BUDGET: Duration = Duration::from_secs(2);
/// `FRAME_BUDGET` as a debug build has it: generous, so that a slow machine
/// does not make a comparison fail.
const FRAME_BUDGET: Duration = Duration::from_millis(200);

/// A module loaded, ready to render.
pub struct Loaded {
    // Kept for as long as the context: it carries the module loader.
    _rt: Runtime,
    ctx: Context,
    deadline: Rc<Cell<Instant>>,
    frame_len: usize,
}

/// What one frame hands the effect besides the time, each as the bootstrap
/// reads it: JSON, or empty for none.
#[derive(Clone, Debug, Default)]
pub struct FrameInputs {
    pub time: f64,
    pub frame_index: u32,
    pub params: String,
    pub presses: String,
    pub clock_ms: f64,
    pub signals: String,
    pub audio: String,
    pub system: String,
}

/// Loads `js` as the module `effect` on `layout`, as the engine does for a
/// swatch or a manifest: one deadline for the module body.
pub fn load(js: &str, layout: &'static Layout) -> Result<Loaded, String> {
    let rt = Runtime::new().map_err(|e| format!("QuickJS : {e}"))?;
    let deadline = Rc::new(Cell::new(Instant::now() + LOAD_BUDGET));
    let watched = Rc::clone(&deadline);
    rt.set_interrupt_handler(Some(Box::new(move || Instant::now() >= watched.get())));
    rt.set_memory_limit(MEMORY_BUDGET);

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
        g.set("__candeo_frame_len", layout.led_count() as u32)
            .map_err(|e| format!("QuickJS : {e}"))?;
        g.set("__candeo_layout", layout_json(layout))
            .map_err(|e| format!("QuickJS : {e}"))?;
        Module::evaluate(ctx.clone(), "bootstrap", BOOTSTRAP_JS)
            .catch(&ctx)
            .map_err(|e| format!("loading the effect: {e}"))?
            .finish::<()>()
            .catch(&ctx)
            .map_err(|e| format!("loading the effect: {e}"))?;
        Ok(())
    })?;

    Ok(Loaded {
        _rt: rt,
        ctx,
        deadline,
        frame_len: layout.led_count(),
    })
}

impl Loaded {
    /// The manifest the module declares, as JSON: what `cache_effect` records.
    pub fn manifest(&self) -> Result<String, String> {
        self.ctx
            .with(|ctx| ctx.globals().get::<_, String>("__candeo_manifest"))
            .map_err(|e| format!("unreadable manifest: {e}"))
    }

    /// One frame, under the frame budget.
    pub fn render(&self, inputs: &FrameInputs) -> Result<Vec<u8>, String> {
        self.deadline.set(Instant::now() + FRAME_BUDGET);
        self.ctx.with(|ctx| {
            let render: Function = ctx
                .globals()
                .get("__candeo_render")
                .map_err(|_| "the render function is gone from the context".to_string())?;
            let mut args = Args::new(ctx.clone(), 9);
            let pushed = (|| {
                args.push_arg(inputs.time)?;
                args.push_arg(inputs.frame_index)?;
                args.push_arg(inputs.params.as_str())?;
                args.push_arg(inputs.presses.as_str())?;
                args.push_arg(inputs.clock_ms)?;
                // Nothing bound: parameters come as configured.
                args.push_arg("")?;
                args.push_arg(inputs.signals.as_str())?;
                args.push_arg(inputs.audio.as_str())?;
                args.push_arg(inputs.system.as_str())
            })();
            pushed.map_err(|e| format!("the frame's inputs could not be handed over: {e}"))?;
            let out: Vec<u8> = render
                .call_arg(args)
                .catch(&ctx)
                .map_err(|e| format!("{e}"))?;
            if out.len() != self.frame_len * 3 {
                return Err(format!(
                    "the effect rendered {} bytes, {} expected",
                    out.len(),
                    self.frame_len * 3
                ));
            }
            Ok(out)
        })
    }
}

/// `layout_json` in `runtime/mod.rs`, without key labels.
pub fn layout_json(l: &'static Layout) -> String {
    let mut keys = String::new();
    for row in 0..l.rows {
        for col in 0..l.cols {
            let Some(k) = l.at(row, col).and_then(|p| l.key(p)) else {
                continue;
            };
            if !keys.is_empty() {
                keys.push(',');
            }
            let mut fields = format!(r#""index":{},"row":{row},"col":{col}"#, k.index);
            if k.scancode != candeo_device::NO_SCANCODE {
                fields.push_str(&format!(r#","scancode":{}"#, k.scancode));
            }
            keys.push_str(&format!(
                r#"{{{fields},"x":{},"y":{},"w":{},"h":{}}}"#,
                k.x, k.y, k.w, k.h
            ));
        }
    }
    format!(
        r#"{{"name":{},"rows":{},"cols":{},"keys":[{keys}]}}"#,
        serde_json::to_string(l.name).expect("a string serialises"),
        l.rows,
        l.cols
    )
}
