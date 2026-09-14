/**
 * The state of a controlled device, and the count the window title carries.
 *
 * Each device says its own state, next to it, instead of one pill summing up
 * all of them: a summary cannot tell which of two keyboards dropped.
 */

import type { DeviceInfo } from '../api/types'

/** The three states of a device someone decided to control. */
export type DeviceStatus = 'controlled' | 'notOpen' | 'unplugged'

export function deviceStatus(device: Pick<DeviceInfo, 'open' | 'present'>): DeviceStatus {
  if (device.open) return 'controlled'
  return device.present ? 'notOpen' : 'unplugged'
}

/** The interface's terms for each state (`AGENTS.md`, interface text). */
export const DEVICE_STATUS_LABELS: Record<DeviceStatus, string> = {
  controlled: 'piloté',
  notOpen: 'non ouvert',
  unplugged: 'débranché',
}

/**
 * "1 appareil piloté", with how many of them cannot be reached.
 *
 * A controlled device that is unplugged still counts: merging the two would
 * leave "controlled but gone" unsayable, which is exactly the loss worth seeing.
 */
export function controlledSummary(devices: readonly Pick<DeviceInfo, 'state' | 'open'>[]): string {
  const controlled = devices.filter((d) => d.state === 'adopted')
  if (controlled.length === 0) return 'aucun appareil piloté'

  const lost = controlled.filter((d) => !d.open).length
  const base =
    controlled.length === 1 ? '1 appareil piloté' : `${controlled.length} appareils pilotés`
  return lost === 0 ? base : `${base} · ${lost} injoignable${lost > 1 ? 's' : ''}`
}
