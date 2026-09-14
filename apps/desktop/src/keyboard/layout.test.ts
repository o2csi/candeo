import { describe, expect, it } from 'vitest'

import type { KeyInfo, Rgb } from '../api/types'
import { extent, layoutProblems, type LayoutView } from './layout'

function key(index: number, x: number, part: Partial<KeyInfo> = {}): KeyInfo {
  return { index, row: 0, col: index, name: `K${index}`, x, y: 0, w: 1, h: 1, ...part }
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
      'image de 2 couleurs pour un gabarit qui en attend 3',
    ])
  })

  it('reports every problem at once', () => {
    const problems = layoutProblems(
      view([key(0, 0), key(0, 1), key(7, 2), key(1, 3, { w: 0 }), key(2, 3.5, { x: -1 })]),
      black(3),
    )

    expect(problems).toEqual([
      'index 0 partagé par « K0 » et « K0 »',
      '« K7 » porte l\'index 7, hors de l\'image',
      '« K1 » (index 1) est sans surface',
      '« K2 » (index 2) sort du dessin par le haut ou la gauche',
    ])
  })

  it('reports keycaps that overlap, not keycaps that touch', () => {
    const touching = view([key(0, 0), key(1, 1)])
    const overlapping = view([key(0, 0), key(1, 0.5)])

    expect(layoutProblems(touching, black(3))).toEqual([])
    expect(layoutProblems(overlapping, black(3))).toEqual(['« K0 » (0) et « K1 » (1) se chevauchent'])
  })
})
