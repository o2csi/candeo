/**
 * The layouts of devices that declare regions, read once per definition: what
 * their pictograms are drawn from (`docs/design/regions.md` §2), open or not.
 *
 * Module-level, as `useDevice` is: *Devices* and *Effects* draw the same
 * pictograms, and going from one to the other must not read them again.
 */

import { ref } from 'vue'

import * as api from '../api/candeo'
import { warn, message } from '../api/journal'
import type { DeviceInfo, LayoutInfo } from '../api/types'

/** Read layouts, by device and definition: choosing another definition reads it again. */
const read = ref<Record<string, LayoutInfo>>({})
/** What is being read, so that two screens do not ask twice. */
const asking = new Set<string>()

type Described = Pick<DeviceInfo, 'vid' | 'pid' | 'origin' | 'file'>

const keyOf = (d: Described) => `${d.vid}:${d.pid}/${d.origin}:${d.file ?? ''}`

export function useLayouts() {
  /** The layout read for this device's definition in force, `null` until it is. */
  function layoutOf(d: Described): LayoutInfo | null {
    return read.value[keyOf(d)] ?? null
  }

  /**
   * Reads the layouts not read yet. A failure leaves a blank pictogram, which
   * the region's name beside it makes up for: worth a line in the log, not on
   * screen.
   */
  async function ensure(devices: readonly Described[]): Promise<void> {
    await Promise.all(
      devices.map(async (d) => {
        const k = keyOf(d)
        if (read.value[k] || asking.has(k)) return
        asking.add(k)
        try {
          const layout = await api.getLayout({ vid: d.vid, pid: d.pid })
          read.value = { ...read.value, [k]: layout }
        } catch (e) {
          warn('layouts', `layout of ${d.vid}:${d.pid} not read: ${message(e, 'en')}`, e)
        } finally {
          asking.delete(k)
        }
      }),
    )
  }

  return { layoutOf, ensure }
}
