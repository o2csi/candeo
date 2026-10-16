/**
 * A region's pictogram: the device's silhouette drawn from its definition's
 * rectangles and outline, the region's lights filled and the others faint
 * (`docs/design/regions.md` §2). The same one marks the region in *Devices* and
 * in *Effects*.
 *
 * A pictogram, not the data: it is drawn about sixteen pixels wide, where a
 * lid's edge a fifth of a unit thick would vanish. Thin lights are thickened
 * around their middle, and nothing here is measured by an effect.
 */

import type { KeyInfo, OutlinePart } from '../api/types'
import { extent } from './layout'
import { archPath } from './shapes'

/**
 * The thinnest a light is drawn, as a share of the device's longest side.
 *
 * At sixteen pixels across, 6 % is about a pixel: under it a light is a
 * hairline the eye does not find, and the 17 R5's lid edges, 0.22 u on a
 * 16 u wide drawing, would be less than a quarter of one.
 */
const THINNEST = 0.06

/** A light as the pictogram draws it. */
export interface PictogramLight {
  index: number
  /** In the region: filled. Out of it: faint. */
  lit: boolean
  x: number
  y: number
  w: number
  h: number
  /** A disc, an arch's path, or else a rectangle. */
  disc: boolean
  path?: string
  stroke?: number
}

export interface Pictogram {
  /** The drawing's box, with room for a light thickened past the edge. */
  viewBox: string
  outline: readonly OutlinePart[]
  lights: PictogramLight[]
}

/** A span at least `min` long, around the same middle. */
function widen(start: number, length: number, min: number): [number, number] {
  if (length >= min) return [start, length]
  return [start - (min - length) / 2, min]
}

export function pictogram(
  keys: readonly KeyInfo[],
  outline: readonly OutlinePart[],
  positions: readonly number[],
): Pictogram {
  const size = extent(keys, outline)
  const min = THINNEST * Math.max(size.w, size.h)
  const lit = new Set(positions)
  const lights = keys.map((k): PictogramLight => {
    const inRegion = lit.has(k.index)
    if (k.shape === 'archUp' || k.shape === 'archDown') {
      // The ring's halves keep their shape: a filled box would read as a lid.
      return {
        index: k.index,
        lit: inRegion,
        x: k.x,
        y: k.y,
        w: k.w,
        h: k.h,
        disc: false,
        path: archPath(k, k.shape === 'archUp'),
        stroke: min,
      }
    }
    const [x, w] = widen(k.x, k.w, min)
    const [y, h] = widen(k.y, k.h, min)
    return { index: k.index, lit: inRegion, x, y, w, h, disc: k.shape === 'disc' }
  })
  const pad = min / 2
  const n = (v: number) => Number(v.toFixed(3))
  return {
    viewBox: `${n(-pad)} ${n(-pad)} ${n(size.w + 2 * pad)} ${n(size.h + 2 * pad)}`,
    outline,
    lights,
  }
}
