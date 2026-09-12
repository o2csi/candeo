/**
 * Images de **démonstration** — code temporaire, à supprimer.
 *
 * Le moteur d'effets (issues #5 à #7) n'existe pas : aucune image réelle ne
 * circule encore. Ce module en fabrique pour que le simulateur ait quelque
 * chose à montrer et pour qu'on puisse le mettre au point. Il disparaîtra avec
 * l'arrivée du canal `subscribe_frames`, et le simulateur ne changera pas d'une
 * ligne — c'est justement ce que garantit son contrat : il ne calcule rien.
 *
 * Ce qu'il sert à vérifier, au passage : le motif est calculé pour les
 * **`frameLen`** cases de la matrice, trous compris, et non pour les
 * `keys.length` touches. Un simulateur qui lirait la couleur au rang de la
 * touche dans `keys` plutôt qu'à son `index` afficherait un motif décalé et
 * disloqué dès la deuxième rangée — le décalage se voit, il ne se devine pas.
 */

import { onBeforeUnmount, onMounted, shallowRef, watch } from 'vue'

import type { Rgb } from '../api/types'
import type { LayoutView } from './layout'

/** Partie fractionnaire, valable aussi pour les négatifs. */
function fract(x: number): number {
  return x - Math.floor(x)
}

/** TSV → RVB, composantes d'entrée dans [0, 1]. */
function hsv(h: number, s: number, v: number): Rgb {
  const sector = Math.floor(h * 6) % 6
  const f = h * 6 - Math.floor(h * 6)
  const p = v * (1 - s)
  const q = v * (1 - f * s)
  const t = v * (1 - (1 - f) * s)

  const rgb: readonly [number, number, number] =
    sector === 0
      ? [v, t, p]
      : sector === 1
        ? [q, v, p]
        : sector === 2
          ? [p, v, t]
          : sector === 3
            ? [p, q, v]
            : sector === 4
              ? [t, p, v]
              : [v, p, q]

  return [Math.round(rgb[0] * 255), Math.round(rgb[1] * 255), Math.round(rgb[2] * 255)]
}

/**
 * Une image de démonstration : arc-en-ciel diagonal, traversé par une onde de
 * luminosité.
 *
 * Deux composantes plutôt qu'une seule : une teinte qui dérive suffirait à
 * remplir l'écran, mais pas à juger un effet **spatial** — c'est l'onde, qui se
 * déplace sur la largeur, qui montre que le dessin respecte la disposition.
 *
 * Le motif est fonction de la position dans la matrice, déduite du rang dans
 * l'image : la case `i` est en ligne `i / cols`, colonne `i % cols`. C'est la
 * définition même d'une image, pas une hypothèse sur ce clavier.
 */
export function demoFrame(layout: LayoutView, seconds: number): Rgb[] {
  const { cols, rows, frameLen } = layout
  const frame = new Array<Rgb>(frameLen)

  for (let i = 0; i < frameLen; i++) {
    const col = i % cols
    const row = (i - col) / cols
    const u = cols > 1 ? col / (cols - 1) : 0
    const v = rows > 1 ? row / (rows - 1) : 0

    const hue = fract(u * 0.8 + v * 0.2 - seconds * 0.15)
    const band = 0.5 + 0.5 * Math.sin(2 * Math.PI * (u * 1.5 - seconds * 0.5))

    // Plancher à 0,28 : un creux d'onde complètement noir ferait croire à des
    // touches éteintes, alors que la démonstration allume tout.
    frame[i] = hsv(hue, 1, 0.28 + 0.72 * band)
  }

  return frame
}

/**
 * Alimente le simulateur en images de démonstration.
 *
 * `shallowRef` : l'image est remplacée en bloc à chaque trame et n'est jamais
 * modifiée en place. Un `ref` profond envelopperait les 132 triplets dans
 * autant de mandataires réactifs, soixante fois par seconde, pour une
 * granularité dont personne n'a l'usage.
 */
export function useDemoFrames(layout: () => LayoutView) {
  // Première image posée tout de suite : sans elle, le premier rendu montrerait
  // un clavier sans couleurs le temps d'une trame.
  const frame = shallowRef<readonly Rgb[]>(demoFrame(layout(), 0))

  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)')
  let raf = 0
  let origin = 0

  function tick(now: number) {
    raf = requestAnimationFrame(tick)
    frame.value = demoFrame(layout(), (now - origin) / 1000)
  }

  function stop() {
    if (raf) cancelAnimationFrame(raf)
    raf = 0
  }

  /**
   * `base.css` neutralise les animations CSS sous `prefers-reduced-motion`,
   * mais une boucle `requestAnimationFrame` lui échappe entièrement : il faut
   * la couper ici. On laisse alors une image fixe — le simulateur reste
   * lisible, il ne bouge plus.
   */
  function apply() {
    stop()
    if (reduced.matches) {
      frame.value = demoFrame(layout(), 0)
      return
    }
    origin = performance.now()
    raf = requestAnimationFrame(tick)
  }

  // Un changement de gabarit — connexion, déconnexion — n'attend pas la
  // prochaine trame quand il n'y en a plus.
  watch(layout, () => {
    if (!raf) frame.value = demoFrame(layout(), 0)
  })

  onMounted(() => {
    apply()
    reduced.addEventListener('change', apply)
  })

  onBeforeUnmount(() => {
    stop()
    reduced.removeEventListener('change', apply)
  })

  return { frame }
}
