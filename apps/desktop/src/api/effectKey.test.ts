import { describe, expect, it } from 'vitest'

import { effectName, isShippedKey, userKey } from './effectKey'

describe('effect keys', () => {
  it('builds the key of a user effect', () => {
    expect(userKey('My effect')).toBe('user:My effect')
  })

  it('shows the name a key holds', () => {
    expect(effectName('shipped:Breathing')).toBe('Breathing')
    expect(effectName('user:Rain (copy)')).toBe('Rain (copy)')
    expect(effectName('hardware:wave')).toBe('wave')
    expect(effectName('Rain')).toBe('Rain')
  })

  it('tells a shipped effect by its key', () => {
    expect(isShippedKey('shipped:Rain')).toBe(true)
    expect(isShippedKey('user:Rain')).toBe(false)
    expect(isShippedKey('user:shipped:Rain')).toBe(false)
  })
})
