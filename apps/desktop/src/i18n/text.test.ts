import { describe, expect, it } from 'vitest'

import en from '../locales/en.json'
import fr from '../locales/fr.json'
import { showIn } from '.'
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
    showIn('fr')
    expect(localized({ en: 'Speed', fr: 'Vitesse' })).toBe('Vitesse')
    showIn('en')
    expect(localized({ en: 'Speed', fr: 'Vitesse' })).toBe('Speed')
  })
})

describe('catalogs', () => {
  /** Every key path of a catalog, `a.b.c`. */
  function keys(catalog: object, prefix = ''): string[] {
    return Object.entries(catalog).flatMap(([k, v]) =>
      typeof v === 'object' && v !== null ? keys(v, `${prefix}${k}.`) : [`${prefix}${k}`],
    )
  }

  /** `vue-tsc` checks French against the English shape; this also catches extra keys. */
  it('say the same things in English and French', () => {
    expect(keys(fr).sort()).toEqual(keys(en).sort())
  })
})
