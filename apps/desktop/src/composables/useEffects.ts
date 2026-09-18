/**
 * Effets matériels, et effet posé par cette session.
 *
 * État au niveau du module, comme `useDevice` : passer sur l'écran
 * Périphériques et revenir ne doit pas effacer ce qu'on vient d'appliquer.
 */

import { readonly, ref } from 'vue'

import * as api from '../api/candeo'
import { message } from '../api/journal'
import type { DeviceRef } from '../api/types'
import { t } from '../i18n'

export interface HardwareEffect {
  id: string
  /** Its name, as the gallery shows it. */
  name: string
  /** One sentence on what it does, when we know. */
  summary: string
}

/**
 * The one effect every device offers: a firmware that draws nothing of its own
 * still goes dark, on a black frame.
 */
export const OFF = 'hardware:off'

/** What an Alienware keyboard's own effects are called, before their number. */
const ALIENWARE = 'hardware:m18-'

/**
 * What each of that keyboard's kinds shows, watched one by one on the hardware.
 * The protocol says nothing about it: the numbers answer, the names come from
 * eyes on a keyboard.
 */
const ALIENWARE_NAMES = {
  '01': 'static',
  '02': 'pulse',
  '03': 'wave',
  '08': 'breathing',
  '09': 'morph',
  '0a': 'scan',
  '0e': 'spectrumCycle',
} as const

/**
 * A firmware effect's name comes from its id, not from a catalogue written here.
 *
 * **One family's modes are not another's.** The layout says which ids a device
 * runs; this only puts words on them. The Alienware keyboard's sixteen kinds are
 * named after their number until someone says what each one shows — which takes
 * eyes on a keyboard, not code.
 */
export function named(id: string): HardwareEffect {
  if (id === OFF) {
    return {
      id,
      name: t('effects.hardwareEffects.off.name'),
      summary: t('effects.hardwareEffects.off.summary'),
    }
  }
  if (id === 'hardware:spectrumCycle' || id === 'hardware:wave') {
    const key = id === 'hardware:wave' ? 'wave' : 'spectrumCycle'
    return {
      id,
      name: t(`effects.hardwareEffects.${key}.name`),
      summary: t(`effects.hardwareEffects.${key}.summary`),
    }
  }
  const kind = id.slice(ALIENWARE.length) as keyof typeof ALIENWARE_NAMES
  const alienware = ALIENWARE_NAMES[kind]
  if (id.startsWith(ALIENWARE) && alienware) {
    return {
      id,
      name: t(`effects.hardwareEffects.${alienware}.name`),
      summary: t(`effects.hardwareEffects.${alienware}.summary`),
    }
  }
  // An id nobody named: shown as it is rather than invented. The gallery only
  // offers what a layout lists, so this is the sign of a layout gone ahead of
  // the words for it.
  return { id, name: id, summary: '' }
}

/**
 * Those a given device runs, **Off included**.
 *
 * Without a layout — no device chosen — only *Off* comes back: naming what an
 * unknown device runs would be inventing it.
 */
export function hardwareEffectsFor(
  layout: { firmwareEffects?: string[] } | null | undefined,
): readonly HardwareEffect[] {
  return [...(layout?.firmwareEffects ?? []), OFF].map(named)
}

/**
 * Ce que cette session a posé, **appareil par appareil**, clé « vid:pid ».
 *
 * Un seul champ global marquerait le même effet actif sur tous les appareils,
 * dans une liste qui décrit ce que fait **un** appareil : ce n'est pas une
 * simplification, c'est une information fausse dès le second clavier.
 */
const posed = ref<Record<string, string>>({})
const error = ref<string | null>(null)

const key = (d: DeviceRef) => `${d.vid}:${d.pid}`

/** Les erreurs remontées par Rust sont déjà lisibles : on les affiche telles quelles. */
export function useEffects() {
  /**
   * Pose un effet matériel **sur un appareil**.
   *
   * Ce qu'on retient dit ce que **cette session** a posé, pas ce que le clavier
   * affiche : le protocole relevé sait écrire un effet, pas le relire. Rien
   * n'est donc marqué au lancement — une supposition serait pire que le vide,
   * puisqu'elle se tromperait silencieusement après un redémarrage.
   */
  async function apply(device: DeviceRef, e: HardwareEffect) {
    error.value = null
    try {
      await api.setEffect(device, e.id)
      // Remplacement plutôt que mutation : `readonly()` interdit d'écrire dans
      // l'objet exposé, et la réactivité ne dépend plus de la clé déjà présente.
      posed.value = { ...posed.value, [key(device)]: e.id }
    } catch (err) {
      error.value = message(err)
    }
  }

  /** L'effet matériel que cette session a posé sur cet appareil, s'il y en a un. */
  function appliedOn(device: DeviceRef | null): string | null {
    return device ? (posed.value[key(device)] ?? null) : null
  }

  /**
   * Oublie ce que cette session avait posé, sur tous les appareils.
   *
   * Appelé après la remise à zéro de la configuration, qui éteint le
   * rétroéclairage et referme les appareils : ce qu'on retenait ici décrivait un
   * mode que plus aucun clavier n'affiche. Le garder ferait marquer « actif » un
   * effet éteint — et cette table ne se corrige pas d'elle-même, puisque le
   * protocole relevé sait écrire un effet, pas le relire.
   */
  function forgetPosed() {
    posed.value = {}
  }

  /** Closes the message: what failed is read, and the screen goes back to work. */
  function dismissError() {
    error.value = null
  }

  return {
    appliedOn,
    error: readonly(error),
    dismissError,
    apply,
    forgetPosed,
  }
}
