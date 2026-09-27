import { describe, expect, it } from 'vitest'

import { applies } from './applies'

const keyboard = { lights: 'keys' as const, capabilities: ['matrix', 'geometry'] }
const zones = { lights: 'zones' as const, capabilities: ['geometry'] }
const effect = (kinds: string[], requires: string[] = [], readsKeys = false) => ({ kinds, requires, readsKeys })

describe('applies', () => {
  it('offers an effect needing nothing everywhere it makes sense', () => {
    expect(applies(effect(['all']), keyboard)).toBe(true)
    expect(applies(effect(['all']), zones)).toBe(true)
  })

  it('keeps an effect drawn on rows off a surface without them', () => {
    expect(applies(effect(['all'], ['matrix']), keyboard)).toBe(true)
    expect(applies(effect(['all'], ['matrix']), zones)).toBe(false)
  })

  it('keeps an effect written for keys, or reading presses, on keys', () => {
    expect(applies(effect(['keys']), zones)).toBe(false)
    expect(applies(effect(['all'], ['geometry'], true), zones)).toBe(false)
    expect(applies(effect(['all'], ['geometry'], true), keyboard)).toBe(true)
  })
})
