<script setup lang="ts">
/**
 * *Your layers*, above the effects in the gallery while the target is a layer
 * or *Add a layer* (`docs/design/layers-ui.md` §5.3).
 *
 * Each item: the effect's swatch framed in dashes — a layer, not an effect —
 * its name, and `{effect} · {mask} · {blend} {opacity}`. Picking one previews
 * it whole in the selected row's place; nothing is written until the sending
 * button puts a copy there. Its ⋯ menu, shown on hover and focus and always in
 * the DOM, renames or deletes it; a deletion leaves *Undo* in the item's place
 * until the next change.
 */
import type { SavedLayerView } from '../composables/layers'
import { t } from '../i18n'
import ActionsMenu from './ActionsMenu.vue'
import EffectSwatch from './EffectSwatch.vue'

const props = defineProps<{
  items: readonly SavedLayerView[]
  /** The one just deleted, at its place in the list, until the next change. */
  deleted: { at: number; name: string } | null
}>()

const emit = defineEmits<{
  pick: [name: string]
  rename: [name: string]
  delete: [name: string]
  undo: []
}>()

/** The items with the deleted one's *Undo* where it stood. */
function rows(): Array<SavedLayerView | { deleted: string }> {
  const out: Array<SavedLayerView | { deleted: string }> = [...props.items]
  if (props.deleted) out.splice(Math.min(props.deleted.at, out.length), 0, { deleted: props.deleted.name })
  return out
}

function onAction(name: string, id: string): void {
  if (id === 'rename') emit('rename', name)
  else if (id === 'delete') emit('delete', name)
}
</script>

<template>
  <section class="saved" :aria-labelledby="'saved-layers-title'">
    <p id="saved-layers-title" class="title">{{ t('layers.saved.title') }}</p>
    <template v-for="(row, i) in rows()" :key="'deleted' in row ? `deleted-${i}` : row.name">
      <div v-if="'deleted' in row" class="gone" role="status">
        <span>{{ t('layers.saved.deleted', { name: row.deleted }) }}</span>
        <button type="button" class="undo" @click="emit('undo')">{{ t('layers.saved.undo') }}</button>
      </div>
      <div v-else class="item" :class="{ selected: row.pressed }">
        <button
          type="button"
          class="pick"
          :aria-pressed="row.pressed"
          :aria-label="`${row.name}, ${row.line}`"
          :title="row.line"
          @click="emit('pick', row.name)"
        >
          <EffectSwatch class="mark" :colors="row.swatch" />
          <span class="text">
            <span class="name">{{ row.name }}</span>
            <span class="line">{{ row.line }}</span>
          </span>
        </button>
        <ActionsMenu
          class="item-menu"
          :label="t('layers.saved.actions', { name: row.name })"
          :items="[
            { id: 'rename', label: t('layers.saved.rename') },
            { id: 'delete', label: t('layers.saved.delete'), danger: true },
          ]"
          @pick="(id) => onAction(row.name, id)"
        />
      </div>
    </template>
  </section>
</template>

<style scoped>
.saved {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin-bottom: var(--gap-3);
}

.title {
  margin: 0 var(--gap-2) var(--gap-1);
  color: var(--text-faint);
  font-size: 10px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.item {
  display: flex;
  gap: 2px;
  align-items: center;
  border: 1px solid transparent;
  border-radius: var(--r-md);
}

.item.selected {
  background: var(--raised-2);
  border-color: var(--accent);
}

.pick {
  display: flex;
  flex: 1;
  gap: var(--gap-2);
  align-items: center;
  min-width: 0;
  min-height: 36px;
  padding: 4px var(--gap-2);
  border-radius: var(--r-md);
  text-align: left;
}

.pick:hover {
  background: var(--raised-2);
}

/* Framed in dashes: a layer, effect and look together, not an effect alone. */
.pick .mark {
  outline: 1px dashed var(--line-strong);
  outline-offset: 2px;
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
  font-size: 13px;
}

.line {
  color: var(--text-muted);
  font-size: 11px;
}

/* Shown on hover and focus, always in the DOM for Tab. */
.item-menu {
  margin-right: 4px;
  opacity: 0;
}

.item:hover .item-menu,
.item:focus-within .item-menu {
  opacity: 1;
}

.gone {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--gap-2);
  min-height: 36px;
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
</style>
