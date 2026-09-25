<script setup lang="ts">
/**
 * What a device's brightness follows, under its slider: nothing, a signal or
 * the sound, and the floor it never goes under
 * (`docs/design/inputs-and-automations.md` §2.2.2).
 *
 * The same switch as an effect setting's, so the two read alike. The parent
 * keeps the value and sends it on; this only holds a *Signal* waiting for its
 * name, which is not a dimming yet.
 */
import { computed, ref, watch } from 'vue'

import type { Dimming, HeldSignal } from '../api/candeo'
import { SOUND_SOURCES, type SoundSource } from '../composables/bindings'
import {
  dimmingMode,
  dimmingSignal,
  dimmingSound,
  followingSignal,
  followingSound,
  withFloor,
  type DimmingMode,
} from '../composables/dimming'
import { signalText } from '../composables/signals'
import { t } from '../i18n'

const props = defineProps<{
  /** Prefix for the ids inside: one card, one of these. */
  id: string
  dimming: Dimming | null
  /** A firmware effect is applied: it sends no frame, so nothing is dimmed. */
  firmware: boolean
  /** The signals held now, suggested to the name field. */
  signals: readonly HeldSignal[]
}>()

const emit = defineEmits<{
  /** `commit`: the end of a gesture, the moment to write it down. */
  change: [dimming: Dimming | null, commit: boolean]
}>()

/** *Signal* chosen, no valid name typed yet. */
const waiting = ref(false)
const name = ref(dimmingSignal(props.dimming) ?? '')

watch(
  () => props.dimming,
  (d) => {
    const signal = dimmingSignal(d)
    if (signal !== null) name.value = signal
    if (d !== null) waiting.value = false
  },
)

// Another device: its own state, nothing carried over.
watch(
  () => props.id,
  () => {
    waiting.value = false
    name.value = dimmingSignal(props.dimming) ?? ''
  },
)

const mode = computed<DimmingMode>(() => (waiting.value ? 'signal' : dimmingMode(props.dimming)))
const sound = computed<SoundSource>(() => dimmingSound(props.dimming) ?? 'beat')
const refused = computed(() => name.value.trim() !== '' && followingSignal(null, name.value) === null)

const SOUND_LABELS = {
  volume: 'effects.params.sound.volume',
  beat: 'effects.params.sound.beat',
  bass: 'effects.params.sound.bass',
  mids: 'effects.params.sound.mids',
  highs: 'effects.params.sound.highs',
  tone: 'effects.params.sound.tone',
} as const satisfies Record<SoundSource, string>

function toValue(): void {
  waiting.value = false
  if (props.dimming !== null) emit('change', null, true)
}

/** The beat first: a light that jumps on it and settles is what most people mean. */
function toSound(): void {
  waiting.value = false
  if (mode.value !== 'sound') emit('change', followingSound(props.dimming, 'beat'), true)
}

function toSignal(): void {
  if (mode.value === 'signal') return
  const known = followingSignal(props.dimming, name.value)
  if (known !== null) {
    emit('change', known, true)
    return
  }
  // Off the sound first: the brightness waits for a name at the slider's level.
  if (props.dimming !== null) emit('change', null, true)
  waiting.value = true
}

function onSound(e: Event): void {
  const chosen = SOUND_SOURCES.find((s) => s === (e.target as HTMLSelectElement).value)
  if (chosen) emit('change', followingSound(props.dimming, chosen), true)
}

function onName(e: Event): void {
  name.value = (e.target as HTMLInputElement).value
}

function onNameEnd(): void {
  const next = followingSignal(props.dimming, name.value)
  if (next !== null && next.source !== props.dimming?.source) emit('change', next, true)
}

function onFloor(e: Event, commit: boolean): void {
  if (props.dimming === null) return
  emit('change', withFloor(props.dimming, Number((e.target as HTMLInputElement).value)), commit)
}
</script>

