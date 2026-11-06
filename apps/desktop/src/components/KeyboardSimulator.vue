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
 *
 * ## Choosing keys
 *
 * With `selecting`, the drawing becomes a mask's selection
 * (`docs/design/layers-ui.md` §4.3): each key a toggle button named by its
 * legend, the chosen ones outlined in the accent with a check, the others
 * faded — never the fading alone (§8, 1.4.1). A click toggles a key, a drag
 * paints from the first key's new state; arrows move between keys, Space
 * toggles, Enter is *Done*, Escape *Cancel*. The parent keeps the selection:
 * this only says what was asked.
 *
 * Out of a selection, `outlined` draws the selected layer's mask in dashes
 * over the final frame.
 */

import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'

import { error } from '../api/journal'
import { t } from '../i18n'
import type { KeyShape, Rgb } from '../api/types'
import { ARROWS, nearestKey, startKey } from '../keyboard/keySelection'
import { extent, focusBounds, layoutProblems, type LayoutView } from '../keyboard/layout'
import { archPath, archStroke } from '../keyboard/shapes'

const props = defineProps<{
  layout: LayoutView
  /** `layout.frameLen` colors, **not** `layout.keys.length`. */
  frame: readonly Rgb[]
  /**
   * The positions of a split device's region: drawn in full colour, the others
   * faded with their real colours, the region's bounds and centre drawn over
   * them (`docs/design/regions.md` §3). Absent: the whole device, as it is.
   */
  focus?: readonly number[]
  /** The region's name, for what the drawing is said to show. */
  focusName?: string
  /** The positions chosen while keys are being chosen; absent or `null` otherwise. */
  selecting?: readonly number[] | null
  /** The selected layer's lights, drawn in dashes over the frame. */
  outlined?: readonly number[] | null
}>()

const emit = defineEmits<{
  /** A click, or Space: this key in or out. */
  toggle: [position: number]
  /** A drag went over this key: in or out, as the key it started on went. */
  paint: [position: number, on: boolean]
  /** Enter: *Done*. */
  done: []
  /** Escape: *Cancel*. */
  cancel: []
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

const size = computed(() => extent(props.layout.keys, props.layout.outline))

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
  /** Absent: a keycap. Otherwise the light's own shape (§4 of `studio.md`). */
  shape?: KeyShape
  /** For an arch: its path and stroke. */
  path?: string
  stroke?: number
  /** Outside the region looked at: drawn faded. */
  faded: boolean
}

/** The region's lights, `null` for the whole device. */
const focused = computed(() => (props.focus ? new Set(props.focus) : null))

/**
 * What the region's effect measures as its device: the box around its lights
 * and its centre, where a radial wave starts.
 */
const bounds = computed(() => (props.focus ? focusBounds(props.layout.keys, props.focus) : null))

/**
 * How far the bounds' dashes sit outside the lights, in pitch units: on their
 * edge, the dashes would run along the keycaps and read as a light.
 */
const BOUNDS_MARGIN = 0.12
/** Half the centre's cross, in pitch units. */
const CROSS = 0.3

/** What the drawing shows, for whoever cannot see it. */
const label = computed(() => {
  const l = props.layout
  if (props.focus && props.focusName !== undefined) {
    const n = props.focus.length
    const params = { layout: l.name, region: props.focusName, n, total: l.keys.length }
    return l.lights === 'zones'
      ? t('effects.simulatorRegion.zones', params, n)
      : t('effects.simulatorRegion.keys', params, n)
  }
  return l.lights === 'zones'
    ? t('effects.simulatorZones', { layout: l.name, zones: l.keys.length })
    : t('effects.simulator', { layout: l.name, keys: l.keys.length })
})

/**
 * A key's color is read at `frame[key.index]`.
 *
 * **Not** at the key's position in `keys`: the frame covers the matrix's
 * `frameLen` cells (132), of which only 106 carry an LED. Mixing the two up is
 * this hardware's trap (`docs/api/commands.md`, "Layout").
 */
const caps = computed<Cap[]>(() =>
  props.layout.keys.map((k) => {
    const fill = css(props.frame[k.index] ?? BLACK)
    const faded = focused.value !== null && !focused.value.has(k.index)
    // A shaped light is drawn as its rectangle says, without the seam between
    // keycaps: the two halves of a ring meet edge to edge.
    if (k.shape === 'archUp' || k.shape === 'archDown') {
      const path = archPath(k, k.shape === 'archUp')
      return {
        index: k.index,
        x: k.x,
        y: k.y,
        w: k.w,
        h: k.h,
        fill,
        shape: k.shape,
        path,
        stroke: archStroke(k),
        faded,
      }
    }
    if (k.shape === 'disc') {
      return { index: k.index, x: k.x, y: k.y, w: k.w, h: k.h, fill, shape: k.shape, faded }
    }
    return {
      index: k.index,
      x: k.x + GAP,
      y: k.y + GAP,
      w: Math.max(0, k.w - 2 * GAP),
      h: Math.max(0, k.h - 2 * GAP),
      fill,
      faded,
    }
  }),
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
        error(
          'simulator',
          `layout and frame disagree (${problems.length}): ${problems.join(' · ')}`,
        )
      }
    },
    { immediate: true },
  )
}

