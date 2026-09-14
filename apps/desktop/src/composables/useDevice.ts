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
 * doit donc toujours savoir lequel elle vise — d'où {@link current}.
 *
 * La colonne des appareils le désigne explicitement, par {@link select} ; les
 * écrans qui n'ont pas de colonne — l'éditeur — reprennent ce même choix. C'est
 * ce qui fait qu'ouvrir l'éditeur depuis la troisième colonne travaille bien sur
 * l'appareil qu'on regardait, sans que l'éditeur ait à poser la question.
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
 * Ce que la colonne des appareils a désigné. `null` tant que personne n'a choisi.
 *
 * Au niveau du module, comme le reste : passer à l'éditeur et revenir ne doit
 * pas ramener la sélection au premier de la liste.
 */
const chosen = ref<DeviceRef | null>(null)

const same = (a: DeviceRef, b: DeviceRef) => a.vid === b.vid && a.pid === b.pid

/**
 * L'appareil visé par les commandes du moteur.
 *
 * Le choix explicite d'abord — mais seulement s'il désigne encore un appareil
 * connu : un gabarit peut disparaître de la liste entre deux relectures, et
 * viser un appareil qui n'existe plus ferait échouer chaque commande sans que
 * rien n'explique pourquoi.
 *
 * À défaut, celui qui est ouvert ; puis le premier branché ; puis le premier
 * gabarit connu. Ce dernier repli n'est pas un pis-aller : un effet se lance et
 * se prévisualise sans clavier branché, et il faut bien un gabarit pour le
 * dessiner.
 */
const current = computed<DeviceRef | null>(() => {
  const list = devices.value
  const voulu = chosen.value
  if (voulu && list.some((d) => same(d, voulu))) return voulu

  const choisi = list.find((d) => d.open) ?? list.find((d) => d.present) ?? list[0]
  return choisi ? { vid: choisi.vid, pid: choisi.pid } : null
})

/** Les erreurs remontées par Rust sont déjà lisibles : on les affiche telles quelles. */
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
   * Désigne l'appareil qu'on configure. C'est le geste de la première colonne.
   *
   * Ne touche à rien côté Rust : aucun appareil n'est ouvert ni refermé, on dit
   * seulement lequel les écrans visent. Ouvrir, c'est `adopt`.
   */
  function select(device: DeviceRef | null) {
    chosen.value = device
  }

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
   *
   * Le gabarit retenu est celui de l'appareil **désigné** quand il est ouvert,
   * et à défaut celui du premier ouvert. Sans cette préférence, l'éditeur
   * dessinerait un appareil pendant que les commandes en viseraient un autre : un
   * seul gabarit est connu aujourd'hui, les deux se confondent, mais l'accord ne
   * doit pas tenir à cette coïncidence.
   */
  async function refresh() {
    const list = await run(api.listDevices)
    if (!list) return
    devices.value = list

    const voulu = chosen.value
    const ouvert =
      (voulu ? list.find((d) => same(d, voulu) && d.open) : undefined) ?? list.find((d) => d.open)
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
