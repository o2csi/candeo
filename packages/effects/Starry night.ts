// Starry night — keys light up here and there and fade out again, over a dark
// sky.
//
// Each key twinkles on its own period and phase, drawn once from its index, and
// changes color at every twinkle. The sky is never black: a keyboard between two
// stars must still look like a night, not like an effect that stopped.

import { defineEffect, hsv, mix } from '@candeo/effects-api'

const DEFAULT_SKY = { r: 0, g: 6, b: 28 }

// A value in [0, 1[ for an integer, the same on every run.
function hash(n = 0) {
  const x = Math.sin(n * 12.9898 + 78.233) * 43758.5453
  return x - Math.floor(x)
}

export default defineEffect({
  description: {
    en: 'Keys twinkle here and there over a dark sky',
    fr: "Des touches scintillent çà et là sur un ciel sombre",
  },
  kinds: ['keyboard'],
  params: {
    sky: { kind: 'color', label: { en: 'Sky', fr: 'Ciel' }, default: DEFAULT_SKY },
    density: { kind: 'number', label: { en: 'Stars', fr: 'Étoiles' }, min: 0.05, max: 1, step: 0.05, default: 0.35 },
    speed: { kind: 'number', label: { en: 'Twinkles per second', fr: 'Scintillements par seconde' }, min: 0.1, max: 3, step: 0.1, default: 0.5 },
    saturation: { kind: 'number', label: { en: 'Star colors', fr: 'Couleur des étoiles' }, min: 0, max: 1, step: 0.05, default: 0.4 },
  },
  render({ layout, time, frame, params }) {
    const sky = params.sky ?? DEFAULT_SKY
    const density = Number(params.density ?? 0.35)
    const speed = Number(params.speed ?? 0.5)
    const saturation = Number(params.saturation ?? 0.4)

    for (const key of layout.keys) {
      if (hash(key.index * 3 + 1) > density) {
        frame.set(key, sky)
        continue
      }

      // Between 0.5 and 1.5 times the chosen rate, so stars never pulse together.
      const phase = time * speed * (0.5 + hash(key.index + 17)) + hash(key.index + 29)
      const twinkle = Math.floor(phase)
      // A sharp peak rather than a sine: a star flashes, it does not breathe.
      const glow = Math.pow(Math.max(0, Math.sin((phase - twinkle) * Math.PI)), 6)
      const star = hsv(hash(key.index * 7 + twinkle) * 360, saturation, 1)
      frame.set(key, mix(sky, star, glow))
    }
  },
})
