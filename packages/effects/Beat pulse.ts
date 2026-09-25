// Beat pulse — the whole keyboard flashes on each beat of the sound playing,
// and glows with its level in between.
//
// A beat is a sudden jump in the low bands (`inputs: ['audio']`), where a kick
// drum lands, as sensitive as Settings › Sound says. The flash fades over the
// chosen time; between beats the keys keep a glow that follows the
// level, so quiet passages are not dark. The colour can turn a step around the
// colour wheel at each beat.
//
// With nothing playing, which is also how the swatch is sampled, the keyboard
// keeps the colour dimly.

import { defineEffect, hsv, mix } from '@candeo/effects-api'

const COLOR = { r: 255, g: 60, b: 180 }
const BLACK = { r: 0, g: 0, b: 0 }
/** How much of the colour stays lit at rest, and how much the level adds. */
const REST = 0.08
const GLOW = 0.35

// The last beat and the colour it turned to, kept from one frame to the next.
let lastBeat = -Infinity
let turns = 0

/** The colour's hue, in degrees, to turn it around the wheel. */
function hueOf(c = BLACK) {
  const max = Math.max(c.r, c.g, c.b)
  const min = Math.min(c.r, c.g, c.b)
  if (max === min) return 0
  const d = max - min
  const h =
    max === c.r ? ((c.g - c.b) / d) % 6 : max === c.g ? (c.b - c.r) / d + 2 : (c.r - c.g) / d + 4
  return h * 60
}

export default defineEffect({
  description: {
    en: 'The whole keyboard flashing on each beat, glowing with the sound in between',
    fr: 'Tout le clavier qui s’illumine à chaque temps fort, et luit avec le son entre deux',
  },
  kinds: ['keyboard'],
  inputs: ['audio'],
  params: {
    color: { kind: 'color', label: { en: 'Colour', fr: 'Couleur' }, default: COLOR },
    fade: {
      kind: 'number',
      label: { en: 'Flash length (s)', fr: 'Durée du flash (s)' },
      min: 0.05,
      max: 1,
      step: 0.05,
      default: 0.3,
    },
    turn: {
      kind: 'boolean',
      label: { en: 'Change colour on each beat', fr: 'Changer de couleur à chaque temps' },
      default: true,
    },
  },
  render({ layout, time, audio, frame, params }) {
    const base = params.color ?? COLOR
    const fade = Math.max(0.01, Number(params.fade ?? 0.3))

    // A time before the last beat is a restart: the preview starts from zero.
    if (time < lastBeat) lastBeat = -Infinity
    if (audio.beat) {
      lastBeat = time
      turns += 1
    }
    const color = (params.turn ?? true) ? hsv(hueOf(base) + turns * 47, 1, 1) : base
    const flash = Math.exp(-(time - lastBeat) / fade)
    const glow = REST + GLOW * audio.level
    const lit = mix(BLACK, color, Math.min(1, Math.max(flash, glow)))
    for (const key of layout.keys) frame.set(key, lit)
  },
})
