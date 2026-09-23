// Status row — one key per signal along the top row, lit by what it says.
//
// A dashboard for what other programs report: `build` is `failed`, `deploy` is
// `ok`, `tests` are `running`. Each signal takes a key of the top row, left to
// right in the alphabetical order of its name, so it keeps its key whatever order
// the signals arrive in; the key's colour says how it is going. Signals beyond
// the row's length are not shown.
//
// It reads every signal held (`inputs: ['signals']`) — the case the bag is for:
// many values at once, without knowing how many. With none, which is how the
// swatch is sampled, the row breathes dimly, waiting.

import { defineEffect, mix } from '@candeo/effects-api'

const GOOD = { r: 32, g: 200, b: 64 }
const BUSY = { r: 255, g: 150, b: 0 }
const BAD = { r: 235, g: 24, b: 24 }
const OTHER = { r: 150, g: 150, b: 165 }
const WAITING = { r: 36, g: 44, b: 60 }
const BLACK = { r: 0, g: 0, b: 0 }

/** Words senders commonly use, lower case. Anything else is shown as other. */
const GOOD_WORDS = new Set([
  'ok', 'success', 'succeeded', 'passed', 'pass', 'green', 'up', 'on', 'done', 'true', 'healthy',
])
const BUSY_WORDS = new Set([
  'running', 'pending', 'busy', 'building', 'queued', 'progress', 'in_progress', 'in-progress',
  'starting', 'warning', 'warn', 'yellow',
])
const BAD_WORDS = new Set([
  'failed', 'failure', 'fail', 'error', 'errored', 'broken', 'down', 'red', 'critical', 'false', 'ko',
])

/** How a word reads: `good`, `busy`, `bad`, or `other` when it is none of those. */
function kindOf(word = '') {
  const w = word.trim().toLowerCase()
  if (GOOD_WORDS.has(w)) return 'good'
  if (BUSY_WORDS.has(w)) return 'busy'
  if (BAD_WORDS.has(w)) return 'bad'
  return 'other'
}

/** A number as a level from 0, fine, to 1 — or 100 — the worst. */
function levelOf(value = 0) {
  const level = value <= 1 ? value : value / 100
  return Math.min(1, Math.max(0, level))
}

export default defineEffect({
  description: {
    en: 'One key of the top row per signal received, green, amber or red by what it says',
    fr: 'Une touche de la rangée du haut par signal reçu, verte, ambre ou rouge selon ce qu’il dit',
  },
  inputs: ['signals'],
  params: {
    good: { kind: 'color', label: { en: 'Going well', fr: 'Tout va bien' }, default: GOOD },
    busy: { kind: 'color', label: { en: 'Under way', fr: 'En cours' }, default: BUSY },
    bad: { kind: 'color', label: { en: 'Something wrong', fr: 'Problème' }, default: BAD },
    pulse: {
      kind: 'boolean',
      label: { en: 'Pulse what is under way', fr: 'Faire pulser ce qui est en cours' },
      default: true,
    },
  },
  render({ layout, time, signals, frame, params }) {
    if (layout.keys.length === 0) return
    const good = params.good ?? GOOD
    const busy = params.busy ?? BUSY
    const bad = params.bad ?? BAD

    // The top row, left to right: function keys on a keyboard, the zones of a
    // device that has only zones.
    const top = Math.min(...layout.keys.map((key) => key.row))
    const row = layout.keys.filter((key) => key.row === top).sort((a, b) => a.col - b.col)
    const names = Object.keys(signals).sort()

    if (names.length === 0) {
      const breath = 0.5 + 0.5 * Math.sin(time * 1.5)
      for (const key of row) frame.set(key, mix(BLACK, WAITING, 0.4 + 0.6 * breath))
      return
    }

    // Once a second, so a busy key reads as busy rather than as a fault.
    const beat = (params.pulse ?? true) ? 0.55 + 0.45 * Math.sin(time * Math.PI * 2) : 1
    row.forEach((key, i) => {
      const name = names[i]
      if (name === undefined) return
      const value = signals[name]
      if (typeof value === 'number') {
        frame.set(key, mix(good, bad, levelOf(value)))
        return
      }
      const kind = typeof value === 'boolean' ? (value ? 'good' : 'bad') : kindOf(String(value))
      const colour =
        kind === 'good' ? good :
        kind === 'bad' ? bad :
        kind === 'busy' ? mix(BLACK, busy, beat) :
        OTHER
      frame.set(key, colour)
    })
  },
})
