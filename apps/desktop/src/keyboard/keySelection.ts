/**
 * Choosing keys on the drawing (`docs/design/layers-ui.md` §4.3): a click
 * toggles one key, a drag paints from the first key's new state, arrows move
 * between keys.
 *
 * Pure, so the selection and the keyboard path are tested without a drawing.
 * Positions are frame positions, `KeyInfo.index`, as masks resolve to.
 */

import type { KeyInfo } from '../api/types'

/** The selection with `position` in it or out of it, ascending. */
export function painted(selection: readonly number[], position: number, on: boolean): number[] {
  const out = new Set(selection)
  if (on) out.add(position)
  else out.delete(position)
  return [...out].sort((a, b) => a - b)
}

/** The selection with `position` switched. */
export function toggled(selection: readonly number[], position: number): number[] {
  return painted(selection, position, !selection.includes(position))
}

export type Direction = 'left' | 'right' | 'up' | 'down'

/** The arrow keys a selection answers to, as `KeyboardEvent.key` names them. */
export const ARROWS: Readonly<Record<string, Direction>> = {
  ArrowLeft: 'left',
  ArrowRight: 'right',
  ArrowUp: 'up',
  ArrowDown: 'down',
}

/**
 * The key an arrow moves to from `from`: the nearest whose centre lies that
 * way, a key off the line counting more than a key further along it, so that
 * *right* stays on the row and *down* goes to the key under.
 *
 * `null` at the edge: the focus stays where it is rather than wrapping, which
 * would jump across the keyboard without a sign.
 */
export function nearestKey(
  keys: readonly KeyInfo[],
  from: number,
  direction: Direction,
): number | null {
  const start = keys.find((k) => k.index === from)
  if (!start) return keys[0]?.index ?? null
  const cx = start.x + start.w / 2
  const cy = start.y + start.h / 2
  let best: number | null = null
  let bestScore = Infinity
  for (const k of keys) {
    if (k.index === from) continue
    const dx = k.x + k.w / 2 - cx
    const dy = k.y + k.h / 2 - cy
    // How far along the arrow, and how far off it.
    const along = direction === 'left' ? -dx : direction === 'right' ? dx : direction === 'up' ? -dy : dy
    const across = direction === 'left' || direction === 'right' ? Math.abs(dy) : Math.abs(dx)
    // Half a key's width tolerates the stagger of rows, never a key behind.
    if (along <= 0.25) continue
    const score = along + across * 3
    if (score < bestScore) {
      bestScore = score
      best = k.index
    }
  }
  return best
}

/** The key the selection starts on: the first selected, else the first of the layout. */
export function startKey(keys: readonly KeyInfo[], selection: readonly number[]): number | null {
  const first = keys.find((k) => selection.includes(k.index)) ?? keys[0]
  return first?.index ?? null
}
