// Rain — drops fall down the columns of the keyboard, each at its own pace,
// leaving a short trail.
//
// It counts in matrix rows and columns, like Sweep: a drop falls from one row to
// the next, and a column is where it falls. It runs on layouts nobody has drawn.
//
// Each column has its own speed and its own gap between drops, drawn once from
// its index: nothing is kept between frames, and the rain at an instant depends
// on that instant only.

import { defineEffect, rgb } from '@candeo/effects-api'

const DEFAULT_COLOR = { r: 40, g: 140, b: 255 }

// A value in [0, 1[ for an integer, the same on every run.
function hash(n = 0) {
  const x = Math.sin(n * 12.9898 + 78.233) * 43758.5453
  return x - Math.floor(x)
}

export default defineEffect({
  description: {
    en: 'Drops fall down the keyboard, each column at its own pace',
    fr: 'Des gouttes tombent sur le clavier, chaque colonne à son rythme',
  },
  kinds: ['keyboard'],
  params: {
    color: { kind: 'color', label: { en: 'Color', fr: 'Couleur' }, default: DEFAULT_COLOR },
    speed: { kind: 'number', label: { en: 'Rows per second', fr: 'Rangées par seconde' }, min: 1, max: 20, step: 0.5, default: 6 },
    trail: { kind: 'number', label: { en: 'Trail (rows)', fr: 'Traînée (rangées)' }, min: 0.5, max: 4, step: 0.5, default: 1.5 },
  },
  render({ layout, time, frame, params }) {
    const color = params.color ?? DEFAULT_COLOR
    const speed = Number(params.speed ?? 6)
    const trail = Number(params.trail ?? 1.5)

    for (const key of layout.keys) {
      // A cycle is the fall through every row, then a pause of 2 to 8 rows
      // before the next drop, so columns do not rain in step.
      const cycle = layout.rows + 2 + hash(key.col + 101) * 6
      const pace = speed * (0.6 + 0.8 * hash(key.col + 7))
      const head = (time * pace + hash(key.col + 43) * cycle) % cycle

      // Lit only behind the head: a drop has not reached the rows below it.
      const behind = head - key.row
      const k = behind >= 0 ? Math.max(0, 1 - behind / trail) : 0
      frame.set(key, rgb(color.r * k, color.g * k, color.b * k))
    }
  },
})
