// Writes the HyperX Alloy Origins and Alloy Elite 2 definitions from what
// OpenRGB does with those keyboards (`docs/design/other-keyboards.md` §2):
//
//   node crates/candeo-device/devices/unverified/hyperx.mjs
//
// No HyperX keyboard has been verified: the protocol comes from OpenRGB. An
// image is one report opening it, then nine feature reports of sixteen lights
// each, `81` and the colour, in the keyboard's order of its lights with some
// positions left dark. That order is the wire's: the files lay their grid out
// as it, nine rows of sixteen, and draw each key where it sits on the board.
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

import { lights } from './keys.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const root = join(here, '../../../..')
const { formatJson } = await import(
  pathToFileURL(join(root, 'apps/desktop/src/editor/jsonFormat.ts')).href
)

const facts = JSON.parse(readFileSync(join(here, 'hyperx.facts.json'), 'utf8'))
const hex = (n) => n.toString(16).padStart(2, '0')
const ROWS = 9
const PER_REPORT = 16

/** Where each light goes on the wire: its place in the order, past the dark positions. */
function positions(family) {
  const skip = new Set(family.skip)
  const at = new Map()
  let p = 0
  for (const t of family.leds) {
    while (skip.has(p)) p++
    if (t) at.set(t, p)
    p++
  }
  if (p > ROWS * PER_REPORT) throw new Error(`${p} positions past ${ROWS} reports`)
  return at
}

const REFERENCE = ['ff0000', '00ff00', '0000ff']

for (const v of facts.variants) {
  const family = facts.families[v.family]
  const wire = positions(family)
  // Drawn on the board, then placed in the grid the wire makes.
  const board = family.grid.map((row) =>
    row
      .split(/\s+/)
      .map((t) => (wire.has(t) ? t : '.'))
      .join(' '),
  )
  const drawn = new Set(board.flatMap((row) => row.split(' ')))
  const missing = [...wire.keys()].filter((t) => !drawn.has(t))
  if (missing.length) throw new Error(`${v.file}: nowhere to draw ${missing.join(', ')}`)
  const placed = lights({ ...v, matrix: board }, (t) => wire.get(t)).items
  const items = placed
    .map((item) => ({ ...item, row: Math.floor(item.address / PER_REPORT), col: item.address % PER_REPORT }))
    .sort((a, b) => a.address - b.address)

  // The reference frame, byte by byte: the opening report, then each row of
  // the wire, the first three lights in the grid's order in the reference
  // colours, every other cell black.
  const colourAt = new Map(items.slice(0, REFERENCE.length).map((item, i) => [item.address, REFERENCE[i]]))
  const reports = [family.open]
  for (let row = 0; row < ROWS; row++) {
    const bytes = []
    for (let col = 0; col < PER_REPORT; col++) {
      const c = colourAt.get(row * PER_REPORT + col) ?? '000000'
      bytes.push('81', c.slice(0, 2), c.slice(2, 4), c.slice(4, 6))
    }
    while (bytes.at(-1) === '00') bytes.pop()
    reports.push(bytes.join(' '))
  }

  const notes = [
    `Unverified, protocol included: never tried on a HyperX keyboard. Written on ${facts.derived} by hyperx.mjs from ${facts.source}`,
    `Feature reports: ${family.open}, then nine of sixteen lights, 81 and the colour, in the keyboard's order of its lights. The grid is that order, nine rows of sixteen; the drawing is the board's.`,
    'OpenRGB opens interface 0 outside Windows: this file names interface 3, as it does on Windows.',
    'The firmware takes its own lighting back when images stop coming: an effect keeps it, a still image may not.',
    `Left out, having no place on the board drawn: ${family.left.join(', ')}.`,
    'Drawn as an ISO board: on an ANSI one, the key left of Z is absent and the backslash lights above Enter.',
    'Does it light? Say so: https://github.com/o2csi/candeo/issues/new?template=device-report.yml',
  ]

  const definition = {
    $schema: 'https://o2csi.github.io/candeo/device-definition.schema.json',
    name: v.name,
    notes,
    match: { vid: v.vid, pid: v.pid, ...v.match },
    report: { wire: 'feature', length: 64, prefix: '00' },
    frame: {
      open: [family.open],
      each: { by: 'row', send: ['{lights}'], light: '81 {r} {g} {b}' },
    },
    lights: { kind: 'keys', rows: ROWS, cols: PER_REPORT, items },
    examples: [{ colours: REFERENCE, reports }],
  }
  writeFileSync(join(here, v.file), formatJson(JSON.stringify(definition)))
  console.log(`${v.file}: ${items.length} lights`)
}
