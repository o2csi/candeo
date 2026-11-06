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
 *
 * Given the `failure` itself rather than its text, the note says it in the
 * current language and offers the way out when there is one: Windows Security,
 * for a write Controlled folder access refused (#394).
 */
import { openControlledFolderAccess } from '../api/candeo'
import { blockedByWindows, error, message } from '../api/journal'
import { t } from '../i18n'

withDefaults(defineProps<{ closable?: boolean; failure?: unknown }>(), {
  closable: true,
  failure: undefined,
})

defineEmits<{ close: [] }>()

function openWindowsSecurity(): void {
  openControlledFolderAccess().catch((e: unknown) => {
    error('failure-note', `Windows Security not opened: ${message(e, 'en')}`, e)
  })
}
</script>

<template>
  <p role="alert">
    <span class="said">
      <slot>{{ message(failure) }}</slot>
      <template v-if="blockedByWindows(failure)">
        {{ ' ' }}
        <button type="button" class="way-out" @click="openWindowsSecurity">
          {{ t('app.openWindowsSecurity') }}
        </button>
      </template>
    </span>
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

.way-out {
  padding: 0;
  font-weight: 600;
  text-decoration: underline;
  text-underline-offset: 2px;
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
