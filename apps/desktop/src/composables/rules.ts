// What the Automations tab computes about rules, apart from drawing them (#106).
//
// Pure, so the tab's decisions — how a duration reads, what a new rule or an
// example holds, how the list reorders — are tested without mounting anything.

import type { DeviceRef } from '../api/types'
import type { EffectManifest, EffectParams, Rule } from '../api/candeo'
import { startingParams } from '../api/candeo'

/** What the "every" chip offers: a second, a minute, a quarter of an hour, an hour. */
export const EVERY_PRESETS: readonly number[] = [1, 60, 900, 3600]

/** What the "for" chip offers. Any number of seconds stays possible. */
export const FOR_PRESETS: readonly number[] = [1, 5, 10, 30, 60]

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
    r.when.kind === 'schedule' &&
    typeof r.when.every === 'number' &&
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
 * A rule to adjust: switched off, so that nothing interrupts a keyboard before
 * someone has read the sentence.
 */
export function blankRule(device: DeviceRef | null, effect: string): Rule {
  return {
    id: newId(),
    name: '',
    enabled: false,
    devices: device ? [device] : [],
    when: { kind: 'schedule', every: 3600 },
    show: { effect, params: {} },
    for: { seconds: 10 },
  }
}

/** The examples the empty tab offers, each a disabled rule to adjust (§3.5). */
export function examples(device: DeviceRef | null, names: { hourly: string; night: string }): Rule[] {
  const devices = device ? [device] : []
  return [
    {
      id: newId(),
      name: names.hourly,
      enabled: false,
      devices,
      when: { kind: 'schedule', every: 3600, aligned: true },
      show: { effect: 'shipped:Clock', params: {} },
      for: { seconds: 10 },
    },
    {
      id: newId(),
      name: names.night,
      enabled: false,
      devices,
      when: { kind: 'schedule', every: 1, between: { from: '22:00', to: '07:00' } },
      show: { effect: 'hardware:off', params: {} },
      for: { seconds: 1 },
    },
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
