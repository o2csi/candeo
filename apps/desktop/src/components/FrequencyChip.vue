<script setup lang="ts">
/**
 * When a rule applies, as a chip in its sentence (#106): "every hour", "at
 * 22:00", or "after 10 min idle" (#179). It writes the minute and hour of a cron
 * expression, or an idle trigger; the days are the next chip's, and anything
 * else is the advanced field's.
 *
 * Disabled while the advanced field holds the rule: one of the two says when, and
 * never both. The idle choice is shown disabled where the system does not say
 * how long the computer has been idle.
 */
import { ref, useId } from 'vue'

import { IDLE_PRESETS, type Frequency } from '../composables/rules'
import { t } from '../i18n'

const props = defineProps<{
  frequency: Frequency
  disabled: boolean
  /** The minutes of an idle rule, or `null` for a cron one. */
  idle: number | null
  idleAvailable: boolean
}>()

const emit = defineEmits<{ change: [frequency: Frequency]; idle: [minutes: number] }>()

const uid = useId()
const box = ref<HTMLDetailsElement | null>(null)

const PRESETS: readonly Frequency[] = [{ kind: 'minute' }, { kind: 'quarter' }, { kind: 'hour' }]

function summary(): string {
  return props.idle === null ? text(props.frequency) : t('automations.idle', { n: props.idle })
}

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

function chooseIdle(minutes: number): void {
  if (box.value) box.value.open = false
  emit('idle', minutes)
}

function idleTyped(event: Event): void {
  const minutes = Number((event.target as HTMLInputElement).value)
  if (Number.isInteger(minutes) && minutes >= 1) chooseIdle(minutes)
}
</script>

<template>
  <span v-if="disabled" class="chip off" aria-disabled="true">{{ summary() }}</span>
  <details v-else ref="box" class="chip">
    <summary>{{ summary() }}</summary>
    <div class="picker">
      <div class="presets">
        <button
          v-for="p in PRESETS"
          :key="p.kind"
          type="button"
          class="preset"
          :aria-pressed="idle === null && p.kind === props.frequency.kind"
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
          :value="idle === null && frequency.kind === 'at' ? frequency.time : ''"
          @change="at"
        />
      </label>

      <div class="idle">
        <span class="custom">{{ t('automations.idleField') }}</span>
        <div class="presets">
          <button
            v-for="m in IDLE_PRESETS"
            :key="m"
            type="button"
            class="preset"
            :aria-pressed="idle === m"
            :disabled="!idleAvailable"
            @click="chooseIdle(m)"
          >
            {{ t('automations.minutes', { n: m }) }}
          </button>
        </div>
        <label :for="`${uid}-idle`" class="custom">
          {{ t('automations.idleMinutes') }}
          <input
            :id="`${uid}-idle`"
            type="number"
            min="1"
            step="1"
            :value="idle ?? ''"
            :disabled="!idleAvailable"
            @change="idleTyped"
          />
        </label>
        <p v-if="!idleAvailable" class="custom">{{ t('automations.idleUnavailable') }}</p>
      </div>
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

.preset:disabled {
  color: var(--text-faint);
  background: none;
  cursor: default;
}

/* The idle choice, below the times: another kind of "when". */
.idle {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  padding-top: var(--gap-2);
  border-top: 1px solid var(--line);
}

.idle p {
  margin: 0;
}

input[type='number'] {
  width: 5em;
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
