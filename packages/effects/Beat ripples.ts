// Beat ripples — each beat of the sound playing sends a ring out from the middle
// of the keyboard.
//
// Ripples, beaten by the music (`inputs: ['audio']`): a beat, a sudden jump in
// the low bands strong enough for the sensitivity chosen, starts a ring at the
// keyboard's physical centre, and it spreads and fades. Its colour can follow the sound: red when the music sits low, blue
// when it sits high, from where its energy is across the bands at that beat.
//
// With nothing playing, which is also how the swatch is sampled, only the
// background shows.

import { bounds, center, defineEffect, hsv, mix } from '@candeo/effects-api'

const COLOR = { r: 80, g: 200, b: 255 }
const BACKGROUND = { r: 4, g: 6, b: 16 }
/** A ring's thickness, in keys. */
const WIDTH = 1.4
/** More rings than this at once is a blur, not ripples. */
const MOST = 8

// The rings under way: when each started, and its colour.
let rings = [{ start: -Infinity, color: COLOR }]

/** Where the sound sits, 0 all low to 1 all high: its energy's centre across the bands. */
function toneOf(bands = [0]) {
  const total = bands.reduce((sum, b) => sum + b, 0)
  if (total === 0) return 0.5
  return bands.reduce((sum, b, i) => sum + b * i, 0) / total / (bands.length - 1)
}

export default defineEffect({
  description: {
    en: 'A ring spreading from the middle of the keyboard on each beat of the sound',
    fr: 'Un anneau qui part du centre du clavier à chaque temps fort du son',
  },
  kinds: ['keyboard'],
  inputs: ['audio'],
  params: {
    color: { kind: 'color', label: { en: 'Colour', fr: 'Couleur' }, default: COLOR },
    follow: {
      kind: 'boolean',
      label: {
        en: 'Colour by the sound, low red to high blue',
        fr: 'Couleur selon le son, grave rouge à aigu bleu',
      },
      default: true,
    },
    background: { kind: 'color', label: { en: 'Background', fr: 'Fond' }, default: BACKGROUND },
    sensitivity: {
      kind: 'number',
      label: { en: 'Beat sensitivity', fr: 'Sensibilité aux temps forts' },
      min: 0,
      max: 1,
      step: 0.05,
      default: 0.5,
    },
    speed: {
      kind: 'number',
      label: { en: 'Speed (keys per second)', fr: 'Vitesse (touches par seconde)' },
      min: 5,
      max: 40,
      step: 1,
      default: 18,
    },
  },
  render({ layout, time, audio, frame, params }) {
    if (layout.keys.length === 0) return
    const background = params.background ?? BACKGROUND
    const speed = Number(params.speed ?? 18)
    // Half, the default, is the analysis's own beat; more catches softer hits.
    const threshold = Math.max(0.1, 1 - Number(params.sensitivity ?? 0.5))
    const last = rings.length ? rings[rings.length - 1].start : -Infinity
    const area = bounds(layout)
    const cx = area.x + area.w / 2
    const cy = area.y + area.h / 2
    // A ring is gone once it has crossed the keyboard.
    const life = Math.hypot(area.w, area.h) / 2 / speed + 0.2

    // A time before the last ring is a restart: the preview starts from zero.
    if (rings.some((ring) => ring.start > time)) rings = []
    if (audio.onset >= threshold && time - last >= 0.12) {
      const tone = toneOf([...audio.bands])
      const color = (params.follow ?? true) ? hsv(240 * tone, 1, 1) : (params.color ?? COLOR)
      rings.push({ start: time, color })
    }
    rings = rings.filter((ring) => time - ring.start < life).slice(-MOST)

    for (const key of layout.keys) {
      const c = center(key)
      const d = Math.hypot(c.x - cx, c.y - cy)
      let color = background
      for (const ring of rings) {
        const age = time - ring.start
        const near = 1 - Math.abs(d - age * speed) / WIDTH
        if (near > 0) color = mix(color, ring.color, near * (1 - age / life))
      }
      frame.set(key, color)
    }
  },
})
