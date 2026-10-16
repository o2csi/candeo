import { describe, expect, it } from 'vitest'

import type { KeyInfo } from '../api/types'
import { pictogram } from './pictogram'

function light(index: number, x: number, y: number, w: number, h: number, part: Partial<KeyInfo> = {}): KeyInfo {
  return { index, row: 0, col: index, x, y, w, h, ...part }
}

describe('pictogram', () => {
  // A lid 16 u wide: a logo, and an edge a fifth of a unit thick.
  const keys = [
    light(0, 7, 3, 2, 2, { shape: 'disc' }),
    light(1, 0.5, 1, 0.2, 6),
    light(2, 2, 10, 4, 4),
  ]
  const outline = [{ x: 0, y: 0, w: 16, h: 8, r: 0.5 }]

  it('fills the region and leaves the other lights faint', () => {
    const p = pictogram(keys, outline, [0, 2])
    expect(p.lights.map((l) => [l.index, l.lit])).toEqual([
      [0, true],
      [1, false],
      [2, true],
    ])
    expect(p.lights[0].disc).toBe(true)
    expect(p.outline).toBe(outline)
  })

  it('thickens a thin light around its middle, and leaves the others as they are', () => {
    const [, edge, block] = pictogram(keys, outline, [1]).lights
    // 6 % of the longest side, 16 u.
    expect(edge.w).toBeCloseTo(0.96)
    expect(edge.x + edge.w / 2).toBeCloseTo(0.6)
    expect(edge.h).toBe(6)
    expect([block.x, block.y, block.w, block.h]).toEqual([2, 10, 4, 4])
  })

  it('keeps an arch its shape, stroked thick enough to be seen', () => {
    const ring = pictogram([light(0, 2, 8, 9, 1.1, { shape: 'archUp' })], [], [0]).lights[0]
    expect(ring.path).toMatch(/^M /)
    expect(ring.stroke).toBeGreaterThan(0)
  })

  it('leaves room for a light thickened past the edge', () => {
    expect(pictogram(keys, outline, []).viewBox).toBe('-0.48 -0.48 16.96 14.96')
  })
})
