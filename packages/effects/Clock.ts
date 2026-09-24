// Clock — the time across the keyboard in lit digits, scrolling or still.
//
// The keyboard is read as a small display: each key a pixel, digits drawn in
// the API's 3×5 font (`banner`) over the five top rows, function keys
// included. The bottom row stays dark: its space bar is six keys wide for one
// light, and would eat a digit's foot. The text travels from right to left in
// physical key units, so a digit keeps its width across the staggered rows and
// the speed is the same with or without a numeric keypad.
//
// Three displays, as Scrolling text has (#217): smooth scrolling, each key lit
// by how much of its width the digits cover, so they glide across the
// staggered rows; sharp scrolling, each key on or off by where its centre
// falls, as Clock was drawn before; and still, the time centred and held, which
// reads best — unless it does not fit, seconds on a narrow keyboard, and then
// it scrolls smoothly.
//
// It reads the wall clock (`inputs: ['clock']`), for which nothing is captured.
// With no clock, which is how the swatch is sampled, it shows 00:00.

import { banner, bounds, center, defineEffect, mix } from '@candeo/effects-api'

const COLOR = { r: 255, g: 176, b: 64 }
const BACKGROUND = { r: 4, g: 6, b: 12 }

/** Two digits, as a clock face shows minutes and seconds. */
function twoDigits(n = 0) {
  return String(n).padStart(2, '0')
}

export default defineEffect({
  description: {
    en: 'The time across the keyboard in lit digits, scrolling or still',
    fr: "L'heure sur le clavier en chiffres lumineux, qui défile ou fixe",
  },
  kinds: ['keyboard'],
  inputs: ['clock'],
  params: {
    color: { kind: 'color', label: { en: 'Digits', fr: 'Chiffres' }, default: COLOR },
    background: { kind: 'color', label: { en: 'Background', fr: 'Fond' }, default: BACKGROUND },
    display: {
      kind: 'choice',
      label: { en: 'Display', fr: 'Affichage' },
      options: [
        { value: 'smooth', label: { en: 'Smooth scrolling', fr: 'Défilement lissé' } },
        { value: 'sharp', label: { en: 'Sharp scrolling', fr: 'Défilement net' } },
        { value: 'still', label: { en: 'Still', fr: 'Fixe' } },
      ],
      default: 'smooth',
    },
    speed: {
      kind: 'number',
      label: {
        en: 'Scrolling speed (keys per second)',
        fr: 'Vitesse du défilement (touches par seconde)',
      },
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
    seconds: {
      kind: 'boolean',
      label: { en: 'Show the seconds', fr: 'Afficher les secondes' },
      default: false,
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
    // Lit for the first half of each second, so the clock shows it is running.
    // A space is as wide as the colon, so the digits do not move when it blinks.
    const colon = (params.blink ?? true) && clock.ms >= 500 ? ' ' : ':'

    let text = (onTwelve ? String(hour) : twoDigits(hour)) + colon + twoDigits(clock.minutes)
    // The seconds change while the text crosses the keyboard, as a ticker's do.
    if (params.seconds ?? false) text += colon + twoDigits(clock.seconds)
    const lines = banner(text)
    const width = lines[0].length
    const area = bounds(layout)

    // On or off, by the text column each key's centre falls in.
    const sharp = (left = 0) => {
      for (const key of layout.keys) {
        // The function row is the text's first line; the bottom row has none.
        const line = key.row
        const column = Math.floor(center(key).x - left)
        const lit =
          line >= 0 && line < 5 && column >= 0 && column < width && lines[line][column] === '#'
        frame.set(key, lit ? color : background)
      }
    }
    // By how much of each key's width, in text columns, the lit pixels cover.
    const smooth = (left = 0) => {
      for (const key of layout.keys) {
        const line = key.row
        if (line < 0 || line >= 5) {
          frame.set(key, background)
          continue
        }
        const keyWidth = key.w ?? 1
        const from = center(key).x - keyWidth / 2 - left
        const to = from + keyWidth
        const last = Math.min(width, Math.ceil(to))
        let lit = 0
        for (let column = Math.max(0, Math.floor(from)); column < last; column++) {
          if (lines[line][column] === '#') lit += Math.min(to, column + 1) - Math.max(from, column)
        }
        frame.set(key, mix(background, color, lit / keyWidth))
      }
    }

    const display = params.display ?? 'smooth'
    if (display === 'still' && width <= area.w) {
      sharp(area.x + (area.w - width) / 2)
      return
    }
    // The text comes in past the right edge and leaves past the left one, then
    // comes round again: a lap is the keyboard's width plus the text's own.
    const left = area.x + area.w - ((time * speed) % (area.w + width))
    if (display === 'sharp') sharp(left)
    else smooth(left)
  },
})
