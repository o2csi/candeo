//! An effect's TypeScript to the JavaScript Candeo's engine runs, in Rust and
//! without a window (#381).
//!
//! Types are stripped and the module stays an ES module, so that
//! `import … from '@candeo/effects-api'` reaches the engine's built-in module.
//! Every problem the engine would only meet when it loads the module is
//! reported where it is written, with its line and column: syntax errors, the
//! early errors a module may not have, constructs the engine cannot run, and
//! imports it cannot resolve.
//!
//! Types are **not** checked: Oxc parses and strips TypeScript, it is not
//! TypeScript's checker. The editor's language service checks them before a
//! save, as it did when the window compiled effects, and a file dropped in the
//! folder was never checked either (`docs/design/headless-transpile-poc.md` §5).
//!
//! # One contract
//!
//! [`compile`] is the only way in. The window's save, the library's
//! recompilation and, later, the MCP server all go through it, so that none of
//! them can run what another refuses (§6.2 of the same document).

mod imports;
mod position;
mod transpile;

use std::fmt;

/// The one module an effect may import: the engine resolves it to `api.js`.
pub const API_MODULE: &str = "@candeo/effects-api";

/// The largest source compiled, in bytes.
///
/// The shipped effects are 1 to 8 KB. A 1.3 MB source still loads within the
/// engine's 32 MB memory budget, a 10.6 MB one does not (§2 of the PoC). The
/// limit is also what bounds the compiler's time, which grows linearly, and the
/// stack its thread reserves (`stack_for`).
pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;

/// A problem where its author finds it: 1-based line and column, the column
/// counted in UTF-16 code units as TypeScript and Monaco count it. The message
/// is in English, like TypeScript's: it is read by the effect's author.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: u32,
    pub column: u32,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    /// The shape the window gave TypeScript's diagnostics.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {}, column {}: {}",
            self.line, self.column, self.message
        )
    }
}

/// What a source compiles to.
#[derive(Clone, Debug)]
pub struct Compiled {
    /// The module the engine loads.
    pub js: String,
    /// A source map (version 3, as JSON) from `js` back to the source: what
    /// brings a runtime error's position back to the line its author wrote.
    pub source_map: String,
}

/// What the effects API exports: the names an effect may import from
/// [`API_MODULE`].
#[derive(Clone, Debug)]
pub struct Api {
    exports: Vec<String>,
}

impl Api {
    /// Read from the module itself, `api.js`, rather than from a list kept by
    /// hand next to it.
    pub fn from_module(js: &str) -> Self {
        Self {
            exports: imports::exports_of(js),
        }
    }

    fn exports(&self) -> &[String] {
        &self.exports
    }
}

/// The JavaScript for this source, or every problem found, in source order.
///
/// Safe to call from any thread: the work runs on a thread of its own, whose
/// stack grows with the source (`stack_for`). Oxc has no recursion guard, and
/// a stack overflow aborts the whole process, which no `catch_unwind` stops
/// (§3.9 of the PoC). A source over [`MAX_SOURCE_BYTES`] is refused before
/// anything is parsed.
pub fn compile(source: &str, api: &Api) -> Result<Compiled, Vec<Diagnostic>> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(vec![Diagnostic {
            line: 1,
            column: 1,
            message: format!(
                "the file is {} bytes, and an effect is at most {MAX_SOURCE_BYTES}",
                source.len()
            ),
        }]);
    }
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("candeo-compiler".into())
            .stack_size(stack_for(source.len()))
            .spawn_scoped(scope, || {
                let compiled = transpile::transpile(source)?;
                let problems = imports::check(source, &compiled.js, api.exports());
                if problems.is_empty() {
                    Ok(compiled)
                } else {
                    Err(problems)
                }
            });
        match worker {
            // A panic in Oxc is a bug of Oxc's, on this source: the effect
            // does not compile, the application goes on.
            Ok(handle) => handle.join().unwrap_or_else(|_| {
                Err(vec![Diagnostic {
                    line: 1,
                    column: 1,
                    message: "the compiler failed on this file".into(),
                }])
            }),
            Err(e) => Err(vec![Diagnostic {
                line: 1,
                column: 1,
                message: format!("the compiler could not start: {e}"),
            }]),
        }
    })
}

/// Stack reserved for compiling a source of `len` bytes.
///
/// Oxc needs a bounded amount of stack per level of nesting, and a source
/// cannot nest deeper than it has bytes: a stack proportional to the length is
/// enough, whatever the source, with no scan of it beforehand. The space is
/// reserved, not committed: a thread uses only what it recurses into, 24 MiB
/// reserved for an 8 KB effect, 2 GiB for the largest source.
fn stack_for(len: usize) -> usize {
    BASE_STACK + len * STACK_PER_BYTE
}

/// What the compiler's own frames need, nesting aside, with room to spare.
const BASE_STACK: usize = 8 * 1024 * 1024;

/// Measured on Windows with rustc 1.99.1, by bisecting the smallest stack 40
/// constructs nested 20 000 deep compile on (2026-11-06). The most per byte:
/// tuple types left open, `type T = [[[…`, 1 171 bytes of stack per byte of
/// source in release and 1 344 in a debug build; then parentheses left open,
/// 944, and arrays, 817. The PoC's 1 KiB per byte (§3.9) was short for the
/// first. Twice that leaves room for what was not measured.
const STACK_PER_BYTE: usize = 2048;

#[cfg(test)]
mod tests {
    use super::*;

    fn api() -> Api {
        Api::from_module(
            "export const BLACK = {}\nexport function defineEffect(e) { return e }\nexport function hsv() {}\nexport function rgb() {}\n",
        )
    }

    #[test]
    fn the_exports_of_the_api_are_read_from_its_syntax() {
        assert_eq!(api().exports(), ["BLACK", "defineEffect", "hsv", "rgb"]);
    }

    #[test]
    fn a_source_over_the_limit_is_refused_before_it_is_parsed() {
        // Not even valid TypeScript: refused for its size alone.
        let source = "(".repeat(MAX_SOURCE_BYTES + 1);
        let errors = compile(&source, &api()).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!((errors[0].line, errors[0].column), (1, 1));
        assert!(errors[0].message.contains("at most 1048576"), "{errors:?}");
    }

    #[test]
    fn a_source_at_the_limit_is_compiled() {
        let source = "0;".repeat(MAX_SOURCE_BYTES / 2);
        assert_eq!(source.len(), MAX_SOURCE_BYTES);
        assert!(compile(&source, &api()).is_ok());
    }

    #[test]
    fn the_stack_grows_with_the_source() {
        assert_eq!(stack_for(0), BASE_STACK);
        assert_eq!(
            stack_for(MAX_SOURCE_BYTES),
            BASE_STACK + 2 * 1024 * 1024 * 1024
        );
    }
}