<template>
  <div class="follow">
    <div class="head">
      <span class="label">{{ t('effects.brightnessFollows') }}</span>
      <span
        class="source"
        role="group"
        :aria-label="t('effects.brightnessFollows')"
        :title="firmware ? t('effects.brightnessFirmware') : undefined"
      >
        <button type="button" class="seg" :aria-pressed="mode === 'value'" :disabled="firmware" @click="toValue">
          {{ t('effects.params.fromValue') }}
        </button>
        <button type="button" class="seg" :aria-pressed="mode === 'signal'" :disabled="firmware" @click="toSignal">
          {{ t('effects.params.fromSignal') }}
        </button>
        <button type="button" class="seg" :aria-pressed="mode === 'sound'" :disabled="firmware" @click="toSound">
          {{ t('effects.params.fromSound') }}
        </button>
      </span>
    </div>

    <select
      v-if="mode === 'sound'"
      class="field"
      :aria-label="t('effects.brightnessSound')"
      :value="sound"
      :disabled="firmware"
      @change="onSound"
    >
      <option v-for="s in SOUND_SOURCES" :key="s" :value="s">{{ t(SOUND_LABELS[s]) }}</option>
    </select>

    <template v-if="mode === 'signal'">
      <input
        class="field mono"
        type="text"
        spellcheck="false"
        autocomplete="off"
        :list="`${id}-held`"
        :placeholder="t('effects.params.signalName')"
        :aria-label="t('effects.brightnessSignal')"
        :aria-invalid="refused"
        :value="name"
        :disabled="firmware"
        @input="onName"
        @change="onNameEnd"
      />
      <datalist :id="`${id}-held`">
        <option v-for="s in signals" :key="s.name" :value="s.name" :label="signalText(s.value)" />
      </datalist>
      <p v-if="refused" class="bad" role="alert">
        {{ t('errors.signalNameInvalid', { name: name.trim() }) }}
      </p>
    </template>

    <template v-if="dimming !== null && mode !== 'value'">
      <label class="head" :for="`${id}-floor`">
        <span class="label">{{ t('effects.brightnessFloor') }}</span>
        <span class="value">{{ t('effects.percent', { n: dimming.floor }) }}</span>
      </label>
      <input
        :id="`${id}-floor`"
        type="range"
        min="0"
        max="100"
        step="1"
        :value="dimming.floor"
        :disabled="firmware"
        @input="onFloor($event, false)"
        @change="onFloor($event, true)"
      />
      <!-- Said once, where it applies: the choice is kept, and comes back with a host effect. -->
      <p v-if="firmware" class="note">{{ t('effects.brightnessFirmware') }}</p>
    </template>
  </div>
</template>

<style scoped>
.follow {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 6px;
}

.head {
  display: flex;
  flex-wrap: wrap;
  gap: 2px var(--gap-2);
  align-items: center;
  color: var(--text-faint);
  font-size: 11px;
}

.label {
  flex: 1;
}

.value {
  color: var(--text-muted);
  font-variant-numeric: tabular-nums;
}

.source {
  display: inline-flex;
}

.seg {
  padding: 1px 6px;
  border: 1px solid var(--line-strong);
  color: var(--text-faint);
  font-size: 11px;
}

.seg + .seg {
  border-left: 0;
}

.seg:first-child {
  border-radius: var(--r-sm) 0 0 var(--r-sm);
}

.seg:last-child {
  border-radius: 0 var(--r-sm) var(--r-sm) 0;
}

.seg:hover:not(:disabled, [aria-pressed='true']) {
  color: var(--text);
  background: var(--raised-2);
}

.seg[aria-pressed='true'] {
  color: var(--accent);
  background: var(--accent-soft);
}

.seg:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.field {
  width: 100%;
  padding: 3px var(--gap-2);
  background: var(--raised);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-sm);
  color: var(--text);
  font: inherit;
  font-size: 12px;
}

.field[aria-invalid='true'] {
  border-color: var(--bad);
}

input[type='range'] {
  width: 100%;
  accent-color: var(--accent);
}

:disabled {
  cursor: not-allowed;
}

input[type='range']:disabled,
.field:disabled {
  opacity: 0.45;
}

.bad,
.note {
  margin: 0;
  font-size: 11px;
  overflow-wrap: anywhere;
}

.bad {
  color: var(--bad);
}

.note {
  color: var(--text-muted);
}
</style>
