// What the Devices page shows, in which order (`docs/design/device-sdk.md` §9).
// Pure, so which device goes where is tested without a DOM.

import type { DefinitionProblem } from '../api/candeo'
import type { DeviceInfo } from '../api/types'

/**
 * Which devices the list shows: those plugged in, what someone came to act on,
 * or every one Candeo knows. Not the controlled ones alone: a keyboard just
 * plugged in, not controlled yet, would be hidden from whoever came to control it.
 */
export type Shown = 'plugged' | 'all'

/** Past this many devices known, *All* offers a text to filter them by. */
export const FILTER_PAST = 10

const byName = (a: DeviceInfo, b: DeviceInfo) => a.name.localeCompare(b.name)

/**
 * The page's one list: the devices plugged in first; under *All*, those known
 * but not plugged in after them, filtered by a text — a name, a maker, or its
 * `vid:pid` — once there are enough to need it. By name, not by state: a card
 * moving when it is controlled or released would bring another's button under
 * the pointer.
 */
export function listed(devices: readonly DeviceInfo[], shown: Shown, text: string): DeviceInfo[] {
  const wanted = shown === 'all' && devices.length > FILTER_PAST ? text.trim().toLowerCase() : ''
  const rank = (d: DeviceInfo) => (d.present ? 0 : 1)
  return devices
    .filter((d) => shown === 'all' || d.present)
    .filter((d) => !wanted || d.name.toLowerCase().includes(wanted) || ids(d).includes(wanted))
    .sort((a, b) => rank(a) - rank(b) || byName(a, b))
}

/**
 * What the definition choice shows: the value of the definition driving the
 * device, or of the file chosen that does not load — the built-in one is `''`.
 * `null` when there is nothing to choose.
 */
export function definitionChoice(d: DeviceInfo): string | null {
  if (d.unloadedChoice) return d.unloadedChoice
  if (d.definitions.length < 2) return null
  return d.origin === 'builtIn' ? '' : (d.file ?? '')
}

/** A file of yours, as *Your definitions* lists it. */
export interface YourFile {
  file: string
  /** The device it defines, when it loads. */
  device?: DeviceInfo
  /** Whether it drives that device: the one chosen on its card. */
  inUse: boolean
  /** Why it drives nothing, when it does not load. */
  reason?: string
}

/** Every file of your folder, loaded or not, by name: chosen or not, each is yours to open. */
export function yourFiles(
  devices: readonly DeviceInfo[],
  problems: readonly DefinitionProblem[],
): YourFile[] {
  const loaded = devices.flatMap((d) =>
    d.definitions
      .filter((o) => o.origin === 'yours')
      .map((o) => ({ file: o.file, device: d, inUse: d.origin === 'yours' && d.file === o.file })),
  )
  const broken = problems.map((p) => ({ file: p.file, inUse: false, reason: p.reason }))
  return [...loaded, ...broken].sort((a, b) => a.file.localeCompare(b.file))
}

/** `vid:pid` as the page writes it, four hexadecimal digits each. */
export function ids(d: Pick<DeviceInfo, 'vid' | 'pid'>): string {
  const hex = (n: number) => n.toString(16).padStart(4, '0')
  return `${hex(d.vid)}:${hex(d.pid)}`
}
