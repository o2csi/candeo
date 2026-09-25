// How the simulator draws a light that is not a rectangle
// (`docs/design/studio.md` §4). The rectangle comes from Rust and stays what
// effects measure with; this only turns it into a path.

import type { KeyInfo } from '../api/types'

/** How thick an arch's stroke is, at most, in pitch units. */
const ARCH_STROKE = 0.35

/** The stroke of an arch drawn inside `k`: thinner in a flat rectangle. */
export function archStroke(k: Pick<KeyInfo, 'h'>): number {
  return Math.min(ARCH_STROKE, k.h * 0.6)
}

const n = (v: number) => Number(v.toFixed(3))

/**
 * Half of a rounded ring inside `k`: `up` for the half over the ports, open at
 * the bottom, the other open at the top. The open ends sit on the rectangle's
 * edge, so two halves stacked edge to edge close into one ring.
 */
export function archPath(k: Pick<KeyInfo, 'x' | 'y' | 'w' | 'h'>, up: boolean): string {
  const t = archStroke(k)
  const left = k.x + t / 2
  const right = k.x + k.w - t / 2
  const r = Math.max(0, Math.min(k.h - t / 2, (right - left) / 2))
  if (up) {
    const top = k.y + t / 2
    const open = k.y + k.h
    return (
      `M ${n(left)} ${n(open)} L ${n(left)} ${n(top + r)} ` +
      `A ${n(r)} ${n(r)} 0 0 1 ${n(left + r)} ${n(top)} L ${n(right - r)} ${n(top)} ` +
      `A ${n(r)} ${n(r)} 0 0 1 ${n(right)} ${n(top + r)} L ${n(right)} ${n(open)}`
    )
  }
  const bottom = k.y + k.h - t / 2
  const open = k.y
  return (
    `M ${n(left)} ${n(open)} L ${n(left)} ${n(bottom - r)} ` +
    `A ${n(r)} ${n(r)} 0 0 0 ${n(left + r)} ${n(bottom)} L ${n(right - r)} ${n(bottom)} ` +
    `A ${n(r)} ${n(r)} 0 0 0 ${n(right)} ${n(bottom - r)} L ${n(right)} ${n(open)}`
  )
}
