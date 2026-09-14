// Noise map — two colors drifting over the keyboard like slow clouds.
//
// Value noise in three dimensions: two for the keyboard, one for time. Moving
// through time rather than sliding a fixed picture is what makes the patches
// grow, merge and fade instead of scrolling past.
//
// Nothing is kept between frames: the color of a key at an instant depends on
// that instant only, so a skipped frame or a preview started later shows
// exactly the same thing.

import { center, defineEffect, mix } from '@candeo/effects-api'

const DEFAULT_FROM = { r: 10, g: 20, b: 120 }
const DEFAULT_TO = { r: 200, g: 0, b: 160 }

// A value in [0, 1[ for a point of the integer lattice, the same on every run.
function lattice(x = 0, y = 0, z = 0) {
  const n = Math.sin(x * 127.1 + y * 311.7 + z * 74.7) * 43758.5453
  return n - Math.floor(n)
}

// Smoothstep: the gradient is zero at lattice points, so cells do not show.
function smooth(t = 0) {
  return t * t * (3 - 2 * t)
}

function noise(x = 0, y = 0, z = 0) {
  const x0 = Math.floor(x)
  const y0 = Math.floor(y)
  const z0 = Math.floor(z)
  const fx = smooth(x - x0)
  const fy = smooth(y - y0)
  const fz = smooth(z - z0)

  const plane = (zi = 0) => {
    const top = lattice(x0, y0, zi) + (lattice(x0 + 1, y0, zi) - lattice(x0, y0, zi)) * fx
    const bottom =
      lattice(x0, y0 + 1, zi) + (lattice(x0 + 1, y0 + 1, zi) - lattice(x0, y0 + 1, zi)) * fx
    return top + (bottom - top) * fy
  }
  const near = plane(z0)
  return near + (plane(z0 + 1) - near) * fz
}

export default defineEffect({
  description: {
    en: 'Two colors drifting over the keyboard like slow clouds',
    fr: 'Deux couleurs qui dérivent sur le clavier comme des nuages lents',
  },
  kinds: ['keyboard'],
  params: {
    from: { kind: 'color', label: { en: 'First color', fr: 'Première couleur' }, default: DEFAULT_FROM },
    to: { kind: 'color', label: { en: 'Second color', fr: 'Seconde couleur' }, default: DEFAULT_TO },
    speed: { kind: 'number', label: { en: 'Speed', fr: 'Vitesse' }, min: 0, max: 3, step: 0.1, default: 0.6 },
    scale: { kind: 'number', label: { en: 'Patch size (keys)', fr: 'Taille des nappes (touches)' }, min: 1, max: 12, step: 0.5, default: 4 },
  },
  render({ layout, time, frame, params }) {
    const from = params.from ?? DEFAULT_FROM
    const to = params.to ?? DEFAULT_TO
    const speed = Number(params.speed ?? 0.6)
    const scale = Math.max(0.5, Number(params.scale ?? 4))
    const z = time * speed

    for (const key of layout.keys) {
      const c = center(key)
      // Two octaves: the second adds detail without breaking the large shapes.
      const n = 0.7 * noise(c.x / scale, c.y / scale, z) + 0.3 * noise((c.x * 2) / scale, (c.y * 2) / scale, z * 1.7)
      // Summed noise rarely leaves 0.3 to 0.7: stretched, so both colors show
      // in full instead of a keyboard of their average.
      const t = Math.min(1, Math.max(0, (n - 0.3) / 0.4))
      frame.set(key, mix(from, to, smooth(t)))
    }
  },
})
