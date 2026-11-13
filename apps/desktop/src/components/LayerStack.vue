<script setup lang="ts">
/**
 * The selected target's layers, under its card in the devices column
 * (`docs/design/layers-ui.md` §2, §6): *Add a layer* on top, the layers top
 * first, the base last.
 *
 * A group named *Layers of {target}*. Selecting a row targets it, as selecting
 * a region does: it writes nothing (§8, 3.2.2). Moving a row keeps the focus on
 * it, for the buttons to be pressed again (2.1.1); removing one leaves *Layer
 * removed · Undo* in its place until the next change (2.2.1).
 */
import { computed, nextTick, ref } from 'vue'

import { sameSelection, type Selection, type StackRowView } from '../composables/layers'
import { t } from '../i18n'
import LayerRow from './LayerRow.vue'

const props = defineProps<{
  /** The device or region the stack is over: *Layers of {target}*. */
  target: string
  /** Top first, the removed layer's placeholder where it stood. */
  rows: ReadonlyArray<StackRowView | { kind: 'removed' }>
  base: { name: string; line: string; swatch: string[] }
  selection: Selection
  /** The add row's words while it is selected: *New layer · Rain*. */
  newLabel: string
  /** Eight effects already: *Add a layer* becomes the sentence. */
  full: boolean
  /** A hardware effect is the base: the layers wait (§4.6). */
  waiting: boolean
  /** One of Candeo's effects runs as the base: a layer can trade places with it (§4.5). */
  baseSwappable: boolean
}>()

const emit = defineEmits<{
  select: [selection: Selection]
  /** `delta` +1 moves towards the top. */
  move: [id: string, delta: 1 | -1]
  drop: [id: string, onto: string]
  /** Layer `id` and the base trade effects. */
  swap: [id: string, follow: 'base' | 'layer']
  remove: [id: string]
  undo: []
}>()

const root = ref<HTMLElement | null>(null)
const dragging = ref<string | null>(null)

const layerCount = () => props.rows.filter((r) => r.kind === 'layer').length

function isSelected(selection: Selection): boolean {
  return sameSelection(selection, props.selection)
}

/**
 * Moves a row, then gives the focus back to the button pressed — or the row's
 * own once it reached an end: the list is drawn again, and a node moved in
 * the DOM loses the focus it held.
 */
async function move(id: string, delta: 1 | -1, position: number): Promise<void> {
  if (delta < 0 && position === 1) return swap(id, 'base')
  emit('move', id, delta)
  await nextTick()
  const row = root.value?.querySelector(`[data-layer="${CSS.escape(id)}"]`)
  const pressed = row?.querySelector<HTMLButtonElement>(delta > 0 ? '.up' : '.down')
  const target = pressed && !pressed.disabled ? pressed : row?.querySelector<HTMLButtonElement>('.pick')
  target?.focus()
}

/** Removes a row; its *Undo* takes the focus, which the row took with it. */
async function remove(id: string): Promise<void> {
  emit('remove', id)
  await nextTick()
  root.value?.querySelector<HTMLButtonElement>('.removed .undo')?.focus()
}

/** What `dragging` holds while the base is dragged: no layer id is empty. */
const BASE_DRAG = ''

/** The layer right above the base, the one its *Move up* trades with. */
const bottomId = computed(() => {
  const row = props.rows.find((r) => r.kind === 'layer' && r.position === 1)
  return row?.kind === 'layer' ? row.id : null
})

/**
 * Layer `id` and the base trade effects; the row holding the effect that moved
 * takes the focus: the base when a layer went down, the layer when the base
 * went up.
 */
async function swap(id: string, follow: 'base' | 'layer'): Promise<void> {
  emit('swap', id, follow)
  await nextTick()
  const row =
    follow === 'base'
      ? root.value?.querySelector('.lrow.base')
      : root.value?.querySelector(`[data-layer="${CSS.escape(id)}"]`)
  row?.querySelector<HTMLButtonElement>('.pick')?.focus()
}

function onDropBase(): void {
  const id = dragging.value
  dragging.value = null
  if (id !== null && id !== BASE_DRAG && props.baseSwappable) void swap(id, 'base')
}

function onDrop(onto: string): void {
  const id = dragging.value
  dragging.value = null
  if (id === BASE_DRAG) {
    if (props.baseSwappable) void swap(onto, 'layer')
  } else if (id !== null && id !== onto) {
    emit('drop', id, onto)
  }
}
</script>

