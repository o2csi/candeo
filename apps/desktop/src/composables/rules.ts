// What the Automations tab computes about rules, apart from drawing them (#106).
//
// Pure, so the tab's decisions — how an expression reads as chips and back, how a
// duration reads, what a new rule or an example holds, how the list reorders —
// are tested without mounting anything.

import type { DeviceRef } from '../api/types'
import type { EffectManifest, EffectParams, Rule } from '../api/candeo'
import { startingParams } from '../api/candeo'

// ---------------------------------------------------------------- when

/**
 * How often, as the simple chips say it. Anything else is an expression someone
 * writes in the advanced field.
 */
export type Frequency =
  | { kind: 'minute' }
  | { kind: 'quarter' }
  | { kind: 'hour' }
  | { kind: 'at'; time: string }

/** Days of the week, 0 for Sunday as cron counts them, sorted, at least one. */
export type Days = number[]

/** A cron expression the chips can show. */
export interface Simple {
  frequency: Frequency
  days: Days
}

export const EVERY_DAY: Days = [0, 1, 2, 3, 4, 5, 6]
export const WEEKDAYS: Days = [1, 2, 3, 4, 5]
export const WEEKEND: Days = [0, 6]

/**
 * The chips for an expression, or `null` when only the advanced field can show
 * it. Only the five-field shapes the chips write are read back: a rule someone
 * wrote by hand stays theirs, in the field they wrote it in.
 */
export function readSimple(expr: string): Simple | null {
  const fields = expr.trim().split(/\s+/)
  if (fields.length !== 5) return null
  const [minute, hour, day, month, weekday] = fields
  if (day !== '*' || month !== '*') return null
  const days = readDays(weekday)
  if (!days) return null

  if (minute === '*' && hour === '*') return { frequency: { kind: 'minute' }, days }
  if (minute === '*/15' && hour === '*') return { frequency: { kind: 'quarter' }, days }
  if (minute === '0' && hour === '*') return { frequency: { kind: 'hour' }, days }
  if (/^\d{1,2}$/.test(minute) && /^\d{1,2}$/.test(hour)) {
    const m = Number(minute)
    const h = Number(hour)
    if (m <= 59 && h <= 23) return { frequency: { kind: 'at', time: `${pad(h)}:${pad(m)}` }, days }
  }
  return null
}

/** The expression for chips: five fields, the way `readSimple` reads them back. */
export function writeSimple(simple: Simple): string {
  const weekday = writeDays(simple.days)
  switch (simple.frequency.kind) {
    case 'minute':
      return `* * * * ${weekday}`
    case 'quarter':
      return `*/15 * * * ${weekday}`
    case 'hour':
      return `0 * * * ${weekday}`
    case 'at': {
      const [h, m] = simple.frequency.time.split(':').map(Number)
      return `${m} ${h} * * ${weekday}`
    }
  }
}

function readDays(field: string): Days | null {
  if (field === '*') return [...EVERY_DAY]
  if (!/^[\d,-]+$/.test(field)) return null
  const days = new Set<number>()
  for (const part of field.split(',')) {
    const range = /^(\d)(?:-(\d))?$/.exec(part)
    if (!range) return null
    const from = Number(range[1])
    const to = range[2] === undefined ? from : Number(range[2])
    if (from > to || to > 7) return null
    // Cron reads 7 as Sunday too.
    for (let d = from; d <= to; d++) days.add(d % 7)
  }
  return [...days].sort((a, b) => a - b)
}

function writeDays(days: Days): string {
  const sorted = [...new Set(days)].sort((a, b) => a - b)
  if (sorted.length === 7) return '*'
  if (same(sorted, WEEKDAYS)) return '1-5'
  return sorted.join(',')
}

export function same(a: readonly number[], b: readonly number[]): boolean {
  return a.length === b.length && a.every((v, i) => v === b[i])
}

function pad(n: number): string {
  return String(n).padStart(2, '0')
}

/** The rule's cron expression, or `null` for a rule that waits for idleness. */
export function expression(rule: Rule): string | null {
  return rule.when.kind === 'cron' ? rule.when.expr : null
}

/** What the idle choice offers, in minutes; any number of minutes stays possible. */
export const IDLE_PRESETS: readonly number[] = [1, 5, 10, 15, 30, 60]

