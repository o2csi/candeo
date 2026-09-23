/**
 * The TypeScript compiler Monaco already ships.
 *
 * Monaco does not ship a home-made parser: its language service is the
 * **complete TypeScript compiler**, published in the package under the name
 * `typescriptServices` and re-exported by `ts.worker.js` under the name `ts`.
 * That module has no type declaration — hence this one.
 *
 * The import goes through the direct path rather than through `ts.worker.js`:
 * the latter sets `self.onmessage`, which a module loaded in the window has no
 * reason to do.
 *
 * **The types come from the `typescript` package, the implementation from
 * Monaco.** `typescript` is already a development dependency (`vue-tsc` uses
 * it): it weighs nothing in the build, since only its types are read. None of
 * this adds a second transpiler — see `docs/design/studio.md` §2.
 */
declare module 'monaco-editor/languages/features/typescript/lib/typescriptServices.js' {
  import type * as ts from 'typescript'

  export const typescript: typeof ts
}
