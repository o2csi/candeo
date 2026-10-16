/**
 * What a command acts on: a whole device, or one region of a split one
 * (`docs/design/regions.md` §11 c).
 *
 * Apart from `candeo.ts`, which the tests replace: the composables read these
 * helpers, and a mocked module would hand them `undefined`.
 */

import type { DeviceRef, RegionRef } from './types'

/** A whole device, or one of its regions. */
export type Target = DeviceRef | RegionRef

/** The region a target names, `null` for a whole device. */
export function regionOf(target: Target): string | null {
  return 'region' in target && typeof target.region === 'string' ? target.region : null
}

/**
 * The model alone, `{ vid, pid }`: what a command's `device` argument carries.
 * A `DeviceInfo` or a `RegionRef` passed as it is would send its other fields too.
 */
export function deviceOf(target: DeviceRef): DeviceRef {
  return { vid: target.vid, pid: target.pid }
}

/**
 * A stable key for a target: `vid:pid` for a device, `vid:pid@region` for a
 * region. A region id holds neither `/` nor `@`, so the key can be read back and
 * extended (`useSettings` appends `/effect`).
 */
export function targetKey(target: Target): string {
  const region = regionOf(target)
  return region === null ? `${target.vid}:${target.pid}` : `${target.vid}:${target.pid}@${region}`
}

/** The target for a device and, when it is split, the region chosen on it. */
export function targetFor(device: DeviceRef, region: string | null): Target {
  return region === null ? deviceOf(device) : { ...deviceOf(device), region }
}
