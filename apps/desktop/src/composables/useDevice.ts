/**
 * Device state, shared by the whole application.
 *
 * Module-level state rather than a dedicated store: a list and a handful of
 * fields. Adding Pinia for that would be one more dependency without solving
 * anything.
 *
 * ## There is no implicit device any more
 *
 * On the Rust side, each device carries its handle, its loop and its effect:
 * every command that acts on a device takes one ([`DeviceRef`]). The interface
 * must therefore always know which one it targets — hence {@link current}.
 *
 * The devices column designates it explicitly, through {@link select}; the
 * screens that have no column — the editor — take up that same choice. It is
 * what makes opening the editor from the third column work on the device that
 * was being looked at, without the editor having to ask.
 */

import { computed, readonly, ref } from 'vue'

import * as api from '../api/candeo'
import { message } from '../api/journal'
import type { DeviceInfo, DeviceRef, LayoutInfo } from '../api/types'

const devices = ref<DeviceInfo[]>([])
const layout = ref<LayoutInfo | null>(null)
const busy = ref(false)
const error = ref<string | null>(null)

/**
 * What the devices column designated. `null` as long as nobody has chosen.
 *
 * At module level, like the rest: moving to the editor and coming back must not
 * bring the selection back to the first in the list.
 */
const chosen = ref<DeviceRef | null>(null)

const same = (a: DeviceRef, b: DeviceRef) => a.vid === b.vid && a.pid === b.pid

/**
 * The device targeted by the engine's commands.
 *
 * The explicit choice first — but only if it still designates a known device: a
 * layout can disappear from the list between two reads, and targeting a device
 * that no longer exists would make every command fail without anything
 * explaining why.
 *
 * Failing that, the one that is open; then the first plugged in; then the first
 * known layout. This last fallback is not a makeshift: an effect starts and is
 * previewed without a keyboard plugged in, and a layout is needed to draw it.
 */
const current = computed<DeviceRef | null>(() => {
  const list = devices.value
  const wanted = chosen.value
  if (wanted && list.some((d) => same(d, wanted))) return wanted

  const picked = list.find((d) => d.open) ?? list.find((d) => d.present) ?? list[0]
  return picked ? { vid: picked.vid, pid: picked.pid } : null
})

/** Errors raised by Rust are already readable: they are shown as they are. */
async function run<T>(task: () => Promise<T>): Promise<T | null> {
  busy.value = true
  error.value = null
  try {
    return await task()
  } catch (e) {
    error.value = message(e)
    return null
  } finally {
    busy.value = false
  }
}

export function useDevice() {
  /**
   * Designates the device being configured. It is the first column's gesture.
   *
   * Touches nothing on the Rust side: no device is opened or closed, it only
   * says which one the screens target. Opening is `adopt`.
   */
  function select(device: DeviceRef | null) {
    chosen.value = device
  }

  /**
   * Reads the list again, then the layout of the open device.
   *
   * The layout cannot be derived from the list: it carries the geometry of the
   * 106 keys, which `list_devices` has no reason to carry for every device,
   * plugged in or not.
   *
   * Everything goes through here — adopt, ignore, connect come down to it. Each
   * device's state and error message come from Rust, which alone knows what
   * opening produced; patching them up on the spot would invent a second truth.
   *
   * The layout kept is that of the **designated** device when it is open, and
   * failing that that of the first open one. Without this preference, the
   * editor would draw one device while the commands targeted another: a single
   * layout is known today, the two coincide, but the agreement must not rest on
   * that coincidence.
   */
  async function refresh() {
    const list = await run(api.listDevices)
    if (!list) return
    devices.value = list

    const wanted = chosen.value
    const open =
      (wanted ? list.find((d) => same(d, wanted) && d.open) : undefined) ?? list.find((d) => d.open)
    if (!open) {
      layout.value = null
      return
    }
    const info = await run(() => api.getLayout({ vid: open.vid, pid: open.pid }))
    if (info) layout.value = info
  }

  /** One-off opening, without deciding anything. */
  async function connect(device: DeviceInfo) {
    const info = await run(() => api.connect(device.vid, device.pid))
    await refresh()
    return info
  }

  /** Decides to control this device — once, and for the times after. */
  async function adopt(device: DeviceInfo) {
    // `null`: adopted but unplugged. It is not an error.
    const info = await run(() => api.adoptDevice(device.vid, device.pid))
    await refresh()
    return info
  }

  async function ignore(device: DeviceInfo) {
    await run(() => api.ignoreDevice(device.vid, device.pid))
    await refresh()
  }

  /** Closes **one** device. The others stay open. */
  async function disconnect(device: DeviceInfo) {
    await run(() => api.disconnect({ vid: device.vid, pid: device.pid }))
    await refresh()
  }

  /** Restores the state after a hot reload of the interface. */
  const restore = refresh

  return {
    devices: readonly(devices),
    current,
    select,
    layout: readonly(layout),
    busy: readonly(busy),
    error: readonly(error),
    refresh,
    connect,
    adopt,
    ignore,
    disconnect,
    restore,
  }
}
