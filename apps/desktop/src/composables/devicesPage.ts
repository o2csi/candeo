// What the Devices page shows, in which order (`docs/design/device-sdk.md` §9).
// Pure, so which device goes where is tested without a DOM.

import type { DeviceInfo } from '../api/types'

/** Which definitions the list of known devices shows. */
export type OriginFilter = 'all' | 'builtIn' | 'yours'

const byName = (a: DeviceInfo, b: DeviceInfo) => a.name.localeCompare(b.name)

/**
 * The devices plugged in, first on the page: what someone came to act on. The
 * controlled ones lead, then those waiting for a decision, then the ignored.
 */
export function pluggedIn(devices: readonly DeviceInfo[]): DeviceInfo[] {
  const rank = (d: DeviceInfo) => (d.state === 'adopted' ? 0 : d.state === 'ignored' ? 2 : 1)
  return devices.filter((d) => d.present).sort((a, b) => rank(a) - rank(b) || byName(a, b))
}

/** `vid:pid` as the page writes it, four hexadecimal digits each. */
export function ids(d: Pick<DeviceInfo, 'vid' | 'pid'>): string {
  const hex = (n: number) => n.toString(16).padStart(4, '0')
  return `${hex(d.vid)}:${hex(d.pid)}`
}

/**
 * Every device Candeo knows, whatever is plugged in, filtered by a text — a
 * name, a maker, or its `vid:pid` — and by whose definition it is. The list
 * grows with each device described: this is what keeps it usable.
 */
export function known(
  devices: readonly DeviceInfo[],
  text: string,
  origin: OriginFilter,
): DeviceInfo[] {
  const wanted = text.trim().toLowerCase()
  return devices
    .filter((d) => origin === 'all' || d.origin === origin)
    .filter((d) => !wanted || d.name.toLowerCase().includes(wanted) || ids(d).includes(wanted))
    .sort(byName)
}
