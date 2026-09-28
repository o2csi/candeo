import { describe, expect, it } from 'vitest'

import { applies, devicesFor, offeredFor, wants } from './applies'

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

describe('pickers', () => {
  const clock = { id: 'Clock', kinds: ['all'], requires: ['matrix'] }
  const pulse = { id: 'Beat pulse', kinds: ['all'], requires: [] }
  const old = { id: 'Old' }

  it('reads a manifest as Rust does: no kinds is keys', () => {
    expect(wants({})).toEqual({ kinds: ['keys'], requires: [], readsKeys: false })
  })

  it('offers what applies to every device, and keeps the effect chosen', () => {
    const all = [clock, pulse, old]
    expect(offeredFor(all, [keyboard]).map((e) => e.id)).toEqual(['Clock', 'Beat pulse', 'Old'])
    expect(offeredFor(all, [keyboard, zones]).map((e) => e.id)).toEqual(['Beat pulse'])
    expect(offeredFor(all, [zones], 'Clock').map((e) => e.id)).toEqual(['Clock', 'Beat pulse'])
    expect(offeredFor(all, [])).toHaveLength(3)
  })

  it('sends an effect where it applies, or everywhere it does when none is left', () => {
    const board = { ...keyboard, name: 'board' }
    const ring = { ...zones, name: 'ring' }
    const names = (list: { name: string }[]) => list.map((d) => d.name)
    expect(names(devicesFor(wants(clock), [board, ring], [board, ring]))).toEqual(['board'])
    expect(names(devicesFor(wants(clock), [ring], [board, ring]))).toEqual(['board'])
    expect(names(devicesFor(wants(pulse), [ring], [board, ring]))).toEqual(['ring'])
    expect(names(devicesFor(null, [ring], [board, ring]))).toEqual(['ring'])
  })
})
