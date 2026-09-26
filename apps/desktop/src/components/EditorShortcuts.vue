<script setup lang="ts">
/**
 * The editor's shortcuts, behind a button in its header and F1: there when
 * someone looks for them, out of the way of the code otherwise.
 */

import { onBeforeUnmount, onMounted, ref, watch } from 'vue'

import { keysOf, readLayout, SHORTCUTS, type LayoutMap } from '../editor/shortcuts'
import { t } from '../i18n'

const open = ref(false)
const root = ref<HTMLElement | null>(null)
/** The keyboard's layout, for the one label it changes. */
const layout = ref<LayoutMap | null>(null)

/** A click anywhere else closes it, as a menu does. */
function outside(e: MouseEvent): void {
  if (root.value && !root.value.contains(e.target as Node)) open.value = false
}

/**
 * F1 from anywhere on the screen, the code included: Monaco leaves it alone,
 * since its command palette is not loaded. Escape closes it from there too.
 */
function key(e: KeyboardEvent): void {
  if (e.key === 'F1') {
    e.preventDefault()
    open.value = !open.value
  } else if (e.key === 'Escape' && open.value) {
    open.value = false
  }
}

watch(open, (now) => {
  if (now) window.addEventListener('mousedown', outside)
  else window.removeEventListener('mousedown', outside)
})

onMounted(async () => {
  window.addEventListener('keydown', key)
  layout.value = await readLayout()
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', key)
  window.removeEventListener('mousedown', outside)
})
</script>

<template>
  <div ref="root" class="shortcuts">
    <button
      class="ghost"
      :aria-expanded="open"
      :title="`${t('editor.shortcuts.title')} (F1)`"
      :aria-label="t('editor.shortcuts.title')"
      @click="open = !open"
    >
      ⌨
    </button>
    <div v-if="open" class="panel" role="dialog" :aria-label="t('editor.shortcuts.title')">
      <table>
        <tbody>
          <tr v-for="s in SHORTCUTS" :key="s.action">
            <td><kbd>{{ keysOf(s, layout) }}</kbd></td>
            <td>{{ t(`editor.shortcuts.${s.action}`) }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.shortcuts {
  position: relative;
}

.ghost {
  padding: 4px var(--gap-3);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  color: var(--text-muted);
  font-size: 13px;
}

.ghost:hover {
  color: var(--text);
  background: var(--raised-2);
}

/* Above Monaco, which stacks its own layers. */
.panel {
  position: absolute;
  top: calc(100% + 6px);
  right: 0;
  z-index: 20;
  padding: var(--gap-2) var(--gap-3);
  background: var(--raised);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  box-shadow: var(--shadow);
  white-space: nowrap;
}

table {
  border-collapse: collapse;
  font-size: 12px;
}

td {
  padding: 3px 0;
}

td:first-child {
  padding-right: var(--gap-3);
  text-align: right;
}

kbd {
  padding: 1px 6px;
  border: 1px solid var(--line-strong);
  border-radius: var(--r-sm);
  background: var(--raised-2);
  font-family: var(--font-mono);
  font-size: 11px;
}
</style>