// ------------------------------------------------------------ choosing keys

const board = ref<SVGSVGElement | null>(null)

/** The chosen positions, as a set to look keys up in; `null` out of a selection. */
const chosen = computed(() => (props.selecting ? new Set(props.selecting) : null))

/** The keys an outline goes around: the chosen ones while choosing, the mask's otherwise. */
const outlines = computed(() => {
  const wanted = chosen.value ?? (props.outlined ? new Set(props.outlined) : null)
  if (!wanted) return []
  return props.layout.keys.filter((k) => wanted.has(k.index))
})

/** How far an outline sits inside its key's rectangle, in pitch units: on the seam, it would read as a light. */
const OUTLINE_INSET = 0.02

/** The key holding the board's one tab stop; arrows move it. */
const current = ref<number | null>(null)

/** A key's name as it is read out: its legend, else its place in the layout. */
function legend(index: number): string {
  const keys = props.layout.keys
  const at = keys.findIndex((k) => k.index === index)
  const label = keys[at]?.label
  if (label) return label
  return props.layout.lights === 'zones'
    ? t('layers.select.zone', { n: at + 1 })
    : t('layers.select.key', { n: at + 1 })
}

/** Dark on a bright light, white on a dark one: the check stays readable on any colour. */
function ink(fill: string): string {
  const v = parseInt(fill.slice(1), 16)
  const luminance = 0.2126 * ((v >> 16) & 255) + 0.7152 * ((v >> 8) & 255) + 0.0722 * (v & 255)
  return luminance > 128 ? '#14131a' : '#ffffff'
}

function focusKey(index: number | null): void {
  if (index === null) return
  current.value = index
  void nextTick(() => {
    board.value?.querySelector<SVGGElement>(`[data-key="${index}"]`)?.focus()
  })
}

// A selection starting takes the focus to its first key: the keyboard path goes
// on in the drawing, where the keys are.
watch(
  () => props.selecting != null,
  (on, was) => {
    if (on && !was) focusKey(startKey(props.layout.keys, props.selecting ?? []))
  },
  { immediate: true },
)

/** What a drag paints with, `null` while none runs. */
const painting = ref<boolean | null>(null)

function endPaint(): void {
  painting.value = null
}
window.addEventListener('pointerup', endPaint)
onBeforeUnmount(() => window.removeEventListener('pointerup', endPaint))

function onPointerDown(e: PointerEvent, index: number): void {
  if (!chosen.value || e.button !== 0) return
  // No text selection nor drag image: the drag paints.
  e.preventDefault()
  painting.value = !chosen.value.has(index)
  focusKey(index)
  emit('toggle', index)
}

function onPointerEnter(index: number): void {
  if (!chosen.value || painting.value === null) return
  if (chosen.value.has(index) !== painting.value) emit('paint', index, painting.value)
}

function onKeyDown(e: KeyboardEvent, index: number): void {
  if (!chosen.value) return
  const direction = ARROWS[e.key]
  if (direction) {
    e.preventDefault()
    const next = nearestKey(props.layout.keys, index, direction)
    if (next !== null) focusKey(next)
  } else if (e.key === ' ') {
    e.preventDefault()
    emit('toggle', index)
  } else if (e.key === 'Enter') {
    e.preventDefault()
    emit('done')
  } else if (e.key === 'Escape') {
    e.preventDefault()
    emit('cancel')
  }
}
</script>

