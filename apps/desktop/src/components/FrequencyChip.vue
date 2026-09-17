<script setup lang="ts">
/**
 * How often a rule applies, as a chip in its sentence (#106): "every hour", "at
 * 22:00". It writes the minute and hour of a cron expression; the days are the
 * next chip's, and anything else is the advanced field's.
 *
 * Disabled while the advanced field holds the rule: one of the two says when, and
 * never both.
 */
import { ref, useId } from 'vue'

import type { Frequency } from '../composables/rules'
import { t } from '../i18n'

const props = defineProps<{ frequency: Frequency; disabled: boolean }>()

const emit = defineEmits<{ change: [frequency: Frequency] }>()

const uid = useId()
const box = ref<HTMLDetailsElement | null>(null)

const PRESETS: readonly Frequency[] = [{ kind: 'minute' }, { kind: 'quarter' }, { kind: 'hour' }]

function text(frequency: Frequency): string {
  switch (frequency.kind) {
    case 'minute':
      return t('automations.everyMinute')
    case 'quarter':
      return t('automations.everyQuarter')
    case 'hour':
      return t('automations.everyHour')
    case 'at':
      return t('automations.at', { time: frequency.time })
  }
}

function choose(frequency: Frequency): void {
  if (box.value) box.value.open = false
  emit('change', frequency)
}

function at(event: Event): void {
  const time = (event.target as HTMLInputElement).value
  if (/^\d\d:\d\d$/.test(time)) choose({ kind: 'at', time })
}
</script>

<template>
  <span v-if="disabled" class="chip off" aria-disabled="true">{{ text(frequency) }}</span>
  <details v-else ref="box" class="chip">
    <summary>{{ text(frequency) }}</summary>
    <div class="picker">
      <div class="presets">
        <button
          v-for="p in PRESETS"
          :key="p.kind"
          type="button"
          class="preset"
          :aria-pressed="p.kind === props.frequency.kind"
          @click="choose(p)"
        >
          {{ text(p) }}
        </button>
      </div>
      <label :for="`${uid}-at`" class="custom">
        {{ t('automations.atField') }}
        <input
          :id="`${uid}-at`"
          type="time"
          :value="frequency.kind === 'at' ? frequency.time : ''"
          @change="at"
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

summary,
.off {
  display: inline-block;
  list-style: none;
  padding: 3px var(--gap-2);
  border-radius: var(--r-md);
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 500;
  white-space: nowrap;
}

summary {
  cursor: pointer;
}

summary::-webkit-details-marker {
  display: none;
}

summary:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

/* The advanced field holds the rule: shown, so the sentence still reads, but inert. */
.off {
  background: var(--raised-2);
  color: var(--text-faint);
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
</style>
