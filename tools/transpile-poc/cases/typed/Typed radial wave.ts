// Radial wave as a TypeScript author writes it, every type spelled out: what an
// agent produces, unlike the shipped effects, which carry no annotations so that
// the Rust tests run them as JavaScript (`shipped.rs`).
//
// Probes: type-only imports, a value import used only as a type, interfaces,
// type aliases, generics, `as const`, `satisfies`, a non-null assertion, an
// angle-bracket assertion, optional parameters, a definite assignment.

import { bounds, center, defineEffect, hsv, Key } from '@candeo/effects-api'
import type { Layout, Rect, Rgb } from '@candeo/effects-api'

interface Point {
  readonly x: number
  readonly y: number
}

type Distance = (from: Point, key: Key) => number

const euclid: Distance = (from, key) => {
  const c = center(key)
  return Math.hypot(c.x - from.x, c.y - from.y)
}

function middle<L extends Layout>(layout: L, rect?: Rect): Point {
  const drawing: Rect = rect ?? bounds(layout)
  return { x: drawing.x + drawing.w / 2, y: drawing.y + drawing.h / 2 }
}

const SATURATION = [1, 0.85] as const satisfies readonly number[]
const WHITE = { r: 255, g: 255, b: 255 } satisfies Rgb

let origin!: Point

export default defineEffect({
  description: 'A typed copy of Radial wave',
  kinds: 'all',
  requires: ['geometry'],
  params: {
    speed: { kind: 'number', label: 'Speed', min: 0, max: 400, default: 120 },
    scale: { kind: 'number', label: 'Scale', min: 1, max: 60, default: 18 },
    sparkle: { kind: 'boolean', label: 'Sparkle', default: true },
  },
  render({ layout, time, frame, params }) {
    origin = middle(layout)
    const speed = <number>params.speed
    const scale = params.scale as number
    const keys: readonly Key[] = layout.keys
    for (const [i, key] of keys.entries()) {
      const d = euclid(origin!, key)
      const s: number = SATURATION[i % SATURATION.length]!
      const sparkle = params.sparkle && (i * 7 + Math.floor(time * 10)) % 53 === 0
      frame.set(key, sparkle ? WHITE : hsv(time * speed + d * scale, s, 1))
    }
  },
})
