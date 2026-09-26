import { describe, expect, it } from 'vitest'

import type { DeviceInfo } from '../api/types'
import { definitionChoice, ids, known, pluggedIn } from './devicesPage'

function device(name: string, part: Partial<DeviceInfo> = {}): DeviceInfo {
  return {
    name,
    vid: 0x1532,
    pid: 0x0292,
    present: false,
    state: 'detected',
    open: false,
    error: null,
    surveyedFirmware: '',
    firmware: null,
    warnings: [],
    origin: 'builtIn',
    file: null,
    definitions: [],
    unloadedChoice: null,
    lights: 'keys',
    lightCount: 106,
    ...part,
  }
}

describe('devices page', () => {
  it('puts the controlled devices plugged in first, the ignored last', () => {
    const list = [
      device('Zeta', { present: true, state: 'ignored' }),
      device('Beta', { present: true }),
      device('Alpha'),
      device('Gamma', { present: true, state: 'adopted' }),
    ]
    expect(pluggedIn(list).map((d) => d.name)).toEqual(['Gamma', 'Beta', 'Zeta'])
  })

  it('offers a choice only between several definitions, or a broken one chosen', () => {
    const builtIn = { file: 'zones.json', origin: 'builtIn' as const }
    const mine = { file: 'mine.json', origin: 'yours' as const }
    expect(definitionChoice(device('Alone', { definitions: [builtIn] }))).toBeNull()
    expect(definitionChoice(device('Default', { definitions: [builtIn, mine] }))).toBe('')
    expect(
      definitionChoice(
        device('Mine', { definitions: [builtIn, mine], origin: 'yours', file: 'mine.json' }),
      ),
    ).toBe('mine.json')
    expect(
      definitionChoice(device('Broken', { definitions: [builtIn], unloadedChoice: 'mine.json' })),
    ).toBe('mine.json')
  })

  it('writes ids as four hexadecimal digits each', () => {
    expect(ids({ vid: 0x187c, pid: 0x551 })).toBe('187c:0551')
  })

  it('filters what it knows by name, by ids and by origin', () => {
    const list = [
      device('Razer DeathStalker V2 Pro'),
      device('Alienware m18 R1', { vid: 0x0d62, pid: 0xaab0 }),
      device('Alienware m18 R1 zones', { vid: 0x187c, pid: 0x0551, origin: 'yours' }),
    ]
    expect(known(list, '', 'all').map((d) => d.name)).toEqual([
      'Alienware m18 R1',
      'Alienware m18 R1 zones',
      'Razer DeathStalker V2 Pro',
    ])
    expect(known(list, ' alienWARE ', 'builtIn').map((d) => d.name)).toEqual(['Alienware m18 R1'])
    expect(known(list, '187c', 'all').map((d) => d.name)).toEqual(['Alienware m18 R1 zones'])
    expect(known(list, '', 'yours')).toHaveLength(1)
    expect(known(list, 'corsair', 'all')).toEqual([])
  })
})