// ---------------------------------------------------------------- for

/** What the "for" chip offers; any number of seconds stays possible. */
export const FOR_PRESETS: readonly number[] = [5, 10, 30, 60, 900, 3600, 28800]

/** A duration as a chip reads it: the largest unit that divides it exactly. */
export interface Duration {
  key: 'automations.hours' | 'automations.minutes' | 'automations.seconds'
  n: number
}

export function duration(seconds: number): Duration {
  if (seconds >= 3600 && seconds % 3600 === 0) {
    return { key: 'automations.hours', n: seconds / 3600 }
  }
  if (seconds >= 60 && seconds % 60 === 0) {
    return { key: 'automations.minutes', n: seconds / 60 }
  }
  return { key: 'automations.seconds', n: seconds }
}

// ---------------------------------------------------------------- rules

/**
 * Whether a rule from the file has the shape the tab can edit.
 *
 * Rules are read raw on the Rust side, one by one, so a rule edited by hand into
 * something else still reaches the window. The tab shows it as it is — with
 * Delete — rather than drawing chips over fields that are not there.
 */
export function editable(rule: unknown): rule is Rule {
  if (typeof rule !== 'object' || rule === null) return false
  const r = rule as Partial<Rule>
  return (
    typeof r.id === 'string' &&
    Array.isArray(r.devices) &&
    typeof r.when === 'object' &&
    r.when !== null &&
    ((r.when.kind === 'cron' && typeof r.when.expr === 'string') ||
      (r.when.kind === 'idle' && typeof r.when.minutes === 'number')) &&
    typeof r.show === 'object' &&
    r.show !== null &&
    typeof r.show.effect === 'string'
  )
}

/** A fresh identifier; rules are named by id, never by position. */
export function newId(): string {
  return crypto.randomUUID()
}

/**
 * A rule to adjust: every hour for ten seconds, switched off, so that nothing
 * interrupts a keyboard before someone has read the sentence.
 */
export function blankRule(device: DeviceRef | null, effect: string): Rule {
  return {
    id: newId(),
    name: '',
    enabled: false,
    devices: device ? [device] : [],
    when: { kind: 'cron', expr: '0 * * * *' },
    show: { effect, params: {} },
    for: { seconds: 10 },
  }
}

/**
 * The examples the empty tab offers, each a disabled rule to adjust (§3.5). The
 * keyboard off when nobody is there only where the system says when that is.
 */
export function examples(
  device: DeviceRef | null,
  names: { hourly: string; night: string; away: string },
  idle: boolean,
): Rule[] {
  const devices = device ? [device] : []
  const away: Rule[] = idle
    ? [
        {
          id: newId(),
          name: names.away,
          enabled: false,
          devices,
          when: { kind: 'idle', minutes: 10 },
          show: { effect: 'hardware:off', params: {} },
          for: { seconds: 10 },
        },
      ]
    : []
  return [
    {
      id: newId(),
      name: names.hourly,
      enabled: false,
      devices,
      when: { kind: 'cron', expr: '0 * * * *' },
      show: { effect: 'shipped:Clock', params: {} },
      for: { seconds: 10 },
    },
    {
      id: newId(),
      name: names.night,
      enabled: false,
      devices,
      when: { kind: 'cron', expr: '0 22 * * *' },
      show: { effect: 'hardware:off', params: {} },
      for: { seconds: 9 * 3600 },
    },
    ...away,
  ]
}

/** The list with the rule at `from` moved to `to`: the new priority order. */
export function moved<T>(list: readonly T[], from: number, to: number): T[] {
  const out = [...list]
  if (from < 0 || from >= out.length || to < 0 || to >= out.length || from === to) return out
  const [item] = out.splice(from, 1)
  out.splice(to, 0, item)
  return out
}

/**
 * The values the settings form shows for a rule's effect: the effect's defaults,
 * overridden by what the rule keeps, limited to what the effect still declares —
 * the rule `storage::starting_params` applies in Rust when the rule runs.
 */
export function ruleValues(
  manifest: Pick<EffectManifest, 'params'> | undefined,
  kept: EffectParams,
): EffectParams {
  if (!manifest) return {}
  const values = startingParams(manifest)
  for (const [id, value] of Object.entries(kept)) {
    if (manifest.params && id in manifest.params) values[id] = value
  }
  return values
}
