/**
 * Effets matériels, et effet posé par cette session.
 *
 * État au niveau du module, comme `useDevice` : passer sur l'écran
 * Périphériques et revenir ne doit pas effacer ce qu'on vient d'appliquer.
 */

import { readonly, ref } from 'vue'

import * as api from '../api/candeo'
import type { DeviceRef, Effect } from '../api/types'

export interface HardwareEffect {
  id: string
  name: string
  /** Ce que l'effet fait, en une phrase. */
  summary: string
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
export const hardwareEffects: readonly HardwareEffect[] = [
  {
    id: 'spectrumCycle',
    name: 'Spectrum Cycle',
    summary: 'Tout le clavier change de teinte ensemble, sans fin.',
    effect: { kind: 'spectrumCycle' },
  },
  {
    id: 'wave',
    name: 'Wave',
    summary: 'Un dégradé traverse le clavier de part en part.',
    effect: { kind: 'wave', direction: WAVE_DIRECTION, speed: WAVE_SPEED },
  },
  {
    id: 'off',
    name: 'Éteint',
    summary: 'Rétroéclairage coupé, sans débrancher quoi que ce soit.',
    effect: { kind: 'off' },
  },
]

/**
 * Ce que cette session a posé, **appareil par appareil**, clé « vid:pid ».
 *
 * Un seul champ global marquerait le même effet actif sur tous les appareils,
 * dans une liste qui décrit ce que fait **un** appareil : ce n'est pas une
 * simplification, c'est une information fausse dès le second clavier.
 */
const posed = ref<Record<string, string>>({})
const applying = ref<string | null>(null)
const error = ref<string | null>(null)

const key = (d: DeviceRef) => `${d.vid}:${d.pid}`

/** Les erreurs remontées par Rust sont déjà lisibles : on les affiche telles quelles. */
function message(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : String(e)
}

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
    applying.value = e.id
    error.value = null
    try {
      await api.setEffect(device, e.effect)
      // Remplacement plutôt que mutation : `readonly()` interdit d'écrire dans
      // l'objet exposé, et la réactivité ne dépend plus de la clé déjà présente.
      posed.value = { ...posed.value, [key(device)]: e.id }
    } catch (err) {
      error.value = message(err)
    } finally {
      applying.value = null
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

  return {
    appliedOn,
    applying: readonly(applying),
    error: readonly(error),
    apply,
    forgetPosed,
  }
}
