<script setup lang="ts">
/**
 * The grip to drag a row by, and *Move up* / *Move down*: how rules and layers
 * are put in order (`docs/design/layers-ui.md` §4.5, §6).
 *
 * Buttons as well as the drag, so that the order changes without a pointer and
 * without a path to follow (WCAG 2.1.1, 2.5.7). The parent says what can move
 * and moves it: the drop target is its row, not this.
 *
 * The labels come from the parent, which knows what the row is: *Move up* for
 * a rule, *Move "Equalizer" up* for a layer.
 */

defineProps<{
  canUp: boolean
  canDown: boolean
  upLabel: string
  downLabel: string
  /** The buttons' tooltips, when the label names the row: *Move up*. Absent: the label. */
  upTitle?: string
  downTitle?: string
  /** The grip's tooltip: *Drag to change the order*. */
  dragTitle: string
}>()

const emit = defineEmits<{
  up: []
  down: []
  /** The grip is being dragged: the parent remembers which row. */
  dragstart: [event: DragEvent]
  dragend: []
}>()

function onDragStart(e: DragEvent): void {
  // Some engines start no drag without data; the row's place is the parent's.
  e.dataTransfer?.setData('text/plain', '')
  if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move'
  emit('dragstart', e)
}
</script>

<template>
  <span class="reorder">
    <!-- The pointer's way only: the buttons beside it are the keyboard's. -->
    <span
      class="grip"
      draggable="true"
      :title="dragTitle"
      aria-hidden="true"
      @dragstart="onDragStart"
      @dragend="emit('dragend')"
      >⋮⋮</span
    >
    <button
      type="button"
      class="move"
      :disabled="!canUp"
      :aria-label="upLabel"
      :title="upTitle ?? upLabel"
      @click="emit('up')"
    >
      <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        <path d="M6 10V2M2.5 5.5 6 2l3.5 3.5" />
      </svg>
    </button>
    <button
      type="button"
      class="move"
      :disabled="!canDown"
      :aria-label="downLabel"
      :title="downTitle ?? downLabel"
      @click="emit('down')"
    >
      <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        <path d="M6 2v8M2.5 6.5 6 10l3.5-3.5" />
      </svg>
    </button>
  </span>
</template>

<style scoped>
.reorder {
  display: inline-flex;
  flex: none;
  align-items: center;
  gap: 2px;
}

.grip {
  padding: 0 2px;
  color: var(--text-faint);
  cursor: grab;
  user-select: none;
  letter-spacing: -2px;
}

/* 24 px at least: a target's minimum (WCAG 2.5.8, `layers-ui.md` §8). */
.move {
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  border-radius: var(--r-sm);
  color: var(--text-muted);
}

.move:hover:not(:disabled) {
  color: var(--text);
  background: var(--raised-2);
}

.move:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}
</style>
