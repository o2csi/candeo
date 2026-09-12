<script setup lang="ts">
/**
 * Simulateur de clavier — afficheur pur.
 *
 * **Entrée** : un gabarit et une image de `frameLen` couleurs. **Sortie** :
 * rien. Il ne calcule aucune couleur, il montre celles qu'on lui donne. C'est
 * ce qui garantit que l'aperçu **est** la production : le simulateur
 * n'interprète pas le code d'un effet, il affiche le résultat du moteur, la
 * même image que celle qui part vers le clavier (`docs/design/studio.md` §3).
 *
 * ## SVG, et pas une grille CSS ni un canevas
 *
 * **Pas une grille CSS** : la disposition ISO n'est pas une grille. Les largeurs
 * valent 1, 1,25, 1,5, 1,75, 2, 2,75 ou 6,25 u, les blocs sont décalés de
 * quarts d'unité (pavé de navigation à 15,25 u, pavé numérique à 18,5 u), deux
 * touches débordent sur deux rangées et l'Entrée en L est faite de deux
 * rectangles de largeurs différentes. Exprimer cela en CSS demanderait une
 * grille au quart d'unité — 90 colonnes sur 26 rangées — dont chaque case
 * n'existerait que pour servir de dénominateur. Le rectangle est la donnée :
 * autant le dessiner.
 *
 * **Pas un canevas** : la charge invoquée pour le justifier n'existe pas. 106
 * rectangles à 60 Hz, c'est 106 écritures d'attribut `fill` par trame — le
 * `<text>` ne change que de couleur, et sa taille qu'au redimensionnement. Le
 * canevas coûterait en échange le redessin intégral du texte à chaque trame, la
 * gestion manuelle du rapport de pixels, et la perte de tout ce que le DOM donne
 * ici gratuitement : texte réellement mis en page par le moteur, mise à
 * l'échelle exacte par `viewBox`, et un libellé accessible.
 *
 * Le `viewBox` fait tout le travail d'adaptation : le dessin est décrit une fois
 * en unités de pas de clavier, `preserveAspectRatio` interdit la déformation, et
 * l'élément prend la largeur qu'on lui laisse.
 */

import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'

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
/** Marge intérieure avant le libellé. */
const INSET = 0.08
/** Hauteur de libellé au repos, en unités de pas. */
const LABEL_U = 0.32
/**
 * Largeur moyenne d'un glyphe, en cadratins, pour la police d'interface.
 * Approximation assumée : mesurer 106 chaînes à chaque redimensionnement
 * coûterait autant de retours de mise en page pour ajuster un demi-pixel.
 */
const GLYPH_EM = 0.55
/** En deçà, un libellé n'est plus lisible : on le retire plutôt que de l'empiler. */
const LABEL_FLOOR_PX = 7.5

const BLACK: Rgb = [0, 0, 0]

const size = computed(() => extent(props.layout.keys))

/**
 * Pixels par unité de pas, mesurés sur le rendu.
 *
 * Nécessaire parce que le SVG se met à l'échelle : une taille de police en
 * unités de pas reste proportionnellement identique à toute taille, alors que
 * la lisibilité, elle, se juge en pixels. C'est la seule grandeur du composant
 * qui ne vienne pas du gabarit.
 */
const box = ref({ w: 0, h: 0 })
const draw = ref<SVGSVGElement | null>(null)
let watcher: ResizeObserver | null = null

// `min` des deux rapports : c'est exactement ce que calcule
// `preserveAspectRatio="meet"`. Dérivé plutôt qu'affecté dans l'observateur,
// pour rester juste si c'est le **gabarit** qui change sans redimensionnement.
const pxPerU = computed(() => Math.min(box.value.w / size.value.w, box.value.h / size.value.h))

onMounted(() => {
  watcher = new ResizeObserver(([entry]) => {
    box.value = { w: entry.contentRect.width, h: entry.contentRect.height }
  })
  if (draw.value) watcher.observe(draw.value)
})

onBeforeUnmount(() => {
  watcher?.disconnect()
  watcher = null
})

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

/** Composante sRVB linéarisée, pour le calcul de luminance. */
function linear(v: number): number {
  const s = byte(v) / 255
  return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
}

/**
 * Encre du libellé : noir ou blanc, celui des deux qui contraste le mieux.
 *
 * Calculé, pas deviné — une touche peut être blanche comme noire, et la couleur
 * change soixante fois par seconde. On compare les deux rapports de contraste
 * WCAG : `(L + 0,05) / 0,05` sur noir contre `1,05 / (L + 0,05)` sur blanc,
 * égalité en `L ≈ 0,179`. Ni l'un ni l'autre ne peut venir des jetons : un
 * jeton suit le thème, alors que le fond à contraster est une couleur émise.
 */
function ink(c: Rgb): string {
  const l = 0.2126 * linear(c[0]) + 0.7152 * linear(c[1]) + 0.0722 * linear(c[2])
  return (l + 0.05) * (l + 0.05) >= 0.05 * 1.05 ? '#000' : '#fff'
}

/**
 * Taille du libellé, ou 0 pour ne pas l'afficher.
 *
 * Rétrécir puis renoncer, plutôt que déborder ou tronquer : un nom qui dépasse
 * de son capuchon salit tout le dessin, et un nom coupé se lit de travers. En
 * panneau étroit, les noms longs s'effacent d'eux-mêmes et les lettres restent.
 */
function label(name: string, w: number): number {
  if (!name) return 0
  const room = w - 2 * GAP - 2 * INSET
  const u = Math.min(LABEL_U, room / (GLYPH_EM * name.length))
  return u * pxPerU.value >= LABEL_FLOOR_PX ? u : 0
}

interface Cap {
  index: number
  name: string
  x: number
  y: number
  w: number
  h: number
  cx: number
  cy: number
  fill: string
  ink: string
  font: number
}

/**
 * La couleur d'une touche se lit à `frame[key.index]`.
 *
 * **Pas** au rang de la touche dans `keys` : l'image couvre les `frameLen`
 * cases de la matrice — 132 —, dont seules 106 portent une LED. Confondre les
 * deux est le piège de ce matériel (`docs/api/commands.md`, « Gabarit »).
 */
const caps = computed<Cap[]>(() =>
  props.layout.keys.map((k) => {
    const c = props.frame[k.index] ?? BLACK
    return {
      index: k.index,
      name: k.name,
      x: k.x + GAP,
      y: k.y + GAP,
      w: Math.max(0, k.w - 2 * GAP),
      h: Math.max(0, k.h - 2 * GAP),
      cx: k.x + k.w / 2,
      cy: k.y + k.h / 2,
      fill: css(c),
      ink: ink(c),
      font: label(k.name, k.w),
    }
  }),
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
          `${problems.length} incohérence(s) gabarit ↔ image : ${problems.join(' · ')}`,
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
      ref="draw"
      class="draw"
      :viewBox="`0 0 ${size.w} ${size.h}`"
      preserveAspectRatio="xMidYMid meet"
      role="img"
      :aria-label="`Aperçu de ${layout.name} : ${layout.keys.length} touches éclairées`"
    >
      <g v-for="cap in caps" :key="cap.index">
        <rect
          :x="cap.x"
          :y="cap.y"
          :width="cap.w"
          :height="cap.h"
          :rx="RADIUS"
          :fill="cap.fill"
        />
        <text
          v-if="cap.font"
          :x="cap.cx"
          :y="cap.cy"
          :font-size="cap.font"
          :fill="cap.ink"
          text-anchor="middle"
          dominant-baseline="central"
        >{{ cap.name }}</text>
      </g>
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
