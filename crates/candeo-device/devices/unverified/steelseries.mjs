// Writes the SteelSeries Apex definitions from what OpenRGB does with those
// keyboards (`docs/design/other-keyboards.md` §2):
//
//   node crates/candeo-device/devices/unverified/steelseries.mjs
//
// No Apex has been verified: the protocol itself comes from OpenRGB, one
// degree below the Razer files, whose protocol a sibling proved. A frame is one
// feature report, a packet id, then the count of keys and, for each, its HID
// usage and its colour; the packet id is the keyboard's generation's, and the
// third generation takes one report `4b` first.
//
// `steelseries.facts.json` holds the models and where the facts come from; the
// keys are drawn by `keys.mjs`, the ISO board the Razer files are drawn on.
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

import { HID, lights } from './keys.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const root = join(here, '../../../..')
const { formatJson } = await import(
  pathToFileURL(join(root, 'apps/desktop/src/editor/jsonFormat.ts')).href
)

const facts = JSON.parse(readFileSync(join(here, 'steelseries.facts.json'), 'utf8'))
const hex = (n) => n.toString(16).padStart(2, '0')

/** What SteelSeries calls each key: its HID usage, or its own code. */
function address(token) {
  const own = facts.addresses[token]
  if (own) return parseInt(own, 16)
  if (HID[token] === undefined) throw new Error(`no address for “${token}”`)
  return HID[token]
}

const REFERENCE = ['ff0000', '00ff00', '0000ff']

for (const v of facts.variants) {
  const generation = facts.generations[v.generation]
  const { rows, cols, items } = lights({ ...v, matrix: facts.sizes[v.size] }, (t) => address(t))

  // The reference frame, byte by byte: the lights in the grid's order, the
  // first three in the reference colours, all of them in one report.
  const colourOf = (i) => REFERENCE[i] ?? '000000'
  const report = [generation.packet, hex(items.length)]
  items.forEach((item, i) => {
    const c = colourOf(i)
    report.push(hex(item.address), c.slice(0, 2), c.slice(2, 4), c.slice(4, 6))
  })
  while (report.at(-1) === '00') report.pop()

  const notes = [
    `Unverified, protocol included: never tried on a SteelSeries keyboard. Written on ${facts.derived} by steelseries.mjs from ${facts.source}`,
    `Generation ${v.generation}: each image is packet ${generation.packet}, the count of keys, then each key's HID usage and colour, in one ${v.length}-byte feature report${generation.takeOver.length ? `; ${generation.takeOver.join(', ')} first, once` : ''}.`,
    ...(v.notes ?? []),
    'The keyboard\'s own lighting comes back with 3b (41 from the third generation), which OpenRGB sends as an output report: left out until someone sees what it does as a feature report.',
    'Drawn as an ISO board: on an ANSI one, the key left of Z is absent and the backslash lights above Enter.',
    'Does it light? Say so: https://github.com/o2csi/candeo/issues/new?template=device-report.yml',
  ]

  const definition = {
    $schema: 'https://o2csi.github.io/candeo/device-definition.schema.json',
    name: v.name,
    notes,
    match: { vid: '1038', pid: v.pid, ...v.match },
    report: { wire: 'feature', length: v.length, prefix: '00' },
    frame: {
      ...(generation.takeOver.length ? { takeOver: generation.takeOver } : {}),
      each: { chunk: items.length, send: [`${generation.packet} {count} {lights}`], light: '{address} {r} {g} {b}' },
    },
    lights: { kind: 'keys', rows, cols, items },
    examples: [{ colours: REFERENCE, reports: [report.join(' ')] }],
  }
  writeFileSync(join(here, v.file), formatJson(JSON.stringify(definition)))
  console.log(`${v.file}: ${items.length} lights`)
}
