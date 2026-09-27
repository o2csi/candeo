/**
 * Games (`docs/design/game-state-integration.md` §4): whether they are on, what
 * a game is saying now, and its *while playing* rule.
 *
 * Module-level state for the switch, like `useDevice`: the rail shows the Games
 * tab from it, and Settings turns it on.
 */

import { ref } from 'vue'

import * as api from '../api/candeo'
import type { HeldSignal, Rule } from '../api/candeo'
import type { DeviceRef } from '../api/types'
import { message, warn } from '../api/journal'
import { newId } from './rules'

/** Valve's game state integration is on: the Games tab shows. */
export const valveGames = ref(false)

export async function refreshGames(): Promise<void> {
  try {
    valveGames.value = (await api.getSignalsApi()).valveGames
  } catch (e) {
    warn('games', `games switch not read: ${message(e, 'en')}`, e)
  }
}

export async function setValveGames(on: boolean): Promise<void> {
  valveGames.value = (await api.setValveGames(on)).valveGames
}

/** What `game` says now: its signals, `cs2.health` and the others, by name. */
export function gameSignals(held: readonly HeldSignal[], game: string): HeldSignal[] {
  return held.filter((s) => s.name.startsWith(`${game}.`)).sort((a, b) => a.name.localeCompare(b.name))
}

/** The signal present for as long as a match is: the round's phase. */
export function playingSignal(game: string): string {
  return `${game}.phase`
}

/** Whether a match is under way: the game is saying which phase its round is in. */
export function playing(held: readonly HeldSignal[], game: string): boolean {
  return held.some((s) => s.name === playingSignal(game))
}

/** The game's *while playing* rule, if it has one. */
export function gameRule(rules: readonly Rule[], game: string): Rule | undefined {
  return rules.find((r) => r.game === game)
}

/** What *while playing* shows, and where. */
export interface WhilePlaying {
  effect: string
  devices: DeviceRef[]
}

/**
 * The rules once a game is connected: its *while playing* made if it has none,
 * the game's own effect on the devices Candeo controls, since connecting a game
 * is wanting its lighting during a match. One already there, perhaps changed,
 * is kept; without an effect or a device, there is nothing to make. The same
 * array when nothing changes.
 */
export function withGameConnected(
  rules: Rule[],
  game: string,
  name: string,
  effect: string | undefined,
  devices: DeviceRef[],
): Rule[] {
  if (gameRule(rules, game) || !effect || devices.length === 0) return rules
  return withGameRule(rules, game, name, { effect, devices })
}

/**
 * The rules with the game's *while playing* set as asked, `null` to remove it.
 *
 * A rule made here goes first: during a match, the game's effect wins over a
 * clock on the hour. One that exists keeps its place and its id — someone may
 * have moved it in Automations.
 */
export function withGameRule(
  rules: readonly Rule[],
  game: string,
  name: string,
  wanted: WhilePlaying | null,
): Rule[] {
  const existing = gameRule(rules, game)
  if (wanted === null) return rules.filter((r) => r !== existing)
  const rule: Rule = {
    id: existing?.id ?? newId(),
    name,
    enabled: true,
    devices: wanted.devices,
    when: { kind: 'signal', name: playingSignal(game), equals: '', hold: true },
    show: { effect: wanted.effect, params: existing?.show.params ?? {}, bindings: existing?.show.bindings ?? {} },
    for: existing?.for ?? { seconds: 10 },
    game,
  }
  return existing ? rules.map((r) => (r === existing ? rule : r)) : [rule, ...rules]
}
