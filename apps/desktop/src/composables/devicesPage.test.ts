import { describe, expect, it } from 'vitest'

import type { DeviceInfo } from '../api/types'
import { FILTER_PAST, definitionChoice, ids, listed, yourFiles } from './devicesPage'

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
    capabilities: ['matrix', 'geometry'],
    ...part,
  }
}

describe('devices page', () => {
  it('shows what is plugged in by name, whatever its state, and under All the others after', () => {
    const list = [
      device('Zeta', { present: true, state: 'ignored' }),
      device('Beta', { present: true }),
      device('Alpha'),
      device('Gamma', { present: true, state: 'adopted' }),
      device('Delta', { state: 'adopted' }),
    ]
    expect(listed(list, 'plugged', '').map((d) => d.name)).toEqual(['Beta', 'Gamma', 'Zeta'])
    expect(listed(list, 'all', '').map((d) => d.name)).toEqual([
      'Beta',
      'Gamma',
      'Zeta',
      'Alpha',
      'Delta',
    ])
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

  it('lists every file of yours, the chosen one in use, the broken one with why', () => {
    const zones = device('Zones', {
      definitions: [
        { file: 'zones.json', origin: 'builtIn' },
        { file: 'b.json', origin: 'yours' },
        { file: 'a.json', origin: 'yours' },
      ],
      origin: 'yours',
      file: 'a.json',
    })
    const files = yourFiles([zones, device('Razer')], [{ file: 'c.json', reason: 'broken' }])
    expect(files.map((f) => [f.file, f.inUse, f.reason ?? f.device?.name])).toEqual([
      ['a.json', true, 'Zones'],
      ['b.json', false, 'Zones'],
      ['c.json', false, 'broken'],
    ])
  })

  it('writes ids as four hexadecimal digits each', () => {
    expect(ids({ vid: 0x187c, pid: 0x551 })).toBe('187c:0551')
  })

  it('filters All by name or ids, once there are enough devices to need it', () => {
    const few = [
      device('Razer DeathStalker V2 Pro'),
      device('Alienware m18 R1 zones', { vid: 0x187c, pid: 0x0551, present: true }),
    ]
    expect(listed(few, 'all', 'corsair')).toHaveLength(2)

    const many = [
      ...few,
      ...Array.from({ length: FILTER_PAST }, (_, i) => device(`Keyboard ${i}`, { pid: i })),
    ]
    expect(listed(many, 'all', ' alienWARE ').map((d) => d.name)).toEqual(['Alienware m18 R1 zones'])
    expect(listed(many, 'all', '187c').map((d) => d.name)).toEqual(['Alienware m18 R1 zones'])
    expect(listed(many, 'all', 'corsair')).toEqual([])
    expect(listed(many, 'plugged', 'corsair')).toHaveLength(1)
  })
})
