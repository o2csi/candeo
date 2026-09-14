// Crossing beams — a vertical beam sweeps the keyboard from side to side while a
// horizontal one sweeps it from top to bottom; where they cross, their colors
// add up.
//
// Both go back and forth rather than wrapping around: a beam that jumped from
// one edge to the other would read as a glitch. They move at different paces,
// so the crossing point wanders instead of tracing the same diagonal.

import { bounds, center, defineEffect, rgb } from '@candeo/effects-api'

const DEFAULT_FIRST = { r: 255, g: 40, b: 0 }
const DEFAULT_SECOND = { r: 0, g: 90, b: 255 }

// 0 → 1 → 0 once per unit of `t`.
function backAndForth(t = 0) {
  const f = t - Math.floor(t)
  return f < 0.5 ? f * 2 : 2 - f * 2
}

export default defineEffect({
  description: {
    en: 'Two beams sweep the keyboard across each other',
    fr: 'Deux faisceaux balaient le clavier en se croisant',
  },
  kinds: ['keyboard'],
  params: {
    first: { kind: 'color', label: { en: 'Vertical beam', fr: 'Faisceau vertical' }, default: DEFAULT_FIRST },
    second: { kind: 'color', label: { en: 'Horizontal beam', fr: 'Faisceau horizontal' }, default: DEFAULT_SECOND },
    speed: { kind: 'number', label: { en: 'Sweeps per second', fr: 'Balayages par seconde' }, min: 0.05, max: 2, step: 0.05, default: 0.3 },
    width: { kind: 'number', label: { en: 'Width (keys)', fr: 'Largeur (touches)' }, min: 0.5, max: 5, step: 0.5, default: 1.5 },
  },
  render({ layout, time, frame, params }) {
    const first = params.first ?? DEFAULT_FIRST
    const second = params.second ?? DEFAULT_SECOND
    const speed = Number(params.speed ?? 0.3)
    const width = Number(params.width ?? 1.5)
    const drawing = bounds(layout)

    const beamX = drawing.x + backAndForth(time * speed) * drawing.w
    // Slower along the short side, so both take a comparable time per key.
    const beamY = drawing.y + backAndForth(time * speed * 0.7 + 0.25) * drawing.h

    for (const key of layout.keys) {
      const c = center(key)
      const a = Math.max(0, 1 - Math.abs(c.x - beamX) / width)
      const b = Math.max(0, 1 - Math.abs(c.y - beamY) / width)
      frame.set(
        key,
        rgb(first.r * a + second.r * b, first.g * a + second.g * b, first.b * a + second.b * b),
      )
    }
  },
})
