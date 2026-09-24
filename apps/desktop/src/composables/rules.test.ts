import { describe, expect, it } from 'vitest'

import {
  EVERY_DAY,
  WEEKDAYS,
  WEEKEND,
  blankRule,
  duration,
  editable,
  examples,
  expression,
  flashedFor,
  holds,
  moved,
  readSimple,
  ruleValues,
  signalTrigger,
  validSignalName,
  whileItHolds,
  withoutSettings,
  writeSimple,
} from './rules'
import type { HeldSignal, Rule } from '../api/candeo'

const KEYBOARD = { vid: 0x1532, pid: 0x0292 }

/** A rule waiting for `build` to equal `failed`. */
function signalRule(hold?: boolean): Rule {
  const when = { kind: 'signal' as const, name: 'build', equals: 'failed' }
  return {
    ...blankRule(KEYBOARD, 'hardware:off'),
    when: hold === undefined ? when : { ...when, hold },
  }
}

describe('simple chips and cron', () => {
  it('reads the shapes the chips write', () => {
    expect(readSimple('0 * * * *')).toEqual({ frequency: { kind: 'hour' }, days: EVERY_DAY })
    expect(readSimple('*/15 * * * 1-5')).toEqual({
      frequency: { kind: 'quarter' },
      days: WEEKDAYS,
    })
    expect(readSimple('* * * * 0,6')).toEqual({ frequency: { kind: 'minute' }, days: WEEKEND })
    expect(readSimple('30 7 * * 1,3,5')).toEqual({
      frequency: { kind: 'at', time: '07:30' },
      days: [1, 3, 5],
    })
  })

  it('writes back what it reads, so a rule never changes by being shown', () => {
    for (const expr of ['0 * * * *', '*/15 * * * 1-5', '* * * * 0,6', '30 7 * * 1,3,5', '0 22 * * *']) {
      const simple = readSimple(expr)
      expect(simple, expr).not.toBeNull()
      expect(writeSimple(simple!)).toBe(expr)
    }
  })

  it('leaves to the advanced field what the chips cannot say', () => {
    for (const expr of [
      '*/30 * * * * *', // seconds
      '0 9-18 * * 1-5', // a range of hours
      '0 0 1 * *', // a day of the month
      '0 12 * JAN *', // a month
      '0 * * * MON', // a named day
      'every hour',
    ]) {
      expect(readSimple(expr), expr).toBeNull()
    }
  })

  it('counts 7 as Sunday, as cron does', () => {
    expect(readSimple('0 * * * 6-7')?.days).toEqual(WEEKEND)
  })
})

describe('duration', () => {
  it('reads in the largest unit that divides it exactly', () => {
    expect(duration(3600)).toEqual({ key: 'automations.hours', n: 1 })
    expect(duration(900)).toEqual({ key: 'automations.minutes', n: 15 })
    expect(duration(10)).toEqual({ key: 'automations.seconds', n: 10 })
    expect(duration(90)).toEqual({ key: 'automations.seconds', n: 90 })
    expect(duration(32400)).toEqual({ key: 'automations.hours', n: 9 })
  })
})

describe('new rules', () => {
  it('start switched off, on the given device, every hour for ten seconds', () => {
    const rule = blankRule(KEYBOARD, 'shipped:Clock')
    expect(rule.enabled).toBe(false)
    expect(rule.devices).toEqual([KEYBOARD])
    expect(rule.when).toEqual({ kind: 'cron', expr: '0 * * * *' })
    expect(rule.for).toEqual({ seconds: 10 })
    expect(blankRule(KEYBOARD, 'shipped:Clock').id).not.toBe(rule.id)
  })

  const names = { hourly: 'Hourly clock', night: 'Night off', away: 'Off when away' }

  it('offers the hourly clock and the night off as disabled examples', () => {
    const [hourly, night] = examples(KEYBOARD, names, false)
    expect(hourly.when).toEqual({ kind: 'cron', expr: '0 * * * *' })
    expect(hourly.show.effect).toBe('shipped:Clock')
    expect(night.when).toEqual({ kind: 'cron', expr: '0 22 * * *' })
    expect(night.for.seconds).toBe(9 * 3600)
    expect(night.show.effect).toBe('hardware:off')
    expect([hourly.enabled, night.enabled]).toEqual([false, false])
  })

  it('adds the keyboard off when away only where the system says when that is', () => {
    expect(examples(KEYBOARD, names, false)).toHaveLength(2)
    const away = examples(KEYBOARD, names, true)[2]
    expect(away.when).toEqual({ kind: 'idle', minutes: 10 })
    expect(away.show.effect).toBe('hardware:off')
    expect(away.enabled).toBe(false)
  })
})

describe('expression', () => {
  it('is the cron expression, and nothing for a rule waiting for idleness or a signal', () => {
    expect(expression(blankRule(KEYBOARD, 'x'))).toBe('0 * * * *')
    expect(expression({ ...blankRule(KEYBOARD, 'x'), when: { kind: 'idle', minutes: 5 } })).toBeNull()
    expect(expression(signalRule())).toBeNull()
  })
})

