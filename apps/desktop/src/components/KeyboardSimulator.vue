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
import { t } from '../i18n'
import type { Rgb } from '../api/types'
import { extent, layoutProblems, type LayoutView } from '../keyboard/layout'

const props = defineProps<{
  layout: LayoutView
  /** `layout.frameLen` colors, **not** `layout.keys.length`. */
  frame: readonly Rgb[]
}>()

/**
 * Clearance between two keycaps, per side, in pitch units.
 *
 * Applied uniformly, without exception: the two arms of the L-shaped Enter are
 * therefore separated by the same seam as the other keys. That is the price of
 * rendering with no special case, and it is not a bad price: the ISO Enter
 * really carries **two** LEDs (indices 57 and 79), and the vertical gradient
 * seen on it on the device would not show if it were painted as one block.
 */
const GAP = 0.04
/** A keycap's corner radius, in pitch units. */
const RADIUS = 0.1

const BLACK: Rgb = [0, 0, 0]

const size = computed(() => extent(props.layout.keys))

function byte(v: number): number {
  return v < 0 ? 0 : v > 255 ? 255 : v | 0
}

/**
 * Emitted color, written literally.
 *
 * The only exception allowed to the style tokens: this is not interface, it is
 * what the keyboard lights. No theme applies to it: a keyboard that is off is
 * black under a light theme too. `EffectSwatch`'s color swatch falls under the
 * same exception, but it writes nothing: it receives its colors from Rust.
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
 * A key's color is read at `frame[key.index]`.
 *
 * **Not** at the key's position in `keys`: the frame covers the matrix's
 * `frameLen` cells (132), of which only 106 carry an LED. Mixing the two up is
 * this hardware's trap (`docs/api/commands.md`, "Layout").
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
 * Consistency check, in development only.
 *
 * A geometry error breaks no test: it only shows to the eye. But everything to
 * do with indexing can be checked: that is what `layoutProblems` does. Triggered
 * when the layout changes and when the frame **size** changes, not on every
 * frame: the invariant is about the frame's shape, not its content.
 *
 * `import.meta.env.DEV` is replaced at compile time: none of this remains in the
 * shipped application.
 */
if (import.meta.env.DEV) {
  watch(
    [() => props.layout, () => props.frame.length],
    () => {
      const problems = layoutProblems(props.layout, props.frame)
      if (problems.length) {
        // A single line, inconsistencies included: the log is a file that gets
        // read back, and a failure split over N lines gets lost between two
        // frames. The count stays first: it is what one looks for first.
        erreur(
          'simulator',
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
      :aria-label="
        layout.lights === 'zones'
          ? t('effects.simulatorZones', { layout: layout.name, zones: layout.keys.length })
          : t('effects.simulator', { layout: layout.name, keys: layout.keys.length })
      "
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
 * The chassis, however, is interface: it does not glow, so it follows the
 * tokens and the theme. Only the keycaps carry a literal color.
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
 * `height: auto` is enough to keep the proportions: an SVG with a `viewBox` has
 * an intrinsic ratio. `max-height` takes over when height is what is short:
 * `preserveAspectRatio` then centers the drawing instead of stretching it.
 */
.draw {
  width: 100%;
  height: auto;
  max-height: 100%;
}
</style>
