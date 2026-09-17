<script setup lang="ts">
/**
 * When in the day a rule may apply, as a chip in its sentence (#106): "at any
 * time", or "between 22:00 and 07:00" — a window that wraps past midnight when
 * it starts later than it ends.
 */
import { useId } from 'vue'

import type { TimeWindow } from '../api/candeo'
import { t } from '../i18n'

const props = defineProps<{ window: TimeWindow | undefined }>()

const emit = defineEmits<{ change: [window: TimeWindow | undefined] }>()

const uid = useId()

/** What a window starts from when someone first restricts a rule: the night. */
const NIGHT: TimeWindow = { from: '22:00', to: '07:00' }

function toggle(event: Event): void {
  const on = (event.target as HTMLInputElement).checked
  emit('change', on ? (props.window ?? NIGHT) : undefined)
}

function edge(which: 'from' | 'to', event: Event): void {
  const value = (event.target as HTMLInputElement).value
  if (!/^\d\d:\d\d$/.test(value)) return
  emit('change', { ...(props.window ?? NIGHT), [which]: value })
}
</script>

<template>
  <details class="chip">
    <summary>
      {{ window ? t('automations.between', { from: window.from, to: window.to }) : t('automations.always') }}
    </summary>
    <div class="picker">
      <label :for="`${uid}-only`" class="row">
        <input :id="`${uid}-only`" type="checkbox" :checked="window !== undefined" @change="toggle" />
        {{ t('automations.onlyBetween') }}
      </label>
      <div v-if="window" class="row">
        <input
          type="time"
          :aria-label="t('automations.from')"
          :value="window.from"
          @change="edge('from', $event)"
        />
        <span>{{ t('automations.and') }}</span>
        <input
          type="time"
          :aria-label="t('automations.to')"
          :value="window.to"
          @change="edge('to', $event)"
        />
      </div>
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
  border: 1px dashed var(--line-strong);
  color: var(--text-muted);
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

.row {
  display: flex;
  align-items: center;
  gap: var(--gap-2);
  color: var(--text-muted);
  font-size: 12px;
  white-space: nowrap;
}
</style>
