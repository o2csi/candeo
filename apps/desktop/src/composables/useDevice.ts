/**
 * État des périphériques, partagé par toute l'application.
 *
 * État au niveau du module plutôt qu'un magasin dédié : une liste et une poignée
 * de champs. Ajouter Pinia pour ça serait une dépendance de plus sans rien
 * résoudre.
 *
 * ## Il n'y a plus d'appareil implicite
 *
 * Côté Rust, chaque appareil porte sa poignée, sa boucle et son effet : toute
 * commande qui agit sur un appareil en prend un ([`DeviceRef`]). L'interface
 * doit donc toujours savoir lequel elle vise — d'où {@link current}, qui le
 * désigne sans rien demander tant qu'un seul appareil est en jeu. L'écran à
 * trois colonnes (issue #27) rendra ce choix explicite ; d'ici là, rien à
 * cliquer de plus.
 */

import { computed, readonly, ref } from 'vue'

import * as api from '../api/candeo'
import type { DeviceInfo, DeviceRef, LayoutInfo } from '../api/types'

const devices = ref<DeviceInfo[]>([])
const layout = ref<LayoutInfo | null>(null)
const busy = ref(false)
const error = ref<string | null>(null)

/**
 * L'appareil sur lequel agissent les écrans qui n'en désignent pas un.
 *
 * Celui qui est ouvert ; à défaut le premier branché ; à défaut le premier
 * gabarit connu. Le dernier repli n'est pas un pis-aller : un effet se lance et
 * se prévisualise sans clavier branché, et il faut bien un gabarit pour le
 * dessiner.
 */
const current = computed<DeviceRef | null>(() => {
  const list = devices.value
  const choisi = list.find((d) => d.open) ?? list.find((d) => d.present) ?? list[0]
  return choisi ? { vid: choisi.vid, pid: choisi.pid } : null
})

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
  /**
   * Relit la liste, puis le gabarit de l'appareil ouvert.
   *
   * Le gabarit ne se déduit pas de la liste : il porte la géométrie des 106
   * touches, que `list_devices` n'a aucune raison de transporter pour chaque
   * appareil branché ou non.
   *
   * Tout passe par ici — adopter, ignorer, connecter s'y ramènent. L'état et le
   * message d'erreur de chaque appareil viennent du Rust, qui seul sait ce que
   * l'ouverture a donné ; les rafistoler sur place inventerait une seconde
   * vérité.
   */
  async function refresh() {
    const list = await run(api.listDevices)
    if (!list) return
    devices.value = list

    const ouvert = list.find((d) => d.open)
    if (!ouvert) {
      layout.value = null
      return
    }
    const info = await run(() => api.getLayout({ vid: ouvert.vid, pid: ouvert.pid }))
    if (info) layout.value = info
  }

  /** Ouverture ponctuelle, sans rien décider. */
  async function connect(device: DeviceInfo) {
    const info = await run(() => api.connect(device.vid, device.pid))
    await refresh()
    return info
  }

  /** Décide de piloter cet appareil — une fois, et pour les fois suivantes. */
  async function adopt(device: DeviceInfo) {
    // `null` : adopté mais débranché. Ce n'est pas une erreur.
    const info = await run(() => api.adoptDevice(device.vid, device.pid))
    await refresh()
    return info
  }

  async function ignore(device: DeviceInfo) {
    await run(() => api.ignoreDevice(device.vid, device.pid))
    await refresh()
  }

  /** Referme **un** appareil. Les autres restent ouverts. */
  async function disconnect(device: DeviceInfo) {
    await run(() => api.disconnect({ vid: device.vid, pid: device.pid }))
    await refresh()
  }

  /** Rétablit l'état après un rechargement à chaud de l'interface. */
  const restore = refresh

  return {
    devices: readonly(devices),
    current,
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
