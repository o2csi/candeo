// Clock — the time, scrolling across the keyboard in lit digits.
//
// The keyboard is read as a small display: each key a pixel, digits drawn in a
// 3×5 font over the five top rows, function keys included. The bottom row stays
// dark: its space bar is six keys wide for one light, and would eat a digit's
// foot. The text travels from right to left in physical key units, so a digit
// keeps its width across the staggered rows and the speed is the same with or
// without a numeric keypad.
//
// It reads the wall clock (`inputs: ['clock']`), for which nothing is captured.
// With no clock, which is how the swatch is sampled, it scrolls 00:00.

import { bounds, center, defineEffect } from '@candeo/effects-api'

const COLOR = { r: 255, g: 176, b: 64 }
const BACKGROUND = { r: 4, g: 6, b: 12 }

/** Digits 0 to 9, three columns by five rows, `#` lit. */
const DIGITS = [
  ['###', '#.#', '#.#', '#.#', '###'],
  ['.#.', '##.', '.#.', '.#.', '###'],
  ['###', '..#', '###', '#..', '###'],
  ['###', '..#', '.##', '..#', '###'],
  ['#.#', '#.#', '###', '..#', '..#'],
  ['###', '#..', '###', '..#', '###'],
  ['###', '#..', '###', '#.#', '###'],
  ['###', '..#', '..#', '..#', '..#'],
  ['###', '#.#', '###', '#.#', '###'],
  ['###', '#.#', '###', '..#', '###'],
]
const COLON = ['.', '#', '.', '#', '.']
const NO_COLON = ['.', '.', '.', '.', '.']

/** The five lines of text these glyphs make, one dark column between two. */
function banner(glyphs = [NO_COLON]) {
  const lines = ['', '', '', '', '']
  glyphs.forEach((glyph, i) => {
    for (let line = 0; line < 5; line++) {
      lines[line] += (i === 0 ? '' : '.') + glyph[line]
    }
  })
  return lines
}

export default defineEffect({
  description: {
    en: 'The time scrolling across the keyboard, in lit digits',
    fr: "L'heure qui défile sur le clavier, en chiffres lumineux",
  },
  kinds: ['keyboard'],
  inputs: ['clock'],
  params: {
    color: { kind: 'color', label: { en: 'Digits', fr: 'Chiffres' }, default: COLOR },
    background: { kind: 'color', label: { en: 'Background', fr: 'Fond' }, default: BACKGROUND },
    speed: {
      kind: 'number',
      label: { en: 'Speed (keys per second)', fr: 'Vitesse (touches par seconde)' },
      min: 1,
      max: 20,
      step: 1,
      default: 6,
    },
    hour12: {
      kind: 'boolean',
      label: { en: 'Show the hour on 12', fr: "Afficher l'heure sur 12" },
      default: false,
    },
    blink: {
      kind: 'boolean',
      label: { en: 'Blink the colon', fr: 'Faire clignoter les deux-points' },
      default: true,
    },
  },
  render({ layout, time, clock, frame, params }) {
    const color = params.color ?? COLOR
    const background = params.background ?? BACKGROUND
    const speed = Number(params.speed ?? 6)
    const onTwelve = params.hour12 ?? false

    // Midnight and noon read 12 on a twelve-hour face, and its hours take no
    // leading zero there: 9:05, where a 24-hour clock reads 09:05.
    const hour = onTwelve ? clock.hours % 12 || 12 : clock.hours
    const hourGlyphs =
      onTwelve && hour < 10 ? [DIGITS[hour]] : [DIGITS[Math.floor(hour / 10)], DIGITS[hour % 10]]
    // Lit for the first half of each second, so the clock shows it is running.
    const colon = (params.blink ?? true) && clock.ms >= 500 ? NO_COLON : COLON

    const lines = banner([
      ...hourGlyphs,
      colon,
      DIGITS[Math.floor(clock.minutes / 10)],
      DIGITS[clock.minutes % 10],
    ])
    const width = lines[0].length

    // The text comes in past the right edge and leaves past the left one, then
    // comes round again: a lap is the keyboard's width plus the text's own.
    const area = bounds(layout)
    const lap = area.w + width
    const left = area.x + area.w - ((time * speed) % lap)

    for (const key of layout.keys) {
      // The function row is the text's first line; the bottom row has none.
      const line = key.row
      const column = Math.floor(center(key).x - left)
      const lit =
        line >= 0 && line < 5 && column >= 0 && column < width && lines[line][column] === '#'
      frame.set(key, lit ? color : background)
    }
  },
})
