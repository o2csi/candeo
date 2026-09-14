// Color wheel — every hue at once, arranged by angle around the center of the
// keyboard, turning.
//
// The angle is measured on the physical drawing, not on the matrix: on a
// keyboard four times wider than it is tall, a matrix angle would squeeze the
// whole wheel into a few columns at each end.

import { bounds, center, defineEffect, hsv } from '@candeo/effects-api'

export default defineEffect({
  description: {
    en: 'Every hue around the center of the keyboard, turning',
    fr: 'Toutes les teintes autour du centre du clavier, qui tournent',
  },
  kinds: ['keyboard'],
  params: {
    speed: { kind: 'number', label: { en: 'Speed (°/s)', fr: 'Vitesse (°/s)' }, min: 0, max: 360, default: 90 },
    reverse: { kind: 'boolean', label: { en: 'Reverse', fr: 'Sens inverse' }, default: false },
  },
  render({ layout, time, frame, params }) {
    const speed = Number(params.speed ?? 90) * (params.reverse ? -1 : 1)
    const drawing = bounds(layout)
    const cx = drawing.x + drawing.w / 2
    const cy = drawing.y + drawing.h / 2

    for (const key of layout.keys) {
      const c = center(key)
      const angle = (Math.atan2(c.y - cy, c.x - cx) * 180) / Math.PI
      frame.set(key, hsv(angle + time * speed, 1, 1))
    }
  },
})
