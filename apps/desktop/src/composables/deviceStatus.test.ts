import { describe, expect, it } from 'vitest'

import type { DeviceState } from '../api/types'
import { controlledSummary, deviceStatus } from './deviceStatus'

const device = (state: DeviceState, open: boolean) => ({ state, open })

describe('deviceStatus', () => {
  it('tells an open device from a plugged one and an unplugged one', () => {
    expect(deviceStatus({ open: true, present: true })).toBe('controlled')
    expect(deviceStatus({ open: false, present: true })).toBe('notOpen')
    expect(deviceStatus({ open: false, present: false })).toBe('unplugged')
  })
})

describe('controlledSummary', () => {
  it('counts the adopted devices only', () => {
    expect(controlledSummary([])).toBe('aucun appareil piloté')
    expect(controlledSummary([device('detected', true), device('ignored', false)])).toBe(
      'aucun appareil piloté',
    )
    expect(controlledSummary([device('adopted', true), device('detected', true)])).toBe(
      '1 appareil piloté',
    )
    expect(controlledSummary([device('adopted', true), device('adopted', true)])).toBe(
      '2 appareils pilotés',
    )
  })

  it('says how many controlled devices cannot be reached', () => {
    expect(controlledSummary([device('adopted', false), device('adopted', true)])).toBe(
      '2 appareils pilotés · 1 injoignable',
    )
    expect(controlledSummary([device('adopted', false), device('adopted', false)])).toBe(
      '2 appareils pilotés · 2 injoignables',
    )
  })
})
