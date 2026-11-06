// Monaco's TypeScript run inside QuickJS, the engine Candeo already embeds: the
// other headless option (`docs/design/headless-transpile-poc.md` §5). Loaded by
// `cargo run --release -- typescript-in-quickjs`, as the module `driver`, next
// to `typescript` (typescriptServices.js) and `typescript-lib` (lib.js).

import { typescript as ts } from 'typescript'
import { libFileMap } from 'typescript-lib'

// The editor's settings, from `apps/desktop/src/editor/monaco.ts`.
const API_PATH = '/node_modules/@candeo/effects-api/index.ts'
const OPTIONS = {
  target: ts.ScriptTarget.ES2020,
  module: ts.ModuleKind.ESNext,
  moduleResolution: ts.ModuleResolutionKind.NodeJs,
  strict: true,
  allowNonTsExtensions: true,
  lib: ['lib.es2020.d.ts'],
  baseUrl: '/',
  paths: { '@candeo/effects-api': [API_PATH] },
  noEmit: true,
}

// `transpile()` from `apps/desktop/src/editor/effect.ts`, line for line.
globalThis.__transpile = (source) => {
  const { outputText, diagnostics } = ts.transpileModule(source, {
    fileName: 'effect.ts',
    reportDiagnostics: true,
    compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
  })
  const first = diagnostics?.find((d) => d.category === ts.DiagnosticCategory.Error)
  return first ? `throw new SyntaxError(${JSON.stringify(located(first))})\n` : outputText
}

function located(diagnostic) {
  const text = ts.flattenDiagnosticMessageText(diagnostic.messageText, ' ')
  if (!diagnostic.file || diagnostic.start === undefined) return text
  const { line, character } = diagnostic.file.getLineAndCharacterOfPosition(diagnostic.start)
  return `line ${line + 1}, column ${character + 1}: ${text}`
}

// The libraries are parsed once and kept: they do not change between checks.
const parsed = new Map()

globalThis.__check = (source, apiSource) => {
  const files = { '/effect.ts': source, [API_PATH]: apiSource }
  const read = (name) => files[name] ?? libFileMap[name.replace(/^\//, '')]
  const host = {
    getSourceFile(name, version) {
      if (name in files) return ts.createSourceFile(name, files[name], version)
      if (!parsed.has(name)) {
        const text = read(name)
        parsed.set(name, text === undefined ? undefined : ts.createSourceFile(name, text, version))
      }
      return parsed.get(name)
    },
    getDefaultLibFileName: () => '/lib.es2020.d.ts',
    writeFile: () => {},
    getCurrentDirectory: () => '/',
    getDirectories: () => [],
    fileExists: (name) => read(name) !== undefined,
    readFile: read,
    getCanonicalFileName: (name) => name,
    useCaseSensitiveFileNames: () => true,
    getNewLine: () => '\n',
  }
  const program = ts.createProgram(['/effect.ts'], OPTIONS, host)
  const diagnostics = ts
    .getPreEmitDiagnostics(program)
    .filter((d) => d.category === ts.DiagnosticCategory.Error)
  return JSON.stringify(
    diagnostics.map((d) => (d.file?.fileName === '/effect.ts' ? located(d) : `${d.file?.fileName ?? ''} ${located(d)}`)),
  )
}

globalThis.__version = ts.version
