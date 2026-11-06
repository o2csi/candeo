//! The comparison of #364: every shipped effect, typed fixtures and
//! adversarial cases through the current path (`transpile()` from
//! `apps/desktop/src/editor/effect.ts`, run in Node) and through Oxc, then both
//! outputs through the engine's load and render path, frame by frame.
//!
//! ```text
//! cargo run --release -- compare                 the whole comparison, as Markdown
//! cargo run --release -- transpile <file.ts>     Oxc's JavaScript, or its errors
//! cargo run --release -- load <file.js>          that JavaScript through the engine
//! cargo run --release -- typescript-in-quickjs   Monaco's TypeScript run in QuickJS
//! ```
//!
//! `transpile`, `load` and `typescript-in-quickjs` take `--stack-mb N` to run on
//! a thread of that size rather than the main thread.
//!
//! Each case runs in a process of its own (`case`), so that one which crashes
//! the transpiler — a stack overflow aborts a Rust process — is recorded
//! instead of ending the comparison. Results: `docs/design/headless-transpile-poc.md`.

mod engine;
mod scenario;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use candeo_device::Layout;
use oxc_transpile::{as_module, check_imports, exports_of, transpile, Options, Target};
use serde_json::{json, Value};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("compare") | None => compare(),
        Some("case") => case(&args[1..]),
        Some("transpile") => on_stack(&args[1..], print_transpiled),
        Some("load") => on_stack(&args[1..], |args| {
            let js = fs::read_to_string(&args[0]).expect("readable JavaScript");
            println!("{}", describe(&run_everywhere(&js)));
        }),
        Some("typescript-in-quickjs") => on_stack(&args[1..], typescript_in_quickjs),
        Some(other) => {
            eprintln!(
                "unknown command {other}: compare | case | transpile | load | typescript-in-quickjs"
            );
            std::process::exit(2);
        }
    }
}

/// Runs `f` on the main thread, or on a thread of `--stack-mb N` megabytes:
/// how deep a nesting each part survives depends on nothing else.
fn on_stack(args: &[String], f: fn(&[String])) {
    let stack_mb = args
        .iter()
        .position(|a| a == "--stack-mb")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse::<usize>().ok());
    match stack_mb {
        Some(mb) => {
            let args = args.to_vec();
            std::thread::Builder::new()
                .stack_size(mb * 1024 * 1024)
                .spawn(move || f(&args))
                .unwrap()
                .join()
                .unwrap();
        }
        None => f(args),
    }
}

/// The repository, from this crate's own place in it.
fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn poc() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// ---------------------------------------------------------------- compare

/// Where a case comes from, which decides the table it goes in.
const DIRS: &[(&str, &str)] = &[
    ("shipped", "packages/effects"),
    ("typed", "tools/transpile-poc/cases/typed"),
    ("adversarial", "tools/transpile-poc/cases/adversarial"),
    (
        "generated",
        "tools/transpile-poc/target/poc-cases/generated",
    ),
    ("deep", "tools/transpile-poc/target/poc-cases/deep"),
];