<template>
  <div class="board" :class="{ choosing: chosen !== null }">
    <svg
      ref="board"
      class="draw"
      :viewBox="`0 0 ${size.w} ${size.h}`"
      preserveAspectRatio="xMidYMid meet"
      :role="chosen ? 'group' : 'img'"
      :aria-label="chosen ? t('layers.select.board') : label"
    >
      <!-- What lights nothing: drawn first, faint, and themed like the chassis. -->
      <rect
        v-for="(part, i) in layout.outline ?? []"
        :key="`part-${i}`"
        class="part"
        :x="part.x"
        :y="part.y"
        :width="part.w"
        :height="part.h"
        :rx="part.r"
        vector-effect="non-scaling-stroke"
      />
      <!--
        Each light in a group: a plain drawing, or while choosing, a toggle
        button named by its legend, the board holding one tab stop.
      -->
      <g
        v-for="cap in caps"
        :key="cap.index"
        :data-key="cap.index"
        :class="{ key: chosen !== null, off: chosen !== null && !chosen.has(cap.index) }"
        :role="chosen ? 'button' : undefined"
        :tabindex="chosen ? (current === cap.index ? 0 : -1) : undefined"
        :aria-pressed="chosen ? chosen.has(cap.index) : undefined"
        :aria-label="chosen ? legend(cap.index) : undefined"
        @pointerdown="onPointerDown($event, cap.index)"
        @pointerenter="onPointerEnter(cap.index)"
        @keydown="onKeyDown($event, cap.index)"
        @focus="current = cap.index"
      >
        <path
          v-if="cap.path"
          :class="{ faded: cap.faded }"
          :d="cap.path"
          fill="none"
          :stroke="cap.fill"
          :stroke-width="cap.stroke"
        />
        <ellipse
          v-else-if="cap.shape === 'disc'"
          :class="{ faded: cap.faded }"
          :cx="cap.x + cap.w / 2"
          :cy="cap.y + cap.h / 2"
          :rx="cap.w / 2"
          :ry="cap.h / 2"
          :fill="cap.fill"
        />
        <rect
          v-else
          :class="{ faded: cap.faded }"
          :x="cap.x"
          :y="cap.y"
          :width="cap.w"
          :height="cap.h"
          :rx="RADIUS"
          :fill="cap.fill"
        />
        <!-- Chosen says itself in a shape too, not by the outline's colour alone. -->
        <path
          v-if="chosen?.has(cap.index)"
          class="check"
          :d="`M ${cap.x + cap.w - 0.36} ${cap.y + cap.h - 0.24} l 0.09 0.09 l 0.17 -0.2`"
          :stroke="ink(cap.fill)"
          vector-effect="non-scaling-stroke"
        />
      </g>
      <!-- Interface over the lights, in the accent: solid for the chosen keys, dashed for a mask. -->
      <rect
        v-for="k in outlines"
        :key="`outline-${k.index}`"
        class="outline"
        :class="{ dashed: chosen === null }"
        :x="k.x + OUTLINE_INSET"
        :y="k.y + OUTLINE_INSET"
        :width="Math.max(0, k.w - 2 * OUTLINE_INSET)"
        :height="Math.max(0, k.h - 2 * OUTLINE_INSET)"
        :rx="RADIUS"
        vector-effect="non-scaling-stroke"
      />
      <!-- Drawn over the lights: interface, in the accent, like a selection. -->
      <template v-if="bounds">
        <rect
          class="bounds"
          :x="bounds.x - BOUNDS_MARGIN"
          :y="bounds.y - BOUNDS_MARGIN"
          :width="bounds.w + 2 * BOUNDS_MARGIN"
          :height="bounds.h + 2 * BOUNDS_MARGIN"
          :rx="RADIUS * 2"
          vector-effect="non-scaling-stroke"
        />
        <path
          class="centre"
          :d="`M ${bounds.cx - CROSS} ${bounds.cy} h ${2 * CROSS} M ${bounds.cx} ${bounds.cy - CROSS} v ${2 * CROSS}`"
          vector-effect="non-scaling-stroke"
        />
      </template>
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

  /* A region's bounds sit just outside its lights, past the drawing's edge for
     a light on it: the board's padding has room for them. */
  overflow: visible;
}

/* A lid, a base, a port: interface, so it follows the theme, and faint enough
   that the lights stay what the eye reads. */
.part {
  fill: none;
  stroke: var(--line-strong);
  stroke-width: 1;
}

/* The other regions: their real colours, faded, so the region reads as what
   is looked at and the device stays whole around it. */
.faded {
  opacity: 0.3;
}

/* Choosing keys: the board is where the hand works, a drag paints rather than scrolls. */
.choosing .draw {
  touch-action: none;
}

.key {
  cursor: pointer;
  outline: none;
}

/* Not chosen: faded; the outline and the check say which are, not the fading alone. */
.key.off {
  opacity: 0.35;
}

/* The focus, visible on any light: the text colour around the key. */
.key:focus-visible > :first-child {
  stroke: var(--text);
  stroke-width: 0.08;
}

.check {
  fill: none;
  stroke-width: 2;
  stroke-linecap: round;
  stroke-linejoin: round;
  pointer-events: none;
}

.outline {
  fill: none;
  stroke: var(--accent);
  stroke-width: 2;
  pointer-events: none;
}

.outline.dashed {
  stroke-width: 1.5;
  stroke-dasharray: 4 3;
}

/* What the region's effect sees as its device: interface, themed. */
.bounds {
  fill: none;
  stroke: var(--accent);
  stroke-width: 1.5;
  stroke-dasharray: 5 3;
}

.centre {
  fill: none;
  stroke: var(--accent);
  stroke-width: 1.5;
}
</style>
