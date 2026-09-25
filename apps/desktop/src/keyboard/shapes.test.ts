import { describe, expect, it } from 'vitest'

import { archPath, archStroke } from './shapes'

describe('archPath', () => {
  const upper = { x: 2, y: 7.9, w: 9, h: 1.1 }
  const lower = { x: 2, y: 9, w: 9, h: 1.1 }

  it('opens the upper half at the bottom and the lower half at the top', () => {
    expect(archPath(upper, true)).toMatch(/^M 2\.175 9 /)
    expect(archPath(upper, true)).toMatch(/ L 10\.825 9$/)
    expect(archPath(lower, false)).toMatch(/^M 2\.175 9 /)
    expect(archPath(lower, false)).toMatch(/ L 10\.825 9$/)
  })

  it('keeps the stroke inside its rectangle', () => {
    // Half the 0.35 stroke in from each edge: 7.9 + 0.175 at the top of the
    // upper half, 10.1 - 0.175 at the bottom of the lower one.
    expect(archPath(upper, true)).toContain(' 8.075 ')
    expect(archPath(lower, false)).toContain(' 9.925 ')
  })

  it('thins its stroke in a flat rectangle', () => {
    expect(archStroke({ h: 1.1 })).toBe(0.35)
    expect(archStroke({ h: 0.4 })).toBeCloseTo(0.24)
  })
})