fn compare() {
    let work = poc().join("target/poc-cases");
    let _ = fs::remove_dir_all(&work);
    generate(&work);

    let ts_json = work.join("typescript.json");
    let mut node = Command::new("node");
    node.current_dir(repo())
        .arg("--no-warnings")
        .arg(poc().join("harness/typescript.mjs"))
        .arg(&ts_json);
    for (_, dir) in DIRS {
        node.arg(repo().join(dir));
    }
    let status = node.status().expect("node runs");
    assert!(status.success(), "the TypeScript harness failed");
    let ts: Value =
        serde_json::from_str(&fs::read_to_string(&ts_json).expect("harness output")).unwrap();

    let ts_out = work.join("ts-out");
    fs::create_dir_all(&ts_out).unwrap();
    let mut rows = Vec::new();
    for (i, result) in ts["results"].as_array().unwrap().iter().enumerate() {
        let dir = Path::new(result["dir"].as_str().unwrap());
        let file = result["file"].as_str().unwrap();
        let category = DIRS
            .iter()
            .find(|(_, d)| dir.ends_with(d))
            .map_or("?", |(c, _)| *c);
        let base = ts_out.join(i.to_string());
        match result["js"].as_str() {
            Some(js) => fs::write(base.with_extension("js"), js).unwrap(),
            None => fs::write(
                base.with_extension("error"),
                result["error"].as_str().unwrap_or("no output"),
            )
            .unwrap(),
        }
        let source = dir.join(file);
        let stacks: &[Option<u32>] = if category == "deep" {
            &[None, Some(8)]
        } else {
            &[None]
        };
        for stack in stacks {
            let mut child = Command::new(std::env::current_exe().unwrap());
            child.arg("case").arg(&source).arg(&base);
            if let Some(mb) = stack {
                child.arg("--stack-mb").arg(mb.to_string());
            }
            let out = child.output().expect("the case runs");
            let report: Value = if out.status.success() {
                serde_json::from_slice(&out.stdout)
                    .unwrap_or_else(|e| json!({ "crash": e.to_string() }))
            } else {
                json!({ "crash": crash(out.status.code(), &out.stderr) })
            };
            rows.push(json!({
                "category": category,
                "file": file,
                "stack": stack.map_or("main thread".to_string(), |mb| format!("{mb} MB thread")),
                "bytes": fs::metadata(&source).map(|m| m.len()).unwrap_or(0),
                "ts": result,
                "report": report,
            }));
        }
    }
    let full = json!({ "typescript": {
        "version": ts["typescriptVersion"], "node": ts["node"],
        "coldMs": ts["coldMs"], "heapAfterLoadMb": ts["heapAfterLoadMb"] },
        "rows": rows });
    fs::write(
        work.join("report.json"),
        serde_json::to_string_pretty(&full).unwrap(),
    )
    .unwrap();
    print_report(&full);
}

/// What ended a case's process, and in which step: [`run_case`] names each
/// one on stderr before it starts.
fn crash(code: Option<i32>, stderr: &[u8]) -> String {
    let tail = String::from_utf8_lossy(stderr);
    let step = tail
        .lines()
        .rev()
        .find_map(|l| l.strip_prefix("step: "))
        .unwrap_or("?");
    match code {
        // STATUS_STACK_OVERFLOW
        Some(-1_073_741_571) => format!("process aborted, stack overflow, in {step}"),
        Some(c) => format!("process exited with {c}, in {step}"),
        None => format!("process killed, in {step}"),
    }
}

/// The cases written by the program: too big, or with bytes Git would change.
fn generate(work: &Path) {
    let generated = work.join("generated");
    let deep = work.join("deep");
    fs::create_dir_all(&generated).unwrap();
    fs::create_dir_all(&deep).unwrap();

    // The editor's template, as `template.ts` makes it from the API's example.
    let example = fs::read_to_string(repo().join("packages/effects-api/src/example.ts")).unwrap();
    fs::write(
        generated.join("Template.ts"),
        example.replace("'./index'", "'@candeo/effects-api'"),
    )
    .unwrap();

    let missing =
        fs::read_to_string(poc().join("cases/adversarial/Syntax - missing parenthesis.ts"))
            .unwrap()
            .replace("\r\n", "\n");
    fs::write(
        generated.join("Syntax - CRLF line endings.ts"),
        missing.replace('\n', "\r\n"),
    )
    .unwrap();
    let template = fs::read_to_string(generated.join("Template.ts")).unwrap();
    fs::write(
        generated.join("Byte order mark.ts"),
        format!("\u{feff}{template}"),
    )
    .unwrap();

    fs::write(generated.join("Large - 1 MB.ts"), large(6_000, 60_000)).unwrap();
    fs::write(generated.join("Large - 8 MB.ts"), large(48_000, 480_000)).unwrap();

    for depth in [500usize, 1_000, 2_000, 10_000, 100_000] {
        let source = format!(
            "import {{ defineEffect }} from '@candeo/effects-api'\n\
             const deep = {}{}\n\
             export default defineEffect({{ render({{ frame }}) {{ frame.fill({{ r: deep.length, g: 0, b: 0 }}) }} }})\n",
            "[".repeat(depth),
            "]".repeat(depth)
        );
        fs::write(deep.join(format!("Deep nesting - {depth}.ts")), source).unwrap();
    }
}

