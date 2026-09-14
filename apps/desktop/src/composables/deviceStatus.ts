/**
 * The state of a controlled device, and the count the window title carries.
 *
 * Each device says its own state, next to it, instead of one pill summing up
 * all of them: a summary cannot tell which of two keyboards dropped.
 */

import type { DeviceInfo } from '../api/types'
import { t } from '../i18n'

/** The three states of a device someone decided to control. */
export type DeviceStatus = 'controlled' | 'notOpen' | 'unplugged'

export function deviceStatus(device: Pick<DeviceInfo, 'open' | 'present'>): DeviceStatus {
  if (device.open) return 'controlled'
  return device.present ? 'notOpen' : 'unplugged'
}

/** The interface's term for a state (`AGENTS.md`, interface text). */
export function statusLabel(status: DeviceStatus): string {
  return t(`devices.status.${status}`)
}

/**
 * "1 device controlled", with how many of them cannot be reached.
 *
 * A controlled device that is unplugged still counts: merging the two would
 * leave "controlled but gone" unsayable, which is exactly the loss worth seeing.
 */
export function controlledSummary(devices: readonly Pick<DeviceInfo, 'state' | 'open'>[]): string {
  const controlled = devices.filter((d) => d.state === 'adopted')
  const summary = t('devices.summary.controlled', controlled.length)
  const lost = controlled.filter((d) => !d.open).length
  return lost === 0 ? summary : t('devices.summary.unreachable', { summary, n: lost }, lost)
}
