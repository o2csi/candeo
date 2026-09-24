// Internal module `@candeo/effects-api`, provided by the host to the QuickJS
// engine.
//
// ⚠️ This file and `packages/effects-api/src/index.ts` describe the SAME API:
// the `.d.ts` is what the editor shows in autocompletion, this is what the
// engine actually provides. If they diverge, the editor promises a function that
// does not exist, and the error only shows at the first frame.
//
// The test `api_js_exports_match_the_typescript_surface` fails if a name
// disappears from here. Any change must touch both files.

export const BLACK = { r: 0, g: 0, b: 0 }

/**
 * Declares an effect. Identity at run time — it only exists to give a
 * contextual type on the editor side, and to spare the author from writing
 * `satisfies EffectModule`.
 */
export function defineEffect(effect) {
  return effect
}

function clampByte(v) {
  // `| 0` truncates toward zero and discards NaN — an effect that produces NaN
  // must give black, not an indeterminate color.
  const n = Math.round(v)
  if (!(n >= 0)) return 0
  return n > 255 ? 255 : n | 0
}

export function rgb(r, g, b) {
  return { r: clampByte(r), g: clampByte(g), b: clampByte(b) }
}

/** Hue 0-360, saturation and value 0-1. */
export function hsv(h, s, v) {
  const c = v * s
  const hp = (((h % 360) + 360) % 360) / 60
  const x = c * (1 - Math.abs((hp % 2) - 1))
  let r, g, b
  if (hp < 1) [r, g, b] = [c, x, 0]
  else if (hp < 2) [r, g, b] = [x, c, 0]
  else if (hp < 3) [r, g, b] = [0, c, x]
  else if (hp < 4) [r, g, b] = [0, x, c]
  else if (hp < 5) [r, g, b] = [x, 0, c]
  else [r, g, b] = [c, 0, x]
  const m = v - c
  return rgb((r + m) * 255, (g + m) * 255, (b + m) * 255)
}

export function lerp(a, b, t) {
  return a + (b - a) * t
}

export function mix(a, b, t) {
  return rgb(lerp(a.r, b.r, t), lerp(a.g, b.g, t), lerp(a.b, b.b, t))
}

// ------------------------------------------------------------------- geometry
//
// Both read a rectangle **or throw**. A layout without surveyed geometry would
// give `undefined`, hence NaN, hence black clamped to zero without an error:
// that is the silence refused here.

function noRectangle(key) {
  const which = key?.label === undefined ? `position ${key?.index}` : `“${key.label}”`
  return (
    `${which} has no rectangle: this layout has no surveyed geometry, ` +
    'and an effect that measures physical distances has nothing to measure on it.'
  )
}

/** The center of the keycap — where the LED is, not its corner. */
export function center(key) {
  const { x, y, w, h } = key ?? {}
  if (x === undefined || y === undefined || w === undefined || h === undefined) {
    throw new TypeError(noRectangle(key))
  }
  return { x: x + w / 2, y: y + h / 2 }
}

/** The footprint of the drawing, in pitch units. */
export function bounds(layout) {
  const keys = layout?.keys ?? []
  if (keys.length === 0) return { x: 0, y: 0, w: 0, h: 0 }

  let x0 = Infinity
  let y0 = Infinity
  let x1 = -Infinity
  let y1 = -Infinity

  for (const key of keys) {
    const { x, y, w, h } = key ?? {}
    if (x === undefined || y === undefined || w === undefined || h === undefined) {
      throw new TypeError(noRectangle(key))
    }
    if (x < x0) x0 = x
    if (y < y0) y0 = y
    if (x + w > x1) x1 = x + w
    if (y + h > y1) y1 = y + h
  }

  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
}

// ----------------------------------------------------------------------- text
//
// The 3x5 font of `index.ts`, line for line: the test
// `the_font_is_the_same_in_both_api_files` compares the two tables.

