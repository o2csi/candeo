/**
 * Regions, as the window reads them (`docs/design/regions.md`): which one a
 * split device's selection falls on, what the engine says of it, and how its
 * preview is laid over the device's frames.
 *
 * Pure, so each is tested without a device that declares regions.
 */

import type { DeviceEngineStatus, RegionEngineStatus } from '../api/candeo'
import type { DeviceInfo, RegionInfo, Rgb } from '../api/types'

/** What a device says of its regions. */
type Splits = Pick<DeviceInfo, 'split' | 'regions'>

/** True while a device runs one effect per region. */
export function isSplit(device: Splits | null | undefined): boolean {
  return device?.split === true && device.regions.length > 0
}

/**
 * The region a split device's selection falls on: the one chosen, while its
 * definition still declares it, else the first. `null` for a device that is
 * whole: there is no region to select, the device is.
 *
 * The definition decides (§2): a region chosen once and no longer declared,
 * after a definition was edited, gives way rather than naming nothing.
 */
export function chosenRegion(device: Splits | null, chosen: string | null | undefined): RegionInfo | null {
  if (!device || !isSplit(device)) return null
  return device.regions.find((r) => r.id === chosen) ?? device.regions[0]
}

/**
 * A region's state in the engine's report, while its device's loop runs its
 * regions. `null` otherwise: during an interruption the device runs a rule's
 * effect on all of it, and none of its regions runs anything of its own.
 */
export function regionStatus(
  status: Pick<DeviceEngineStatus, 'regions'> | null | undefined,
  region: string,
): RegionEngineStatus | null {
  return status?.regions?.find((r) => r.region === region) ?? null
}

/**
 * A region's preview laid over the device's frame: its own positions from the
 * preview, every other from the device, what the device shows now (§3).
 *
 * The preview's frame has the device's length, black outside the region
 * (§7): taking it whole would black the other regions out. Two frames of
 * different lengths describe two layouts, and only the preview is about the
 * effect being looked at: it is returned as it is.
 */
export function overlay(
  device: readonly Rgb[],
  preview: readonly Rgb[],
  positions: readonly number[],
): readonly Rgb[] {
  if (device.length !== preview.length) return preview
  const out = device.slice()
  for (const p of positions) {
    if (p >= 0 && p < preview.length) out[p] = preview[p]
  }
  return out
}
