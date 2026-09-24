/**
 * From TypeScript source to the JavaScript the engine runs.
 *
 * ## The transpiler was already in the box
 *
 * Monaco's language service **is** the TypeScript compiler. Stripping types is
 * therefore free: `transpileModule()` is a text-to-text function, deterministic,
 * with no run-time semantics. Neither `esbuild-wasm`, nor `oxc`, nor `swc` — the
 * only place that mattered, **execution**, is on the Rust side
 * (`docs/design/studio.md` §3).
 *
 * The compiler is loaded on demand, not when the editor opens: it is only needed
 * to save, or to compile the files a Refresh found stale, and it weighs a few
 * megabytes.
 *
 * ## The manifest is not read here
 *
 * What an effect declares — description, parameters, API version — is read by
 * Rust, which runs the module once when its JavaScript is recorded
 * (`docs/design/effects-library.md` §3). Reading it from the syntax tree here
 * required every default to be a literal; running it does not.
 */

import type * as TS from 'typescript'

/** File name given to the compiler: it only shows in its messages. */
const FILE = 'effect.ts'

let loading: Promise<typeof TS> | null = null

/**
 * Monaco's compiler, loaded once.
 *
 * Dynamic import: Vite makes it a chunk of its own, read at the first save rather
 * than when the window opens.
 */
function compiler(): Promise<typeof TS> {
  loading ??= import(
    'monaco-editor/languages/features/typescript/lib/typescriptServices.js'
  ).then((m) => m.typescript)
  return loading
}

/**
 * The JavaScript for this source.
 *
 * Type errors do not stop it — the editor checks them before saving, and a file
 * dropped in the folder is the author's business. A module that does not load
 * is reported by Rust when the JavaScript is recorded.
 *
 * A syntax error does stop it. The compiler repairs what it can without a word:
 * a file edited outside Candeo with a parenthesis missing ran as the compiler's
 * guess, listed as ready (#226). The module throws the error instead, so Rust
 * records an effect that does not load, saying why, as it does for any other.
 */
export async function transpile(source: string): Promise<string> {
  const ts = await compiler()
  const { outputText, diagnostics } = ts.transpileModule(source, {
    fileName: FILE,
    reportDiagnostics: true,
    compilerOptions: {
      target: ts.ScriptTarget.ES2020,
      // **Not CommonJS.** `import { hsv } from '@candeo/effects-api'` must stay
      // an import: rquickjs's module loader resolves it to the host's internal
      // module.
      module: ts.ModuleKind.ESNext,
    },
  })
  const first = diagnostics?.find((d) => d.category === ts.DiagnosticCategory.Error)
  return first ? `throw new SyntaxError(${JSON.stringify(located(ts, first))})\n` : outputText
}

/** A diagnostic as its author finds it: the line and column, then what is wrong. */
function located(ts: typeof TS, diagnostic: TS.Diagnostic): string {
  const text = ts.flattenDiagnosticMessageText(diagnostic.messageText, ' ')
  if (!diagnostic.file || diagnostic.start === undefined) return text
  const { line, character } = diagnostic.file.getLineAndCharacterOfPosition(diagnostic.start)
  return `line ${line + 1}, column ${character + 1}: ${text}`
}
