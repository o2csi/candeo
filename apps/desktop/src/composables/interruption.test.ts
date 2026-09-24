import { describe, expect, it } from 'vitest'

import { goesLive, interruptionLine, showsDeviceFrames } from './interruption'

const NOW = new Date(2026, 8, 17, 14, 0, 0).getTime()

describe('interruptionLine', () => {
  it('counts down a short interruption, and names what comes back', () => {
    const line = interruptionLine(
      { rule: 'hourly', name: '', effect: 'shipped:Clock', until: NOW + 6_200 },
      'Clock',
      'Bubbles',
      NOW,
    )
    expect(line).toEqual({
      key: 'effects.interruptedFor',
      params: { rule: 'Clock', seconds: 7, applied: 'Bubbles' },
    })
  })

  it('says only the countdown when nothing named comes back', () => {
    const line = interruptionLine(
      { rule: 'hourly', name: 'Hourly', effect: 'shipped:Clock', until: NOW + 3_000 },
      'Hourly',
      null,
      NOW,
    )
    expect(line.key).toBe('effects.interruptedForAlone')
    expect(line.params).toEqual({ rule: 'Hourly', seconds: 3 })
  })

  it('gives the time a long interruption ends at, rather than thousands of seconds', () => {
    const sevenTomorrow = new Date(2026, 8, 18, 7, 0, 0).getTime()
    const line = interruptionLine(
      { rule: 'night', name: 'Night', effect: 'hardware:off', until: sevenTomorrow },
      'Night',
      'Bubbles',
      NOW,
    )
    expect(line).toEqual({
      key: 'effects.interruptedUntil',
      params: { rule: 'Night', time: '07:00', applied: 'Bubbles' },
    })
  })

  it('has no end to announce for a rule that never stops', () => {
    const line = interruptionLine(
      { rule: 'always', name: '', effect: 'shipped:Clock' },
      'Clock',
      'Bubbles',
      NOW,
    )
    expect(line).toEqual({ key: 'effects.interruptedOpen', params: { rule: 'Clock' } })
  })

  it('never counts below zero while the engine catches up', () => {
    const line = interruptionLine(
      { rule: 'hourly', name: '', effect: 'shipped:Clock', until: NOW - 400 },
      'Clock',
      null,
      NOW,
    )
    expect(line.params.seconds).toBe(0)
  })
})

describe('goesLive', () => {
  it('sends a change to the device running that effect', () => {
    expect(goesLive('shipped:Bubbles', 'shipped:Bubbles', false)).toBe(true)
  })

  it('keeps it from a device running another effect, or none', () => {
    expect(goesLive('shipped:Rain', 'shipped:Bubbles', false)).toBe(false)
    expect(goesLive('shipped:Rain', null, false)).toBe(false)
  })

  it('keeps it from the rule an interruption runs, even for the applied effect', () => {
    expect(goesLive('shipped:Bubbles', 'shipped:Bubbles', true)).toBe(false)
  })
})

describe('showsDeviceFrames', () => {
  it('shows the device when it runs the selected effect', () => {
    expect(showsDeviceFrames(true, true, false)).toBe(true)
  })

  it('previews an effect the device does not run, or runs nothing', () => {
    expect(showsDeviceFrames(true, false, false)).toBe(false)
    expect(showsDeviceFrames(false, true, false)).toBe(false)
  })

  it('previews the applied effect while a rule runs its own', () => {
    expect(showsDeviceFrames(true, true, true)).toBe(false)
  })
})
