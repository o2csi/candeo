/**
 * Effets matériels, et effet posé par cette session.
 *
 * État au niveau du module, comme `useDevice` : passer sur l'écran
 * Périphériques et revenir ne doit pas effacer ce qu'on vient d'appliquer.
 */

import { readonly, ref } from 'vue'

import * as api from '../api/candeo'
import type { Effect } from '../api/types'

/**
 * Motif d'aperçu de la vignette. Purement visuel — il figure ce que fait
 * l'effet, il ne le calcule pas.
 */
export type Preview = 'cycle' | 'wave' | 'off'

export interface HardwareEffect {
  id: string
  name: string
  /** Ce que l'effet fait, en une phrase. */
  summary: string
  preview: Preview
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
    preview: 'cycle',
    effect: { kind: 'spectrumCycle' },
  },
  {
    id: 'wave',
    name: 'Wave',
    summary: 'Un dégradé traverse le clavier de part en part.',
    preview: 'wave',
    effect: { kind: 'wave', direction: WAVE_DIRECTION, speed: WAVE_SPEED },
  },
  {
    id: 'off',
    name: 'Éteint',
    summary: 'Rétroéclairage coupé, sans débrancher quoi que ce soit.',
    preview: 'off',
    effect: { kind: 'off' },
  },
]

const applied = ref<string | null>(null)
const applying = ref<string | null>(null)
const error = ref<string | null>(null)

/** Les erreurs remontées par Rust sont déjà lisibles : on les affiche telles quelles. */
function message(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : String(e)
}

export function useEffects() {
  /**
   * `applied` dit ce que **cette session** a posé, pas ce que le clavier
   * affiche : le protocole relevé sait écrire un effet, pas le relire. Rien
   * n'est donc marqué au lancement — une supposition serait pire que le vide,
   * puisqu'elle se tromperait silencieusement après un redémarrage.
   */
  async function apply(e: HardwareEffect) {
    applying.value = e.id
    error.value = null
    try {
      await api.setEffect(e.effect)
      applied.value = e.id
    } catch (err) {
      error.value = message(err)
    } finally {
      applying.value = null
    }
  }

  return {
    applied: readonly(applied),
    applying: readonly(applying),
    error: readonly(error),
    apply,
  }
}
