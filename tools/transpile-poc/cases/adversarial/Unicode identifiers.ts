import { defineEffect, hsv } from '@candeo/effects-api'

// Identifiers and strings beyond ASCII, as a French-speaking author writes.
const teinteDépart: number = 200
const étoile = '★'

export default defineEffect({
  description: { en: `Stars ${étoile}`, fr: `Étoiles ${étoile}` },
  render({ layout, frame, time }) {
    for (const [i, touche] of layout.keys.entries()) {
      frame.set(touche, hsv(teinteDépart + i * 3 + time * 20, 1, 1))
    }
  },
})
