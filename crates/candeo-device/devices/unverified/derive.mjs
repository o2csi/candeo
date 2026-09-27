// Derives the unverified Razer keyboard definitions from the verified one and
// the facts that differ (`docs/design/device-sdk.md` §9):
//
//   node crates/candeo-device/devices/unverified/derive.mjs
//
// `variants.json` holds, for each model, what differs — product id, interface,
// transaction byte, and its matrix: which key each light is, row by row, as
// `openrgb.mjs` read it from OpenRGB — and where those facts come from.
// Everything else is the verified definition's: its reports with the model's
// transaction byte, a key's rectangle on a standard ISO board.
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

// ------------------------------------------------------------------ keys

/**
 * Each token of a matrix: the key's scancode, PS/2 set 1 as Windows reports
 * it, and its place across a standard ISO board, as the verified definition
 * draws it — `x` and width in keys, the row giving the height. Enter is two
 * lights, its arms: the upper one on the letters' first row.
 */
const KEYS = {
  esc: ['01', 0], f1: ['3b', 2], f2: ['3c', 3], f3: ['3d', 4], f4: ['3e', 5],
  f5: ['3f', 6.5], f6: ['40', 7.5], f7: ['41', 8.5], f8: ['42', 9.5],
  f9: ['43', 11], f10: ['44', 12], f11: ['57', 13], f12: ['58', 14],
  prtsc: ['e037', 15.25], scrlk: ['46', 16.25], pause: ['e11d', 17.25],
  grave: ['29', 0], 1: ['02', 1], 2: ['03', 2], 3: ['04', 3], 4: ['05', 4], 5: ['06', 5], 6: ['07', 6],
  7: ['08', 7], 8: ['09', 8], 9: ['0a', 9], 0: ['0b', 10], minus: ['0c', 11], equal: ['0d', 12], bksp: ['0e', 13, 2],
  ins: ['e052', 15.25], home: ['e047', 16.25], pgup: ['e049', 17.25],
  numlk: ['45', 18.5], kpdiv: ['e035', 19.5], kpmul: ['37', 20.5], kpsub: ['4a', 21.5],
  tab: ['0f', 0, 1.5], q: ['10', 1.5], w: ['11', 2.5], e: ['12', 3.5], r: ['13', 4.5], t: ['14', 5.5],
  y: ['15', 6.5], u: ['16', 7.5], i: ['17', 8.5], o: ['18', 9.5], p: ['19', 10.5],
  lbracket: ['1a', 11.5], rbracket: ['1b', 12.5], backslash: ['2b', 13.5, 1.5],
  del: ['e053', 15.25], end: ['e04f', 16.25], pgdn: ['e051', 17.25],
  kp7: ['47', 18.5], kp8: ['48', 19.5], kp9: ['49', 20.5], kpadd: ['4e', 21.5, 1, 2],
  caps: ['3a', 0, 1.75], a: ['1e', 1.75], s: ['1f', 2.75], d: ['20', 3.75], f: ['21', 4.75], g: ['22', 5.75],
  h: ['23', 6.75], j: ['24', 7.75], k: ['25', 8.75], l: ['26', 9.75], semicolon: ['27', 10.75],
  quote: ['28', 11.75], hash: ['2b', 12.75], enter: ['1c', 13.75, 1.25],
  kp4: ['4b', 18.5], kp5: ['4c', 19.5], kp6: ['4d', 20.5],
  lshift: ['2a', 0, 1.25], iso: ['56', 1.25], z: ['2c', 2.25], x: ['2d', 3.25], c: ['2e', 4.25], v: ['2f', 5.25],
  b: ['30', 6.25], n: ['31', 7.25], m: ['32', 8.25], comma: ['33', 9.25], period: ['34', 10.25],
  slash: ['35', 11.25], rshift: ['36', 12.25, 2.75], up: ['e048', 16.25],
  kp1: ['4f', 18.5], kp2: ['50', 19.5], kp3: ['51', 20.5], kpenter: ['e01c', 21.5, 1, 2],
  lctrl: ['1d', 0, 1.25], lwin: ['e05b', 1.25, 1.25], lalt: ['38', 2.5, 1.25], space: ['39', 3.75, 6.25],
  ralt: ['e038', 10, 1.25], fn: [null, 11.25, 1.25], menu: ['e05d', 12.5, 1.25], rctrl: ['e01d', 13.75, 1.25],
  left: ['e04b', 15.25], down: ['e050', 16.25], right: ['e04d', 17.25], kp0: ['52', 18.5, 2], kpdot: ['53', 20.5],
  // Above the numeric keypad, and beside the up arrow on the Cynosa V2.
  prev: ['e010', 18.5], play: ['e022', 19.5], next: ['e019', 20.5], mute: ['e020', 21.5],
  voldown: ['e02e', 15.25], volup: ['e030', 17.25],
}

