// Respiration — une couleur unique dont l'intensité enfle et retombe.
//
// Aucune variation dans l'espace : tout le clavier respire ensemble. C'est le
// contre-exemple de l'onde, et la démonstration qu'un paramètre de couleur
// suffit à faire un effet.

import { defineEffect, rgb } from '@candeo/effects-api'

const DEFAUT = { r: 255, g: 96, b: 0 }

export default defineEffect({
  uid: '62fdbb90-6033-461a-83c0-3b186e8d1f51',
  name: 'Respiration',
  description: "Tout le clavier respire, d'une seule couleur",
  params: {
    color: { kind: 'color', label: 'Couleur', default: DEFAUT },
    period: { kind: 'number', label: 'Période (s)', min: 1, max: 20, step: 0.5, default: 5 },
  },
  render({ layout, time, frame, params }) {
    const color = params.color ?? DEFAUT
    const period = Number(params.period ?? 5)

    // Sinus, et non cosinus inversé : l'effet démarre donc à mi-intensité au
    // lieu du noir. Un effet qui commence par une image noire ressemble à un
    // effet qui n'a pas démarré.
    const k = 0.5 + 0.5 * Math.sin((2 * Math.PI * time) / period)

    for (const key of layout.keys) {
      frame.set(key, rgb(color.r * k, color.g * k, color.b * k))
    }
  },
})
