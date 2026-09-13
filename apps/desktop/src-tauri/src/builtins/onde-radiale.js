// Radial wave — a hue wave spreads in circles from the center of the keyboard,
// at the **physical** distance of the keys.
//
// "Radial" is a geometric promise, and the surveyed geometry keeps it: the space
// bar is far from the center because it is 6.25 u wide, not because it would
// take several cells — it takes one. Matrix holes no longer count as distance,
// since they take no space.
//
// The wave that counts cells is "Onde diagonale", and it is a different
// effect: it leaves a corner in diagonals. The two follow each other in the
// gallery.

import { bounds, center, defineEffect, hsv } from '@candeo/effects-api'

export default defineEffect({
  uid: '33d117dc-57f2-48dd-a717-d160ad0f0cfc',
  name: 'Onde radiale',
  description: 'Une onde de teinte se propage en cercles, à la distance physique des touches',
  params: {
    speed: { kind: 'number', label: 'Vitesse', min: 0, max: 400, default: 120 },
    scale: { kind: 'number', label: 'Échelle', min: 1, max: 60, default: 18 },
  },
  render({ layout, time, frame, params }) {
    const speed = Number(params.speed ?? 120)
    const scale = Number(params.scale ?? 18)

    // The center of the drawing, not of the matrix: on a layout without a
    // numpad it moves with it, and the wave stays centered on the device at
    // hand. `bounds` throws if the layout was not drawn, rather than letting the
    // distance be NaN and the keyboard stay dark without a word.
    const drawing = bounds(layout)
    const cx = drawing.x + drawing.w / 2
    const cy = drawing.y + drawing.h / 2

    // `layout.keys` only holds positions that carry an LED: matrix holes stay
    // dark, which is the right default.
    for (const key of layout.keys) {
      const c = center(key)
      const d = Math.hypot(c.x - cx, c.y - cy)
      frame.set(key, hsv(time * speed + d * scale, 1, 1))
    }
  },
})
