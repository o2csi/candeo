// What the Devices page shows, in which order (`docs/design/device-sdk.md` §9).
// Pure, so which device goes where is tested without a DOM.

import type { DefinitionProblem } from '../api/candeo'
import type { DeviceInfo, UnsupportedDevice } from '../api/types'

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
 * What *All* filters by, lowercase, or `''` for nothing: only once the page
 * lists more than {@link FILTER_PAST} devices, `count`, known or not.
 */
function filtering(count: number, shown: Shown, text: string): string {
  return shown === 'all' && count > FILTER_PAST ? text.trim().toLowerCase() : ''
}

/**
 * The page's one list: the devices plugged in first; under *All*, those known
 * but not plugged in after them, and last those only an unverified definition
 * describes (#300), filtered by a text — a name, a maker, or its `vid:pid` —
 * once there are enough to need it. By name, not by state: a card moving when
 * it is controlled or released would bring another's button under the pointer.
 *
 * `count` is every device the page lists, those not supported yet included.
 */
export function listed(
  devices: readonly DeviceInfo[],
  shown: Shown,
  text: string,
  count = devices.length,
): DeviceInfo[] {
  const wanted = filtering(count, shown, text)
  const rank = (d: DeviceInfo) => (d.present ? 0 : d.origin === 'unverified' ? 2 : 1)
  return devices
    .filter((d) => shown === 'all' || d.present)
    .filter((d) => !wanted || d.name.toLowerCase().includes(wanted) || ids(d).includes(wanted))
    .sort((a, b) => rank(a) - rank(b) || byName(a, b))
}

/**
 * The devices detected but not supported yet (#327), listed after the known
 * ones: plugged in, so under *Plugged in* as under *All*, where the same text
 * filters them by maker or `vid:pid`. By maker, those naming none last.
 */
export function notSupported(
  devices: readonly UnsupportedDevice[],
  shown: Shown,
  text: string,
  count: number,
): UnsupportedDevice[] {
  const wanted = filtering(count, shown, text)
  const maker = (d: UnsupportedDevice) => d.maker?.toLowerCase() ?? ''
  return devices
    .filter((d) => !wanted || maker(d).includes(wanted) || ids(d).includes(wanted))
    .sort(
      (a, b) =>
        Number(a.maker === null) - Number(b.maker === null) ||
        maker(a).localeCompare(maker(b)) ||
        ids(a).localeCompare(ids(b)),
    )
}

/** The usage pages of its collections, each once, as `0xff00`, in the order listed. */
export function usagePages(d: UnsupportedDevice): string[] {
  const pages = d.collections.map((c) => `0x${c.usagePage.toString(16).padStart(4, '0')}`)
  return [...new Set(pages)]
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
