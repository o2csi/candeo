<script setup lang="ts">
/**
 * Keyboard simulator: a pure display.
 *
 * **In**: a layout and a frame of `frameLen` colors. **Out**: nothing. It
 * computes no color, it shows the ones it is given. That is what makes the
 * preview **be** production: the simulator does not interpret an effect's code,
 * it shows the engine's result, the same frame that goes to the keyboard
 * (`docs/design/studio.md` §3).
 *
 * ## SVG, not a CSS grid or a canvas
 *
 * **Not a CSS grid**: the ISO arrangement is not a grid. Widths are 1, 1.25,
 * 1.5, 1.75, 2, 2.75 or 6.25 u, blocks are offset by quarter units (navigation
 * cluster at 15.25 u, keypad at 18.5 u), two keys span two rows and the L-shaped
 * Enter is two rectangles of different widths. A quarter-unit grid of 90 columns
 * by 26 rows would exist only as a denominator. The rectangle is the data: draw
 * it.
 *
 * **Not a canvas**: the load that would justify one does not exist. 106
 * rectangles at 30 Hz are 106 `fill` attribute writes per frame. A canvas would
 * cost manual pixel-ratio handling, and lose the exact scaling `viewBox` gives
 * and the accessible label.
 *
 * The `viewBox` does all the fitting: the drawing is described once in keyboard
 * pitch units, `preserveAspectRatio` prevents distortion, and the element takes
 * the width it is given.
 *
 * ## No legends
 *
 * Keys are drawn without their legend. Legends change with the layout variant —
 * the ones in the layout are this keyboard's AZERTY — and none fits a keycap at
 * the size the gallery gives the preview: shrinking them until they were dropped
 * left some keys labelled and others blank. What an effect works with is the
 * arrangement, and that is what is drawn.
 */

import { computed, watch } from 'vue'

import { erreur } from '../api/journal'
import type { Rgb } from '../api/types'
import { extent, layoutProblems, type LayoutView } from '../keyboard/layout'

const props = defineProps<{
  layout: LayoutView
  /** `layout.frameLen` couleurs — **pas** `layout.keys.length`. */
  frame: readonly Rgb[]
}>()

/**
 * Jeu entre deux capuchons, par côté, en unités de pas.
 *
 * Appliqué uniformément, sans exception : les deux bras de l'Entrée en L sont
 * donc séparés par la même couture que les autres touches. C'est le prix d'un
 * rendu sans cas particulier, et ce n'est pas un mauvais prix — l'Entrée ISO
 * porte réellement **deux** LED (index 57 et 79), et le dégradé vertical qu'on
 * y voit sur l'appareil n'apparaîtrait pas si on la peignait d'un bloc.
 */
const GAP = 0.04
/** Arrondi d'un capuchon, en unités de pas. */
const RADIUS = 0.1

const BLACK: Rgb = [0, 0, 0]

const size = computed(() => extent(props.layout.keys))

function byte(v: number): number {
  return v < 0 ? 0 : v > 255 ? 255 : v | 0
}

/**
 * Couleur émise, écrite en clair.
 *
 * Seule entorse admise aux jetons de style : ce n'est pas de l'interface, c'est
 * ce que le clavier éclaire. Aucun thème ne s'y applique — un clavier éteint est
 * noir sous un thème clair aussi. Le repère de couleurs d'`EffectSwatch` relève
 * de la même exception, mais lui n'écrit rien : il reçoit ses couleurs du Rust.
 */
function css(c: Rgb): string {
  return `#${((1 << 24) | (byte(c[0]) << 16) | (byte(c[1]) << 8) | byte(c[2])).toString(16).slice(1)}`
}

interface Cap {
  index: number
  x: number
  y: number
  w: number
  h: number
  fill: string
}

/**
 * La couleur d'une touche se lit à `frame[key.index]`.
 *
 * **Pas** au rang de la touche dans `keys` : l'image couvre les `frameLen`
 * cases de la matrice — 132 —, dont seules 106 portent une LED. Confondre les
 * deux est le piège de ce matériel (`docs/api/commands.md`, « Gabarit »).
 */
const caps = computed<Cap[]>(() =>
  props.layout.keys.map((k) => ({
    index: k.index,
    x: k.x + GAP,
    y: k.y + GAP,
    w: Math.max(0, k.w - 2 * GAP),
    h: Math.max(0, k.h - 2 * GAP),
    fill: css(props.frame[k.index] ?? BLACK),
  })),
)

/**
 * Contrôle de cohérence, en développement seulement.
 *
 * Une erreur de géométrie ne casse aucun test — elle ne se voit qu'à l'œil.
 * Mais tout ce qui touche à l'indexation, lui, se vérifie : c'est ce que fait
 * `layoutProblems`. Déclenché au changement de gabarit et au changement de
 * **taille** d'image, pas à chaque trame : l'invariant porte sur la forme de
 * l'image, pas sur son contenu.
 *
 * `import.meta.env.DEV` est remplacé à la compilation : rien de tout ceci ne
 * subsiste dans l'application livrée.
 */
if (import.meta.env.DEV) {
  watch(
    [() => props.layout, () => props.frame.length],
    () => {
      const problems = layoutProblems(props.layout, props.frame)
      if (problems.length) {
        // Une seule ligne, incohérences comprises : le journal est un fichier
        // qu'on relit, et une panne éclatée sur N lignes se perd entre deux
        // images. Le compte reste en tête — c'est ce qu'on cherche d'abord.
        erreur(
          'simulateur',
          `layout and frame disagree (${problems.length}): ${problems.join(' · ')}`,
        )
      }
    },
    { immediate: true },
  )
}
</script>

<template>
  <div class="board">
    <svg
      class="draw"
      :viewBox="`0 0 ${size.w} ${size.h}`"
      preserveAspectRatio="xMidYMid meet"
      role="img"
      :aria-label="`Aperçu de ${layout.name} : ${layout.keys.length} touches éclairées`"
    >
      <rect
        v-for="cap in caps"
        :key="cap.index"
        :x="cap.x"
        :y="cap.y"
        :width="cap.w"
        :height="cap.h"
        :rx="RADIUS"
        :fill="cap.fill"
      />
    </svg>
  </div>
</template>

<style scoped>
/*
 * Le châssis, lui, est de l'interface : il ne rayonne pas, il suit donc les
 * jetons et le thème. Seuls les capuchons portent une couleur en clair.
 */
.board {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  padding: var(--gap-3);
  background: var(--raised);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
}

/*
 * `height: auto` suffit à tenir les proportions : un SVG à `viewBox` a un
 * rapport intrinsèque. `max-height` cède la main quand c'est la hauteur qui
 * manque — `preserveAspectRatio` centre alors le dessin au lieu de l'étirer.
 */
.draw {
  width: 100%;
  height: auto;
  max-height: 100%;
}
</style>