describe('editable', () => {
  it('accepts a rule the tab built, and refuses what a hand edit turned into something else', () => {
    expect(editable(blankRule(KEYBOARD, 'shipped:Clock'))).toBe(true)
    expect(editable({ ...blankRule(KEYBOARD, 'x'), when: { kind: 'idle', minutes: 10 } })).toBe(true)
    expect(editable({ id: 'odd', when: 'whenever', show: 12 })).toBe(false)
    expect(editable({ ...blankRule(KEYBOARD, 'x'), when: { kind: 'schedule', every: 60 } })).toBe(false)
    expect(editable({ ...blankRule(KEYBOARD, 'x'), when: { kind: 'idle' } })).toBe(false)
    expect(editable(null)).toBe(false)
  })

  it('accepts a signal rule with or without `hold`, as Rust reads it', () => {
    expect(editable(signalRule(false))).toBe(true)
    expect(editable(signalRule())).toBe(true)
    const when = { kind: 'signal', name: 'build', equals: 'failed' }
    expect(editable({ ...blankRule(KEYBOARD, 'x'), when: { ...when, hold: 'yes' } })).toBe(false)
    expect(editable({ ...blankRule(KEYBOARD, 'x'), when: { kind: 'signal', name: 'build' } })).toBe(
      false,
    )
    expect(editable({ ...blankRule(KEYBOARD, 'x'), when: { ...when, equals: 1 } })).toBe(false)
  })
})

describe('validSignalName', () => {
  it('takes letters, digits and _ - . : up to 64 characters, as Rust does', () => {
    for (const name of ['build', 'ci.status', 'home:door-bell_1', 'A'.repeat(64)]) {
      expect(validSignalName(name), name).toBe(true)
    }
  })

  it('refuses what could hide a control character or pass for another name', () => {
    for (const name of ['', 'A'.repeat(65), 'door bell', 'café', 'a/b', 'a"b', ' build']) {
      expect(validSignalName(name), name).toBe(false)
    }
  })
})

describe('signalTrigger', () => {
  const held: HeldSignal[] = [
    { name: 'build', value: 'failed', received: 0, expires: null },
    { name: 'volume', value: 1, received: 0, expires: null },
  ]
  const cron = blankRule(KEYBOARD, 'x').when

  it('is nothing for a name that cannot be one', () => {
    expect(signalTrigger('', 'x', cron, held)).toBeNull()
    expect(signalTrigger('door bell', 'ring', cron, held)).toBeNull()
  })

  it('holds by default, with the name trimmed and the value as typed', () => {
    expect(signalTrigger(' doorbell ', 'ring', cron, held)).toEqual({
      kind: 'signal',
      name: 'doorbell',
      equals: 'ring',
      hold: true,
    })
  })

  it('takes the value held now when none is typed: send it, see it, then write the rule', () => {
    expect(signalTrigger('build', '', cron, held)?.equals).toBe('failed')
    expect(signalTrigger('volume', '', null, held)?.equals).toBe('1')
    expect(signalTrigger('build', 'passed', cron, held)?.equals).toBe('passed')
    expect(signalTrigger('doorbell', '', cron, held)?.equals).toBe('')
  })

  it('keeps the duration a signal rule already had', () => {
    expect(signalTrigger('ci', 'red', signalRule(false).when, held)?.hold).toBe(false)
    expect(signalTrigger('ci', 'red', signalRule().when, held)?.hold).toBe(true)
  })
})

describe('signal durations', () => {
  it('holds unless said otherwise, as Rust reads a rule without `hold`', () => {
    expect(holds(signalRule().when)).toBe(true)
    expect(holds(signalRule(true).when)).toBe(true)
    expect(holds(signalRule(false).when)).toBe(false)
    expect(holds(blankRule(KEYBOARD, 'x').when)).toBe(false)
  })

  it('turns a held rule into a flash of so many seconds, and back, keeping them for Try', () => {
    const flash = flashedFor(signalRule(), 5)
    expect(flash.when).toEqual({ kind: 'signal', name: 'build', equals: 'failed', hold: false })
    expect(flash.for).toEqual({ seconds: 5 })
    const held = whileItHolds(flash)
    expect(held.when).toEqual({ kind: 'signal', name: 'build', equals: 'failed', hold: true })
    expect(held.for).toEqual({ seconds: 5 })
  })

  it('leaves a rule that waits for no signal as it is', () => {
    const rule = blankRule(KEYBOARD, 'x')
    expect(flashedFor(rule, 5)).toBe(rule)
    expect(whileItHolds(rule)).toBe(rule)
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

describe('withoutSettings', () => {
  it('gives the effect back its declared values, and unbinds every parameter', () => {
    const show = {
      effect: 'shipped:Fixed gradient',
      params: { speed: 1.5 },
      bindings: { colour: 'signal:status' },
    }

    expect(withoutSettings(show)).toEqual({ effect: 'shipped:Fixed gradient', params: {} })
  })
})
