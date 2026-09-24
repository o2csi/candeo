// Fireworks — bursts of light on random keys at each beat of the sound
// playing, more of them when it is loud.
//
// Each beat (`inputs: ['audio']`) sets off one to four bursts, by the level:
// each opens from a key, a disc of light that widens and fades in about half a
// second, in a colour of its own. Where two meet, the brighter shows.
//
// With nothing playing, which is also how the swatch is sampled, only the
// background shows.

import { center, defineEffect, hsv, mix } from '@candeo/effects-api'

const BACKGROUND = { r: 3, g: 3, b: 10 }
/** How far a burst reaches, in keys, and how long it lasts, in seconds. */
const REACH = 2.5
const LIFE = 0.6
/** More bursts than this at once is noise, not fireworks. */
const MOST = 12

/** A number from 0 to 1 that looks random and is the same for the same `n`. */
function hash(n = 0) {
  const x = Math.sin(n * 12.9898 + 78.233) * 43758.5453
  return x - Math.floor(x)
}

// The bursts under way, and how many were set off, to draw the next ones.
let bursts = [{ start: -Infinity, x: 0, y: 0, color: BACKGROUND }]
let fired = 0

export default defineEffect({
  description: {
    en: 'Bursts of light on random keys at each beat of the sound, more when it is loud',
    fr: 'Des gerbes de lumière sur des touches au hasard à chaque temps fort, plus nombreuses quand c’est fort',
  },
  kinds: ['keyboard'],
  inputs: ['audio'],
  params: {
    background: { kind: 'color', label: { en: 'Background', fr: 'Fond' }, default: BACKGROUND },
    saturation: {
      kind: 'number',
      label: { en: 'Colour saturation', fr: 'Saturation des couleurs' },
      min: 0,
      max: 1,
      step: 0.05,
      default: 0.85,
    },
  },
  render({ layout, time, audio, frame, params }) {
    if (layout.keys.length === 0) return
    const background = params.background ?? BACKGROUND
    const saturation = Number(params.saturation ?? 0.85)

    // A time before the last burst is a restart: the preview starts from zero.
    if (bursts.some((burst) => burst.start > time)) bursts = []
    if (audio.beat) {
      const count = 1 + Math.floor(Math.min(1, audio.level) * 3)
      for (let i = 0; i < count; i++) {
        fired += 1
        const key = layout.keys[Math.floor(hash(fired) * layout.keys.length)]
        const at = center(key)
        const color = hsv(hash(fired + 0.5) * 360, saturation, 1)
        bursts.push({ start: time, x: at.x, y: at.y, color })
      }
    }
    bursts = bursts.filter((burst) => time - burst.start < LIFE).slice(-MOST)

    for (const key of layout.keys) {
      const c = center(key)
      let color = background
      let strongest = 0
      for (const burst of bursts) {
        const age = (time - burst.start) / LIFE
        const radius = 0.6 + REACH * Math.sqrt(age)
        const near = 1 - Math.hypot(c.x - burst.x, c.y - burst.y) / radius
        const strength = near * (1 - age)
        if (strength > strongest) {
          strongest = strength
          color = mix(background, burst.color, strength)
        }
      }
      frame.set(key, color)
    }
  },
})
