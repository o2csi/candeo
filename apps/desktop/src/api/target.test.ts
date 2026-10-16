import { describe, expect, it } from 'vitest'

import { deviceOf, regionOf, targetFor, targetKey } from './target'

describe('targets', () => {
  const device = { vid: 0x187c, pid: 0x0550 }
  const lid = { ...device, region: 'lid' }

  it('names a region beside its model, and none for a whole device', () => {
    expect(regionOf(lid)).toBe('lid')
    expect(regionOf(device)).toBeNull()
    expect(deviceOf(lid)).toEqual(device)
  })

  it('keys a region apart from its device', () => {
    expect(targetKey(device)).toBe('6268:1360')
    expect(targetKey(lid)).toBe('6268:1360@lid')
  })

  it('builds a target from a device and the region chosen on it', () => {
    expect(targetFor({ ...device, name: 'Alienware 17 R5' } as typeof device, null)).toEqual(device)
    expect(targetFor(device, 'lid')).toEqual(lid)
  })
})
