<script setup lang="ts">
/**
 * A ⋯ button and the menu of actions it opens: a saved mask's, chosen keys',
 * a saved layer's (`docs/design/layers-ui.md` §5.3, §5.4).
 *
 * A menu button as WAI-ARIA has it (§8): `aria-haspopup`, `aria-expanded`, a
 * name saying whose actions they are; the list a `menu` of `menuitem`s.
 * Arrows move, Enter runs, Escape closes and gives the focus back to the
 * button; a click elsewhere or Tab closes it.
 */
import { nextTick, onBeforeUnmount, ref, useId } from 'vue'

import type { MenuItem } from '../composables/layers'

defineProps<{
  /** *Actions of the mask "Game keys"*. */
  label: string
  items: readonly MenuItem[]
  /** A line over the actions: *Used by 3 layers*. */
  note?: string
  disabled?: boolean
}>()

const emit = defineEmits<{
  pick: [id: string]
}>()

const uid = useId()
const open = ref(false)
const root = ref<HTMLElement | null>(null)
const button = ref<HTMLButtonElement | null>(null)

function entries(): HTMLButtonElement[] {
  return [...(root.value?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? [])]
}

function onOutside(e: PointerEvent): void {
  if (root.value && !root.value.contains(e.target as Node)) close(false)
}

function show(): void {
  open.value = true
  document.addEventListener('pointerdown', onOutside, true)
  void nextTick(() => entries()[0]?.focus())
}

function close(refocus: boolean): void {
  if (!open.value) return
  open.value = false
  document.removeEventListener('pointerdown', onOutside, true)
  if (refocus) void nextTick(() => button.value?.focus())
}

onBeforeUnmount(() => document.removeEventListener('pointerdown', onOutside, true))

function toggle(): void {
  if (open.value) close(true)
  else show()
}

function onKeyDown(e: KeyboardEvent): void {
  const list = entries()
  const at = list.indexOf(document.activeElement as HTMLButtonElement)
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault()
    const step = e.key === 'ArrowDown' ? 1 : -1
    list[(at + step + list.length) % list.length]?.focus()
  } else if (e.key === 'Home' || e.key === 'End') {
    e.preventDefault()
    list[e.key === 'Home' ? 0 : list.length - 1]?.focus()
  } else if (e.key === 'Escape') {
    e.preventDefault()
    close(true)
  } else if (e.key === 'Tab') {
    close(false)
  }
}

function run(id: string): void {
  // The action may open a dialog that takes the focus: the menu gives it back
  // to its button first, where a dialog returns it.
  close(true)
  emit('pick', id)
}

defineExpose({ close: () => close(false) })
</script>

<template>
  <span ref="root" class="actions">
    <button
      ref="button"
      type="button"
      class="more"
      aria-haspopup="menu"
      :aria-expanded="open"
      :aria-controls="open ? `${uid}-menu` : undefined"
      :aria-label="label"
      :title="label"
      :disabled="disabled"
      @click="toggle"
    >
      <svg viewBox="0 0 14 4" width="14" height="4" aria-hidden="true" fill="currentColor">
        <circle cx="2" cy="2" r="1.5" />
        <circle cx="7" cy="2" r="1.5" />
        <circle cx="12" cy="2" r="1.5" />
      </svg>
    </button>
    <div
      v-if="open"
      :id="`${uid}-menu`"
      class="menu"
      role="menu"
      :aria-label="label"
      @keydown="onKeyDown"
    >
      <p v-if="note" class="note">{{ note }}</p>
      <button
        v-for="item in items"
        :key="item.id"
        type="button"
        role="menuitem"
        tabindex="-1"
        class="item"
        :class="{ danger: item.danger }"
        @click="run(item.id)"
      >
        {{ item.label }}
      </button>
    </div>
  </span>
</template>

<style scoped>
.actions {
  position: relative;
  display: inline-flex;
  flex: none;
}

/* 24 px at least (WCAG 2.5.8); its text is the accent's icon, not a border alone. */
.more {
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  border: 1px solid var(--line-strong);
  border-radius: var(--r-sm);
  color: var(--text-muted);
}

.more:hover:not(:disabled),
.more[aria-expanded='true'] {
  color: var(--accent);
  border-color: var(--accent);
}

.more:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.menu {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  z-index: 5;
  min-width: 200px;
  padding: var(--gap-1) 0;
  background: var(--raised);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  box-shadow: var(--shadow);
}

.note {
  padding: 4px var(--gap-3) 6px;
  border-bottom: 1px solid var(--line);
  color: var(--text-muted);
  font-size: 12px;
}

.item {
  display: block;
  width: 100%;
  min-height: 32px;
  padding: 6px var(--gap-3);
  text-align: left;
  font-size: 13px;
  white-space: nowrap;
}

.item:hover,
.item:focus-visible {
  background: var(--raised-2);
  outline: none;
}

.item.danger {
  color: var(--bad);
}
</style>
