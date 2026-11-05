import { defineEffect, hsv } from '@candeo/effects-api'

// Type errors only: the editor's language service underlines all three, and
// neither transpiler says a word. The effect loads and renders.
const level: number = 'bright'
const hue = hsv('red', 1, 1)

export default defineEffect({
  params: {
    speed: { kind: 'number', label: 'Speed', min: 0, max: 10, default: 'fast' },
  },
  render({ layout, frame, time }) {
    for (const key of layout.keys) frame.set(key, time > 1 ? hue : { r: level, g: 0, b: 0 })
  },
})
