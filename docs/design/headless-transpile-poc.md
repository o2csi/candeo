# Headless TypeScript transpilation — proof of concept (#364)

> **Status: proof of concept, measured on 2026-11-06, then adopted.** It
> answers the open decision of [`mcp-architecture.md`](mcp-architecture.md) §4
> with numbers, for the MCP work of
> [#365](https://github.com/o2csi/candeo/issues/365). The code is in
> [`tools/transpile-poc/`](../../tools/transpile-poc/). Since #381 the
> application compiles every effect this way: §9 says what it kept, what it
> changed, and why.

**In short:** Oxc can transpile Candeo's effects without a WebView, and the
engine renders the same thing either way. All 26 shipped effects and every
typed fixture give the same frames, byte for byte. Oxc is about 30 times faster
than the window's compiler (25 to 50 times, by file) and adds about 2.3 MiB to
a release binary. It needs four conditions, set out in §6. Two of them matter
most: one setting so that class fields behave as they do with TypeScript, and a
guard, because Oxc has no limit on recursion and a deeply nested source crashes
the whole process. Oxc does not check types. When a check is needed,
TypeScript's own checker runs inside the QuickJS Candeo already embeds, in
about 0.6 s (§5).

## 1. What was compared

| | Current path | Oxc path |
|---|---|---|
| Code | `transpile()` from `apps/desktop/src/editor/effect.ts`, imported unchanged | `tools/transpile-poc/oxc-transpile` |
| Compiler | Monaco 0.57.0's TypeScript 5.9.3, the one the window loads, run in Node 24.14.1 | Oxc 0.157.0: parser, semantic checks, transformer, code generator |
| Settings | `target: ES2020`, `module: ESNext`, first error → a module that throws it | §3 |

Both outputs then go through **the engine's load and render path**:
`tools/transpile-poc/src/engine.rs` copies `prepare_with_layout` and
`render_with` from `runtime/mod.rs`, with the application's own `api.js` and
`bootstrap.js`, included from where they live. It uses rquickjs 0.14
(QuickJS-ng 0.16.2), the same 32 MB memory budget and the same load deadline.
The copy leaves out two things: key labels, which come from the system's
keyboard layout, and dimming, which applies after the effect. Each effect runs
on the 4 built-in layouts. On each it renders 5 frames at rest, as a swatch
samples them, then 90 frames in a row with every input fed: presses, clock,
signals, sound and system. That makes 380 frames per effect. The declared
manifest and every frame's bytes are compared.

Cases:

- the **26 shipped effects**;
- **3 typed fixtures** (`cases/typed/`). The shipped effects carry no type
  annotations on purpose (`shipped.rs`). These carry what an agent writes:
  interfaces, generics, `satisfies`, `as const`, enums and const enums,
  namespaces, classes with `#private` fields, parameter properties, overloads,
  `declare`, type-only imports and re-exports;
- the **editor's template** (`example.ts`, as `template.ts` adapts it);
- **34 adversarial cases** (`cases/adversarial/`), plus generated ones: a byte
  order mark, CRLF line endings, sources of 1.3 MB and 10.6 MB, and arrays
  nested 500 to 100 000 deep.

Reproduce:

```bash
pnpm install                                       # Monaco's TypeScript, for the current path
cd tools/transpile-poc
cargo test --workspace                             # the Oxc wrapper's unit tests
cargo run --release -- compare                     # every table in §2, as Markdown
cargo run --release -- typescript-in-quickjs --stack-mb 64   # §5, after compare
```

The harness needs Node 22.18 or later: it relies on Node's type stripping to
import `effect.ts` as it is. #381 removed `effect.ts`: the TypeScript side now
runs from a checkout of a668a44 or earlier, and what it gave the shipped effects
is kept in `apps/desktop/src-tauri/fixtures/typescript/` (§9).

**Why a workspace of its own.** Oxc brings in 109 crates and takes about
1 min 45 s for a clean release build (§4). As a member of the repository's
workspace it would add all of that to `cargo test --workspace`, to clippy and to
every release build, for code nothing ships. So `tools/transpile-poc/` has its
own `[workspace]` and `Cargo.lock`, and CI does not build it.

**Why a document of its own.** The result tables and the conditions are longer
than §4 should be. §4 keeps the decision and points here.

## 2. Results

| Group | Cases | Same behaviour in the engine |
|---|---:|---|
| Shipped effects | 26 | 26: same manifest, same 380 frames |
| Typed fixtures and the template | 4 | 4 |
| Other valid code: top-level `await`, `using`, the `v` regex flag, a BOM, Unicode identifiers, a 1.3 MB source, 500 levels of nesting, both class-field cases (§3.1), type errors | 10 | 10 |
| Runtime errors | 2 | 2: the same frame throws the same message. The positions differ (§3.8) |
| Errors both refuse to load | 23 | 23. Oxc locates 11 that the current path reports without a position (§3.5, §3.6) |
| Valid TypeScript the engine cannot run after Oxc: standard decorators, `accessor` | 2 | **no**, refused with a position (§3.3) |
| What an ES module cannot have, which TypeScript drops silently: `import x = require()`, a parameter decorator | 2 | **no**, refused with a position (§3.4) |
| Nesting 1 000 to 100 000 deep | 4 | **no**: the current path refuses with a catchable error, and Oxc crashes the process on a small stack (§3.9) |

Shipped effects. Times are medians of 20 calls after a warm-up. "Arena" is the
memory Oxc reserved for the call, freed when it returns:

| Effect | Bytes | TS ms | Oxc ms | Oxc arena KB | Frames |
|---|---:|---:|---:|---:|---|
| Aurora | 7 786 | 6.08 | 0.17 | 111 | identical, 380 |
| Beat pulse | 2 660 | 1.73 | 0.04 | 47 | identical, 380 |
| Beat ripples | 3 329 | 2.17 | 0.06 | 47 | identical, 380 |
| Breathing | 1 280 | 0.94 | 0.02 | 15 | identical, 380 |
| Bubbles | 2 501 | 1.52 | 0.05 | 47 | identical, 380 |
| Clock | 5 630 | 2.70 | 0.08 | 47 | identical, 380 |
| Color wheel | 1 275 | 0.92 | 0.02 | 15 | identical, 380 |
| Counter-Strike 2 | 3 593 | 2.23 | 0.06 | 47 | identical, 380 |
| Crossing beams | 2 282 | 1.38 | 0.04 | 47 | identical, 380 |
| Diagonal wave | 1 797 | 0.90 | 0.02 | 15 | identical, 380 |
| Dota 2 | 3 919 | 2.09 | 0.08 | 47 | identical, 380 |
| Equalizer | 4 504 | 1.68 | 0.07 | 47 | identical, 380 |
| Fireworks | 3 124 | 1.45 | 0.05 | 47 | identical, 380 |
| Fixed gradient | 1 928 | 0.77 | 0.02 | 15 | identical, 380 |
| Lightning | 2 603 | 1.36 | 0.05 | 47 | identical, 380 |
| Noise map | 3 021 | 1.51 | 0.06 | 47 | identical, 380 |
| Radial wave | 1 976 | 0.93 | 0.02 | 15 | identical, 380 |
| Rain | 2 159 | 0.94 | 0.03 | 15 | identical, 380 |
| Ripples | 2 649 | 1.27 | 0.04 | 47 | identical, 380 |
| Scrolling text | 5 824 | 2.51 | 0.12 | 47 | identical, 380 |
| Starry night | 2 175 | 1.03 | 0.07 | 47 | identical, 380 |
| Status row | 4 523 | 2.10 | 0.08 | 47 | identical, 380 |
| Sweep | 2 299 | 0.96 | 0.04 | 15 | identical, 380 |
| Swirl circles | 1 936 | 1.07 | 0.06 | 47 | identical, 380 |
| Temperature | 1 774 | 0.99 | 0.03 | 15 | identical, 380 |
| Waterfall | 3 481 | 2.35 | 0.06 | 47 | identical, 380 |
| **All 26** | 80 028 | **43.6** | **1.43** | | 9 880 frames, all identical |
| Typed namespaces | 1 952 | 1.68 | 0.05 | 47 | identical, 380 |
| Typed particles | 2 707 | 3.66 | 0.08 | 47 | identical, 380 |
| Typed radial wave | 2 021 | 1.31 | 0.04 | 47 | identical, 380 |
| Template | 1 641 | 1.17 | 0.02 | 15 | identical, 380 |

Adversarial cases, by what the author ends up seeing. "Load error" means the
library records an effect that does not load, with that message.

| Case | Current path | Oxc path |
|---|---|---|
| Missing `)` (#226's case) | line 4, column 35: `')' expected.` | line 4, column 35: ``Expected `)` but found `Identifier` `` |
| Error after accents and an emoji on the line | line 5, column 35 | line 5, column 35. Columns are counted in UTF-16 units, as TypeScript counts them |
| Same, CRLF line endings | line 4, column 35 | line 4, column 35 |
| Type annotation with no type | line 4, column 14: `Type expected.` | line 4, column 14: `Unexpected token` |
| JSX in a `.ts` file | line 4, column 25 | line 4, column 25 |
| Unterminated string | line 3, **column 27** (end of line) | line 3, **column 14** (start of the string) |
| Legacy octal `0177` | line 4, column 15 | line 4, column 15 |
| Redeclared `let` | load error, no position: `invalid redefinition of global identifier` | line 5, column 5 |
| `with` statement | load error, no position: `invalid keyword: with` | line 8, column 5 |
| Duplicate regex group names | load error, no position: `duplicate group name` | line 5, column 19 |
| `import _ from 'lodash'`, `node:fs`, `./palette.ts` | load error: `Error resolving module 'lodash' from 'effect'` | same load error. The import check also locates it: line 2, column 15 |
| `import { sparkle }` from the API | load error: `Could not find export 'sparkle'` | same. The check locates it and names the module: line 1, column 24 |
| Default import of the API | load error: `Could not find export 'default'` | same, located |
| `export { Rgb } from` the API (a type) | load error: `Could not find export 'Rgb'` | same, located, with the fix: `export type { … }` |
| `await import('node:fs')` | load error | same, located: `import()` is not supported |
| `require('fs')` | load error: `require is not defined` | same |
| `import fs = require('fs')` | **loads**: TypeScript drops the line, `fs` is undefined | refused, line 3, column 13 |
| `export = { … }` | load error: no default export (TypeScript emits `export {}`) | refused, line 2, column 1, with the fix |
| Parameter decorator | **loads**: TypeScript drops the decorator | refused, line 8, column 15 |
| Standard decorators | **loads**: TypeScript lowers them | refused, line 15, column 1 (§3.3) |
| `accessor` field | **loads**: TypeScript lowers it | refused, line 5, column 3 (§3.3) |
| Type errors (`const n: number = 'x'`, `hsv('red')`, a wrong default) | loads, renders | loads, same frames. Neither path checks types (§5) |
| `api.sparkle()` through a namespace | frame 0 throws at `effect:6:24` | frame 0 throws at `effect:5:17`, which the source map turns into `effect.ts:7:20` |
| Runtime error on source line 15 | throws at `effect:6:17` | throws at `effect:6:14` → source map `effect.ts:15:16` |
| Endless loop at load | `interrupted`, load budget | same |
| No default export, empty file | the bootstrap's message | same |
| 10.6 MB source | load error: `out of memory` (32 MB budget) | load error, worded differently. The memory limit, not the transpiler |

## 3. Differences found, and what the PoC does about them

1. **Class fields: a semantic difference, resolved.** For ES2020, TypeScript
   compiles fields with `useDefineForClassFields: false`. A field is assigned in
   the constructor, after the parameter properties. With Oxc's defaults,
   `level = this.base * 2` next to `constructor(readonly base: number)` is a
   native field that runs first, so `level` is NaN where TypeScript gives 100.
   A field over an inherited accessor also behaves differently: defined, it
   hides the setter; assigned, it calls it. Two settings give TypeScript's
   semantics: `assumptions.set_public_class_fields = true` and
   `typescript.remove_class_fields_without_initializer = true`. Both cases
   (`cases/adversarial/Class fields - …`) now render identical frames.
2. **No ES2020 target for Oxc.** For ES2020, Oxc lowers class fields and
   `#private` members with helpers. By default it imports them from
   `@oxc-project/runtime`, which the engine cannot resolve. The *External* mode
   reads them from a `babelHelpers` global, which does not exist. *Inline* is
   not implemented in 0.157. QuickJS-ng 0.16 runs ES2023 natively, `using`
   included, so the PoC compiles for `esnext` and lowers nothing. The *Oxc
   es2020* column of the comparison shows the failure on *Typed particles*.
3. **Decorators and `accessor`: a gap, refused with a position.** TypeScript
   lowers standard decorators and `accessor` for ES2020. Oxc 0.157 leaves them
   in the output, and QuickJS then refuses the module without a position. The
   PoC walks the transformed program and refuses them where they are written.
   An effect using them works in the window today and would not work headless.
   None of the shipped effects uses them.
4. **TypeScript drops what an ES module cannot have, without a word.**
   `import fs = require('fs')`, `export =` and a parameter decorator (without
   `experimentalDecorators`) vanish from TypeScript's output. The effect then
   loads with `fs` undefined, or without its export. The editor's language
   service flags all three, but a file dropped in the folder runs as it is. Oxc
   turns the first two into `require` and `module.exports`, which the engine
   does not have. The PoC refuses all three with a position.
5. **Early errors, located.** A redeclared `let`, a `with` statement and
   duplicate regex group names are refused by any engine before the module
   runs. `transpileModule` reports none of them, so QuickJS refuses the module
   without a position. Oxc reports them with their position: semantic checks
   on, and regex parsing on, which is off by default.
6. **Imports, statically.** Both paths fail the same way at link time.
   `check_imports` reads what the module will import at run time from the
   **JavaScript** (`import { Rgb }` used only as a type is gone by then) and
   locates it in the **source**. It finds any module other than
   `@candeo/effects-api`, a name `api.js` does not export (read from `api.js`
   itself, with no list kept by hand), a default import, a type re-exported
   as a value and `import()`.
7. **Syntax error positions agree, wordings do not.** 6 of the 7 syntax errors
   both paths locate are on the same line and column. The unterminated string is
   reported at its end by TypeScript and at its start by Oxc. The messages are
   worded differently (`')' expected.` against ``Expected `)` but found
   `Identifier` ``). An agent or a person reading them will not care. A test
   comparing texts would.
8. **Runtime errors point into the JavaScript, in both paths.** QuickJS reports
   lines of the compiled module, and neither path maps them back today. With
   the type declarations stripped, both report source line 15 as line 6. Oxc's
   source map, produced in the same pass, brings the position back to the
   source (`effect.ts:15:16`). TypeScript could produce one as well, but
   `effect.ts` does not ask for it.
9. **Deep nesting crashes the process.** Oxc 0.157 has no recursion guard: it
   has no stack check and does not use `stacker`. A stack overflow aborts the
   whole process (`0xC00000FD`), and no `catch_unwind` can stop it. Deepest
   nesting each one survives:

   | | 1 MB (Windows main thread) | 2 MB (Rust's default thread) | 8 MB | 64 MB |
   |---|---:|---:|---:|---:|
   | Oxc, nested arrays | 1 235 | 2 520 | 10 231 | 82 191 |
   | Oxc, nested parentheses | 1 065 | | 8 842 | |
   | Oxc, `a+a+…+1` chain | 1 611 | | 13 373 | |
   | **QuickJS**, loading nested arrays | **744, then aborts** | throws a catchable error, up to 400 000 tried | throws, up to 10 000 tried | |
   | TypeScript in Node (V8) | throws a catchable `RangeError` beyond 585 arrays and 1 366 parentheses | | | |

   The guard does not need to scan the source. The stack Oxc needs is about
   1 KiB per level of nesting, and a source cannot nest deeper than its length.
   So a size limit and a thread whose reserved stack grows with it are enough.
   Measured: a 1 MiB source of 524 000 nested `[` or `(` transpiles in
   0.75 s or 0.63 s on a thread that reserves 1 GiB of address space. That
   space is committed only as the stack is used.

   TypeScript does not overflow on a long `a+a+…+1` chain, which it parses
   without recursing, but it takes quadratic time over it: 0.45 s for 2 000
   terms, 19 s for 20 000 (a 40 KB file), 615 s for 100 000 (200 KB). It runs
   in the window's page, not in a worker. Oxc takes 0.16 s, 0.29 s and, for
   500 000 terms (1 MB), 0.49 s, process start included. Neither transpiler can
   be interrupted. Oxc's time grows linearly, so a size limit bounds it; a
   size limit does not bound TypeScript's.

   The bottom rows matter for the engine too. QuickJS's own limit, 1 MB by
   default, protects it only on a thread with more stack than that. Sync Tauri
   commands run on the main thread, which has 1 MB on Windows: from Tauri's
   documentation, not measured in the app. Through the window, TypeScript gives
   up first for nested arrays (585 < 744). Headless, Oxc would hand QuickJS
   nesting TypeScript never produced.

## 4. Measurements

On a 32-thread Windows 11 laptop, with rustc 1.99.1 and Node 24.14.1. Three
runs of `compare` agree within the ranges given.

| | Current path (TypeScript in V8) | Oxc |
|---|---|---|
| 26 shipped effects, warm | 42–44 ms | 1.3–1.4 ms |
| One shipped effect | 0.8–6 ms | 0.02–0.17 ms |
| 1.3 MB source | 434–592 ms | 17–23 ms |
| 10.6 MB source | 3.6–5.0 s | 145–157 ms |
| `a+a+…+1`, 20 000 terms (40 KB) | 19 s | 0.16 s with process start, on a 1 GiB stack (§3.9) |
| Cold | 302–325 ms to load the compiler and make the first call, 13.4 MB of heap after loading | nothing to load: 0.1–0.35 ms for the first call in a new process. Starting a process is 2.3 ms slower with Oxc in it (38.0 ms against 35.7 ms) |
| Memory | the compiler's heap, kept for as long as the window lives | an arena freed at return: 15–111 KB per shipped effect, about 12 times the source for large inputs (16 MB for 1.3 MB, 128 MB for 10.6 MB) |
| Binary | already in the frontend assets (9 MB of `typescriptServices.js`) | **+2.33 MiB** (2 440 192 bytes): `size-oxc` against `size-baseline`, stripped, LTO, the repository's release profile |
| Clean build | — | 107 s release, 86 s dev (dependencies at `opt-level = 2`, as the workspace sets them), Oxc alone |
| Dependencies | — | 109 crates. 37 are already in `Cargo.lock` at the same version and 16 at a compatible one, so they would unify. 6 would be second copies: `num-bigint` 0.5, `phf`, `phf_generator`, `phf_macros` and `phf_shared` 0.14, `synstructure` 0.14. 50 are new, 23 of them `oxc_*` |
| Minimum Rust | — | **1.97**. The workspace declares `rust-version = "1.82"` |

Oxc 0.x changes its API between minor versions: in 0.157 the AST splits export
declarations, and `ParserReturn::panicked` became `fatal_error`. The PoC pins
`=0.157.0`. An upgrade should be a deliberate change, rerunning this
comparison.

## 5. Type diagnostics

**Oxc does not check types, and neither does today's path.** `transpileModule`
strips types without checking them. Type errors reach a person only through
Monaco's language service, in the editor, before saving. A file dropped in the
folder is never type-checked, and an agent working without a window would not
be either. Every type error case renders the same frames in both paths.

Two ways to check types without Node:

1. **TypeScript's own checker inside QuickJS**, the engine Candeo already
   embeds. Measured with `typescript-in-quickjs`: Monaco's
   `typescriptServices.js` and its `lib.js` load in **326 ms** and take
   **44.7 MB**. The JavaScript it emits is **byte-identical** to Node's in 67
   cases out of 67. It is slow: 10–70 ms per effect, 746 ms for the 26 shipped
   effects, 14 s for 1.3 MB. A **type check** with the editor's settings
   (`strict`, `lib: es2020`, `paths` to the API) takes **0.4–0.7 s per
   effect**, even with the parsed libraries kept between calls. Memory reached
   249 MB after some sixty checks. It reports the editor's own diagnostics:
   TS2610 for the field over an accessor, `Decorators are not valid here`,
   `Cannot find module 'lodash'`, `Property 'sparkle' does not exist`, and so
   on.
2. **tsgo**, the native port of TypeScript, as a separate binary. Not measured.
   It would be another executable to sign and ship, for a check.

Two things the checker shows about the editor's own settings, for whoever
revisits them:

- it does not flag `export { Rgb } from '@candeo/effects-api'`. Only
  `isolatedModules` (or `verbatimModuleSyntax`) makes TypeScript report what
  `transpileModule` cannot remove;
- it flags the `v` regex flag as needing ES2024, though QuickJS runs it. The
  editor's ES2020 target is stricter than the engine.

**Proposed decision.** Headless validation reports syntax, imports, unsupported
constructs, loading and a few frames rendered: everything that decides whether
an effect runs. Types stay with the editor. If an agent workflow needs them,
#365 can offer a separate, explicit step that runs TypeScript's checker in
QuickJS. At about 0.6 s and up to a few hundred MB, that step should be
optional and bounded, not part of every validation.

## 6. Recommendation

**Adopt Oxc for headless transpilation, under four conditions:**

1. **These settings, kept together:** `esnext` target, with no lowering and no
   helpers; TypeScript's ES2020 class-field semantics (§3.1); semantic checks
   and regex parsing on; every source parsed as a module. With them, every
   tested effect renders the same frames. One was missing, found in the
   product (§9): enum evaluation.
2. **One validation contract in Rust**, the same function for Tauri IPC and for
   the MCP, so that neither can drift from the other (§4 point 4 of the
   proposal):
   1. a size limit. 1 MiB is proposed: the 1.3 MB case loads within the
      32 MB budget, the 10.6 MB case does not. No limit exists today;
   2. Oxc on a dedicated thread that reserves 1 KiB of stack per byte of
      source (1 GiB for 1 MiB). Measured since on more constructs, that was
      short: the product reserves 2 KiB (§9);
   3. syntax errors, early errors and refused constructs, with their line and
      column;
   4. the import check against `api.js`'s exports;
   5. the existing load under `LOAD_BUDGET` and `MEMORY_BUDGET`, on a thread
      with at least 2 MB of stack (or with `Runtime::set_max_stack_size`
      below the thread's), then the manifest and a few frames under
      `FRAME_BUDGET`;
   6. runtime errors brought back to the source through the source map.
3. **The window compiles through the same contract.** With Oxc headless and
   TypeScript in the window, a file using decorators would be ready in one and
   broken in the other: the silent divergence §4 rules out. The simplest
   single contract is that `cache_effect` receives the source and Rust
   compiles it. Monaco keeps everything it is there for: types, completion,
   inline errors. The cost is the gap of §3.3: an existing user effect with
   standard decorators or `accessor` would stop loading, saying where.
4. **The workspace's `rust-version` raised to 1.97**, and Oxc pinned to an
   exact version, upgraded only by rerunning this comparison.

Not chosen:

- **Reusing TypeScript in QuickJS for transpilation.** It gives identical
  output by construction, but it is 500 times slower than Oxc, takes 45 MB or
  more, and needs Monaco's 12 MB of JavaScript to be reachable from Rust.
  Worth keeping for the optional type check of §5, not for every save.
- **Node, Bun or a hidden WebView.** Ruled out by #364's acceptance criteria.

## 7. What remains for #365

- The contract of §6.2 as code, with tests: where it lives (a crate next to
  `candeo-protocol`, or `src-tauri`), its thread, its limits.
- Whether the window switches to it (§6.3), and the note for effects that use
  decorators or `accessor`.
- The shape of a diagnostic for MCP clients: line, column, message, and the
  step that found it (syntax, import, load, render). Messages stay in English,
  like TypeScript's: they are read by the effect's author (`AGENTS.md`).
- Effect names and paths: unrelated to transpilation. The MCP's `save` must go
  through `storage::validate_name` and the existing atomic write. Headless
  validation needs neither a file nor a saved source: it takes a string.
- The optional type check (§5), if an agent workflow needs it.
- The engine's stack (§3.9), whatever the transpiler.

## 8. Not verified

- **The application itself.** It was not started. Both outputs ran through a
  copy of its load and render path, not through it, so bound parameters,
  dimming and key labels were not exercised.
- **Linux and macOS.** Every measurement is from Windows 11. That includes
  reserving 1 GiB of stack, which depends on `ulimit` and overcommit there.
- **WebView2.** TypeScript's times and recursion limits were measured in Node's
  V8, not in the window's.
- **The thread a sync Tauri command runs on**, and its 1 MB stack: from Tauri's
  documentation, not observed in Candeo.
- **The binary size of `candeo.exe`** with Oxc linked in. The 2.33 MiB is the
  difference between two minimal binaries. Build time on CI runners was not
  measured either.
- **Source map columns on lines with non-ASCII text.** QuickJS's column unit
  was checked on ASCII lines only.
- **Process memory.** Oxc's arena and QuickJS's allocator were measured, not
  the process's resident memory.
- **tsgo.**
- **Whether Rust can read `typescriptServices.js` from the frontend assets
  Tauri embeds**, rather than shipping it a second time, for the type check of
  §5.

## 9. In the product (#381)

What the application took from §6, and where it departs from it.

**Where it lives.** [`crates/candeo-compiler`](../../crates/candeo-compiler/),
a crate of the workspace next to `candeo-protocol` and `candeo-device`, rather
than a module of `candeo-desktop`. It needs neither Tauri nor the window, so its
tests run without them, and an MCP server in a process of its own
([`mcp-architecture.md`](mcp-architecture.md) §6) can link it. Its contract is
one function, `compile(source, api)`: JavaScript, a version 3 source map, or
every problem with its line and column. The application calls it through
`runtime::compile`, which hands it `api.js`'s exports; the MCP server's
validation is to call the same function.

**The window compiles through it (§6.3).** `cache_effect(key, hash, js)`
became `compile_effect(key, hash)`: Rust reads the file, checks its hash,
compiles it, loads it and records the result. The window still asks for what is
stale, at startup and on Refresh, and the editor for what it saves.
`editor/effect.ts`, its test and the declaration of Monaco's
`typescriptServices` are gone. Monaco keeps editing, completion and type
diagnostics, and both npm packages stay: `monaco-editor` for the editor,
`typescript` for `vue-tsc` and `effectFormat.test.ts`.

**The four conditions, as built:**

1. The settings of §6.1, and one the PoC missed:
   `SemanticBuilder::with_enum_eval(true)`. The enum transform reads the
   members' values from the semantic pass; without them, `Quick = Fast`, a
   string, gets a reverse mapping TypeScript does not give it (oxc#21667).
   Only a debug assertion says so, and the PoC ran in release. The product's
   debug tests hit it on *Typed namespaces*.
2. The size limit, 1 MiB, refused before anything is parsed.
3. The thread, with **2 KiB of stack per byte** of source, not 1. Bisecting the
   smallest stack 40 constructs nested 20 000 deep compile on: tuple types left
   open, `type T = [[[…`, need 1 171 bytes per byte in release and 1 344 in a
   debug build; parentheses left open 944, arrays 817, `!` 627. At 1 KiB, a
   1 MiB file of the first would have aborted the process. The thread reserves
   2 GiB at the limit, 24 MiB for an 8 KB effect, and commits what it uses.
4. `rust-version` 1.97, and Oxc pinned at `=0.157.0`. The new minimum turns on
   two clippy lints for APIs newer than 1.82, fixed where they fired.

**The engine's stack (§3.9, §7).** Recording an effect loads it in QuickJS.
That ran in the command, on the main thread, where 744 nested arrays abort the
process; TypeScript, in V8, let nothing deeper than 585 through. Oxc lets through
whatever the size limit allows, so a record is now built on a thread of its own
with the 2 MiB a render loop has (`runtime::on_engine_thread`). There QuickJS's
own limit refuses deep nesting with an error, and the effect is recorded
broken. A test compiles 20 000 levels from a 1 MiB thread.

**How errors show.** A file that does not compile is recorded `broken`, like a
module that does not load, with its first problem: ``compiling the effect:
line 4, column 35: Expected `)` but found `Identifier` ``. The editor shows it
when saving and the effects list next to the effect, where a load error shows.
The import check of §3.6 is part of the contract. A file that is not UTF-8 text
is recorded broken too; it used to stay stale, with a warning in the log.

**The cache.** `CACHE_FORMAT` 7: every record holds Oxc's JavaScript. A record
of an older format is compiled again from its file at startup, without the
window (`Store::recompile_cache`, which replaces `redeclare_cache`: that one
declared again from the record's own JavaScript, having no compiler). A record
whose file changed since is left stale for the next Refresh.

**The comparison, kept.** `runtime::equivalence` loads, through the engine's
own `prepare_with` and `render_with`, on the four built-in layouts, what the
window's TypeScript gave before the move (`apps/desktop/src-tauri/fixtures/typescript/`)
and what Oxc gives: the 26 shipped effects, the typed fixtures and the template of §1, and both
class-field cases of §3.1, 95 frames a layout, plus the manifest. All identical.
Without the class-field cases, it did not notice `set_public_class_fields`
turned off. It is what an upgrade of Oxc reruns. A shipped effect changed since
the move fails it until its fixture goes: no TypeScript output describes it any
more.

**Measured** on the same laptop, clean builds one after the other, with the
front end built as `tauri build` builds it:

| | Before (a668a44) | After | Difference |
|---|---:|---:|---:|
| `candeo-desktop.exe`, release | 22 408 192 bytes | 24 004 096 bytes | +1 595 904 bytes (+1.52 MiB) |
| Front end (`dist/`) | 16 188 214 bytes | 12 741 867 bytes | −3 446 347 bytes (−3.29 MiB) |
| Clean release build | 420 s | 459 s | +39 s (+9 %) |

The front end loses its own copy of TypeScript, `typescriptServices-*.js`,
which the window loaded at the first save; the language service's worker keeps
the one it bundles. The executable embeds the front end compressed, so its
1.52 MiB are Oxc less that copy: Oxc alone was 2.33 MiB (§4).

**Not done here:**

- Runtime errors brought back to the source through the source map (§6.2.6):
  the contract produces the map, but nothing stores or reads it yet.
- A command validating a source that is not saved, for the MCP server (#365):
  `runtime::compile` is what it will call.
- The optional type check of §5.