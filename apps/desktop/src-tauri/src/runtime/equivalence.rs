//! The move from the window's TypeScript to Oxc (#381), checked frame by frame
//! through the engine's own load and render path: every shipped effect, the
//! typed cases of #364 and its two cases where class fields behave
//! differently, render the same bytes either way.
//!
//! `fixtures/typescript/` holds what `transpile()` in
//! `apps/desktop/src/editor/effect.ts` gave before the move: Monaco 0.57's
//! TypeScript 5.9.3, `transpileModule` for ES2020. Each file starts with the
//! SHA-256 of the source it was compiled from. A shipped effect's source is
//! its own, in `packages/effects/`; the typed cases, since no shipped effect
//! carries types, keep theirs next to their output.
//!
//! A shipped effect changed since the move has no TypeScript output left to
//! compare with: the test says so, and its fixture goes. Oxc's output is the
//! reference from then on, and these cases are what an upgrade of Oxc
//! compares again.

use std::fs;
use std::path::{Path, PathBuf};

use candeo_device::Layout;
use sha2::{Digest, Sha256};

use super::{prepare_with, render_with, FrameInputs};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/typescript")
}

struct Case {
    name: String,
    source: String,
    /// What the window's TypeScript compiled it to.
    typescript: String,
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn cases() -> Vec<Case> {
    let mut cases: Vec<Case> = crate::shipped::ALL
        .iter()
        .map(|s| Case {
            name: s.name.to_owned(),
            source: s.source.to_owned(),
            typescript: read(&fixtures().join(format!("{}.js", s.name))),
        })
        .collect();
    let typed = fixtures().join("typed");
    let mut sources: Vec<PathBuf> = fs::read_dir(&typed)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "ts"))
        .collect();
    sources.sort();
    for source in sources {
        cases.push(Case {
            name: source.file_stem().unwrap().to_string_lossy().into_owned(),
            source: read(&source),
            typescript: read(&source.with_extension("js")),
        });
    }
    cases
}

/// The SHA-256 a fixture's first line names.
fn compiled_from(js: &str) -> &str {
    js.lines()
        .next()
        .and_then(|l| l.rsplit("SHA-256: ").next())
        .unwrap_or("")
}

