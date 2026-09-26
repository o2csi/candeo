import { describe, expect, it, vi } from 'vitest'

vi.mock('../api/candeo', () => ({
  firmwareEffects: vi.fn(async () => [
    {
      id: 'hardware:m18-08',
      colours: 0,
      name: { en: 'Breathing', fr: 'Respiration' },
      summary: null,
      looks: 'breathing',
    },
  ]),
}))

import { named, OFF, refreshFirmwareEffects } from './useEffects'

describe('a firmware effect is named by its definition', () => {
  it('in the interface language, from what the layout declares', () => {
    const own = { id: 'hardware:x', colours: 1, name: 'Glow', summary: { en: 'Warm.', fr: 'Chaud.' } }
    expect(named('hardware:x', 1, { ...own, looks: 'pulse' })).toEqual({
      id: 'hardware:x',
      colours: 1,
      name: 'Glow',
      summary: 'Warm.',
      looks: 'pulse',
    })
  })

  it('outside the gallery, from every definition Candeo knows', async () => {
    expect(named('hardware:m18-08').name).toBe('hardware:m18-08')
    await refreshFirmwareEffects()
    expect(named('hardware:m18-08').name).toBe('Breathing')
  })

  it('shows its id when nobody named it, and Off is the application’s', () => {
    expect(named('hardware:kind02')).toMatchObject({ name: 'hardware:kind02', summary: '' })
    expect(named(OFF).name).toBe('Off')
  })
})
