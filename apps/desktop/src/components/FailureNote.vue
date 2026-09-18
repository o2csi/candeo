<script setup lang="ts">
/**
 * What went wrong, said once, in a line someone can copy and close.
 *
 * The window forbids selecting text everywhere else — a desktop application is
 * not a page — but an error is precisely the text a bug report needs verbatim,
 * and retyping it invites a typo in the one sentence that has to be exact.
 *
 * The cross is asked for, not guessed. A failure the window cannot clear — an
 * appliance's own state, read at each refresh — says `closable: false`, because
 * a cross that closes nothing is a lie.
 *
 * **It is a property and not a listener on purpose.** Reading `useAttrs()` for
 * an `onClose` looked tidier and was wrong: Vue takes a declared emit out of the
 * attributes, so the test was always false and no message anywhere had a cross —
 * a whole feature shipped invisible.
 */
import { t } from '../i18n'

withDefaults(defineProps<{ closable?: boolean }>(), { closable: true })

defineEmits<{ close: [] }>()
</script>

<template>
  <p role="alert">
    <span class="said"><slot /></span>
    <button
      v-if="closable"
      type="button"
      class="dismiss"
      :title="t('app.dismiss')"
      :aria-label="t('app.dismiss')"
      @click="$emit('close')"
    >
      ×
    </button>
  </p>
</template>

<style scoped>
p {
  display: flex;
  align-items: flex-start;
  gap: var(--gap-2);
}

.said {
  flex: 1;
  user-select: text;
  cursor: text;
}

.dismiss {
  flex: none;
  padding: 0;
  background: none;
  border: 0;
  color: inherit;
  font-family: inherit;
  font-size: 15px;
  line-height: 1.1;
  opacity: 0.6;
  cursor: pointer;
}

.dismiss:hover,
.dismiss:focus-visible {
  opacity: 1;
}
</style>
