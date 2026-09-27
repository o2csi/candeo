// Writes the ASUS ROG and TUF definitions from what OpenRGB does with those
// keyboards (`docs/design/other-keyboards.md` §2):
//
//   node crates/candeo-device/devices/unverified/asus.mjs --from <OpenRGB checkout>
//   node crates/candeo-device/devices/unverified/asus.mjs
//
// The first reads each keyboard's addresses from OpenRGB's tables into
// `asus.facts.json`, the second writes the definitions from those facts.
//
// No ASUS keyboard has been verified: the protocol comes from OpenRGB. A frame
// is output reports written on the interrupt pipe, each `c0 81`, a count, `00`,
// then up to fifteen keys, each its address and its colour. The address is the
// keyboard's own, from a table per model and per language: the UK one, an ISO
// board like the others of this folder, or the US one where there is no other.
// Only the full-size boards are written, drawn on the same ISO board.
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

import { lights, token } from './keys.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const root = join(here, '../../../..')
const { formatJson } = await import(
  pathToFileURL(join(root, 'apps/desktop/src/editor/jsonFormat.ts')).href
)
const factsPath = join(here, 'asus.facts.json')
const facts = JSON.parse(readFileSync(factsPath, 'utf8'))
const hex = (n) => n.toString(16).padStart(2, '0')

// ------------------------------------------------------------ OpenRGB's tables

/** Each table's languages, and each language's keys: OpenRGB's name, the address. */
function tables(src) {
  const maps = [...src.matchAll(/static std::map<int,layout_info> (\w+) =/g)]
  const out = {}
  maps.forEach((m, i) => {
    const body = src.slice(m.index, maps[i + 1]?.index ?? src.length)
    const marks = [...body.matchAll(/\bASUS_TUF_K7_LAYOUT_([A-Z_]+),\s*\{/g)]
    out[m[1]] = Object.fromEntries(
      marks.map((mark, j) => {
        const part = body.slice(mark.index, marks[j + 1]?.index ?? body.length)
        const keys = [...part.matchAll(/\{\s*(KEY_\w+|"[^"]*")\s*,\s*(0x[0-9A-Fa-f]+)\s*\}/g)]
        return [mark[1], keys.map((k) => [k[1].replace(/^"|"$/g, ''), parseInt(k[2], 16)])]
      }),
    )
  })
  return out
}

/** The names OpenRGB gives lights the shared tables call otherwise. */
const ALSO = { KEY_EN_ISO_ENTER: 'enter', 'Logo 1': 'logo', 'Logo 2': 'logo2' }

function read(checkout) {
  const src = readFileSync(
    join(checkout, 'Controllers/AsusAuraUSBController/AsusAuraTUFKeyboardController/AsusAuraTUFKeyboardLayouts.h'),
    'utf8',
  )
  const all = tables(src)
  for (const name of new Set(facts.variants.map((v) => v.table))) {
    const languages = all[name]
    if (!languages) throw new Error(`${name}: not in OpenRGB's tables`)
    const layout = languages.UK ? 'UK' : 'US'
    // Pairs rather than an object: keys named "1" or "0" would be reordered.
    const keys = []
    const left = []
    for (const [what, address] of languages[layout]) {
      let t
      try {
        t = ALSO[what] ?? token(what)
      } catch {
        left.push(what)
        continue
      }
      keys.push([t, hex(address)])
    }
    facts.tables[name] = { layout, keys, left }
    console.log(`${name}: ${layout}, ${keys.length} keys, ${left.length} lights left out`)
  }
  writeFileSync(factsPath, formatJson(JSON.stringify(facts)))
}

// ---------------------------------------------------------------- definitions

const REFERENCE = ['ff0000', '00ff00', '0000ff']
const PER_REPORT = 15

function write() {
  for (const v of facts.variants) {
    const table = facts.tables[v.table]
    if (!table) throw new Error(`${v.table}: run with --from first`)
    const address = new Map(table.keys)
    // The board drawn with the keys this table names, and those only.
    const grid = facts.grid.map((row) =>
      row
        .split(/\s+/)
        .map((t) => (address.has(t) ? t : '.'))
        .join(' '),
    )
    const drawn = new Set(grid.flatMap((row) => row.split(' ')))
    const missing = [...address.keys()].filter((t) => !drawn.has(t))
    if (missing.length) throw new Error(`${v.file}: nowhere to draw ${missing.join(', ')}`)
    const { rows, cols, items } = lights({ ...v, matrix: grid }, (t) => parseInt(address.get(t), 16))

    // The reference frame, byte by byte: the lights in the grid's order,
    // fifteen a report, the first three in the reference colours.
    const reports = []
    for (let at = 0; at < items.length; at += PER_REPORT) {
      const chunk = items.slice(at, at + PER_REPORT)
      const bytes = ['c0', '81', hex(chunk.length), '00']
      chunk.forEach((item, i) => {
        const c = REFERENCE[at + i] ?? '000000'
        bytes.push(hex(item.address), c.slice(0, 2), c.slice(2, 4), c.slice(4, 6))
      })
      while (bytes.at(-1) === '00') bytes.pop()
      reports.push(bytes.join(' '))
    }

    const notes = [
      `Unverified, protocol included: never tried on an ASUS keyboard. Written on ${facts.derived} by asus.mjs from ${facts.source}`,
      `Output reports written on the interrupt pipe: c0 81, the count of keys, 00, then up to fifteen keys, each its address and colour. The addresses are OpenRGB's ${table.layout} table for this keyboard.`,
      'OpenRGB waits for an answer after each report; a definition cannot, so this one sends at most twenty images a second.',
      ...(table.left.length ? [`Left out, having no place on the board drawn: ${table.left.join(', ')}.`] : []),
      'Drawn as an ISO board: on an ANSI one, the key left of Z is absent and the backslash lights above Enter. Lights that are not keys are placed by convention.',
      'Does it light? Say so: https://github.com/o2csi/candeo/issues/new?template=device-report.yml',
    ]

    const definition = {
      $schema: 'https://o2csi.github.io/candeo/device-definition.schema.json',
      name: v.name,
      notes,
      match: { vid: '0b05', pid: v.pid, interface: 1, usagePage: 'ff00' },
      report: { wire: 'write', length: 64, prefix: '00' },
      frame: {
        pace: { atLeastMs: 50 },
        each: { chunk: PER_REPORT, send: ['c0 81 {count} 00 {lights}'], light: '{address} {r} {g} {b}' },
      },
      lights: { kind: 'keys', rows, cols, items },
      examples: [{ colours: REFERENCE, reports }],
    }
    writeFileSync(join(here, v.file), formatJson(JSON.stringify(definition)))
    console.log(`${v.file}: ${items.length} lights, ${reports.length} reports an image`)
  }
}

const from = process.argv.indexOf('--from')
if (from > 0) read(process.argv[from + 1])
else write()
