// Swirl circles — two glowing circles orbit the center of the keyboard, on
// opposite sides, changing hue as they turn.
//
// The orbit is an ellipse that follows the keyboard's proportions: a circle
// would leave both ends dark on a keyboard four times wider than it is tall.

import { bounds, center, defineEffect, hsv, rgb } from '@candeo/effects-api'

export default defineEffect({
  description: {
    en: 'Two glowing circles orbit the center of the keyboard',
    fr: 'Deux cercles lumineux tournent autour du centre du clavier',
  },
  kinds: ['keyboard'],
  params: {
    speed: { kind: 'number', label: { en: 'Turns per second', fr: 'Tours par seconde' }, min: 0, max: 2, step: 0.05, default: 0.25 },
    radius: { kind: 'number', label: { en: 'Glow (keys)', fr: 'Halo (touches)' }, min: 1, max: 8, step: 0.5, default: 4.5 },
  },
  render({ layout, time, frame, params }) {
    const speed = Number(params.speed ?? 0.25)
    const radius = Number(params.radius ?? 4.5)
    const drawing = bounds(layout)
    const cx = drawing.x + drawing.w / 2
    const cy = drawing.y + drawing.h / 2
    const angle = time * speed * 2 * Math.PI

    const circles = [0, Math.PI].map((offset, i) => ({
      x: cx + Math.cos(angle + offset) * drawing.w * 0.32,
      y: cy + Math.sin(angle + offset) * drawing.h * 0.32,
      // Opposite hues, drifting together.
      color: hsv(time * 40 + i * 180, 1, 1),
    }))

    for (const key of layout.keys) {
      const c = center(key)
      let r = 0
      let g = 0
      let b = 0
      for (const circle of circles) {
        const d = Math.hypot(c.x - circle.x, c.y - circle.y)
        // A falloff steeper than linear: a bright core and a soft edge.
        const k = Math.pow(Math.max(0, 1 - d / radius), 1.5)
        r += circle.color.r * k
        g += circle.color.g * k
        b += circle.color.b * k
      }
      frame.set(key, rgb(r, g, b))
    }
  },
})
