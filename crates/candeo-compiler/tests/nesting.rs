//! Deep nesting, which aborts the process when Oxc runs out of stack: Oxc has
//! no recursion guard, and a stack overflow is not a panic
//! (`docs/design/headless-transpile-poc.md` §3.9). A file of its own, so that
//! a regression ends this test binary only, and says so.

use candeo_compiler::{compile, Api, MAX_SOURCE_BYTES};

fn api() -> Api {
    Api::from_module("export function defineEffect(e) { return e }\n")
}

/// The worst case measured, at the size limit: tuple types left open, which
/// need the most stack per byte of source of the 40 constructs measured
/// (`STACK_PER_BYTE`). On the 1 MB a Windows main thread has, 1 000 levels
/// of arrays already aborted.
#[test]
fn the_deepest_nesting_the_limit_allows_is_an_error_not_a_crash() {
    let prefix = "type T = ";
    let source = format!("{prefix}{}", "[".repeat(MAX_SOURCE_BYTES - prefix.len()));
    let errors = compile(&source, &api()).unwrap_err();
    assert_eq!(errors[0].line, 1, "{errors:?}");
}

/// Valid code nested deeper than the window's TypeScript accepted (585 arrays,
/// then a catchable `RangeError`) compiles: whether the engine loads it is the
/// engine's own limit.
#[test]
fn deeply_nested_valid_code_compiles() {
    let depth = 100_000;
    let source = format!(
        "export default {}{}\n",
        "[".repeat(depth),
        "]".repeat(depth)
    );
    let js = compile(&source, &api()).unwrap().js;
    assert_eq!(js.matches('[').count(), depth);
}
