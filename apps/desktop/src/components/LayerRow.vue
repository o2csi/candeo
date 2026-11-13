<script setup lang="ts">
/**
 * One row of a target's stack: a layer, or the base under them
 * (`docs/design/layers-ui.md` §6).
 *
 * A button with `aria-pressed`, as a region's card is, named with its place:
 * *Equalizer, Number row · Screen 100%, layer 2 of 3*. Its tools — *Move up*,
 * *Move down*, the grip and *Remove*, 24 × 24 — show on hover and focus and are
 * always in the DOM, so Tab reaches them (§8).
 *
 * Its state is said in words, colour as a second channel: *Stopped on an
 * error* in `--bad`, *Nothing to light here* in `--warn` with the swatch
 * hatched.
 */
import type { RowState } from '../composables/layers'
import { t } from '../i18n'
import EffectSwatch from './EffectSwatch.vue'
import ReorderControls from './ReorderControls.vue'

const props = defineProps<{
  /** The effect it runs, or the one previewed in its place. */
  name: string
  /** `{mask} · {blend} {opacity}`, or *base*. */
  line: string
  swatch: string[]
  state: RowState
  pressed: boolean
  /** What it is read out as, its place included. */
  label: string
  /**
   * The bottom row: it stays there, and is never removed. Its *Move up*, when
   * `canUp`, trades its effect with the layer above (`layers-ui.md` §4.5).
   */
  base?: boolean
  canUp?: boolean
  canDown?: boolean
  /** The bottom layer over a base it can trade with: *Move down* swaps them. */
  swapsDown?: boolean
}>()

const emit = defineEmits<{
  select: []
  up: []
  down: []
  remove: []
  dragstart: []
  dragend: []
}>()

/** The line, unless the state replaces it. */
function shownLine(): string {
  if (props.state === 'failed') return t('layers.failed')
  if (props.state === 'nothing') return t('layers.nothingHere')
  return props.line
}
</script>

<template>
  <div class="lrow" :class="[state, { selected: pressed, base }]">
    <button
      type="button"
      class="pick"
      :aria-pressed="pressed"
      :aria-label="label"
      :title="`${name} · ${shownLine()}`"
      @click="emit('select')"
    >
      <!-- Hatched when the mask lights nothing: the swatch would show colours nobody sees. -->
      <EffectSwatch class="mark" :colors="state === 'nothing' ? [] : swatch" />
      <span class="text">
        <span class="name">{{ name }}</span>
        <span class="line">{{ shownLine() }}</span>
      </span>
    </button>
    <span v-if="!base || canUp" class="tools">
      <ReorderControls
        :can-up="canUp ?? false"
        :can-down="canDown ?? false"
        :up-label="base ? t('layers.swapUpOf', { name }) : t('layers.moveUpOf', { name })"
        :down-label="swapsDown ? t('layers.swapOf', { name }) : t('layers.moveDownOf', { name })"
        :up-title="base ? t('layers.swapUp') : t('layers.moveUp')"
        :down-title="swapsDown ? t('layers.swap') : t('layers.moveDown')"
        :drag-title="t('layers.drag')"
        @up="emit('up')"
        @down="emit('down')"
        @dragstart="emit('dragstart')"
        @dragend="emit('dragend')"
      />
      <button
        v-if="!base"
        type="button"
        class="remove"
        :aria-label="t('layers.removeOf', { name })"
        :title="t('layers.remove')"
        @click="emit('remove')"
      >
        <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round">
          <path d="M3 3l6 6M9 3 3 9" />
        </svg>
      </button>
    </span>
  </div>
</template>

<style scoped>
.lrow {
  position: relative;
  display: flex;
  align-items: center;
  gap: 2px;
  min-height: 32px;
  border: 1px solid transparent;
  border-radius: var(--r-md);
}

/* As a selected region: the accent frame over its soft fill. */
.lrow.selected {
  background: var(--accent-soft);
  border-color: var(--accent);
}

.pick {
  display: flex;
  flex: 1;
  gap: var(--gap-2);
  align-items: center;
  min-width: 0;
  min-height: 32px;
  padding: 3px 6px;
  border-radius: var(--r-md);
  text-align: left;
}

.pick:hover {
  background: var(--raised-2);
}

.lrow.selected .pick:hover {
  background: none;
}

/* Narrower than in the gallery: the row shares the column with its tools. */
.pick .mark {
  width: 20px;
  height: 16px;
  pointer-events: none;
}

.text {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.name,
.line {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.name {
  font-size: 12px;
  font-weight: 500;
}

.line {
  color: var(--text-muted);
  font-size: 11px;
}

.lrow.failed .line {
  color: var(--bad);
}

.lrow.nothing .line {
  color: var(--warn);
}

/*
 * Shown on hover and focus, always there for Tab and screen readers. Laid over
 * the end of the row rather than beside it: the line keeps the row's width
 * the rest of the time, and its whole text is in the row's title.
 */
.tools {
  position: absolute;
  top: 50%;
  right: 2px;
  display: inline-flex;
  align-items: center;
  padding: 0 2px;
  border-radius: var(--r-sm);
  background: var(--raised-2);
  opacity: 0;
  transform: translateY(-50%);
}

.lrow:hover .tools,
.lrow:focus-within .tools {
  opacity: 1;
}

.remove {
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  border-radius: var(--r-sm);
  color: var(--text-muted);
}

.remove:hover {
  color: var(--bad);
  background: var(--raised-2);
}
</style>
