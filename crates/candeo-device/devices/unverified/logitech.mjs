// Writes the Logitech G815, G813 and G915 definitions from what OpenRGB does
// with those keyboards (`docs/design/other-keyboards.md` §2):
//
//   node crates/candeo-device/devices/unverified/logitech.mjs
//
// No Logitech keyboard has been verified: the protocol comes from OpenRGB, the
// take-over from G HUB as OpenRGB captured it. Reports are HID++ long reports,
// id 11, written on the interrupt pipe: the device index (ff by cable, 01
// through a receiver), the index of a feature, a function. Four keys a report
// through the per-key feature (8081), each by its HID usage less three, then a
// commit.
//
// Two departures from OpenRGB, both said in each file's notes:
// - its take-over switches the firmware's effects off with *persist* set,
//   writing to the keyboard's memory each time; these files leave it unset;
// - it waits for the answer to every report, which a definition cannot: these
//   files send at most ten images a second, and none when nothing changed.
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

import { HID, lights } from './keys.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const root = join(here, '../../../..')
const { formatJson } = await import(
  pathToFileURL(join(root, 'apps/desktop/src/editor/jsonFormat.ts')).href
)

const facts = JSON.parse(readFileSync(join(here, 'logitech.facts.json'), 'utf8'))
const hex = (n) => n.toString(16).padStart(2, '0')

/** What the per-key feature calls each light: modifiers from 0x68, keys at their usage less three. */
function address(token) {
  const own = facts.addresses[token]
  if (own) return parseInt(own, 16)
  const usage = HID[token]
  if (usage === undefined) throw new Error(`no address for “${token}”`)
  return usage >= 0xe0 ? usage - 0x78 : usage - 3
}

/** A report as a definition writes it, the zeros padding it left out. */
const trimmed = (bytes) => {
  const out = [...bytes]
  while (out.at(-1) === '00') out.pop()
  return out.join(' ')
}

const REFERENCE = ['ff0000', '00ff00', '0000ff']
const PER_REPORT = 4

for (const v of facts.variants) {
  const link = facts.links[v.link]
  const grid = facts.sizes[v.size]
  if (new Set(grid.map((r) => r.split(/\s+/).length)).size !== 1) throw new Error(`${v.size}: rows differ`)
  const { rows, cols, items } = lights({ ...v, matrix: grid }, (t) => address(t))

  // The reference frame, byte by byte: the lights in the grid's order, four a
  // report, the first three in the reference colours, then the commit.
  const reports = []
  for (let at = 0; at < items.length; at += PER_REPORT) {
    const bytes = [link.device, link.perKey, '1f']
    items.slice(at, at + PER_REPORT).forEach((item, i) => {
      const c = REFERENCE[at + i] ?? '000000'
      bytes.push(hex(item.address), c.slice(0, 2), c.slice(2, 4), c.slice(4, 6))
    })
    reports.push(trimmed(bytes))
  }
  const commit = `${link.device} ${link.perKey} 7f`
  reports.push(commit)

  const notes = [
    `Unverified, protocol included: never tried on a Logitech keyboard. Written on ${facts.derived} by logitech.mjs from ${facts.source}`,
    `HID++ long reports (id 11) written on the interrupt pipe, device ${link.device}: four keys a report through the per-key feature at index ${link.perKey}, then a commit.`,
    `The take-over is G HUB's, as OpenRGB captured it, but with persist left unset: OpenRGB sets it, writing to the keyboard's memory each time.`,
    'OpenRGB waits for the answer to every report; a definition cannot, so this one sends at most ten images a second, and none when nothing changed.',
    ...(v.notes ?? []),
    'Drawn as an ISO board: on an ANSI one, the key left of Z is absent and the backslash lights above Enter. Lights that are not keys are placed by convention.',
    'Does it light? Say so: https://github.com/o2csi/candeo/issues/new?template=device-report.yml',
  ]

  const definition = {
    $schema: 'https://o2csi.github.io/candeo/device-definition.schema.json',
    name: v.name,
    notes,
    match: { vid: '046d', pid: v.pid, ...v.match },
    report: { wire: 'write', length: 19, prefix: '11' },
    frame: {
      pace: { atLeastMs: 100, skipUnchanged: true },
      takeOver: [
        `${link.device} ${link.effects} 3e`,
        `${link.device} ${link.effects} 1e`,
        `${link.device} ${link.rgb} 1e 00`,
        `${link.device} ${link.rgb} 1e 01`,
      ],
      each: { chunk: PER_REPORT, send: [`${link.device} ${link.perKey} 1f {lights}`], light: '{address} {r} {g} {b}' },
      close: [commit],
    },
    lights: { kind: 'keys', rows, cols, items },
    examples: [{ colours: REFERENCE, reports }],
  }
  writeFileSync(join(here, v.file), formatJson(JSON.stringify(definition)))
  console.log(`${v.file}: ${items.length} lights, ${reports.length} reports an image`)
}
