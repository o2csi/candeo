// Declarations TypeScript has and JavaScript does not: namespaces, string
// enums, ambient declarations, type-level code that leaves nothing behind.
//
// Probes: a namespace with a nested one, a string enum, `declare`, function
// overloads, `keyof typeof`, mapped and conditional types, an assertion
// function, `export type` and a type-only re-export, `import type` of a
// namespace.

import { defineEffect, hsv, lerp } from '@candeo/effects-api'
import type * as Api from '@candeo/effects-api'

export type { Rgb } from '@candeo/effects-api'

declare const __candeo_layout: string

enum Mode {
  Wave = 'wave',
  Pulse = 'pulse',
}

namespace Palette {
  export const BASE = 200
  export function at(i: number): number {
    return (BASE + i * Inner.STEP) % 360
  }
  export namespace Inner {
    export const STEP = 23
  }
}

const SPEEDS = { slow: 0.5, normal: 1, fast: 2 } as const
type Speed = keyof typeof SPEEDS
type Levels = { readonly [K in Speed]: number }
type Of<T> = T extends readonly (infer U)[] ? U : never
type KeyOf = Of<Api.Layout['keys']>

function assertKnown(mode: string): asserts mode is Mode {
  if (mode !== Mode.Wave && mode !== Mode.Pulse) throw new Error(`unknown mode ${mode}`)
}

function level(t: number): number
function level(t: number, speed: Speed): number
function level(t: number, speed: Speed = 'normal'): number {
  const levels: Levels = SPEEDS
  return (Math.sin(t * levels[speed] * Math.PI) + 1) / 2
}

export default defineEffect({
  description: 'Hues from a namespace, by mode',
  params: {
    mode: { kind: 'choice', label: 'Mode', options: ['wave', 'pulse'], default: 'wave' },
  },
  render({ layout, time, frame, params }) {
    const mode: string = params.mode
    assertKnown(mode)
    layout.keys.forEach((key: KeyOf, i: number) => {
      const v = mode === Mode.Pulse ? level(time, 'fast') : lerp(0.2, 1, level(time + i / 10))
      frame.set(key, hsv(Palette.at(i), 1, v))
    })
  },
})
