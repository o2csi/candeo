<script setup lang="ts">
/**
 * Settings › Sound (#107, `docs/design/inputs-and-automations.md` §2.2.1): how
 * the sound playing is heard, once for every effect and every setting that
 * follows it, and a meter to set it against what plays.
 *
 * The capture runs while this block is shown and the window visible, for the
 * meter; nowhere else is it started by Settings.
 *
 * Its root is a `.block` of the Settings page, which gives it its place and its
 * separator; the rest is styled here.
 */
import { onBeforeUnmount, onMounted, ref, useId } from 'vue'

import {
  getSoundSettings,
  setSoundSettings,
  soundNow,
  watchSound,
  type SoundNow,
  type SoundTuning,
} from '../api/candeo'
import { message, warn } from '../api/journal'
import { SOUND_SOURCES } from '../composables/bindings'
import { t } from '../i18n'

const uid = useId()

const tuning = ref<SoundTuning | null>(null)
const now = ref<SoundNow | null>(null)
const problem = ref<string | null>(null)

/** Each source of the sound, as the meter names it: the words of the settings' list. */
const LABELS = {
  volume: 'effects.params.sound.volume',
  beat: 'effects.params.sound.beat',
  bass: 'effects.params.sound.bass',
  mids: 'effects.params.sound.mids',
  highs: 'effects.params.sound.highs',
  tone: 'effects.params.sound.tone',
} as const

/** Ten a second: enough for bars to move with the music, little for the IPC. */
const METER_MS = 100
let polling: ReturnType<typeof setInterval> | null = null

async function read(): Promise<void> {
  try {
    now.value = await soundNow()
  } catch {
    // A missed reading is the next one's to show.
  }
}

/** The meter runs while the window is visible: a hidden window measures nothing for anyone. */
async function watch(on: boolean): Promise<void> {
  if (polling !== null) clearInterval(polling)
  polling = null
  await watchSound(on).catch((e: unknown) => warn('sound', `meter: ${message(e, 'en')}`, e))
  if (on) polling = setInterval(() => void read(), METER_MS)
}

function onVisibility(): void {
  void watch(document.visibilityState === 'visible')
}

onMounted(async () => {
  try {
    tuning.value = await getSoundSettings()
  } catch (e) {
    problem.value = message(e)
  }
  document.addEventListener('visibilitychange', onVisibility)
  void watch(document.visibilityState === 'visible')
})

onBeforeUnmount(() => {
  document.removeEventListener('visibilitychange', onVisibility)
  if (polling !== null) clearInterval(polling)
  void watchSound(false).catch(() => null)
})

const input = (e: Event) => Number((e.target as HTMLInputElement).value)

/** While dragging, the number beside the slider follows; the release saves. */
function onGain(e: Event): void {
  if (tuning.value) tuning.value = { ...tuning.value, gain: input(e) }
}

function onSensitivity(e: Event): void {
  if (tuning.value) tuning.value = { ...tuning.value, sensitivity: input(e) }
}

async function save(): Promise<void> {
  if (!tuning.value) return
  problem.value = null
  try {
    tuning.value = await setSoundSettings(tuning.value.gain, tuning.value.sensitivity)
  } catch (e) {
    problem.value = message(e)
  }
}
</script>

<template>
  <section class="block" :aria-labelledby="`${uid}-title`">
    <h2 :id="`${uid}-title`">{{ t('settings.sound.title') }}</h2>

    <template v-if="tuning">
      <div class="tune">
        <label :for="`${uid}-gain`">{{ t('settings.sound.gain') }}</label>
        <input
          :id="`${uid}-gain`"
          type="range"
          min="0.5"
          max="3"
          step="0.1"
          :value="tuning.gain"
          @input="onGain"
          @change="save"
        />
        <span class="num">{{ t('settings.sound.gainValue', { value: tuning.gain.toFixed(1) }) }}</span>

        <label :for="`${uid}-sensitivity`">{{ t('settings.sound.sensitivity') }}</label>
        <input
          :id="`${uid}-sensitivity`"
          type="range"
          min="0"
          max="1"
          step="0.05"
          :value="tuning.sensitivity"
          @input="onSensitivity"
          @change="save"
        />
        <span class="num">{{
          t('settings.sound.sensitivityValue', { value: Math.round(tuning.sensitivity * 100) })
        }}</span>
      </div>
    </template>

    <p v-if="now?.state === 'unavailable'" class="note">{{ t('settings.sound.unavailable') }}</p>
    <!-- Not a live region: it changes ten times a second. -->
    <div v-else class="meter" aria-hidden="true">
      <template v-for="s in SOUND_SOURCES" :key="s">
        <span class="source">{{ t(LABELS[s]) }}</span>
        <span class="track">
          <span class="rest" :style="{ width: `${100 - Math.round((now?.[s] ?? 0) * 100)}%` }" />
        </span>
      </template>
    </div>

    <p v-if="problem" class="note bad">{{ problem }}</p>
  </section>
</template>

<style scoped>
.tune,
.meter {
  display: grid;
  grid-template-columns: max-content minmax(0, 18rem) max-content;
  align-items: center;
  gap: var(--gap-2) var(--gap-3);
}

.meter {
  grid-template-columns: max-content minmax(0, 18rem);
  gap: 6px var(--gap-3);
}

.num {
  color: var(--text-faint);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}

.source {
  color: var(--text-faint);
  font-size: 12px;
}

/*
 * As a VU meter reads: green, then amber, then red as it climbs. The gradient
 * spans the whole track, and what is not reached is covered from the right, so
 * a colour always means the same level.
 */
.track {
  position: relative;
  height: 6px;
  border-radius: 3px;
  background: linear-gradient(to right, var(--ok) 0%, var(--ok) 55%, var(--warn) 75%, var(--bad) 100%);
  overflow: hidden;
}

/* No transition: the meter shows what the analysis hears now, not a smoothed story. */
.rest {
  position: absolute;
  top: 0;
  right: 0;
  height: 100%;
  background: var(--raised-2);
}

.note {
  margin: 0;
  color: var(--text-faint);
  font-size: 12px;
}

.bad {
  color: var(--bad);
}
</style>
