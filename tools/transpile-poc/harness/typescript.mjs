// The current path, as the editor runs it: `transpile()` from
// apps/desktop/src/editor/effect.ts, imported as it is (Node strips its types),
// with the TypeScript compiler Monaco ships.
//
//   node harness/typescript.mjs <out.json> <dir>...
//
// Writes, for every `.ts` file of every directory, the JavaScript the editor
// would hand to `cache_effect` and the median time of one call. Run by
// `cargo run --release -- compare`, which compares it with Oxc's.

import { readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { performance } from 'node:perf_hooks'

const [out, ...dirs] = process.argv.slice(2)
if (!out || dirs.length === 0) {
  console.error('usage: node harness/typescript.mjs <out.json> <dir>...')
  process.exit(2)
}

const heapBefore = process.memoryUsage().heapUsed
const started = performance.now()
const { transpile } = await import('../../../apps/desktop/src/editor/effect.ts')
// The compiler is loaded on the first call, as in the window.
await transpile('export default {}')
const coldMs = performance.now() - started
const heapAfter = process.memoryUsage().heapUsed

const { typescriptVersion } = await import(
  '../../../apps/desktop/node_modules/monaco-editor/esm/vs/languages/features/typescript/lib/typescriptServicesMetadata.js'
)

const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b)
  return sorted[Math.floor(sorted.length / 2)]
}

const results = []
for (const dir of dirs) {
  for (const file of readdirSync(dir).filter((f) => f.endsWith('.ts')).sort()) {
    const source = readFileSync(join(dir, file), 'utf8')
    // Fewer runs for the inputs measured in megabytes.
    const runs = source.length > 200_000 ? 3 : 20
    let js = null
    let error = null
    const times = []
    for (let i = 0; i < runs + 1; i++) {
      const t = performance.now()
      try {
        js = await transpile(source)
      } catch (e) {
        // What the window would show: the call itself failed.
        error = `${e?.name ?? 'Error'}: ${e?.message ?? e}`
        break
      }
      if (i > 0) times.push(performance.now() - t)
    }
    results.push({ dir, file, js, error, ms: times.length ? median(times) : null })
  }
}

writeFileSync(
  out,
  JSON.stringify({
    typescriptVersion,
    node: process.version,
    coldMs,
    heapAfterLoadMb: (heapAfter - heapBefore) / (1024 * 1024),
    results,
  }),
)
