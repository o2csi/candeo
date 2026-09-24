// Scrolling text — any words scrolling across the keyboard, as Clock scrolls
// the time.
//
// Typed in the settings, or sent by another program: switch *Text* to a signal
// and the keyboard shows what it says — a build's state, a message, a
// temperature. The same small display as Clock: the API's 3×5 font (`banner`)
// over the five top rows, the bottom row dark, the text travelling from right
// to left in physical key units. Upper case only; what the font cannot draw is
// a space.

import { banner, bounds, center, defineEffect } from '@candeo/effects-api'

const TEXT = 'CANDEO'
const COLOR = { r: 90, g: 200, b: 255 }
const BACKGROUND = { r: 4, g: 6, b: 12 }

export default defineEffect({
  description: {
    en: 'Any text scrolling across the keyboard, typed here or sent by a signal',
    fr: "N'importe quel texte qui défile sur le clavier, tapé ici ou envoyé par un signal",
  },
  kinds: ['keyboard'],
  params: {
    // As long as a signal's value, so a bound message is never cut.
    text: { kind: 'text', label: { en: 'Text', fr: 'Texte' }, maxLength: 256, default: TEXT },
    color: { kind: 'color', label: { en: 'Letters', fr: 'Lettres' }, default: COLOR },
    background: { kind: 'color', label: { en: 'Background', fr: 'Fond' }, default: BACKGROUND },
    speed: {
      kind: 'number',
      label: { en: 'Speed (keys per second)', fr: 'Vitesse (touches par seconde)' },
      min: 1,
      max: 20,
      step: 1,
      default: 6,
    },
  },
  render({ layout, time, frame, params }) {
    const color = params.color ?? COLOR
    const background = params.background ?? BACKGROUND
    const speed = Number(params.speed ?? 6)
    const lines = banner(params.text ?? TEXT)
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
