import { describe, expect, it } from 'vitest'

import type { RegionInfo, Rgb } from '../api/types'
import { applies } from './applies'
import { chosenRegion, isSplit, overlay, regionStatus } from './regions'

function region(id: string, positions: number[], part: Partial<RegionInfo> = {}): RegionInfo {
  return { id, name: { en: id }, positions, lights: 'zones', capabilities: ['geometry'], ...part }
}

const keyboard = region('keyboard', [4, 5, 6, 7, 12])
const lid = region('lid', [0, 1, 10, 11])
const regions = [keyboard, lid]

describe('chosenRegion', () => {
  it('falls on the region chosen, else on the first', () => {
    const device = { split: true, regions }
    expect(chosenRegion(device, 'lid')).toBe(lid)
    expect(chosenRegion(device, undefined)).toBe(keyboard)
  })

  it('gives way when the definition no longer declares the region chosen', () => {
    expect(chosenRegion({ split: true, regions }, 'base')).toBe(keyboard)
  })

  it('is none on a whole device, even one that declares regions', () => {
    expect(chosenRegion({ split: false, regions }, 'lid')).toBeNull()
    expect(chosenRegion({ split: true, regions: [] }, 'lid')).toBeNull()
    expect(chosenRegion(null, 'lid')).toBeNull()
    expect(isSplit({ split: true, regions: [] })).toBe(false)
  })
})

describe('regionStatus', () => {
  const running = { region: 'lid', running: true, effectId: 'shipped:Breathing', error: null }

  it('reads a region while its device runs its regions', () => {
    expect(regionStatus({ regions: [running] }, 'lid')).toBe(running)
    expect(regionStatus({ regions: [running] }, 'keyboard')).toBeNull()
  })

  it('reads none during an interruption, when the loop runs the whole device', () => {
    expect(regionStatus({}, 'lid')).toBeNull()
    expect(regionStatus(null, 'lid')).toBeNull()
  })
})

describe('overlay', () => {
  const red: Rgb = [255, 0, 0]
  const blue: Rgb = [0, 0, 255]
  const black: Rgb = [0, 0, 0]

  it('takes the region from the preview and the rest from the device', () => {
    const device: Rgb[] = [red, red, red, red]
    // The preview's frame has the device's length, black outside the region.
    const preview: Rgb[] = [black, blue, blue, black]

    expect(overlay(device, preview, [1, 2])).toEqual([red, blue, blue, red])
    expect(device).toEqual([red, red, red, red])
  })

  it('keeps a region light the preview paints black', () => {
    expect(overlay([red, red], [black, black], [0])).toEqual([black, red])
  })

  it('shows the preview alone when the frames describe two layouts', () => {
    expect(overlay([red], [blue, blue], [0, 1])).toEqual([blue, blue])
  })

  it('ignores positions outside the frame', () => {
    expect(overlay([red, red], [blue, blue], [1, 7, -1])).toEqual([red, blue])
  })
})

describe('applies per region', () => {
  const rain = { kinds: ['keys'], requires: ['matrix'], readsKeys: false }
  const pulse = { kinds: ['all'], requires: [], readsKeys: false }

  it('checks an effect against the region, not its device', () => {
    const keys = region('keys', [0, 1], { lights: 'keys', capabilities: ['matrix', 'geometry'] })
    expect(applies(rain, keys)).toBe(true)
    expect(applies(rain, lid)).toBe(false)
    expect(applies(pulse, lid)).toBe(true)
  })
})
