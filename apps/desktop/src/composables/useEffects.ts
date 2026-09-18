/**
 * Effets matériels, et effet posé par cette session.
 *
 * État au niveau du module, comme `useDevice` : passer sur l'écran
 * Périphériques et revenir ne doit pas effacer ce qu'on vient d'appliquer.
 */

import { readonly, ref } from 'vue'

import * as api from '../api/candeo'
import { message } from '../api/journal'
import type { DeviceRef, Effect } from '../api/types'

export interface HardwareEffect {
  id: string
  /** Its name and one-sentence summary, under `effects.hardwareEffects.<key>`. */
  key: 'spectrumCycle' | 'wave' | 'off'
  effect: Effect
}

/**
 * Seules valeurs relevées à la capture, cf. `docs/protocol/deathstalker-v2-pro.md`.
 * La plage réelle de ces deux paramètres fait partie des questions ouvertes du
 * relevé : offrir un réglage reviendrait à inventer une échelle et à laisser
 * l'utilisateur découvrir tout seul quelles valeurs ne font rien.
 */
const WAVE_DIRECTION = 0x02
const WAVE_SPEED = 0x28

/**
 * Le catalogue est écrit ici, et non demandé au Rust : ces modes sont figés par
 * le micrologiciel, pas découverts. Une commande de listage n'apprendrait rien
 * et ajouterait un aller-retour au lancement.
 *
 * `custom` n'y figure pas volontairement. C'est le mode piloté par l'hôte : le
 * poser sans moteur pour pousser des images laisserait le clavier sur sa
 * dernière image, c'est-à-dire un effet qui ne fait rien, présenté comme un
 * effet. Il apparaîtra avec le moteur (issue #6).
 */
/**
 * Those a given device runs, **Off included**: a firmware that draws nothing of
 * its own still goes dark, on a black frame.
 *
 * Without a layout — no device chosen — the whole catalogue comes back: the list
 * is then a presentation, not a command.
 */
export function hardwareEffectsFor(
  layout: { firmwareEffects?: string[] } | null | undefined,
): readonly HardwareEffect[] {
  const offered = layout?.firmwareEffects
  if (!offered) return hardwareEffects
  return hardwareEffects.filter((h) => h.key === 'off' || offered.includes(h.id))
}

export const hardwareEffects: readonly HardwareEffect[] = [
  { id: 'hardware:spectrumCycle', key: 'spectrumCycle', effect: { kind: 'spectrumCycle' } },
  {
    id: 'hardware:wave',
    key: 'wave',
    effect: { kind: 'wave', direction: WAVE_DIRECTION, speed: WAVE_SPEED },
  },
  { id: 'hardware:off', key: 'off', effect: { kind: 'off' } },
]

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
      await api.setEffect(device, e.effect)
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