const FONT = {
  // font:begin
  ' ': ['.', '.', '.', '.', '.'],
  '!': ['#', '#', '#', '.', '#'],
  '"': ['#.#', '#.#', '...', '...', '...'],
  "'": ['#', '#', '.', '.', '.'],
  '%': ['#.#', '..#', '.#.', '#..', '#.#'],
  '(': ['.#', '#.', '#.', '#.', '.#'],
  ')': ['#.', '.#', '.#', '.#', '#.'],
  '*': ['...', '#.#', '.#.', '#.#', '...'],
  '+': ['...', '.#.', '###', '.#.', '...'],
  ',': ['.', '.', '.', '#', '#'],
  '-': ['...', '...', '###', '...', '...'],
  '.': ['.', '.', '.', '.', '#'],
  '/': ['..#', '..#', '.#.', '#..', '#..'],
  '0': ['###', '#.#', '#.#', '#.#', '###'],
  '1': ['.#.', '##.', '.#.', '.#.', '###'],
  '2': ['###', '..#', '###', '#..', '###'],
  '3': ['###', '..#', '.##', '..#', '###'],
  '4': ['#.#', '#.#', '###', '..#', '..#'],
  '5': ['###', '#..', '###', '..#', '###'],
  '6': ['###', '#..', '###', '#.#', '###'],
  '7': ['###', '..#', '..#', '..#', '..#'],
  '8': ['###', '#.#', '###', '#.#', '###'],
  '9': ['###', '#.#', '###', '..#', '###'],
  ':': ['.', '#', '.', '#', '.'],
  ';': ['.', '#', '.', '#', '#'],
  '<': ['..#', '.#.', '#..', '.#.', '..#'],
  '=': ['...', '###', '...', '###', '...'],
  '>': ['#..', '.#.', '..#', '.#.', '#..'],
  '?': ['##.', '..#', '.#.', '...', '.#.'],
  A: ['.#.', '#.#', '###', '#.#', '#.#'],
  B: ['##.', '#.#', '##.', '#.#', '##.'],
  C: ['.##', '#..', '#..', '#..', '.##'],
  D: ['##.', '#.#', '#.#', '#.#', '##.'],
  E: ['###', '#..', '##.', '#..', '###'],
  F: ['###', '#..', '##.', '#..', '#..'],
  G: ['.##', '#..', '#.#', '#.#', '.##'],
  H: ['#.#', '#.#', '###', '#.#', '#.#'],
  I: ['###', '.#.', '.#.', '.#.', '###'],
  J: ['..#', '..#', '..#', '#.#', '.#.'],
  K: ['#.#', '#.#', '##.', '#.#', '#.#'],
  L: ['#..', '#..', '#..', '#..', '###'],
  M: ['#.#', '###', '###', '#.#', '#.#'],
  N: ['##.', '#.#', '#.#', '#.#', '#.#'],
  O: ['.#.', '#.#', '#.#', '#.#', '.#.'],
  P: ['##.', '#.#', '##.', '#..', '#..'],
  Q: ['.#.', '#.#', '#.#', '##.', '.##'],
  R: ['##.', '#.#', '##.', '#.#', '#.#'],
  S: ['.##', '#..', '.#.', '..#', '##.'],
  T: ['###', '.#.', '.#.', '.#.', '.#.'],
  U: ['#.#', '#.#', '#.#', '#.#', '###'],
  V: ['#.#', '#.#', '#.#', '#.#', '.#.'],
  W: ['#.#', '#.#', '###', '###', '#.#'],
  X: ['#.#', '#.#', '.#.', '#.#', '#.#'],
  Y: ['#.#', '#.#', '.#.', '.#.', '.#.'],
  Z: ['###', '..#', '.#.', '#..', '###'],
  _: ['...', '...', '...', '...', '###'],
  '°': ['##', '##', '..', '..', '..'],
  // font:end
}

/** The five lines of `text` in the 3x5 font, `#` lit and `.` dark. */
export function banner(text) {
  const lines = ['', '', '', '', '']
  const plain = String(text).normalize('NFD').replace(/[\u0300-\u036f]/g, '').toUpperCase()
  Array.from(plain).forEach((char, i) => {
    const glyph = FONT[char] ?? FONT[' ']
    for (let line = 0; line < 5; line++) lines[line] += (i === 0 ? '' : '.') + glyph[line]
  })
  return lines
}
