import { describe, expect, it } from 'vitest'

import { deviceEffect } from './effectSelection'

const library = ['Bubbles', 'Rain', 'hardware:spectrum']

describe('deviceEffect', () => {
  it('selects the effect running on the device', () => {
    expect(deviceEffect('Rain', 'Bubbles', library)).toBe('Rain')
    expect(deviceEffect('hardware:spectrum', null, library)).toBe('hardware:spectrum')
  })

  it('selects the remembered effect of a stopped device', () => {
    expect(deviceEffect(null, 'Bubbles', library)).toBe('Bubbles')
  })

  it('skips an effect the library no longer holds', () => {
    expect(deviceEffect('Gone', 'Bubbles', library)).toBe('Bubbles')
    expect(deviceEffect(null, 'Gone', library)).toBeNull()
  })

  it('leaves the fallback to the screen when the device has no effect', () => {
    expect(deviceEffect(null, null, library)).toBeNull()
  })
})
