<script setup lang="ts">
/**
 * *Mask and blend*: what a layer may light, how it combines with what is under
 * it, and how much (`docs/design/layers-ui.md` §3, §4.3, §4.4). Absent for the
 * base, which has none of them.
 *
 * The blend is a radio group of native radios, so arrows change the mode, and
 * it is described by the selected mode's sentence (§8). The opacity is a
 * native range with its value, and what it follows is the brightness follow's
 * switch. A saved layer previewed whole shows its values read-only until it is
 * added (§5.3): the fieldset disables every control at once.
 */
import { useId } from 'vue'

import type { Dimming, HeldSignal } from '../api/candeo'
import type { Blend, Look, Mask, MaskChoices } from '../api/layers'
import {
  BLENDS,
  blendName,
  blendOf,
  blendSentence,
  opacityOf,
  type MaskAction,
  type MaskContext,
  type MaskPick,
} from '../composables/layers'
import { t } from '../i18n'
import BrightnessFollow from './BrightnessFollow.vue'
import MaskPicker from './MaskPicker.vue'

defineProps<{
  mask: Mask | undefined
  look: Look
  /** A saved layer previewed: its values shown, not changed. */
  readOnly: boolean
  choices: MaskChoices | null
  ctx: MaskContext
  /** What the mask lights here, `null` while unknown. */
  count: number | null
  /** How many layers name the saved mask in force. */
  usage: number
  deleted: { name: string; count: number } | null
  signals: readonly HeldSignal[]
}>()

const emit = defineEmits<{
  mask: [pick: MaskPick]
  maskAction: [action: MaskAction]
  undoMask: []
  blend: [blend: Blend]
  /** `commit`: the slider released, the moment to write it down. */
  opacity: [value: number, commit: boolean]
  follows: [follows: Dimming | null, commit: boolean]
}>()

const uid = useId()

function onOpacity(e: Event, commit: boolean): void {
  emit('opacity', Number((e.target as HTMLInputElement).value), commit)
}
</script>

<template>
  <section class="section look" :aria-labelledby="`${uid}-title`">
    <h2 :id="`${uid}-title`" class="title">{{ t('layers.look.title') }}</h2>
    <fieldset class="grid" :disabled="readOnly">
      <label class="label" :for="`${uid}-mask`">{{ t('layers.look.mask') }}</label>
      <div class="control">
        <MaskPicker
          :id="`${uid}-mask`"
          :mask="mask"
          :choices="choices"
          :ctx="ctx"
          :count="count"
          :usage="usage"
          :deleted="deleted"
          :disabled="readOnly"
          @pick="emit('mask', $event)"
          @action="emit('maskAction', $event)"
          @undo="emit('undoMask')"
        />
        <!-- Said where it is chosen: the row says it too, in fewer words. -->
        <p v-if="count === 0" class="nothing" role="status">{{ t('layers.nothingHereCard') }}</p>
      </div>

      <span :id="`${uid}-blend`" class="label">{{ t('layers.look.blend') }}</span>
      <div class="control">
        <div
          class="blends"
          role="radiogroup"
          :aria-labelledby="`${uid}-blend`"
          :aria-describedby="`${uid}-blend-desc`"
        >
          <label v-for="b in BLENDS" :key="b" class="seg" :class="{ on: blendOf(look) === b }">
            <input
              type="radio"
              class="radio"
              :name="`${uid}-blend`"
              :value="b"
              :checked="blendOf(look) === b"
              @change="emit('blend', b)"
            />
            {{ blendName(b) }}
          </label>
        </div>
        <p :id="`${uid}-blend-desc`" class="sentence">{{ blendSentence(blendOf(look)) }}</p>
      </div>

      <label class="label" :for="`${uid}-opacity`">{{ t('layers.look.opacity') }}</label>
      <div class="control">
        <div class="opacity">
          <input
            :id="`${uid}-opacity`"
            type="range"
            min="0"
            max="100"
            step="1"
            :value="opacityOf(look)"
            @input="onOpacity($event, false)"
            @change="onOpacity($event, true)"
          />
          <span class="value">{{ t('effects.percent', { n: opacityOf(look) }) }}</span>
        </div>
        <BrightnessFollow
          :id="`${uid}-follow`"
          subject="opacity"
          :dimming="look.follows ?? null"
          :firmware="false"
          :signals="signals"
          @change="(d, commit) => emit('follows', d, commit)"
        />
      </div>
    </fieldset>
  </section>
</template>

<style scoped>
.title {
  margin-bottom: var(--gap-3);
  color: var(--text-faint);
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

/* `min-width: 0`: a fieldset is as wide as its longest label otherwise. */
.grid {
  display: grid;
  grid-template-columns: 96px minmax(0, 1fr);
  gap: 14px var(--gap-4);
  align-items: start;
  min-width: 0;
  margin: 0;
  padding: 0;
  border: 0;
}

.grid:disabled {
  opacity: 0.7;
}

.label {
  padding-top: 6px;
  color: var(--text-muted);
  font-size: 13px;
}

.control {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.nothing {
  color: var(--warn);
  font-size: 12px;
}

.blends {
  display: flex;
  flex-wrap: wrap;
  align-self: flex-start;
  overflow: hidden;
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
}

/* 30 px high: segments past the 24 px minimum (WCAG 2.5.8); the selected one in the accent's words. */
.seg {
  position: relative;
  display: inline-flex;
  align-items: center;
  min-height: 30px;
  padding: 4px var(--gap-3);
  font-size: 13px;
  cursor: pointer;
}

.seg + .seg {
  border-left: 1px solid var(--line);
}

.seg.on {
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 600;
}

.seg:focus-within {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}

/* The radio itself is read out and takes the arrows; the label is what is seen. */
.radio {
  position: absolute;
  width: 1px;
  height: 1px;
  opacity: 0;
}

.sentence {
  color: var(--text-muted);
  font-size: 12px;
}

.opacity {
  display: flex;
  gap: var(--gap-3);
  align-items: center;
}

.opacity input {
  flex: 0 1 240px;
  accent-color: var(--accent);
}

.value {
  min-width: 44px;
  font-family: var(--font-mono);
  font-size: 13px;
}

/* The follow switch, under the slider it acts on, as wide as the card allows. */
.control :deep(.follow) {
  max-width: 360px;
}
</style>
