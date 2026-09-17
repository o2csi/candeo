<script setup lang="ts">
/**
 * A duration, as a chip in a rule's sentence (#106, design §3.5): it reads
 * "1 h", and opens a small picker with presets and any number of seconds.
 *
 * A `<details>`: opening, closing and the keyboard come with the element, and
 * choosing a preset closes it.
 */
import { ref, useId } from 'vue'

import { duration } from '../composables/rules'
import { t } from '../i18n'

const props = defineProps<{
  seconds: number
  presets: readonly number[]
  /** What the chip stands for, for a screen reader: "Every", "For". */
  label: string
}>()

const emit = defineEmits<{ change: [seconds: number] }>()

const uid = useId()
const box = ref<HTMLDetailsElement | null>(null)

function text(seconds: number): string {
  const d = duration(seconds)
  return t(d.key, { n: d.n })
}

function choose(seconds: number): void {
  if (!Number.isInteger(seconds) || seconds < 1) return
  if (box.value) box.value.open = false
  if (seconds !== props.seconds) emit('change', seconds)
}

function typed(event: Event): void {
  choose(Number((event.target as HTMLInputElement).value))
}
</script>

<template>
  <details ref="box" class="chip">
    <summary :aria-label="`${label} ${text(seconds)}`">{{ text(seconds) }}</summary>
    <div class="picker">
      <div class="presets">
        <button
          v-for="p in presets"
          :key="p"
          type="button"
          class="preset"
          :aria-pressed="p === seconds"
          @click="choose(p)"
        >
          {{ text(p) }}
        </button>
      </div>
      <label :for="`${uid}-seconds`" class="custom">
        {{ t('automations.secondsField') }}
        <input
          :id="`${uid}-seconds`"
          type="number"
          min="1"
          step="1"
          :value="seconds"
          @change="typed"
        />
      </label>
    </div>
  </details>
</template>

<style scoped>
.chip {
  position: relative;
  display: inline-block;
}

summary {
  list-style: none;
  padding: 3px var(--gap-2);
  border-radius: var(--r-md);
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 500;
  cursor: pointer;
  white-space: nowrap;
}

summary::-webkit-details-marker {
  display: none;
}

summary:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.picker {
  position: absolute;
  z-index: 10;
  top: calc(100% + 4px);
  left: 0;
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  padding: var(--gap-2);
  background: var(--raised);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  box-shadow: 0 6px 20px rgb(0 0 0 / 18%);
}

.presets {
  display: flex;
  gap: 4px;
}

.preset {
  padding: 3px var(--gap-2);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  color: var(--text-muted);
  white-space: nowrap;
}

.preset:hover {
  color: var(--text);
  background: var(--raised-2);
}

.preset[aria-pressed='true'] {
  color: var(--accent);
  border-color: var(--accent);
}

.custom {
  display: flex;
  align-items: center;
  gap: var(--gap-2);
  color: var(--text-muted);
  font-size: 12px;
  white-space: nowrap;
}

.custom input {
  width: 7em;
}
</style>
