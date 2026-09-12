/**
 * Les images du moteur, telles qu'elles partent vers le clavier.
 *
 * Ce module remplace les images de démonstration : plus rien n'est calculé ici.
 * Le simulateur affiche ce que le moteur produit, octet pour octet — c'est ce
 * qui fait que l'aperçu **est** la production, et non sa ressemblance
 * (`docs/design/studio.md` §3).
 *
 * ## Le canal ne s'ouvre qu'une fois l'effet lancé
 *
 * `subscribe_frames` dépose le canal dans l'état de la boucle en cours. Sans
 * boucle, il n'y a pas d'état où le déposer, et `start_effect` en crée un neuf
 * à chaque démarrage : il faut donc se réabonner **après** chaque lancement,
 * pas une fois pour toutes à l'ouverture de l'éditeur.
 *
 * ## Se désabonner n'arrête pas l'effet
 *
 * Quitter l'éditeur libère le canal et le flux s'arrête ; la boucle, elle,
 * continue d'alimenter le clavier. C'est le comportement voulu : un effet
 * tourne fenêtre fermée.
 *
 * ## Et `prefers-reduced-motion` ?
 *
 * Rien n'est neutralisé ici, et c'est délibéré : ce n'est pas une décoration
 * qui bouge toute seule, c'est le sujet de l'écran, et il ne s'anime qu'après
 * que l'utilisateur a lancé l'effet. Les animations d'interface, elles, restent
 * couvertes par `styles/base.css`.
 */

import { computed, onBeforeUnmount, shallowRef } from 'vue'

import { subscribeFrames } from '../api/candeo'
import type { Rgb } from '../api/types'
import type { LayoutView } from './layout'

const BLACK: Rgb = [0, 0, 0]

/**
 * Une image noire complète.
 *
 * `frameLen` positions, pas `keys.length` : une image couvre toute la matrice,
 * trous compris. Un tableau vide ferait un simulateur tout aussi noir, mais
 * violerait l'invariant que `layoutProblems` vérifie — autant le respecter dès
 * la première image.
 */
function dark(layout: LayoutView | null): readonly Rgb[] {
  return layout ? new Array<Rgb>(layout.frameLen).fill(BLACK) : []
}

/**
 * Une image brute vers les triplets qu'attend le simulateur.
 *
 * `shallowRef` côté appelant : l'image est remplacée en bloc, jamais modifiée
 * en place. Un `ref` profond envelopperait 132 triplets dans autant de
 * mandataires réactifs, soixante fois par seconde.
 */
function colors(bytes: Uint8Array): Rgb[] {
  const out = new Array<Rgb>(Math.floor(bytes.length / 3))
  for (let i = 0; i < out.length; i++) {
    out[i] = [bytes[i * 3], bytes[i * 3 + 1], bytes[i * 3 + 2]]
  }
  return out
}

export function useEngineFrames(layout: () => LayoutView | null) {
  const received = shallowRef<readonly Rgb[] | null>(null)

  /**
   * Tant que rien n'est arrivé, une image noire du bon gabarit — lequel vient
   * du Rust, donc plus tard que le premier rendu.
   */
  const frame = computed<readonly Rgb[]>(() => received.value ?? dark(layout()))

  /** Ce qui ferme le canal. `null` quand personne n'écoute. */
  let release: (() => void) | null = null
  /** Faux dès la destruction : l'abonnement est asynchrone, il peut aboutir après. */
  let alive = true

  /**
   * S'abonne au flux. Réabonnable : le moteur ne retient qu'un canal, le
   * nouveau remplace l'ancien.
   *
   * On ne se **désabonne pas d'abord** : ce serait deux commandes en vol dont
   * l'ordre d'arrivée n'est pas garanti, et un désabonnement qui arriverait le
   * second effacerait le canal qu'on vient d'ouvrir — le simulateur resterait
   * figé sans que rien ne le signale.
   */
  async function listen(): Promise<void> {
    const close = await subscribeFrames((bytes) => {
      received.value = colors(bytes)
    })
    // Le composant a pu disparaître pendant l'aller-retour. Fermer tout de
    // suite plutôt que de laisser un canal alimenter une vue détruite.
    if (alive) release = close
    else close()
  }

  /**
   * Ferme le canal. La dernière image reste affichée : c'est encore celle que
   * le clavier éclaire — arrêter la boucle n'éteint pas les LED.
   */
  function stop(): void {
    release?.()
    release = null
  }

  onBeforeUnmount(() => {
    alive = false
    stop()
  })

  return { frame, listen, stop }
}
