<script setup lang="ts">
/**
 * A layer's mask: a native `select`, its ⋯ menu, and the deleted mask's *Undo*
 * (`docs/design/layers-ui.md` §4.3, §5.4).
 *
 * The `select` groups what this target offers, each with what it lights here:
 * the whole device, the key groups its layout has, the regions its definition
 * declares, *Your masks*, the chosen keys, and *Choose keys…*. What lights
 * nothing here is absent, not disabled — but the mask in force is always
 * listed, so the `select` never shows another than the one the layer has.
 *
 * The ⋯ menu is offered for a saved mask (*Used by*, *Edit keys…*, *Rename…*,
 * *Delete*) and for chosen keys (*Edit keys…*, *Save as mask…*); none for the
 * whole device, a group or a region, which nobody edits.
 */
import { computed } from 'vue'

import type { Mask, MaskChoices } from '../api/layers'
import {
  countedName,
  groupName,
  maskKind,
  maskLabel,
  maskOfValue,
  maskValue,
  savedMaskName,
  type MaskAction,
  type MaskContext,
  type MaskPick,
  type MenuItem,
} from '../composables/layers'
import { t } from '../i18n'
import ActionsMenu from './ActionsMenu.vue'

const props = defineProps<{
  /** The `select`'s id, for its label. */
  id: string
  mask: Mask | undefined
  /** What this target offers; `null` until Rust answered. */
  choices: MaskChoices | null
  ctx: MaskContext
  /** What the mask in force lights here, `null` while unknown. */
  count: number | null
  /** How many layers and saved layers name the saved mask in force. */
  usage: number
  /** A saved mask just deleted, until the next change: its *Undo* line. */
  deleted: { name: string; count: number } | null
  disabled?: boolean
}>()

const emit = defineEmits<{
  pick: [pick: MaskPick]
  action: [action: MaskAction]
  undo: []
}>()

interface Option {
  value: string
  label: string
}

const current = computed(() => maskValue(props.mask))
const kind = computed(() => maskKind(props.mask))
const zones = computed(() => props.ctx.lights === 'zones')

const groups = computed<Option[]>(() =>
  (props.choices?.groups ?? []).map((c) => ({
    value: `group:${c.id}`,
    label: countedName(groupName(c.id), c.positions.length, props.ctx.lights),
  })),
)

const regions = computed<Option[]>(() =>
  (props.choices?.regions ?? []).map((c) => ({
    value: `region:${c.id}`,
    label: countedName(props.ctx.regionName(c.id), c.positions.length, props.ctx.lights),
  })),
)

const saved = computed<Option[]>(() =>
  (props.choices?.savedMasks ?? []).map((c) => ({
    value: `saved:${c.id}`,
    label: countedName(savedMaskName(c.id, props.ctx.savedMasks) ?? c.id, c.positions.length, props.ctx.lights),
  })),
)

/**
 * The mask in force when no group lists it: a group or a saved mask lighting
 * nothing here, a region of another definition. Listed alone, said as it is.
 */
const stray = computed<Option | null>(() => {
  const value = current.value
  if (value === 'whole' || value === 'chosen') return null
  if ([...groups.value, ...regions.value, ...saved.value].some((o) => o.value === value)) return null
  return { value, label: maskLabel(props.mask, props.ctx, props.count ?? 0) }
})

const savedName = computed(() =>
  props.mask?.saved ? (savedMaskName(props.mask.saved, props.ctx.savedMasks) ?? '') : '',
)

const menu = computed<{ label: string; note?: string; items: MenuItem[] } | null>(() => {
  const edit = { id: 'edit', label: zones.value ? t('layers.mask.editZones') : t('layers.mask.editKeys') }
  if (kind.value === 'saved' && savedName.value !== '') {
    return {
      label: t('layers.mask.actions', { name: savedName.value }),
      note: t('layers.mask.usedBy', { n: props.usage }, props.usage),
      items: [
        edit,
        { id: 'rename', label: t('layers.mask.rename') },
        { id: 'delete', label: t('layers.mask.delete'), danger: true },
      ],
    }
  }
  if (kind.value === 'chosen') {
    return {
      label: t('layers.mask.actionsChosen'),
      items: [edit, { id: 'saveAs', label: t('layers.mask.saveAs') }],
    }
  }
  return null
})

function onChange(e: Event): void {
  const select = e.target as HTMLSelectElement
  const pick = maskOfValue(select.value)
  // *Choose keys…* opens the selection: until it ends, the layer keeps its mask,
  // and the `select` says so.
  if (pick.kind !== 'mask') select.value = current.value
  if (pick.kind !== 'keep') emit('pick', pick)
}

function onAction(id: string): void {
  if (id === 'edit' || id === 'rename' || id === 'delete' || id === 'saveAs') emit('action', id)
}
</script>

<template>
  <div class="mask">
    <div class="line">
      <select :id="id" class="field" :value="current" :disabled="disabled" @change="onChange">
        <option value="whole">
          {{ ctx.region ? t('layers.mask.wholeRegion') : t('layers.mask.whole') }}
        </option>
        <optgroup v-if="groups.length" :label="t('layers.mask.groups')">
          <option v-for="o in groups" :key="o.value" :value="o.value">{{ o.label }}</option>
        </optgroup>
        <optgroup v-if="regions.length" :label="t('layers.mask.regions')">
          <option v-for="o in regions" :key="o.value" :value="o.value">{{ o.label }}</option>
        </optgroup>
        <optgroup v-if="saved.length" :label="t('layers.mask.saved')">
          <option v-for="o in saved" :key="o.value" :value="o.value">{{ o.label }}</option>
        </optgroup>
        <option v-if="stray" :value="stray.value">{{ stray.label }}</option>
        <option v-if="kind === 'chosen'" value="chosen">{{ maskLabel(mask, ctx, count) }}</option>
        <option value="choose">
          {{ zones ? t('layers.mask.chooseZones') : t('layers.mask.chooseKeys') }}
        </option>
      </select>
      <ActionsMenu
        v-if="menu"
        :label="menu.label"
        :note="menu.note"
        :items="menu.items"
        :disabled="disabled"
        @pick="onAction"
      />
    </div>
    <div v-if="deleted" class="deleted" role="status">
      <span>{{ t('layers.mask.deleted', { name: deleted.name, n: deleted.count }, deleted.count) }}</span>
      <button type="button" class="undo" @click="emit('undo')">{{ t('layers.mask.undo') }}</button>
    </div>
  </div>
</template>

<style scoped>
.mask {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.line {
  display: flex;
  gap: 6px;
  align-items: center;
}

.field {
  flex: 0 1 340px;
  min-width: 0;
  min-height: 32px;
  padding: 4px var(--gap-2);
  background: var(--raised);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  color: var(--text);
  font: inherit;
  font-size: 13px;
}

.field:disabled {
  opacity: 0.7;
  cursor: not-allowed;
}

.deleted {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-2);
  align-items: center;
  align-self: flex-start;
  padding: 4px var(--gap-2);
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
