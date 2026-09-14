import { describe, expect, it } from 'vitest'

import { localized } from './text'

describe('localized', () => {
  it('takes a plain string as it is', () => {
    expect(localized('Vitesse', 'en')).toBe('Vitesse')
  })

  it('takes the language asked, then English, then the first entry', () => {
    const text = { en: 'Speed', fr: 'Vitesse' }
    expect(localized(text, 'fr')).toBe('Vitesse')
    expect(localized(text, 'de')).toBe('Speed')
    expect(localized({ it: 'Velocità', es: 'Velocidad' }, 'de')).toBe('Velocità')
  })

  it('is empty when there is nothing to say', () => {
    expect(localized(undefined)).toBe('')
    expect(localized({}, 'fr')).toBe('')
  })

  it('uses the interface language by default', () => {
    expect(localized({ en: 'Speed', fr: 'Vitesse' })).toBe('Vitesse')
  })
})
