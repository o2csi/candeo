import { describe, expect, it } from 'vitest'

import { blankRule, duration, editable, examples, moved, ruleValues } from './rules'

const KEYBOARD = { vid: 0x1532, pid: 0x0292 }

describe('duration', () => {
  it('reads in the largest unit that divides it exactly', () => {
    expect(duration(3600)).toEqual({ key: 'automations.hours', n: 1 })
    expect(duration(900)).toEqual({ key: 'automations.minutes', n: 15 })
    expect(duration(10)).toEqual({ key: 'automations.seconds', n: 10 })
    expect(duration(90)).toEqual({ key: 'automations.seconds', n: 90 })
    expect(duration(7200)).toEqual({ key: 'automations.hours', n: 2 })
  })
})

describe('new rules', () => {
  it('start switched off, on the given device, every hour for ten seconds', () => {
    const rule = blankRule(KEYBOARD, 'shipped:Clock')
    expect(rule.enabled).toBe(false)
    expect(rule.devices).toEqual([KEYBOARD])
    expect(rule.when).toEqual({ kind: 'schedule', every: 3600 })
    expect(rule.for).toEqual({ seconds: 10 })
    expect(blankRule(KEYBOARD, 'shipped:Clock').id).not.toBe(rule.id)
  })

  it('offers the hourly clock and the night off as disabled examples', () => {
    const [hourly, night] = examples(KEYBOARD, { hourly: 'Hourly clock', night: 'Night off' })
    expect(hourly.show.effect).toBe('shipped:Clock')
    expect(hourly.when).toMatchObject({ every: 3600, aligned: true })
    expect(night.show.effect).toBe('hardware:off')
    expect(night.when.between).toEqual({ from: '22:00', to: '07:00' })
    expect([hourly.enabled, night.enabled]).toEqual([false, false])
  })
})

describe('editable', () => {
  it('accepts a rule the tab built, and refuses what a hand edit turned into something else', () => {
    expect(editable(blankRule(KEYBOARD, 'shipped:Clock'))).toBe(true)
    expect(editable({ id: 'odd', when: 'whenever', show: 12 })).toBe(false)
    expect(editable(null)).toBe(false)
  })
})

describe('moved', () => {
  it('moves a rule to a new place, which is its new priority', () => {
    expect(moved(['a', 'b', 'c'], 2, 0)).toEqual(['c', 'a', 'b'])
    expect(moved(['a', 'b', 'c'], 0, 1)).toEqual(['b', 'a', 'c'])
  })

  it('leaves the list alone for a move that goes nowhere', () => {
    expect(moved(['a', 'b'], 1, 1)).toEqual(['a', 'b'])
    expect(moved(['a', 'b'], 0, 5)).toEqual(['a', 'b'])
  })
})

describe('ruleValues', () => {
  const manifest = {
    params: {
      speed: { kind: 'number' as const, label: 'Speed', min: 1, max: 20, default: 6 },
      seconds: { kind: 'boolean' as const, label: 'Seconds', default: false },
    },
  }

  it('fills the defaults and keeps what the rule set, for declared settings only', () => {
    expect(ruleValues(manifest, { seconds: true, gone: 3 })).toEqual({ speed: 6, seconds: true })
  })

  it('has nothing to show for a firmware effect', () => {
    expect(ruleValues(undefined, {})).toEqual({})
  })
})