/// An effect of `functions` small typed functions and a table of `numbers`.
fn large(functions: usize, numbers: usize) -> String {
    let mut s = String::from("import { defineEffect, hsv } from '@candeo/effects-api'\n\n");
    s.push_str("const TABLE: readonly number[] = [\n");
    for i in 0..numbers {
        s.push_str(&format!("  {:.5},\n", (i as f64 * 0.618_034) % 1.0));
    }
    s.push_str("]\n\n");
    for i in 0..functions {
        s.push_str(&format!(
            "function f{i}(x: number): number {{\n  return (x * {} + TABLE[{} % TABLE.length]! * 360) % 360\n}}\n",
            i % 17,
            i * 31
        ));
    }
    s.push_str("const FNS: ReadonlyArray<(x: number) => number> = [\n");
    for i in 0..functions {
        s.push_str(&format!("  f{i},\n"));
    }
    s.push_str(
        "]\n\nexport default defineEffect({\n  render({ layout, frame, time }) {\n    \
         for (const [i, key] of layout.keys.entries()) frame.set(key, hsv(FNS[i % FNS.length]!(time * 100), 1, 1))\n  },\n})\n",
    );
    s
}

// ---------------------------------------------------------------- one case

fn case(args: &[String]) {
    let source_path = PathBuf::from(&args[0]);
    let base = PathBuf::from(&args[1]);
    let stack_mb = args
        .iter()
        .position(|a| a == "--stack-mb")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse::<usize>().ok());
    let run = move || run_case(&source_path, &base);
    let report = match stack_mb {
        Some(mb) => std::thread::Builder::new()
            .stack_size(mb * 1024 * 1024)
            .spawn(run)
            .unwrap()
            .join()
            .unwrap(),
        None => run(),
    };
    println!("{report}");
}

