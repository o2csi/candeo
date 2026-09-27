// The keys of a standard ISO board, shared by the generators of this folder
// (`docs/design/device-sdk.md` §9): each token of a matrix drawn where the
// verified DeathStalker V2 Pro draws that key, with the scancode it sends, and
// the HID usage that names it for the makers that address keys that way.

/**
 * Each token of a matrix: the key's scancode, PS/2 set 1 as Windows reports
 * it, and its place across a standard ISO board, as the verified definition
 * draws it — `x` and width in keys, the row giving the height. Enter is two
 * lights, its arms: the upper one on the letters' first row.
 */
export const KEYS = {
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
  // SteelSeries: a right Windows key, and Fn where others put Menu.
  rwin: ['e05c', 11.25, 1.25], fnkey: [null, 12.5, 1.25],
  left: ['e04b', 15.25], down: ['e050', 16.25], right: ['e04d', 17.25], kp0: ['52', 18.5, 2], kpdot: ['53', 20.5],
  // Above the numeric keypad, and beside the up arrow on the Cynosa V2.
  prev: ['e010', 18.5], play: ['e022', 19.5], next: ['e019', 20.5], mute: ['e020', 21.5],
  voldown: ['e02e', 15.25], volup: ['e030', 17.25],
}

/** Lights that are not keys: a name, and a place of their own. */
export const OTHERS = {
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

/**
 * A grid of tokens as lights: each one's scancode and rectangle from the tables
 * above, and its address from `address` — its position in the matrix for a
 * Razer, its HID usage for a keyboard addressing keys that way.
 */
export function lights(v, address) {
  const grid = v.matrix.map((row) => row.split(/\s+/))
  for (const f of v.fixes ?? []) grid[f.at[0]][f.at[1]] = f.key
  const cols = grid[0].length
  const shift = grid.some((row) => row.some((t) => MACRO.has(t))) ? 1.25 : 0
  const items = []
  grid.forEach((row, r) =>
    row.forEach((t, c) => {
      if (t === '.') return
      const item = { row: r, col: c, address: address(t, r, c, cols) }
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

/**
 * The HID usage of each key (page 7), what SteelSeries, Logitech and others
 * address a key by. Tokens absent here have none: a family naming such a light
 * says what it calls it.
 */
export const HID = {
  esc: 0x29, f1: 0x3a, f2: 0x3b, f3: 0x3c, f4: 0x3d, f5: 0x3e, f6: 0x3f, f7: 0x40, f8: 0x41,
  f9: 0x42, f10: 0x43, f11: 0x44, f12: 0x45, prtsc: 0x46, scrlk: 0x47, pause: 0x48,
  grave: 0x35, 1: 0x1e, 2: 0x1f, 3: 0x20, 4: 0x21, 5: 0x22, 6: 0x23, 7: 0x24, 8: 0x25, 9: 0x26, 0: 0x27,
  minus: 0x2d, equal: 0x2e, bksp: 0x2a, ins: 0x49, home: 0x4a, pgup: 0x4b,
  numlk: 0x53, kpdiv: 0x54, kpmul: 0x55, kpsub: 0x56,
  tab: 0x2b, q: 0x14, w: 0x1a, e: 0x08, r: 0x15, t: 0x17, y: 0x1c, u: 0x18, i: 0x0c, o: 0x12, p: 0x13,
  lbracket: 0x2f, rbracket: 0x30, backslash: 0x31, del: 0x4c, end: 0x4d, pgdn: 0x4e,
  kp7: 0x5f, kp8: 0x60, kp9: 0x61, kpadd: 0x57,
  caps: 0x39, a: 0x04, s: 0x16, d: 0x07, f: 0x09, g: 0x0a, h: 0x0b, j: 0x0d, k: 0x0e, l: 0x0f,
  semicolon: 0x33, quote: 0x34, hash: 0x32, enter: 0x28, kp4: 0x5c, kp5: 0x5d, kp6: 0x5e,
  lshift: 0xe1, iso: 0x64, z: 0x1d, x: 0x1b, c: 0x06, v: 0x19, b: 0x05, n: 0x11, m: 0x10,
  comma: 0x36, period: 0x37, slash: 0x38, rshift: 0xe5, up: 0x52, kp1: 0x59, kp2: 0x5a, kp3: 0x5b, kpenter: 0x58,
  lctrl: 0xe0, lwin: 0xe3, lalt: 0xe2, space: 0x2c, ralt: 0xe6, rwin: 0xe7, menu: 0x65, rctrl: 0xe4,
  left: 0x50, down: 0x51, right: 0x4f, kp0: 0x62, kpdot: 0x63,
}
