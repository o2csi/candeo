// How the device card tells what a rule is doing to a device (#106):
// "Clock — for 7 s, then Bubbles".
//
// Pure, so the phrasing is tested without an engine: the card polls the engine
// every second, and this turns what it reports into the words to show.

import type { InterruptionStatus } from '../api/candeo'

/** Past this, a countdown in seconds says less than the time it ends at. */
const COUNTDOWN_LIMIT_S = 90

/** Which sentence the card uses, and what fills it. */
export interface InterruptionLine {
  key:
    | 'effects.interruptedFor'
    | 'effects.interruptedForAlone'
    | 'effects.interruptedUntil'
    | 'effects.interruptedUntilAlone'
    | 'effects.interruptedOpen'
  params: { rule: string; applied?: string; seconds?: number; time?: string }
}

/**
 * The sentence for an interruption.
 *
 * - `rule` names it: the rule's name, or the effect it shows when it has none.
 * - `applied` is the effect the device goes back to, when it goes back to one
 *   the window can name.
 * - `now` is the window's clock, in epoch milliseconds.
 */
export function interruptionLine(
  interruption: InterruptionStatus,
  rule: string,
  applied: string | null,
  now: number,
): InterruptionLine {
  const then = applied === null ? {} : { applied }
  if (interruption.until === undefined) {
    return { key: 'effects.interruptedOpen', params: { rule } }
  }
  const seconds = Math.max(0, Math.ceil((interruption.until - now) / 1000))
  if (seconds <= COUNTDOWN_LIMIT_S) {
    return {
      key: applied === null ? 'effects.interruptedForAlone' : 'effects.interruptedFor',
      params: { rule, seconds, ...then },
    }
  }
  const end = new Date(interruption.until)
  const time = `${pad(end.getHours())}:${pad(end.getMinutes())}`
  return {
    key: applied === null ? 'effects.interruptedUntilAlone' : 'effects.interruptedUntil',
    params: { rule, time, ...then },
  }
}

function pad(n: number): string {
  return String(n).padStart(2, '0')
}
