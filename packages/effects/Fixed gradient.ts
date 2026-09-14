// Fixed gradient — two colors, and nothing moves.
//
// Its `render` does not use `time`: an effect does not have to be an animation.
// It is also the one kept on while working.
//
// It interpolates over the **matrix column**, and was reviewed when "Radial wave"
// moved to physical distances without being changed. What a gradient promises is
// an order — "this color on one side, that one on the other" — not a distance,
// and the order of the columns is the order of the keys. What shifts by a few
// tenths is the bottom row, where three wide keys share four columns; it shows
// when looked for, and making it exact would demand the geometry for an effect
// that does not need it.

import { defineEffect, mix } from '@candeo/effects-api'

const DEFAULT_FROM = { r: 255, g: 0, b: 128 }
const DEFAULT_TO = { r: 0, g: 128, b: 255 }

export default defineEffect({
  description: 'Un dégradé entre deux couleurs, immobile',
  kinds: ['keyboard'],
  params: {
    from: { kind: 'color', label: 'Couleur de départ', default: DEFAULT_FROM },
    to: { kind: 'color', label: "Couleur d'arrivée", default: DEFAULT_TO },
    axis: {
      kind: 'choice',
      label: 'Sens',
      options: ['horizontal', 'vertical'],
      default: 'horizontal',
    },
  },
  render({ layout, frame, params }) {
    const from = params.from ?? DEFAULT_FROM
    const to = params.to ?? DEFAULT_TO
    const vertical = params.axis === 'vertical'
    const span = vertical ? layout.rows - 1 : layout.cols - 1

    for (const key of layout.keys) {
      frame.set(key, mix(from, to, (vertical ? key.row : key.col) / span))
    }
  },
})
