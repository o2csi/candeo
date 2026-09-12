// Dégradé fixe — deux couleurs, et rien qui bouge.
//
// Son `render` n'utilise pas `time` : un effet n'est pas tenu d'être une
// animation. C'est aussi celui qu'on garde allumé en travaillant.

import { defineEffect, mix } from '@candeo/effects-api'

const DEPART = { r: 255, g: 0, b: 128 }
const ARRIVEE = { r: 0, g: 128, b: 255 }

export default defineEffect({
  name: 'Dégradé fixe',
  description: 'Un dégradé entre deux couleurs, immobile',
  params: {
    from: { kind: 'color', label: 'Couleur de départ', default: DEPART },
    to: { kind: 'color', label: "Couleur d'arrivée", default: ARRIVEE },
    axis: {
      kind: 'choice',
      label: 'Sens',
      options: ['horizontal', 'vertical'],
      default: 'horizontal',
    },
  },
  render({ layout, frame, params }) {
    const from = params.from ?? DEPART
    const to = params.to ?? ARRIVEE
    const vertical = params.axis === 'vertical'
    const span = vertical ? layout.rows - 1 : layout.cols - 1

    for (const key of layout.keys) {
      frame.set(key, mix(from, to, (vertical ? key.row : key.col) / span))
    }
  },
})
