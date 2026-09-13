/**
 * Les images du moteur, telles qu'elles partent vers le clavier.
 *
 * Ce module remplace les images de démonstration : plus rien n'est calculé ici.
 * Le simulateur affiche ce que le moteur produit, octet pour octet — c'est ce
 * qui fait que l'aperçu **est** la production, et non sa ressemblance
 * (`docs/design/studio.md` §3).
 *
 * ## Le canal ne s'ouvre qu'une fois l'effet lancé, et sur **un** appareil
 *
 * `subscribe_frames` dépose le canal dans l'état de la boucle en cours, celle
 * de l'appareil visé. Sans boucle, il n'y a pas d'état où le déposer, et
 * `start_effect` en crée un neuf à chaque démarrage : il faut donc se réabonner
 * **après** chaque lancement, pas une fois pour toutes à l'ouverture de
 * l'éditeur.
 *
 * Chaque appareil a sa boucle et son canal : le simulateur suit celui qu'on a
 * sélectionné, changer de sélection ferme un canal et en ouvre un autre.
 *
 * ## Deux sources, jamais les deux à la fois
 *
 * Le moteur produit deux flux distincts : celui d'un **appareil**, qui est
 * exactement ce qui part vers ses LED, et celui de l'**aperçu**, qui ne part
 * nulle part. Le simulateur n'en dessine qu'un, et l'écran dit lequel — sans
 * quoi on regarderait un aperçu en croyant voir son clavier, ce que l'issue #63
 * refuse. C'est ce que tient {@link Source} : basculer de l'un à l'autre ferme
 * le précédent, il n'y a jamais deux canaux ouverts sur ce composant.
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

import { subscribeFrames, subscribePreviewFrames } from '../api/candeo'
import type { DeviceRef, Rgb } from '../api/types'
import type { LayoutView } from './layout'

const BLACK: Rgb = [0, 0, 0]

/**
 * D'où viennent les images affichées : un appareil, ou l'aperçu.
 *
 * `'preview'` plutôt qu'un second drapeau à côté du `DeviceRef` : les deux
 * s'excluent, et un type qui le dit vaut mieux qu'une paire de variables dont
 * une combinaison sur quatre n'a pas de sens.
 */
type Source = DeviceRef | 'preview'

function sameSource(a: Source, b: Source): boolean {
  if (a === 'preview' || b === 'preview') return a === b
  return a.vid === b.vid && a.pid === b.pid
}

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
  /** Ce à quoi ce canal est abonné, pour savoir quand il faut le fermer. */
  let source: Source | null = null
  /** Faux dès la destruction : l'abonnement est asynchrone, il peut aboutir après. */
  let alive = true

  /**
   * S'abonne au flux d'une source. Réabonnable : le moteur ne retient qu'un
   * canal **par boucle**, le nouveau remplace l'ancien.
   *
   * Sur la même source, on ne se **désabonne pas d'abord** : ce serait deux
   * commandes en vol dont l'ordre d'arrivée n'est pas garanti, et un
   * désabonnement qui arriverait le second effacerait le canal qu'on vient
   * d'ouvrir — le simulateur resterait figé sans que rien ne le signale.
   *
   * Changer de source, en revanche, exige de fermer l'ancienne : le moteur ne
   * remplacerait pas un canal posé sur une autre boucle, et les deux flux
   * alimenteraient le même simulateur.
   */
  async function subscribe(voulue: Source): Promise<void> {
    if (source !== null && !sameSource(source, voulue)) stop()

    source = voulue
    const close =
      voulue === 'preview'
        ? await subscribePreviewFrames((bytes) => {
            received.value = colors(bytes)
          })
        : await subscribeFrames(voulue, (bytes) => {
            received.value = colors(bytes)
          })
    // Le composant a pu disparaître pendant l'aller-retour. Fermer tout de
    // suite plutôt que de laisser un canal alimenter une vue détruite.
    if (alive) release = close
    else close()
  }

  /** Les images qui partent vers **cet appareil**, octet pour octet. */
  function listen(device: DeviceRef): Promise<void> {
    return subscribe(device)
  }

  /** Les images de l'aperçu, qui ne partent nulle part. */
  function listenPreview(): Promise<void> {
    return subscribe('preview')
  }

  /**
   * Ferme le canal. La dernière image reste affichée : c'est encore celle que
   * le clavier éclaire — arrêter la boucle n'éteint pas les LED.
   */
  function stop(): void {
    release?.()
    release = null
    source = null
  }

  onBeforeUnmount(() => {
    alive = false
    stop()
  })

  return { frame, listen, listenPreview, stop }
}
