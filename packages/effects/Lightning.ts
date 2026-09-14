// Lightning — a dark sky, and now and then a flash that strikes somewhere along
// the keyboard and dies out, sometimes with a second flicker.
//
// Time is cut into slots; each slot holds one flash, at a moment and a place
// drawn from its number. The sky under it is never black: between two flashes
// the keyboard must still look like a storm, not like an effect that stopped.

import { bounds, center, defineEffect, mix } from '@candeo/effects-api'

const DEFAULT_SKY = { r: 4, g: 6, b: 24 }
const DEFAULT_FLASH = { r: 210, g: 225, b: 255 }

// A value in [0, 1[ for an integer, the same on every run.
function hash(n = 0) {
  const x = Math.sin(n * 12.9898 + 78.233) * 43758.5453
  return x - Math.floor(x)
}

export default defineEffect({
  description: {
    en: 'A dark sky, and flashes that strike along the keyboard',
    fr: 'Un ciel sombre, et des éclairs qui frappent le long du clavier',
  },
  kinds: ['keyboard'],
  params: {
    sky: { kind: 'color', label: { en: 'Sky', fr: 'Ciel' }, default: DEFAULT_SKY },
    flash: { kind: 'color', label: { en: 'Flash', fr: 'Éclair' }, default: DEFAULT_FLASH },
    frequency: { kind: 'number', label: { en: 'Flashes per second', fr: 'Éclairs par seconde' }, min: 0.1, max: 3, step: 0.1, default: 0.5 },
  },
  render({ layout, time, frame, params }) {
    const sky = params.sky ?? DEFAULT_SKY
    const flash = params.flash ?? DEFAULT_FLASH
    const slot = 1 / Math.max(0.1, Number(params.frequency ?? 0.5))
    const drawing = bounds(layout)

    // The current slot and the previous one: a flash near the end of a slot is
    // still dying out at the start of the next.
    const current = Math.floor(time / slot)
    const strikes = [current - 1, current].map((n) => {
      const start = (n + hash(n * 3 + 1) * 0.7) * slot
      const since = time - start
      // A fast decay, and a second, weaker flicker in two slots out of three.
      let strength = since >= 0 ? Math.exp(-since * 9) : 0
      const flicker = since - 0.12
      if (hash(n * 5 + 2) < 0.66 && flicker >= 0) strength += 0.6 * Math.exp(-flicker * 11)
      return { x: drawing.x + hash(n * 7 + 3) * drawing.w, strength: Math.min(1, strength) }
    })

    for (const key of layout.keys) {
      const c = center(key)
      let k = 0
      for (const strike of strikes) {
        // Brightest where it strikes, the rest of the sky lit a little.
        const reach = Math.max(0.2, 1 - Math.abs(c.x - strike.x) / (drawing.w * 0.4))
        k = Math.max(k, strike.strength * reach)
      }
      frame.set(key, mix(sky, flash, k))
    }
  },
})
