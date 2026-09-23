/**
 * Setting Monaco up: workers, theme, TypeScript language service.
 *
 * ## Why Monaco
 *
 * For its language service. By giving it the declaration of
 * `@candeo/effects-api`, we get types, autocompletion and inline errors without
 * writing a single rule (`docs/design/studio.md` §2).
 *
 * ## How the declaration reaches it — without a copy
 *
 * `?raw` reads `packages/effects-api/src/index.ts` **at build time** and writes
 * it into the bundle as a string. The file therefore stays the only source: it
 * is neither copied, nor regenerated, nor converted to `.d.ts`. Monaco does not
 * require a declaration — `addExtraLib` accepts any TypeScript, and the service
 * draws the same from it. Function bodies included, which is better: the
 * tooltip of `hsv` then shows the real code.
 *
 * The file is placed where a package lives, `node_modules/<name>/index.ts`, and
 * `paths` sends it there directly: resolution then depends on no subtlety of
 * the search in `node_modules` inside the worker.
 *
 * ## The workers do not come from a CDN
 *
 * Vite's `?worker` bundles each worker as a project asset, in development as in
 * the shipped application. Nothing is downloaded during use: the application
 * opens offline.
 */

import * as monaco from 'monaco-editor/editor/editor.api.js'
import EditorWorker from 'monaco-editor/editor/editor.worker.js?worker'
import TsWorker from 'monaco-editor/language/typescript/ts.worker.js?worker'
// Registers the "typescript" language identifier and its highlighting. Without
// it, `languages.onLanguage('typescript')` is never triggered and the language
// service does not start.
import 'monaco-editor/languages/definitions/typescript/register.js'
import {
  getTypeScriptWorker,
  ModuleKind,
  ModuleResolutionKind,
  ScriptTarget,
  typescriptDefaults,
} from 'monaco-editor/languages/features/typescript/register.js'

import effectsApiSource from '@candeo/effects-api/src/index.ts?raw'

/** Where the language service finds `@candeo/effects-api`. */
const API_PATH = 'node_modules/@candeo/effects-api/index.ts'
const API_URI = `file:///${API_PATH}`

/** The file being edited, as the language service sees it. */
export const EFFECT_URI = monaco.Uri.parse('file:///effect.ts')

const THEME = 'candeo'

self.MonacoEnvironment = {
  getWorker(_id, label) {
    return label === 'typescript' || label === 'javascript' ? new TsWorker() : new EditorWorker()
  },
}

/**
 * The editor's colors, read from the style tokens.
 *
 * Monaco wants plain colors: it cannot read a CSS variable. They are therefore
 * given to it resolved, but they stay defined in the one place that defines
 * them — `styles/tokens.css`. A renamed token makes a color disappear from the
 * editor, it does not bring it down.
 */
function palette(style: CSSStyleDeclaration): Record<string, string> {
  const wanted: Record<string, string> = {
    'editor.background': '--ground',
    'editor.foreground': '--text',
    'editorLineNumber.foreground': '--text-faint',
    'editorLineNumber.activeForeground': '--text-muted',
    'editorCursor.foreground': '--accent',
    'editor.lineHighlightBackground': '--raised',
    'editor.selectionBackground': '--raised-2',
    'editorIndentGuide.background1': '--line',
    'editorWidget.background': '--raised',
    'editorWidget.border': '--line',
    'editorSuggestWidget.background': '--raised',
    'editorSuggestWidget.border': '--line',
    'editorSuggestWidget.selectedBackground': '--raised-2',
    'editorHoverWidget.background': '--raised',
    'editorHoverWidget.border': '--line',
    'editorError.foreground': '--bad',
    'editorWarning.foreground': '--warn',
    'scrollbarSlider.background': '--line',
    'scrollbarSlider.hoverBackground': '--line-strong',
  }

  const colors: Record<string, string> = {}
  for (const [key, token] of Object.entries(wanted)) {
    const value = style.getPropertyValue(token).trim()
    if (value) colors[key] = value
  }
  return colors
}

/**
 * Matches the editor to the application's theme.
 *
 * Light or dark is not guessed again here: `tokens.css` already sets
 * `color-scheme` on the root, taking into account the system setting **and**
 * `data-theme`. It is read, not recomputed.
 */
function applyTheme(): void {
  const style = getComputedStyle(document.documentElement)
  const dark = style.colorScheme.includes('dark')
  monaco.editor.defineTheme(THEME, {
    base: dark ? 'vs-dark' : 'vs',
    inherit: true,
    // Monaco's light theme writes numbers in a green that reads at 4.4:1 on this
    // background, just under what small text needs; the same green darkened by a
    // shade clears it. Everything else its themes use passes (#157).
    rules: dark ? [] : [{ token: 'number', foreground: '07734b' }],
    colors: palette(style),
  })
  monaco.editor.setTheme(THEME)
}

