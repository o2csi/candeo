// Sweep — a lit row moves down the keyboard, leaving a trail.
//
// The only shipped effect where most of the keyboard is dark at any instant: that
// is what makes it recognizable at a glance.
//
// It counts in **matrix rows**, and that is not the flaw "Radial wave" had: it is
// its subject. Its two settings read "rows per second" and "trail in rows", its
// head moves one row at a time, and a row is a matrix notion. Moving it to
// physical units would make another effect — one that would linger on the 1.5 u
// between the function row and the rest — for nothing this one does wrong, and
// would cost it running on layouts that are not drawn.

import { defineEffect, rgb } from '@candeo/effects-api'

const DEFAULT_COLOR = { r: 0, g: 180, b: 255 }

export default defineEffect({
  description: 'Une rangée éclairée descend le clavier en laissant une traînée',
  kinds: ['keyboard'],
  params: {
    color: { kind: 'color', label: 'Couleur', default: DEFAULT_COLOR },
    speed: { kind: 'number', label: 'Rangées par seconde', min: 0.5, max: 12, step: 0.5, default: 3 },
    trail: { kind: 'number', label: 'Traînée (rangées)', min: 0.5, max: 6, step: 0.5, default: 2 },
    bounce: { kind: 'boolean', label: 'Rebond', default: false },
  },
  render({ layout, time, frame, params }) {
    const color = params.color ?? DEFAULT_COLOR
    const speed = Number(params.speed ?? 3)
    const trail = Number(params.trail ?? 2)
    const last = layout.rows - 1

    // Position of the head. When bouncing, the cycle covers the way down *and*
    // back, hence its doubled length and the folding of its second half.
    const cycle = params.bounce ? 2 * last : layout.rows
    const p = (time * speed) % cycle
    const head = params.bounce && p > last ? cycle - p : p

    // Intensity falls with the distance to the head, and is zero beyond the
    // trail: a far key must be off, not faintly lit.
    for (const key of layout.keys) {
      const k = Math.max(0, 1 - Math.abs(key.row - head) / trail)
      frame.set(key, rgb(color.r * k, color.g * k, color.b * k))
    }
  },
})
