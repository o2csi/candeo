/**
 * Reference effect — a wave spreading from the center of the matrix.
 *
 * It is the template a new effect starts from: an effect fits in a few lines and
 * reads as what it does. It also shows the contract the engine expects — **a
 * default export**, and nothing else.
 *
 * It measures in **matrix coordinates**: one cell per key, whatever its size.
 * That works everywhere, including on a layout whose arrangement nobody has
 * drawn. For a round wave on the desk rather than in the matrix, the shipped
 * effect "Radial wave" reads the keys' rectangles — see `Key.x` for what that
 * implies.
 */

import { defineEffect, hsv } from './index'

export default defineEffect({
  description: {
    en: 'A hue wave spreads from the center of the matrix',
    fr: 'Une onde de teinte se propage depuis le centre de la matrice',
  },
  params: {
    speed: { kind: 'number', label: { en: 'Speed', fr: 'Vitesse' }, min: 0, max: 400, default: 120 },
    scale: { kind: 'number', label: { en: 'Scale', fr: 'Échelle' }, min: 1, max: 60, default: 18 },
  },
  render({ layout, time, frame, params }) {
    const cx = (layout.cols - 1) / 2
    const cy = (layout.rows - 1) / 2
    const speed = Number(params.speed ?? 120)
    const scale = Number(params.scale ?? 18)

    // `layout.keys` holds only the positions that carry an LED. The matrix holes
    // stay black, which is the right default: a frame covers 132 positions, the
    // keyboard lights 106.
    for (const key of layout.keys) {
      const d = Math.hypot(key.col - cx, key.row - cy)
      frame.set(key, hsv(time * speed + d * scale, 1, 1))
    }
  },
})