let started = false

/**
 * Configures the language service. Idempotent: the settings are global to
 * Monaco, setting them twice would make no sense.
 */
export function setupMonaco(): void {
  if (started) return
  started = true

  typescriptDefaults.setCompilerOptions({
    target: ScriptTarget.ES2020,
    // **ESNext, never CommonJS**: `import { hsv } from '@candeo/effects-api'`
    // must stay an import in the output, it is rquickjs's module loader that
    // resolves it to the host's internal module.
    module: ModuleKind.ESNext,
    moduleResolution: ModuleResolutionKind.NodeJs,
    // `strict` in full, `noImplicitAny` included.
    //
    // Disabling it would make `layout`, `time` and `frame` implicitly `any` as
    // soon as the author removes the `satisfies EffectModule` from the template
    // — that is, it would **remove autocompletion** in the only case where it
    // is missing, and without saying anything. Yet it is what justifies Monaco.
    //
    // `satisfies EffectModule` is therefore part of the contract, and the
    // starting template as well as the documentation write it. An author who
    // removes it sees an explicit error rather than typing that evaporates.
    strict: true,
    allowNonTsExtensions: true,
    // Neither DOM nor Node: an effect runs in QuickJS, on the Rust side. Neither
    // `document`, nor `fetch`, nor even `console` exist there — offering them in
    // autocompletion would promise what the engine does not provide.
    lib: ['es2020'],
    baseUrl: 'file:///',
    paths: { '@candeo/effects-api': [API_PATH] },
  })

  typescriptDefaults.addExtraLib(effectsApiSource, API_URI)

  // The worker receives the models as soon as they are created, without waiting
  // for the editor to push them. Without this, querying the service right after
  // opening may be about a file it does not have yet.
  typescriptDefaults.setEagerModelSync(true)

  applyTheme()
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme)
  // The theme chosen in the top bar (#110) changes `data-theme`, not the media query.
  new MutationObserver(applyTheme).observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['data-theme'],
  })
}

/**
 * The model of the edited file.
 *
 * A single one, reused: the URI is what links the editor to the language
 * service, and two models cannot carry the same one. Reopening the editor
 * therefore replaces the content rather than creating a second file.
 */
export function effectModel(source: string): monaco.editor.ITextModel {
  const existing = monaco.editor.getModel(EFFECT_URI)
  if (existing) {
    if (existing.getValue() !== source) existing.setValue(source)
    return existing
  }
  return monaco.editor.createModel(source, 'typescript', EFFECT_URI)
}

/** An error from the language service, reduced to what the interface shows. */
export interface EffectError {
  line: number
  message: string
}

/**
 * The errors the language service sees in the file.
 *
 * **Asked of the worker, and not read from the editor's markers.** Markers are
 * set asynchronously: a validation started shortly after opening could read
 * those of an earlier pass, made before the declaration of
 * `@candeo/effects-api` had reached the worker. The service then reported
 * implicitly `any` parameters — hence a refusal, on a perfectly correct effect,
 * with a message that matched nothing visible on screen.
 *
 * The worker, for its part, answers on the current state. There is no longer a
 * window during which the answer is wrong.
 */
export async function errors(): Promise<EffectError[]> {
  const uri = EFFECT_URI.toString()
  const [syntactic, semantic] = await whenReady(async () => {
    const worker = await (await getTypeScriptWorker())(EFFECT_URI)
    return Promise.all([worker.getSyntacticDiagnostics(uri), worker.getSemanticDiagnostics(uri)])
  })

  const model = monaco.editor.getModel(EFFECT_URI)
  return [...syntactic, ...semantic].map((d) => ({
    line: model && d.start !== undefined ? model.getPositionAt(d.start).lineNumber : 1,
    message:
      typeof d.messageText === 'string' ? d.messageText : d.messageText.messageText,
  }))
}

/**
 * Runs `ask` until the TypeScript worker can answer it, for five seconds at most.
 *
 * Monaco sets the TypeScript mode up lazily, then hands the worker the model a
 * moment later: asking in between throws "TypeScript not registered!", then
 * "Could not find source file". A save clicked right after the editor opens must
 * wait for both, not fail.
 */
async function whenReady<T>(ask: () => Promise<T>): Promise<T> {
  const deadline = Date.now() + 5000
  for (;;) {
    try {
      return await ask()
    } catch (e) {
      if (Date.now() >= deadline) throw e
      await new Promise((resolve) => window.setTimeout(resolve, 100))
    }
  }
}

export { monaco }
