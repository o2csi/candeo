import { describe, expect, it } from 'vitest'

import type { KeyInfo } from '../api/types'
import { nearestKey, painted, startKey, toggled } from './keySelection'

/** Two staggered rows: Q W E over A S D, a quarter key to the right. */
function key(index: number, x: number, y: number): KeyInfo {
  return { index, row: y, col: index, x, y, w: 1, h: 1 }
}
const keys = [key(10, 1.5, 1), key(11, 2.5, 1), key(12, 3.5, 1), key(20, 1.75, 2), key(21, 2.75, 2), key(22, 3.75, 2)]

describe('selection', () => {
  it('toggles one key and keeps the positions ascending', () => {
    expect(toggled([21, 10], 11)).toEqual([10, 11, 21])
    expect(toggled([10, 11], 10)).toEqual([11])
  })

  it('paints every key a drag crosses with the first key’s new state', () => {
    expect(painted([10], 11, true)).toEqual([10, 11])
    expect(painted([10, 11], 11, true)).toEqual([10, 11])
    expect(painted([10, 11], 10, false)).toEqual([11])
  })
})

describe('nearestKey', () => {
  it('stays on the row going sideways', () => {
    expect(nearestKey(keys, 11, 'right')).toBe(12)
    expect(nearestKey(keys, 11, 'left')).toBe(10)
  })

  it('goes to the key under, across the stagger', () => {
    expect(nearestKey(keys, 11, 'down')).toBe(21)
    expect(nearestKey(keys, 21, 'up')).toBe(11)
  })

  it('stays put at the edge', () => {
    expect(nearestKey(keys, 12, 'right')).toBeNull()
    expect(nearestKey(keys, 10, 'up')).toBeNull()
  })

  it('starts on the first key selected', () => {
    expect(startKey(keys, [21, 12])).toBe(12)
    expect(startKey(keys, [])).toBe(10)
    expect(startKey([], [])).toBeNull()
  })
})
