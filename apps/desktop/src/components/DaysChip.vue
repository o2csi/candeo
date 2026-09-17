<script setup lang="ts">
/**
 * Which days a rule applies, as a chip in its sentence (#106): "every day", "on
 * weekdays", "on Mon, Wed, Fri". It writes the weekday field of a cron expression.
 *
 * Day names come from the platform in the interface language, the way a date
 * would: they are a formatting of data, not text to translate.
 */
import { computed } from 'vue'

import { EVERY_DAY, WEEKDAYS, WEEKEND, same, type Days } from '../composables/rules'
import { currentLanguage, t } from '../i18n'

const props = defineProps<{ days: Days; disabled: boolean }>()

const emit = defineEmits<{ change: [days: Days] }>()

/** Monday first, as a week is read where Candeo is written; cron counts Sunday 0. */
const WEEK: readonly number[] = [1, 2, 3, 4, 5, 6, 0]

/** A Sunday, 4 January 1970; each day of the week follows it. */
const SUNDAY_MS = 3 * 86_400_000

function dayName(day: number): string {
  return new Intl.DateTimeFormat(currentLanguage(), { weekday: 'short', timeZone: 'UTC' }).format(
    new Date(SUNDAY_MS + day * 86_400_000),
  )
}

const text = computed(() => {
  if (same(props.days, EVERY_DAY)) return t('automations.everyDay')
  if (same(props.days, WEEKDAYS)) return t('automations.weekdays')
  if (same(props.days, WEEKEND)) return t('automations.weekend')
  const names = WEEK.filter((d) => props.days.includes(d)).map(dayName)
  return t('automations.onDays', { days: names.join(', ') })
})

function toggle(day: number): void {
  const next = props.days.includes(day) ? props.days.filter((d) => d !== day) : [...props.days, day]
  // A rule on no day would be a rule that never applies: that is switching it off.
  if (next.length) emit('change', [...next].sort((a, b) => a - b))
}
</script>

<template>
  <span v-if="disabled" class="chip off" aria-disabled="true">{{ text }}</span>
  <details v-else class="chip">
    <summary>{{ text }}</summary>
    <div class="picker">
      <div class="presets">
        <button
          type="button"
          class="preset"
          :aria-pressed="same(days, EVERY_DAY)"
          @click="emit('change', [...EVERY_DAY])"
        >
          {{ t('automations.everyDay') }}
        </button>
        <button
          type="button"
          class="preset"
          :aria-pressed="same(days, WEEKDAYS)"
          @click="emit('change', [...WEEKDAYS])"
        >
          {{ t('automations.weekdays') }}
        </button>
        <button
          type="button"
          class="preset"
          :aria-pressed="same(days, WEEKEND)"
          @click="emit('change', [...WEEKEND])"
        >
          {{ t('automations.weekend') }}
        </button>
      </div>
      <div class="week">
        <button
          v-for="d in WEEK"
          :key="d"
          type="button"
          class="day"
          :aria-pressed="days.includes(d)"
          @click="toggle(d)"
        >
          {{ dayName(d) }}
        </button>
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

.presets,
.week {
  display: flex;
  gap: 4px;
}

.preset,
.day {
  padding: 3px var(--gap-2);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  color: var(--text-muted);
  white-space: nowrap;
}

.preset:hover,
.day:hover {
  color: var(--text);
  background: var(--raised-2);
}

.preset[aria-pressed='true'],
.day[aria-pressed='true'] {
  color: var(--accent);
  border-color: var(--accent);
}
</style>