fn run_case(source_path: &Path, base: &Path) -> Value {
    let source = fs::read_to_string(source_path).expect("readable source");
    let api_exports = exports_of(engine::API_JS);
    let esnext = Options::default();
    let es2020 = Options {
        target: Target::Es2020,
        ..Options::default()
    };

    // Timed: the first call in a fresh process, then the median of warm calls.
    eprintln!("step: Oxc");
    let started = Instant::now();
    let first = transpile(&source, &esnext);
    let first_ms = started.elapsed().as_secs_f64() * 1000.0;
    let runs = if source.len() > 200_000 { 3 } else { 20 };
    let mut times: Vec<f64> = (0..runs)
        .map(|_| {
            let t = Instant::now();
            let _ = transpile(&source, &esnext);
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    times.sort_by(f64::total_cmp);
    let median_ms = times[times.len() / 2];

    let oxc = match &first {
        Ok(out) => json!({
            "ok": true, "firstMs": first_ms, "medianMs": median_ms,
            "arenaBytes": out.arena_bytes, "jsBytes": out.js.len(),
            "imports": check_imports(&source, &out.js, &api_exports)
                .iter().map(ToString::to_string).collect::<Vec<_>>(),
        }),
        Err(errors) => json!({
            "ok": false, "firstMs": first_ms, "medianMs": median_ms,
            "errors": errors.iter().map(ToString::to_string).collect::<Vec<_>>(),
        }),
    };
    let oxc_js = as_module(&source, &esnext);
    let oxc2020_js = as_module(&source, &es2020);

    let ts_js = fs::read_to_string(base.with_extension("js")).ok();
    eprintln!("step: QuickJS loading the TypeScript output");
    let ts_runs = match &ts_js {
        Some(js) => run_everywhere(js),
        None => vec![Run {
            layout: "-",
            scenario: "-",
            outcome: Outcome::NoOutput(
                fs::read_to_string(base.with_extension("error")).unwrap_or_default(),
            ),
        }],
    };
    eprintln!("step: QuickJS loading the Oxc output");
    let oxc_runs = run_everywhere(&oxc_js);
    let oxc2020_runs = run_everywhere(&oxc2020_js);

    // A runtime error, brought back to the source by Oxc's source map: what
    // neither path does today.
    let mapped = runtime_error(&oxc_runs).and_then(|error| {
        let out = transpile(
            &source,
            &Options {
                source_map: true,
                ..esnext
            },
        )
        .ok()?;
        Some(remap(&error, |line, column| out.original(line, column)))
    });

    json!({
        "oxcMappedError": mapped,
        "oxc": oxc,
        "oxcEs2020": {
            "helpers": oxc2020_js.contains("babelHelpers"),
            "describe": describe(&oxc2020_runs),
        },
        "tsDescribe": describe(&ts_runs),
        "oxcDescribe": describe(&oxc_runs),
        "tsVsOxc": verdict(&ts_runs, &oxc_runs),
        "tsVsOxcEs2020": verdict(&ts_runs, &oxc2020_runs),
        "tsJsBytes": ts_js.as_ref().map_or(0, String::len),
    })
}

fn runtime_error(runs: &[Run]) -> Option<String> {
    runs.iter().find_map(|r| match &r.outcome {
        Outcome::Loaded {
            error: Some((_, e)),
            ..
        } => Some(e.clone()),
        _ => None,
    })
}

/// `at render (effect:12:22)` → `at render (effect.ts:21:5)`.
fn remap(error: &str, original: impl Fn(u32, u32) -> Option<(u32, u32)>) -> String {
    error
        .lines()
        .map(|line| {
            let Some(at) = line.find("(effect:") else {
                return line.to_string();
            };
            let inner = &line[at + 8..line.len().saturating_sub(1)];
            let mut parts = inner.split(':').map(|p| p.parse::<u32>().ok());
            match (parts.next().flatten(), parts.next().flatten()) {
                (Some(l), Some(c)) => match original(l, c) {
                    Some((sl, sc)) => format!("{}(effect.ts:{sl}:{sc})", &line[..at]),
                    None => line.to_string(),
                },
                _ => line.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

enum Outcome {
    /// The transpiler produced nothing: the call itself failed.
    NoOutput(String),
    LoadFailed(String),
    Loaded {
        manifest: String,
        frames: Vec<Vec<u8>>,
        error: Option<(usize, String)>,
    },
}

struct Run {
    layout: &'static str,
    scenario: &'static str,
    outcome: Outcome,
}

/// Both scenarios on every built-in layout; a module that does not load is
/// tried once, since the load budget makes each try cost two seconds.
fn run_everywhere(js: &str) -> Vec<Run> {
    let mut runs = Vec::new();
    for layout in candeo_device::definition::builtin() {
        let layout: &'static Layout = layout;
        let manifest = match engine::load(js, layout).and_then(|l| l.manifest()) {
            Ok(m) => m,
            Err(e) => {
                runs.push(Run {
                    layout: layout.name,
                    scenario: "load",
                    outcome: Outcome::LoadFailed(e),
                });
                return runs;
            }
        };
        let params = scenario::default_params(&manifest);
        for (scenario, frames) in [
            ("rest", scenario::at_rest(&params)),
            ("active", scenario::active(&params)),
        ] {
            let outcome = match engine::load(js, layout) {
                Err(e) => Outcome::LoadFailed(e),
                Ok(loaded) => {
                    let mut rendered = Vec::new();
                    let mut error = None;
                    for (i, inputs) in frames.iter().enumerate() {
                        match loaded.render(inputs) {
                            Ok(bytes) => rendered.push(bytes),
                            Err(e) => {
                                error = Some((i, e));
                                break;
                            }
                        }
                    }
                    Outcome::Loaded {
                        manifest: manifest.clone(),
                        frames: rendered,
                        error,
                    }
                }
            };
            runs.push(Run {
                layout: layout.name,
                scenario,
                outcome,
            });
        }
    }
    runs
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("")
}

fn describe(runs: &[Run]) -> String {
    match &runs[0].outcome {
        Outcome::NoOutput(e) => return format!("no output: {}", first_line(e)),
        Outcome::LoadFailed(e) => return format!("does not load: {}", first_line(e)),
        Outcome::Loaded { .. } => {}
    }
    let mut frames = 0;
    let mut layouts = std::collections::BTreeSet::new();
    for run in runs {
        layouts.insert(run.layout);
        if let Outcome::Loaded {
            frames: f, error, ..
        } = &run.outcome
        {
            frames += f.len();
            if let Some((i, e)) = error {
                // The message and the innermost frame: where it threw.
                let at = e
                    .lines()
                    .find(|l| l.contains("(effect:"))
                    .unwrap_or("")
                    .trim();
                return format!("loads; frame {i} throws: {} {at}", first_line(e));
            }
        }
    }
    format!("renders {frames} frames on {} layouts", layouts.len())
}

/// Whether two outputs behave the same in the engine: the same manifest, the
/// same bytes in every frame, the same frame failing. Load and render error
/// texts are compared on their first line, which is where the message is.
fn verdict(a: &[Run], b: &[Run]) -> Value {
    if a.len() != b.len() {
        return json!({ "same": false, "detail": format!("{} runs against {}", a.len(), b.len()) });
    }
    let mut frames = 0;
    for (x, y) in a.iter().zip(b) {
        let place = format!("{} / {}", x.layout, x.scenario);
        match (&x.outcome, &y.outcome) {
            (Outcome::LoadFailed(e1), Outcome::LoadFailed(e2))
            | (Outcome::NoOutput(e1), Outcome::LoadFailed(e2))
            | (Outcome::LoadFailed(e1), Outcome::NoOutput(e2)) => {
                let same_text = first_line(e1) == first_line(e2);
                return json!({ "same": true, "detail": if same_text {
                    "both refuse to load, same message".to_string()
                } else {
                    "both refuse to load, messages differ".to_string()
                } });
            }
            (
                Outcome::Loaded {
                    manifest: m1,
                    frames: f1,
                    error: e1,
                },
                Outcome::Loaded {
                    manifest: m2,
                    frames: f2,
                    error: e2,
                },
            ) => {
                if m1 != m2 {
                    return json!({ "same": false, "detail": format!("{place}: manifests differ") });
                }
                if let Some(i) = f1.iter().zip(f2).position(|(p, q)| p != q) {
                    return json!({ "same": false, "detail": format!("{place}: frame {i} differs") });
                }
                if f1.len() != f2.len() {
                    return json!({ "same": false, "detail": format!(
                        "{place}: {} frames against {}", f1.len(), f2.len()) });
                }
                match (e1, e2) {
                    (Some((i, t1)), Some((j, t2))) if i == j => {
                        if first_line(t1) != first_line(t2) {
                            return json!({ "same": true, "detail": format!(
                                "same frames, frame {i} throws in both, messages differ") });
                        }
                    }
                    (None, None) => {}
                    _ => {
                        return json!({ "same": false, "detail": format!("{place}: one throws, not the other") })
                    }
                }
                frames += f1.len();
            }
            _ => {
                return json!({ "same": false, "detail": format!("{place}: one loads, not the other") })
            }
        }
    }
    json!({ "same": true, "detail": format!("identical, {frames} frames") })
}

// ---------------------------------------------------------------- report

fn ms(v: &Value) -> String {
    v.as_f64().map_or("–".into(), |x| format!("{x:.2}"))
}

fn cell(s: &str) -> String {
    let s = s.replace('|', "\\|").replace('\n', " ");
    if s.chars().count() > 160 {
        format!("{}…", s.chars().take(160).collect::<String>())
    } else {
        s
    }
}

fn print_report(full: &Value) {
    let t = &full["typescript"];
    println!(
        "TypeScript {} (Monaco's) on Node {}: compiler loaded and first call in {} ms, {:.1} MB of heap after loading.\n",
        t["version"].as_str().unwrap_or("?"),
        t["node"].as_str().unwrap_or("?"),
        ms(&t["coldMs"]),
        t["heapAfterLoadMb"].as_f64().unwrap_or(0.0)
    );
    let rows = full["rows"].as_array().unwrap();
    let mut by: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
    for row in rows {
        by.entry(row["category"].as_str().unwrap())
            .or_default()
            .push(row);
    }
    // By depth rather than by name: the size grows with it.
    if let Some(deep) = by.get_mut("deep") {
        deep.sort_by_key(|r| r["bytes"].as_u64());
    }

    for category in ["shipped", "typed"] {
        println!("### {category}\n");
        println!("| Effect | Bytes | TS ms | Oxc ms | Oxc arena KB | TS ↔ Oxc in the engine | Oxc es2020 |");
        println!("|---|---:|---:|---:|---:|---|---|");
        let (mut ts_total, mut oxc_total) = (0.0, 0.0);
        for row in by.get(category).into_iter().flatten() {
            let r = &row["report"];
            ts_total += row["ts"]["ms"].as_f64().unwrap_or(0.0);
            oxc_total += r["oxc"]["medianMs"].as_f64().unwrap_or(0.0);
            println!(
                "| {} | {} | {} | {} | {} | {} | {} |",
                row["file"].as_str().unwrap().trim_end_matches(".ts"),
                row["bytes"],
                ms(&row["ts"]["ms"]),
                ms(&r["oxc"]["medianMs"]),
                r["oxc"]["arenaBytes"]
                    .as_u64()
                    .map_or("–".into(), |b| (b / 1024).to_string()),
                cell(&format!(
                    "{} {}",
                    if r["tsVsOxc"]["same"] == true {
                        "same:"
                    } else {
                        "**DIFFERENT**:"
                    },
                    r["tsVsOxc"]["detail"]
                        .as_str()
                        .unwrap_or(r["crash"].as_str().unwrap_or(""))
                )),
                cell(&format!(
                    "{}{}",
                    if r["tsVsOxcEs2020"]["same"] == true {
                        "same"
                    } else {
                        "different"
                    },
                    if r["oxcEs2020"]["helpers"] == true {
                        ", needs helpers"
                    } else {
                        ""
                    }
                )),
            );
        }
        println!("| **Total** | | **{ts_total:.2}** | **{oxc_total:.2}** | | | |\n");
    }

    for category in ["adversarial", "generated", "deep"] {
        println!("### {category}\n");
        println!("| Case | Current path: TS, then the engine | Oxc, then the engine | Oxc import check | Same behaviour? |");
        println!("|---|---|---|---|---|");
        for row in by.get(category).into_iter().flatten() {
            let r = &row["report"];
            let name = row["file"].as_str().unwrap().trim_end_matches(".ts");
            let name = if category == "deep" {
                format!("{name} ({})", row["stack"].as_str().unwrap_or(""))
            } else {
                name.to_string()
            };
            if let Some(c) = r["crash"].as_str() {
                println!(
                    "| {} | {} | **{}** | | |",
                    name,
                    cell(row["ts"]["error"].as_str().unwrap_or("see TS output")),
                    cell(c)
                );
                continue;
            }
            let oxc = if r["oxc"]["ok"] == true {
                let described = r["oxcDescribe"].as_str().unwrap_or("").to_string();
                match r["oxcMappedError"].as_str() {
                    Some(mapped) => format!(
                        "{described}, source map: {}",
                        mapped
                            .lines()
                            .find(|l| l.contains("effect.ts:"))
                            .map_or("", str::trim)
                    ),
                    None => described,
                }
            } else {
                format!(
                    "refused: {} → {}",
                    r["oxc"]["errors"][0].as_str().unwrap_or(""),
                    r["oxcDescribe"].as_str().unwrap_or("")
                )
            };
            let imports = r["oxc"]["imports"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .unwrap_or_default();
            println!(
                "| {} | {} | {} | {} | {} |",
                name,
                cell(r["tsDescribe"].as_str().unwrap_or("")),
                cell(&oxc),
                cell(&imports),
                cell(&format!(
                    "{} {}",
                    if r["tsVsOxc"]["same"] == true {
                        "yes:"
                    } else {
                        "**no**:"
                    },
                    r["tsVsOxc"]["detail"].as_str().unwrap_or("")
                )),
            );
        }
        println!();
    }

    println!("### timings beyond the shipped effects\n");
    println!("| Case | Bytes | TS ms | Oxc first call ms | Oxc ms | Oxc arena KB |");
    println!("|---|---:|---:|---:|---:|---:|");
    let timed = by
        .get("generated")
        .into_iter()
        .chain(by.get("deep"))
        .flatten()
        // A deep case once, from the run that survived: the 8 MB thread's.
        .filter(|r| r["category"] != "deep" || r["stack"] != "main thread");
    for row in timed {
        if row["report"]["crash"].is_string() {
            continue;
        }
        let r = &row["report"];
        println!(
            "| {} | {} | {} | {} | {} | {} |",
            row["file"].as_str().unwrap().trim_end_matches(".ts"),
            row["bytes"],
            ms(&row["ts"]["ms"]),
            ms(&r["oxc"]["firstMs"]),
            ms(&r["oxc"]["medianMs"]),
            r["oxc"]["arenaBytes"]
                .as_u64()
                .map_or("–".into(), |b| (b / 1024).to_string()),
        );
    }
}

// ---------------------------------------------------------------- transpile

fn print_transpiled(args: &[String]) {
    let source = fs::read_to_string(&args[0]).expect("readable source");
    let options = Options {
        target: if args.iter().any(|a| a == "--es2020") {
            Target::Es2020
        } else {
            Target::EsNext
        },
        source_map: false,
    };
    match transpile(&source, &options) {
        Ok(out) => {
            print!("{}", out.js);
            for problem in check_imports(&source, &out.js, &exports_of(engine::API_JS)) {
                eprintln!("import: {problem}");
            }
        }
        Err(errors) => {
            for e in errors {
                eprintln!("{e}");
            }
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------- TypeScript in QuickJS

/// The other headless option: Monaco's own TypeScript, run in the QuickJS
/// Candeo embeds. Run after `compare`, whose TypeScript outputs it checks its
/// own against, byte for byte.
fn typescript_in_quickjs(_args: &[String]) {
    use rquickjs::loader::{BuiltinLoader, BuiltinResolver};
    use rquickjs::{CatchResultExt, Context, Function, Module, Runtime};

    let lib = repo()
        .join("apps/desktop/node_modules/monaco-editor/esm/vs/languages/features/typescript/lib");
    let services = fs::read_to_string(lib.join("typescriptServices.js")).expect("Monaco installed");
    let libs = fs::read_to_string(lib.join("lib.js")).expect("Monaco installed");
    let driver = fs::read_to_string(poc().join("harness/typescript-in-quickjs.js")).unwrap();
    let api_source = fs::read_to_string(repo().join("packages/effects-api/src/index.ts")).unwrap();
    let ts_json: Value = serde_json::from_str(
        &fs::read_to_string(poc().join("target/poc-cases/typescript.json"))
            .expect("run `compare` first"),
    )
    .unwrap();

    let rt = Runtime::new().unwrap();
    // The compiler recurses deeper than an effect: QuickJS's own limit, under
    // the thread's, rather than its 1 MB default.
    rt.set_max_stack_size(32 * 1024 * 1024);
    rt.set_loader(
        BuiltinResolver::default()
            .with_module("typescript")
            .with_module("typescript-lib")
            .with_module("driver"),
        BuiltinLoader::default()
            .with_module("typescript", services)
            .with_module("typescript-lib", libs)
            .with_module("driver", driver),
    );
    let ctx = Context::full(&rt).unwrap();
    let started = Instant::now();
    ctx.with(|ctx| {
        Module::evaluate(ctx.clone(), "main", "import 'driver'")
            .catch(&ctx)
            .map_err(|e| e.to_string())
            .and_then(|p| p.finish::<()>().catch(&ctx).map_err(|e| e.to_string()))
            .expect("the compiler loads in QuickJS");
    });
    let load_ms = started.elapsed().as_secs_f64() * 1000.0;
    let after_load = rt.memory_usage().malloc_size;
    let version: String = ctx.with(|ctx| ctx.globals().get("__version").unwrap());
    println!(
        "TypeScript {version} in QuickJS: compiler loaded in {load_ms:.0} ms, {:.1} MB allocated.\n",
        after_load as f64 / 1_048_576.0
    );

    let call = |name: &str, args: (&str, &str)| -> Result<(String, f64), String> {
        ctx.with(|ctx| {
            let f: Function = ctx.globals().get(name).unwrap();
            let t = Instant::now();
            let out: String = f.call(args).catch(&ctx).map_err(|e| e.to_string())?;
            Ok((out, t.elapsed().as_secs_f64() * 1000.0))
        })
    };

    println!("| Case | transpile ms (first, then median of 5) | Same bytes as in Node | Type check ms (first, then again) | Type errors |");
    println!("|---|---|---|---|---|");
    let mut total = 0.0;
    let mut identical = 0;
    let mut compared = 0;
    for result in ts_json["results"].as_array().unwrap() {
        let dir = Path::new(result["dir"].as_str().unwrap());
        let file = result["file"].as_str().unwrap();
        if dir.ends_with("deep") || file.starts_with("Large - 8") {
            continue;
        }
        let source = fs::read_to_string(dir.join(file)).unwrap();
        let (js, first) = match call("__transpile", (&source, "")) {
            Ok(r) => r,
            Err(e) => {
                println!("| {file} | fails: {} | | | |", cell(&e));
                continue;
            }
        };
        let mut times: Vec<f64> = (0..5)
            .map(|_| call("__transpile", (&source, "")).map_or(f64::NAN, |r| r.1))
            .collect();
        times.sort_by(f64::total_cmp);
        let median = times[2];
        if dir.ends_with("packages/effects") {
            total += median;
        }
        compared += 1;
        let same = result["js"].as_str() == Some(js.as_str());
        identical += usize::from(same);
        // A type check on the effects whose types say something: the typed
        // ones, the type errors and the shipped effects, under `strict`.
        let checked = !dir.ends_with("generated") || file == "Template.ts";
        let (check, errors) = if checked {
            let first = call("__check", (&source, &api_source));
            let again = call("__check", (&source, &api_source));
            match (first, again) {
                (Ok((out, t1)), Ok((_, t2))) => {
                    let list: Vec<String> = serde_json::from_str(&out).unwrap_or_default();
                    (format!("{t1:.0}, {t2:.0}"), list.join("; "))
                }
                (Err(e), _) | (_, Err(e)) => ("fails".into(), e),
            }
        } else {
            (String::new(), String::new())
        };
        println!(
            "| {} | {first:.1}, {median:.1} | {} | {check} | {} |",
            file.trim_end_matches(".ts"),
            if same { "yes" } else { "**no**" },
            cell(if errors.is_empty() && checked {
                "none"
            } else {
                &errors
            })
        );
    }
    println!(
        "\n{identical} of {compared} outputs byte-identical to Node's; shipped effects transpiled in {total:.1} ms in all. Memory after the run: {:.1} MB.",
        rt.memory_usage().malloc_size as f64 / 1_048_576.0
    );
}
