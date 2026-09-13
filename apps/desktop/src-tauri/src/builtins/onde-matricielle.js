// Diagonal wave — a hue wave leaves the top-left corner and crosses the keyboard
// in diagonals.
//
// The wave from before the geometry, and an effect in its own right: its
// distance counts **steps through the matrix** (column + row), not millimetres,
// and matrix holes count as distance.
//
// It starts from a corner, not from the center, on purpose. On a keyboard 22
// cells wide and 6 high, any wave measured from the center turns into
// near-vertical bands, whatever its shape: it looked like "Onde radiale". A
// corner changes the motion itself, and the two can be told apart at a glance.
//
// It reads neither `x` nor `y`: of the two waves, it is the only one that runs
// on a layout nobody has drawn.

import { defineEffect, hsv } from '@candeo/effects-api'

export default defineEffect({
  name: 'Onde diagonale',
  description: 'Une onde de teinte part du coin supérieur gauche et traverse le clavier en diagonale',
  params: {
    speed: { kind: 'number', label: 'Vitesse', min: 0, max: 400, default: 120 },
    scale: { kind: 'number', label: 'Échelle', min: 1, max: 60, default: 18 },
  },
  render({ layout, time, frame, params }) {
    const speed = Number(params.speed ?? 120)
    const scale = Number(params.scale ?? 18)

    // `layout.keys` only holds positions that carry an LED: matrix holes stay
    // dark, which is the right default.
    for (const key of layout.keys) {
      const steps = key.col + key.row
      // Minus: a given hue sits further from the corner as time passes, so the
      // bands move away from it.
      frame.set(key, hsv(time * speed - steps * scale, 1, 1))
    }
  },
})
