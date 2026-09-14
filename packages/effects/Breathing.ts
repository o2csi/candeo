// Breathing — a single color whose intensity swells and falls.
//
// No variation in space: the whole keyboard breathes together. It is the
// counterexample to the waves, and the demonstration that one color parameter is
// enough to make an effect.

import { defineEffect, rgb } from '@candeo/effects-api'

const DEFAULT_COLOR = { r: 255, g: 96, b: 0 }

export default defineEffect({
  description: {
    en: 'The whole keyboard breathes, in a single color',
    fr: "Tout le clavier respire, d'une seule couleur",
  },
  kinds: ['keyboard'],
  params: {
    color: { kind: 'color', label: { en: 'Color', fr: 'Couleur' }, default: DEFAULT_COLOR },
    period: { kind: 'number', label: { en: 'Period (s)', fr: 'Période (s)' }, min: 1, max: 20, step: 0.5, default: 5 },
  },
  render({ layout, time, frame, params }) {
    const color = params.color ?? DEFAULT_COLOR
    const period = Number(params.period ?? 5)

    // A sine, not an inverted cosine: the effect starts at half intensity rather
    // than black. An effect whose first frame is black looks like an effect that
    // did not start.
    const k = 0.5 + 0.5 * Math.sin((2 * Math.PI * time) / period)

    for (const key of layout.keys) {
      frame.set(key, rgb(color.r * k, color.g * k, color.b * k))
    }
  },
})