/** Lights that are not keys: a name, and a place of their own. */
const OTHERS = {
  m1: ['M1'], m2: ['M2'], m3: ['M3'], m4: ['M4'], m5: ['M5'], m6: ['M6'], dial: ['Dial', 'disc'],
  media: ['Media keys', 'rect', [18.5, 0, 2, 1]],
  voldial: ['Volume dial', 'disc', [20.5, 0, 1, 1]],
  gamebar: ['Game Bar key', 'rect'],
  // Where the logo sits is not in the sources: drawn under the space bar.
  logo: ['Logo', 'disc', [6.5, 6.75, 0.5, 0.5]],
}

/** Enter's upper arm, when the row below has its lower one. */
const UPPER_ARM = ['1c', 13.5, 1.5]

/** The macro keys make a column of their own, left of Esc. */
const MACRO = new Set(['m1', 'm2', 'm3', 'm4', 'm5', 'm6', 'dial'])

/** A row's height on the board: the function row, a gap, then the others. */
const top = (row) => (row === 0 ? 0 : row + 0.5)

function lights(v) {
  const grid = v.matrix.map((row) => row.split(/\s+/))
  for (const f of v.fixes ?? []) grid[f.at[0]][f.at[1]] = f.key
  const cols = grid[0].length
  const shift = grid.some((row) => row.some((t) => MACRO.has(t))) ? 1.25 : 0
  const items = []
  grid.forEach((row, r) =>
    row.forEach((t, c) => {
      if (t === '.') return
      const item = { row: r, col: c, address: r * cols + c }
      const rect = v.rects?.[`${r}:${c}`]
      if (KEYS[t]) {
        const upper = t === 'enter' && grid[r + 1]?.includes('enter')
        const [scancode, x, w = 1, h = 1] = upper ? UPPER_ARM : KEYS[t]
        if (scancode) item.scancode = scancode
        item.rect = rect ?? [x + shift, top(r), w, h]
      } else if (OTHERS[t]) {
        const [name, shape = 'rect', place] = OTHERS[t]
        item.name = name
        if (shape !== 'rect') item.shape = shape
        item.rect = rect ?? place ?? [0, top(r), 1, 1]
      } else {
        throw new Error(`${v.file}: no key "${t}" at ${r}:${c}`)
      }
      items.push(item)
    }),
  )
  return { rows: grid.length, cols, items }
}

// ------------------------------------------------------------ definitions

const REFERENCE = ['ff0000', '00ff00', '0000ff']
const rgb = (text) => [0, 2, 4].map((at) => parseInt(text.slice(at, at + 2), 16))

for (const v of facts.variants) {
  const d = verified
  const { rows, cols, items } = lights(v)

  // The model's frames: the reference colours on the first lights, in the
  // file's order, black elsewhere, then the report handing them the lights.
  const colourAt = new Map(items.slice(0, REFERENCE.length).map((item, i) => [`${item.row}:${item.col}`, REFERENCE[i]]))
  const reports = []
  for (let row = 0; row < rows; row++) {
    const colours = []
    for (let col = 0; col < cols; col++) colours.push(rgb(colourAt.get(`${row}:${col}`) ?? '000000'))
    reports.push(rowReport(v.transaction, row, colours, cols))
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
    `What differs: product id ${v.pid}, interface ${v.interface}, transaction byte ${v.transaction}, a ${rows}×${cols} matrix (${v.connection}).`,
    ...facts.sources.map((s) => `From ${s}`),
    ...(v.notes ?? []),
    ...(v.fixes ?? []).map((f) => `At row ${f.at[0]}, column ${f.at[1]}: ${f.key}. ${f.why}`),
    'Drawn as an ISO board: on an ANSI one, the key left of Z is absent and the backslash lights with Enter. Lights that are not keys are placed by convention.',
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
    lights: { kind: d.lights.kind, rows, cols, items },
    examples: [{ colours: REFERENCE, reports }],
  }
  writeFileSync(join(here, v.file), formatJson(JSON.stringify(definition)))
  console.log(`${v.file}: ${items.length} lights`)
}
