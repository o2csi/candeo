/**
 * État du périphérique, partagé par toute l'application.
 *
 * État au niveau du module plutôt qu'un magasin dédié : il y a un périphérique
 * connecté à la fois et une poignée de champs. Ajouter Pinia pour ça serait
 * une dépendance de plus sans rien résoudre.
 */

import { readonly, ref } from 'vue'

import * as api from '../api/candeo'
import type { DeviceInfo, LayoutInfo } from '../api/types'

const devices = ref<DeviceInfo[]>([])
const layout = ref<LayoutInfo | null>(null)
const busy = ref(false)
const error = ref<string | null>(null)

/** Les erreurs remontées par Rust sont déjà lisibles : on les affiche telles quelles. */
function message(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : String(e)
}

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
  async function refresh() {
    const list = await run(api.listDevices)
    if (list) devices.value = list
  }

  async function connect(device: DeviceInfo) {
    const info = await run(() => api.connect(device.vid, device.pid))
    if (info) layout.value = info
    return info
  }

  async function disconnect() {
    await run(api.disconnect)
    layout.value = null
  }

  /** Rétablit l'état après un rechargement à chaud de l'interface. */
  async function restore() {
    if (await run(api.isConnected)) {
      const info = await run(api.getLayout)
      if (info) layout.value = info
    }
    await refresh()
  }

  return {
    devices: readonly(devices),
    layout: readonly(layout),
    busy: readonly(busy),
    error: readonly(error),
    refresh,
    connect,
    disconnect,
    restore,
  }
}
