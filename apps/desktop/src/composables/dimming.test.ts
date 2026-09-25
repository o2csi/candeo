import { describe, expect, it } from 'vitest'

import {
  dimmingFloor,
  dimmingMode,
  dimmingSignal,
  dimmingSound,
  followingSignal,
  followingSound,
  withFloor,
} from './dimming'

describe('dimming', () => {
  it('reads nothing as the slider alone', () => {
    expect(dimmingMode(null)).toBe('value')
    expect(dimmingFloor(null)).toBe(20)
  })

  it('tells the sound from a signal', () => {
    expect(dimmingMode({ source: 'sound:bass', floor: 10 })).toBe('sound')
    expect(dimmingSound({ source: 'sound:bass', floor: 10 })).toBe('bass')
    expect(dimmingMode({ source: 'signal:lux', floor: 10 })).toBe('signal')
    expect(dimmingSignal({ source: 'signal:lux', floor: 10 })).toBe('lux')
  })

  it('reads a source Rust would refuse as none', () => {
    expect(dimmingMode({ source: 'sound:pitch', floor: 20 })).toBe('value')
  })

  it('keeps the floor when the source changes', () => {
    const before = { source: 'sound:bass', floor: 35 }
    expect(followingSound(before, 'beat')).toEqual({ source: 'sound:beat', floor: 35 })
    expect(followingSignal(before, ' lux ')).toEqual({ source: 'signal:lux', floor: 35 })
    expect(followingSound(null, 'volume')).toEqual({ source: 'sound:volume', floor: 20 })
  })

  it('waits for a name a sender could use', () => {
    expect(followingSignal(null, '')).toBeNull()
    expect(followingSignal(null, 'two words')).toBeNull()
  })

  it('holds the floor between 0 and 100', () => {
    const d = { source: 'sound:bass', floor: 20 }
    expect(withFloor(d, 140).floor).toBe(100)
    expect(withFloor(d, -5).floor).toBe(0)
    expect(withFloor(d, 42.4).floor).toBe(42)
  })
})
