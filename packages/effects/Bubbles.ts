// Bubbles — rings grow from random points of the keyboard and fade as they
// widen.
//
// Distances are physical, so a ring stays round on a keyboard four times wider
// than it is tall. Each bubble lives on its own cycle; its center and color are
// drawn from the cycle's number, so a new bubble appears elsewhere every time
// and nothing has to be kept between frames.

import { bounds, center, defineEffect, hsv, rgb } from '@candeo/effects-api'

// A value in [0, 1[ for an integer, the same on every run.
function hash(n = 0) {
  const x = Math.sin(n * 12.9898 + 78.233) * 43758.5453
  return x - Math.floor(x)
}

export default defineEffect({
  description: {
    en: 'Rings grow from random points and fade as they widen',
    fr: "Des anneaux naissent au hasard et s'effacent en grandissant",
  },
  kinds: ['keyboard'],
  params: {
    count: { kind: 'number', label: { en: 'Bubbles', fr: 'Bulles' }, min: 1, max: 8, step: 1, default: 4 },
    speed: { kind: 'number', label: { en: 'Speed', fr: 'Vitesse' }, min: 0.1, max: 2, step: 0.1, default: 0.5 },
    size: { kind: 'number', label: { en: 'Size (keys)', fr: 'Taille (touches)' }, min: 2, max: 16, step: 0.5, default: 7 },
  },
  render({ layout, time, frame, params }) {
    const count = Math.max(1, Math.round(Number(params.count ?? 4)))
    const speed = Number(params.speed ?? 0.5)
    const size = Number(params.size ?? 7)
    const drawing = bounds(layout)

    const bubbles = Array.from({ length: count }, (_, i) => {
      const phase = time * speed + hash(i * 13 + 5)
      const cycle = Math.floor(phase)
      const age = phase - cycle
      return {
        x: drawing.x + hash(i * 31 + cycle * 7) * drawing.w,
        y: drawing.y + hash(i * 17 + cycle * 11) * drawing.h,
        radius: age * size,
        // Bright while small, gone when full size.
        strength: 1 - age,
        color: hsv(hash(i * 7 + cycle * 3) * 360, 1, 1),
      }
    })

    for (const key of layout.keys) {
      const c = center(key)
      let r = 0
      let g = 0
      let b = 0
      for (const bubble of bubbles) {
        const distance = Math.hypot(c.x - bubble.x, c.y - bubble.y)
        // A ring about a key and a half thick: lit on the circle, dark inside.
        const k = Math.max(0, 1 - Math.abs(distance - bubble.radius) / 1.5) * bubble.strength
        r += bubble.color.r * k
        g += bubble.color.g * k
        b += bubble.color.b * k
      }
      frame.set(key, rgb(r, g, b))
    }
  },
})
