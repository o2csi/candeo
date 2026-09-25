/**
 * Layout ↔ frame consistency check, for the simulator.
 *
 * ⚠️ This file holds **no geometry**. It lives in the one place where it is
 * tested — `crates/candeo-device/src/layout.rs` — and arrives through the
 * `get_default_layout()` command when nothing is plugged in. A transcription
 * here would be a second source of truth, and that one does not diverge
 * loudly: it diverges silently.
 *
 * The geometry is not a survey, either: the device only exposes its 6 × 22 grid
 * and declares no dimension. See `docs/api/commands.md`, "The geometry is not
 * read from the device".
 */

import type { KeyInfo, OutlinePart, Rgb } from '../api/types'

/**
 * What the simulator asks of a layout.
 *
 * Identical to the `LayoutInfo` of `api/types`, except for `keys`, here
 * **read-only**: Vue's `readonly()` returns a `readonly KeyInfo[]`, which is not
 * assignable to `KeyInfo[]`. A `LayoutInfo` is still accepted as it is, the
 * reverse is not — and that is the right direction for the relation, the
 * simulator never modifies the layout it is given.
 */
export interface LayoutView {
  name: string
  rows: number
  cols: number
  frameLen: number
  keys: readonly KeyInfo[]
  /**
   * What these lights are: the drawing is the same, what is said about it is
   * not — "103 lit keys" over a ring and a logo would be false. Absent, they
   * are read as keys, as before a device could say.
   */
  lights?: 'keys' | 'zones'
  /** What lights nothing and is drawn under the lights. Absent: nothing. */
  outline?: readonly OutlinePart[]
}

/**
 * Footprint of the drawing, in pitch units — 22.5 × 6.5 on this keyboard.
 *
 * Computed and not hard-coded: it is what lets the `viewBox` follow any layout,
 * whatever the model Rust returns.
 */
export function extent(
  keys: readonly KeyInfo[],
  outline: readonly OutlinePart[] = [],
): { w: number; h: number } {
  let w = 0
  let h = 0
  // The outline counts too: a lid drawn wider than the lights it carries must
  // not be cut off at their edge.
  for (const k of [...keys, ...outline]) {
    if (k.x + k.w > w) w = k.x + k.w
    if (k.y + k.h > h) h = k.y + k.h
  }
  // An empty layout would give a `viewBox` of zero area, which the browser
  // refuses to draw: a unit box is returned rather than a black hole.
  return { w: w || 1, h: h || 1 }
}

/**
 * Layout ↔ frame consistency check.
 *
 * The geometry itself is checked on the Rust side, where it is written. What
 * remains are the invariants only the assembly can betray, and they are exactly
 * those this hardware sets traps for: reading a color at the key's rank rather
 * than at its `index`, receiving 106 colors instead of 132, seeing two keys
 * share an index.
 *
 * Returned as a list rather than through an exception: we want **all** the
 * flaws at once, and a simulator that refuses to display helps nobody. Called
 * by `KeyboardSimulator` in development only.
 */
export function layoutProblems(layout: LayoutView, frame: readonly Rgb[]): string[] {
  const problems: string[] = []

  if (frame.length !== layout.frameLen) {
    problems.push(`frame of ${frame.length} colors for a layout expecting ${layout.frameLen}`)
  }

  const seen = new Set<number>()
  for (const k of layout.keys) {
    if (!Number.isInteger(k.index) || k.index < 0 || k.index >= layout.frameLen) {
      problems.push(`key ${k.index} is outside the frame`)
    } else if (frame[k.index] === undefined) {
      problems.push(`key ${k.index} has no color`)
    }

    if (seen.has(k.index)) problems.push(`index ${k.index} is used by two keys`)
    seen.add(k.index)

    if (!(k.w > 0) || !(k.h > 0)) {
      problems.push(`key ${k.index} has no area`)
    }
    if (k.x < 0 || k.y < 0) {
      problems.push(`key ${k.index} leaves the drawing at the top or left`)
    }
  }

  // Two keycaps cannot occupy the same space — including the two arms of the
  // L-shaped Enter, which are adjoining and not overlapping. Deliberate
  // duplicate of the Rust test `key_rectangles_do_not_overlap`: this one holds
  // for the layout that actually arrives, whatever it is.
  const keys = layout.keys
  for (let i = 0; i < keys.length; i++) {
    const a = keys[i]
    for (let j = i + 1; j < keys.length; j++) {
      const b = keys[j]
      const disjoint =
        a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y
      if (!disjoint) {
        problems.push(`keys ${a.index} and ${b.index} overlap`)
      }
    }
  }

  return problems
}
