// Derives the unverified definitions of a family from its verified one
// (`docs/design/device-sdk.md` §9):
//
//   node crates/candeo-device/devices/unverified/derive.mjs
//
// `variants.json` holds what differs for each model — product id, interface,
// transaction byte, matrix width — and where those facts come from. Everything
// else is the verified definition's: its reports with the model's transaction
// byte, its lights cut to the model's grid.
//
// The reference frames are worked out here, byte by byte, and not by the
// interpreter that will replay them: a mistake in either shows as a mismatch
// in the tests. They prove the file says what its facts say — not that the
// device answers, which only the device can.
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const root = join(here, '../../../..')
const { formatJson } = await import(
  pathToFileURL(join(root, 'apps/desktop/src/editor/jsonFormat.ts')).href
)

const facts = JSON.parse(readFileSync(join(here, 'variants.json'), 'utf8'))
const verified = JSON.parse(readFileSync(join(here, '..', facts.from), 'utf8'))

const hex = (n) => n.toString(16).padStart(2, '0')

/** The transaction byte is the report's second: `00 9f …`. */
function withTransaction(report, transaction) {
  return report.replace(/^([0-9a-f]{2}) [0-9a-f]{2}/i, `$1 ${transaction}`)
}

/**
 * The Razer report carrying one row: status, transaction, then the row's
 * arguments from byte 8, and the XOR of bytes 2 to 87 in byte 88. Written as
 * a definition writes it: without the zeros padding it.
 */
function rowReport(transaction, row, colours, cols) {
  const args = [0, 0, row, 0, cols - 1, ...colours.flat()]
  return sealed([0x00, parseInt(transaction, 16), 0, 0, 0, args.length, 0x0f, 0x03, ...args])
}

/** A report's bytes padded to 90, the XOR of bytes 2 to 87 in byte 88, the padding left out. */
function sealed(start) {
  const bytes = new Array(90).fill(0)
  bytes.splice(0, start.length, ...start)
  bytes[88] = bytes.slice(2, 88).reduce((x, b) => x ^ b, 0)
  const end = bytes.findLastIndex((b) => b !== 0) + 1
  return bytes.slice(0, end).map(hex).join(' ')
}

/** A fixed report as a frame sends it: its checksum filled in. */
const fixed = (report) => sealed(report.trim().split(/\s+/).map((b) => parseInt(b, 16)))

const REFERENCE = ['ff0000', '00ff00', '0000ff']

for (const v of facts.variants) {
  const d = structuredClone(verified)
  const items = d.lights.items.filter((item) => item.col < v.cols)

  // The model's frames: the reference colours on the first lights, in the
  // file's order, black elsewhere, then the report handing them the lights.
  const colourAt = new Map(items.slice(0, REFERENCE.length).map((item, i) => [`${item.row}:${item.col}`, REFERENCE[i]]))
  const rgb = (text) => [0, 2, 4].map((at) => parseInt(text.slice(at, at + 2), 16))
  const reports = []
  for (let row = 0; row < d.lights.rows; row++) {
    const colours = []
    for (let col = 0; col < v.cols; col++) colours.push(rgb(colourAt.get(`${row}:${col}`) ?? '000000'))
    reports.push(rowReport(v.transaction, row, colours, v.cols))
  }
  const close = d.frame.close.map((r) => withTransaction(r, v.transaction))
  reports.push(...close.map(fixed))

  const firmware = d.firmware && {
    ...d.firmware,
    send: withTransaction(d.firmware.send, v.transaction),
    effects: d.firmware.effects.map((e) => (e.send ? { ...e, send: withTransaction(e.send, v.transaction) } : e)),
  }

  const notes = [
    `Unverified: never tried on the device. Derived on ${facts.derived} from ${facts.from}, verified on firmware v${verified.surveyed}, by derive.mjs.`,
    `What differs, from ${facts.source}: product id ${v.pid}, interface ${v.interface}, transaction byte ${v.transaction}, a ${d.lights.rows}×${v.cols} grid (${v.connection}).`,
    ...(v.cols < d.lights.cols
      ? [`The grid is the full-size one without its numeric keypad: columns ${v.cols} to ${d.lights.cols - 1} are left out.`]
      : []),
    'The inspection is left out until someone confirms the device answers it.',
    'Does it light? Say so: https://github.com/o2csi/candeo/issues/new?template=device-report.yml',
  ]

  const definition = {
    $schema: d.$schema,
    name: v.name,
    notes,
    match: { vid: d.match.vid, pid: v.pid, interface: v.interface },
    report: d.report,
    frame: {
      ...d.frame,
      each: { ...d.frame.each, send: d.frame.each.send.map((r) => withTransaction(r, v.transaction)) },
      close,
    },
    brightness: d.brightness.map((r) => withTransaction(r, v.transaction)),
    ...(firmware ? { firmware } : {}),
    lights: { ...d.lights, cols: v.cols, items },
    examples: [{ colours: REFERENCE, reports }],
  }
  writeFileSync(join(here, v.file), formatJson(JSON.stringify(definition)))
  console.log(`${v.file}: ${items.length} lights`)
}
