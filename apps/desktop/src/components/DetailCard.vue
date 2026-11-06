<script setup lang="ts">
/**
 * The card of the selected row: the base, a layer, or the new layer
 * (`docs/design/layers-ui.md` §3).
 *
 * Its header names the row and its effect — the swatch, *Layer 2 of 3*, the
 * effect as the screen's only `h1` — and carries *Save the layer…*, which
 * keeps both sections under it: the place answers what is saved before the
 * dialog does. The sections come as the default slot, *Mask and blend* then
 * *Effect*; the header's own actions, *Stop* and *Resume* for the base, as
 * `actions`.
 */
import { useId } from 'vue'

import { t } from '../i18n'
import EffectSwatch from './EffectSwatch.vue'

defineProps<{
  /** *Base*, *Layer 2 of 3*, *New layer · from "CI on Escape"*. */
  kicker: string
  /** The effect's name, or *Choose an effect*. */
  title: string
  /** The effect's swatch; `null` draws an empty dashed frame, for no effect yet. */
  swatch: string[] | null
  /** Under the title, for a new layer without an effect. */
  hint?: string
  /** Offers *Save the layer…*: a layer that previews nothing. */
  saveable?: boolean
}>()

const emit = defineEmits<{
  save: []
}>()

const uid = useId()
</script>

<template>
  <section class="card" :aria-labelledby="`${uid}-title`">
    <header class="head">
      <EffectSwatch v-if="swatch" class="mark" :colors="swatch" />
      <span v-else class="mark none" aria-hidden="true" />
      <div class="names">
        <span class="kicker">{{ kicker }}</span>
        <h1 :id="`${uid}-title`">{{ title }}</h1>
        <p v-if="hint" class="hint">{{ hint }}</p>
      </div>
      <span class="spacer" />
      <slot name="actions" />
      <button
        v-if="saveable"
        type="button"
        class="save"
        aria-haspopup="dialog"
        @click="emit('save')"
      >
        <svg viewBox="0 0 12 14" width="12" height="14" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round">
          <path d="M2 1.5h8v11L6 9.5l-4 3z" />
        </svg>
        {{ t('layers.card.saveLayer') }}
      </button>
    </header>
    <slot />
  </section>
</template>

<style scoped>
.card {
  display: flex;
  flex-direction: column;
  background: var(--raised);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
}

.head {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-3);
  align-items: center;
  padding: 14px var(--gap-4) var(--gap-3);
}

/* Over the gallery's 34 × 18: the card's swatch is the row's, larger. */
.head .mark {
  width: 32px;
  height: 32px;
  border-radius: var(--r-md);
}

.head .mark.none {
  flex: none;
  border: 1px dashed var(--line-strong);
}

.names {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.kicker {
  color: var(--text-faint);
  font-size: 11px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

h1 {
  overflow-wrap: anywhere;
}

.hint {
  color: var(--text-muted);
  font-size: 13px;
}

.spacer {
  flex: 1;
}

.save {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  min-height: 32px;
  padding: 4px var(--gap-3);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  color: var(--text);
  font-size: 13px;
}

.save:hover {
  border-color: var(--accent);
  color: var(--accent);
}

/* Each section under a rule of its own, as the card frames them. */
.card > :deep(.section) {
  padding: 14px var(--gap-4) var(--gap-4);
  border-top: 1px solid var(--line);
}
</style>
