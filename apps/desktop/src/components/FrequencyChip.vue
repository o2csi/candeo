<script setup lang="ts">
/**
 * When a rule applies, as a chip in its sentence (#106): "every hour", "at
 * 22:00", "after 10 min idle" (#179), or "when “build” is “failed”" (#108). It
 * writes the minute and hour of a cron expression, an idle trigger or a signal
 * trigger; the days are the next chip's, and anything else is the advanced
 * field's.
 *
 * Disabled while the advanced field holds the rule: one of the two says when, and
 * never both. The idle choice is shown disabled where the system does not say
 * how long the computer has been idle.
 *
 * A signal is named rather than picked: the names held now are suggested, and a
 * name no sender has used yet can be typed, since a rule is often written before
 * its sender runs.
 */
import { computed, ref, useId, watch } from 'vue'

import type { HeldSignal, SignalTrigger } from '../api/candeo'
import { IDLE_PRESETS, signalTrigger, validSignalName, type Frequency } from '../composables/rules'
import { signalText } from '../composables/signals'
import { t } from '../i18n'

const props = defineProps<{
  frequency: Frequency
  disabled: boolean
  /** The minutes of an idle rule, or `null` for another one. */
  idle: number | null
  idleAvailable: boolean
  /** The trigger of a signal rule, or `null` for another one. */
  signal: SignalTrigger | null
  /** The signals held now, whose names are suggested. */
  signals: readonly HeldSignal[]
}>()

const emit = defineEmits<{
  change: [frequency: Frequency]
  idle: [minutes: number]
  signal: [trigger: SignalTrigger]
}>()

const uid = useId()
const box = ref<HTMLDetailsElement | null>(null)

/**
 * What the signal fields hold, typed or not yet saved. A render would otherwise
 * put the rule's values back over them: a name refused would vanish with the
 * value typed beside it.
 */
const draftName = ref(props.signal?.name ?? '')
const draftValue = ref(props.signal?.equals ?? '')
/** A name typed that cannot be one, to say so beside the field. */
const badName = ref<string | null>(null)

// The rule changed — saved, put back after a refusal, or no longer a signal:
// the fields show what it holds now.
watch(
  () => `${props.signal?.name ?? ''}\u0000${props.signal?.equals ?? ''}`,
  () => {
    draftName.value = props.signal?.name ?? ''
    draftValue.value = props.signal?.equals ?? ''
  },
)

const PRESETS: readonly Frequency[] = [{ kind: 'minute' }, { kind: 'quarter' }, { kind: 'hour' }]

/** Neither idle nor a signal: a cron expression, which the presets write. */
const scheduled = computed(() => props.idle === null && props.signal === null)

function summary(): string {
  if (props.signal) {
    return t('automations.signal', { name: props.signal.name, value: props.signal.equals })
  }
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

/**
 * Either field committed: the rule waits for that signal as soon as its name can
 * be one. The picker stays open, so that the value can follow the name.
 */
function signalTyped(): void {
  const name = draftName.value.trim()
  badName.value = name === '' || validSignalName(name) ? null : name
  const trigger = signalTrigger(name, draftValue.value, props.signal, props.signals)
  if (!trigger) return
  // The value may come from the signal held: shown before the rule is saved.
  draftValue.value = trigger.equals
  if (trigger.name !== props.signal?.name || trigger.equals !== props.signal?.equals) {
    emit('signal', trigger)
  }
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
          :aria-pressed="scheduled && p.kind === props.frequency.kind"
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
          :value="scheduled && frequency.kind === 'at' ? frequency.time : ''"
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

      <div class="section">
        <span class="custom">{{ t('automations.signalField') }}</span>
        <label :for="`${uid}-signal`" class="custom">
          {{ t('automations.signalName') }}
          <input
            :id="`${uid}-signal`"
            v-model="draftName"
            class="mono"
            type="text"
            spellcheck="false"
            autocomplete="off"
            :list="`${uid}-held`"
            @change="signalTyped"
          />
        </label>
        <!-- Each name held now, with its value beside it. -->
        <datalist :id="`${uid}-held`">
          <option v-for="s in signals" :key="s.name" :value="s.name" :label="signalText(s.value)" />
        </datalist>
        <label :for="`${uid}-equals`" class="custom">
          {{ t('automations.signalValue') }}
          <input
            :id="`${uid}-equals`"
            v-model="draftValue"
            class="mono"
            type="text"
            spellcheck="false"
            autocomplete="off"
            @change="signalTyped"
          />
        </label>
        <p v-if="badName !== null" class="bad" role="alert">
          {{ t('errors.signalNameInvalid', { name: badName }) }}
        </p>
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

/* The idle choice and the signal, below the times: other kinds of "when". */
.idle,
.section {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  padding-top: var(--gap-2);
  border-top: 1px solid var(--line);
}

.idle p,
.section p {
  margin: 0;
}

input[type='number'] {
  width: 5em;
}

input[type='text'] {
  width: 14em;
}

/* Beside the field, in the words Rust would refuse the rule with. */
.bad {
  max-width: 28em;
  color: var(--bad);
  font-size: 12px;
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