<template>
  <div ref="root" class="stack" role="group" :aria-label="t('layers.stackOf', { target })">
    <p class="title">{{ t('layers.title') }}</p>

    <p v-if="full" class="limit">{{ t('layers.limit') }}</p>
    <button
      v-else
      type="button"
      class="add"
      :class="{ selected: isSelected({ kind: 'new' }) }"
      :aria-pressed="isSelected({ kind: 'new' })"
      @click="emit('select', { kind: 'new' })"
    >
      <template v-if="isSelected({ kind: 'new' })">{{ newLabel }}</template>
      <template v-else><span aria-hidden="true">+ </span>{{ t('layers.add') }}</template>
    </button>

    <div class="rows" :class="{ waiting }">
      <template v-for="(row, i) in rows" :key="row.kind === 'layer' ? row.id : `removed-${i}`">
        <div v-if="row.kind === 'removed'" class="removed" role="status">
          <span>{{ t('layers.removed') }}</span>
          <button type="button" class="undo" @click="emit('undo')">{{ t('layers.undo') }}</button>
        </div>
        <div
          v-else
          :data-layer="row.id"
          @dragover.prevent
          @drop.prevent="onDrop(row.id)"
        >
          <LayerRow
            :name="row.name"
            :line="row.line"
            :swatch="row.swatch"
            :state="row.state"
            :pressed="isSelected({ kind: 'layer', id: row.id })"
            :label="t('layers.rowName', { effect: row.name, line: row.line, n: row.position, total: layerCount() })"
            :can-up="row.position < layerCount()"
            :can-down="row.position > 1 || baseSwappable"
            :swaps-down="row.position === 1 && baseSwappable"
            @select="emit('select', { kind: 'layer', id: row.id })"
            @up="move(row.id, 1, row.position)"
            @down="move(row.id, -1, row.position)"
            @remove="remove(row.id)"
            @dragstart="dragging = row.id"
            @dragend="dragging = null"
          />
        </div>
      </template>

      <!-- A layer dropped here trades effects with the base; the base dropped on a layer, too. -->
      <div @dragover="baseSwappable && $event.preventDefault()" @drop.prevent="onDropBase">
        <LayerRow
          base
          :name="base.name"
          :line="base.line"
          :swatch="base.swatch"
          state="default"
          :pressed="isSelected({ kind: 'base' })"
          :label="t('layers.baseName', { effect: base.name })"
          :can-up="baseSwappable && bottomId !== null"
          @select="emit('select', { kind: 'base' })"
          @up="bottomId !== null && swap(bottomId, 'layer')"
          @dragstart="dragging = BASE_DRAG"
          @dragend="dragging = null"
        />
      </div>
    </div>

    <p v-if="waiting && layerCount() > 0" class="waiting-note">{{ t('layers.waiting') }}</p>
  </div>
</template>

<style scoped>
.stack {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin: 2px var(--gap-1) var(--gap-1);
  padding-top: 6px;
  border-top: 1px solid var(--line);
}

.title {
  margin: 0 4px 2px;
  color: var(--text-faint);
  font-size: 10px;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

/* Muted text and a dashed border: an action, not a layer; its words identify it. */
.add {
  min-height: 32px;
  padding: 4px var(--gap-2);
  border: 1px dashed var(--line-strong);
  border-radius: var(--r-md);
  color: var(--text-muted);
  font-size: 12px;
  text-align: left;
}

.add:hover {
  color: var(--accent);
  border-color: var(--accent);
}

.add.selected {
  border-style: solid;
  border-color: var(--accent);
  background: var(--accent-soft);
  color: var(--text);
}

.limit {
  padding: 4px var(--gap-2);
  color: var(--text-muted);
  font-size: 12px;
}

.rows {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

/* A hardware effect runs: the rows stay, dimmed, under the sentence that says why. */
.rows.waiting {
  opacity: 0.6;
}

.removed {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--gap-2);
  min-height: 32px;
  padding: 0 var(--gap-2);
  border-radius: var(--r-md);
  background: var(--raised-2);
  color: var(--text-muted);
  font-size: 12px;
}

.undo {
  min-height: 24px;
  padding: 2px 6px;
  color: var(--accent);
  font-weight: 500;
}

.waiting-note {
  margin: 2px 4px 0;
  color: var(--warn);
  font-size: 11px;
}
</style>
