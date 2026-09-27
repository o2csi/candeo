import { describe, expect, it, vi } from 'vitest'

vi.mock('../api/candeo', () => ({}))

import type { HeldSignal, Rule } from '../api/candeo'
import { gameRule, gameSignals, playing, withGameRule } from './games'

const held = (name: string, value: string | number): HeldSignal => ({
  name,
  value,
  received: 0,
  expires: null,
})

const clock: Rule = {
  id: 'clock',
  name: 'Clock on the hour',
  enabled: true,
  devices: [{ vid: 1, pid: 2 }],
  when: { kind: 'cron', expr: '0 * * * *' },
  show: { effect: 'shipped:Clock', params: {} },
  for: { seconds: 10 },
}

describe('games', () => {
  it('reads what a game says, and whether a match is under way', () => {
    const signals = [held('cs2.health', 64), held('build', 'ok'), held('cs2.ammo', 50)]
    expect(gameSignals(signals, 'cs2').map((s) => s.name)).toEqual(['cs2.ammo', 'cs2.health'])
    expect(playing(signals, 'cs2')).toBe(false)
    expect(playing([...signals, held('cs2.phase', 'live')], 'cs2')).toBe(true)
  })

  it('puts a new while-playing rule first, on any value of the phase', () => {
    const rules = withGameRule([clock], 'cs2', 'While playing Counter-Strike 2', {
      effect: 'shipped:Counter-Strike 2',
      devices: [{ vid: 1, pid: 2 }],
    })
    expect(rules.map((r) => r.id)[1]).toBe('clock')
    const made = gameRule(rules, 'cs2')!
    expect(made.when).toEqual({ kind: 'signal', name: 'cs2.phase', equals: '', hold: true })
    expect(made.show.effect).toBe('shipped:Counter-Strike 2')
    expect(made.enabled).toBe(true)
  })

  it('keeps the rule where it was moved, and removes it on request', () => {
    const first = withGameRule([clock], 'cs2', 'While playing', { effect: 'a', devices: [] })
    const moved = [first[1], first[0]]
    const changed = withGameRule(moved, 'cs2', 'While playing', { effect: 'b', devices: [] })
    expect(changed.map((r) => r.show.effect)).toEqual(['shipped:Clock', 'b'])
    expect(changed[1].id).toBe(first[0].id)
    expect(withGameRule(changed, 'cs2', 'While playing', null)).toEqual([clock])
  })
})
