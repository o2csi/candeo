import { describe, expect, it } from 'vitest'

import type { KeyInfo, Rgb } from '../api/types'
import { extent, layoutProblems, type LayoutView } from './layout'

function key(index: number, x: number, part: Partial<KeyInfo> = {}): KeyInfo {
  return { index, row: 0, col: index, x, y: 0, w: 1, h: 1, ...part }
}

const black = (n: number): Rgb[] => Array.from({ length: n }, () => [0, 0, 0])

function view(keys: KeyInfo[], frameLen = 3): LayoutView {
  return { name: 'test', rows: 1, cols: 3, frameLen, keys }
}

describe('extent', () => {
  it('spans the furthest key edges', () => {
    expect(extent([key(0, 0), key(1, 1.5, { w: 2.25, y: 1, h: 2 })])).toEqual({ w: 3.75, h: 3 })
  })

  it('is a unit box for an empty layout, never a zero area', () => {
    expect(extent([])).toEqual({ w: 1, h: 1 })
  })
})

describe('layoutProblems', () => {
  it('finds nothing in a consistent layout, matrix holes included', () => {
    expect(layoutProblems(view([key(0, 0), key(2, 1)]), black(3))).toEqual([])
  })

  it('reports a frame that does not cover the whole matrix', () => {
    expect(layoutProblems(view([key(0, 0)]), black(2))).toEqual([
      'frame of 2 colors for a layout expecting 3',
    ])
  })

  it('reports every problem at once', () => {
    const problems = layoutProblems(
      view([key(0, 0), key(0, 1), key(7, 2), key(1, 3, { w: 0 }), key(2, 3.5, { x: -1 })]),
      black(3),
    )

    expect(problems).toEqual([
      'index 0 is used by two keys',
      'key 7 is outside the frame',
      'key 1 has no area',
      'key 2 leaves the drawing at the top or left',
    ])
  })

  it('reports keycaps that overlap, not keycaps that touch', () => {
    const touching = view([key(0, 0), key(1, 1)])
    const overlapping = view([key(0, 0), key(1, 0.5)])

    expect(layoutProblems(touching, black(3))).toEqual([])
    expect(layoutProblems(overlapping, black(3))).toEqual(['keys 0 and 1 overlap'])
  })
})