fn sha256(source: &str) -> String {
    Sha256::digest(source.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// One frame's inputs, owned.
#[derive(Default)]
struct Frame {
    time: f64,
    index: u32,
    presses: String,
    clock_ms: f64,
    signals: String,
    audio: String,
    system: String,
}

/// A few frames with no input, as a swatch samples them.
fn at_rest() -> Vec<Frame> {
    [0.0, 0.25, 1.0, 2.5, 7.5]
        .iter()
        .enumerate()
        .map(|(i, &time)| Frame {
            time,
            index: i as u32,
            ..Frame::default()
        })
        .collect()
}

/// Three seconds at 30 frames per second, with every input an effect may
/// read: presses, the clock, signals, sound, the system. Each effect reads
/// only those it declares.
fn active() -> Vec<Frame> {
    const PRESSES: [(u32, f64); 5] = [(3, 0.2), (17, 0.7), (40, 1.1), (60, 1.9), (5, 2.4)];
    // Any fixed instant: 2026-11-05 12:34:56.789 UTC.
    const CLOCK_MS: f64 = 1_793_882_096_789.0;
    (0..90u32)
        .map(|i| {
            let time = f64::from(i) / 30.0;
            let presses: Vec<String> = PRESSES
                .iter()
                .filter(|(_, at)| *at <= time)
                .map(|(k, at)| format!(r#"{{"k":{k},"at":{at}}}"#))
                .collect();
            let beat = i % 15 == 0;
            let bands: Vec<String> = (0..16)
                .map(|b| {
                    let v = 0.5 + 0.5 * (time * 3.0 + f64::from(b) * 0.7).sin();
                    format!("{:.4}", v * if b < 4 && beat { 1.0 } else { 0.7 })
                })
                .collect();
            let level = 0.4 + 0.3 * (time * 2.0).sin();
            Frame {
                time,
                index: i,
                presses: format!("[{}]", presses.join(",")),
                clock_ms: CLOCK_MS + time * 1000.0,
                signals: format!(
                    concat!(
                        r#"{{"cs2.health":{health},"cs2.ammo":40,"cs2.flashed":{flash},"#,
                        r#""cs2.burning":0,"cs2.bomb":"planted","#,
                        r#""dota2.health":55,"dota2.mana":80,"dota2.respawn":"","#,
                        r#""dota2.stunned":"yes","dota2.smoked":"no","dota2.daytime":"night","#,
                        r#""build":"failed","deploy":"running","tests":"passed","load":0.7}}"#
                    ),
                    health = 100 - i,
                    flash = if (30..45).contains(&i) { 80 } else { 0 },
                ),
                audio: format!(
                    r#"{{"level":{level:.4},"peak":{:.4},"bands":[{}],"beat":{beat},"onset":{}}}"#,
                    level + 0.1,
                    bands.join(","),
                    if beat { 0.8 } else { 0.1 },
                ),
                system: format!(
                    concat!(
                        r#"{{"cpu":{cpu:.4},"memory":0.63,"gpu":0.2,"cpuTemp":0.5,"gpuTemp":0.4,"#,
                        r#""cpuPower":0.3,"cpuCelsius":67.5,"gpuCelsius":52,"cpuWatts":45}}"#
                    ),
                    cpu = 0.5 + 0.4 * time.sin(),
                ),
            }
        })
        .collect()
}

/// The parameters an effect gets when nobody changed them: each `default` its
/// manifest declares.
fn default_params(manifest: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(manifest).unwrap_or_default();
    let mut params = serde_json::Map::new();
    if let Some(specs) = value.get("params").and_then(|p| p.as_object()) {
        for (name, spec) in specs {
            if let Some(default) = spec.get("default") {
                params.insert(name.clone(), default.clone());
            }
        }
    }
    serde_json::Value::Object(params).to_string()
}

/// What a module did on a layout: its manifest, then for each scenario, in a
/// context of its own, the frames until the first error, and that error's
/// first line: the message, without positions in one JavaScript or the other.
#[derive(Debug, PartialEq)]
struct Run {
    manifest: String,
    scenarios: Vec<(Vec<Vec<u8>>, Option<String>)>,
}

fn first_line(e: &str) -> String {
    e.lines().next().unwrap_or("").to_owned()
}

fn run(js: &str, layout: &'static Layout) -> Result<Run, String> {
    let manifest = {
        let (_rt, ctx) = prepare_with(js, layout, None).map_err(|e| first_line(&e))?;
        ctx.with(|ctx| ctx.globals().get::<_, String>("__candeo_manifest"))
            .map_err(|e| e.to_string())?
    };
    let params = default_params(&manifest);
    let mut scenarios = Vec::new();
    for frames in [at_rest(), active()] {
        let (_rt, ctx) = prepare_with(js, layout, None).map_err(|e| first_line(&e))?;
        let mut rendered = Vec::new();
        let mut error = None;
        for f in &frames {
            let inputs = FrameInputs {
                params: &params,
                presses: &f.presses,
                clock_ms: f.clock_ms,
                bound: "",
                signals: &f.signals,
                audio: &f.audio,
                system: &f.system,
            };
            match render_with(&ctx, f.time, f.index, &inputs, layout.led_count()) {
                Ok(bytes) => rendered.push(bytes),
                Err(e) => {
                    error = Some(first_line(&e));
                    break;
                }
            }
        }
        scenarios.push((rendered, error));
    }
    Ok(Run {
        manifest,
        scenarios,
    })
}

/// The shipped effects and the 6 typed cases, each on every built-in layout:
/// 95 frames a layout, compared byte for byte, with the manifest and any
/// error.
#[test]
fn every_case_renders_the_same_frames_compiled_by_oxc_as_by_typescript() {
    let cases = cases();
    assert_eq!(cases.len(), crate::shipped::ALL.len() + 6);
    let mut stale = Vec::new();
    for case in &cases {
        if compiled_from(&case.typescript) != sha256(&case.source) {
            stale.push(case.name.clone());
            continue;
        }
        let oxc = super::compile(&case.source)
            .unwrap_or_else(|e| panic!("{}: {}", case.name, e[0]))
            .js;
        for layout in candeo_device::definition::builtin() {
            let before = run(&case.typescript, layout);
            let after = run(&oxc, layout);
            let at = format!("{} on {}", case.name, layout.name);
            match (&before, &after) {
                (Ok(b), Ok(a)) => {
                    assert_eq!(b.manifest, a.manifest, "{at}: manifests differ");
                    for (s, ((bf, be), (af, ae))) in
                        b.scenarios.iter().zip(&a.scenarios).enumerate()
                    {
                        if let Some(i) = bf.iter().zip(af).position(|(x, y)| x != y) {
                            panic!("{at}: scenario {s}, frame {i} differs");
                        }
                        assert_eq!(bf.len(), af.len(), "{at}: scenario {s}");
                        assert_eq!(be, ae, "{at}: scenario {s}");
                    }
                }
                _ => assert_eq!(before, after, "{at}"),
            }
        }
    }
    assert!(
        stale.is_empty(),
        "changed since the move to Oxc (#381), so no TypeScript output describes \
         them any more: delete their fixture in fixtures/typescript/: {stale:?}"
    );
}
