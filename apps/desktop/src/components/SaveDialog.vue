<script setup lang="ts">
/**
 * Saving a layer or a mask, and renaming either (`docs/design/layers-ui.md`
 * §5.2, §5.4): a name, and *What is saved*, a list with a check per line, so
 * that nobody has to guess whether the blend goes with it.
 *
 * A modal `dialog` named by its title (§8, 2.4.3, 2.1.2): `showModal` keeps
 * the focus inside and makes the rest inert; the name takes the focus,
 * selected; Enter saves, Escape cancels; closing gives the focus back to the
 * button that opened it.
 *
 * The parent hands the command as a function: a name Rust refuses is said
 * under the field, which keeps what was typed.
 */
import { computed, nextTick, ref, useId, watch } from 'vue'

import { message } from '../api/journal'
import { SAVED_NAME_MAX, savedName, type SavedItem } from '../composables/layers'
import { t } from '../i18n'

const props = defineProps<{
  open: boolean
  title: string
  /** The name proposed, selected so that typing replaces it. */
  name: string
  /** *What is saved*; empty for a rename. */
  items: readonly SavedItem[]
  /** Under the list: where it will appear. */
  note?: string
  /** *Save*, or *Rename*. */
  confirm: string
  /** Whether a name is taken, ignoring case: said before Rust is asked. */
  taken: (name: string) => boolean
  submit: (name: string) => Promise<void>
}>()

const emit = defineEmits<{
  close: []
}>()

const uid = useId()
const dialog = ref<HTMLDialogElement | null>(null)
const field = ref<HTMLInputElement | null>(null)
const draft = ref('')
const busy = ref(false)
const problem = ref<string | null>(null)

/** Where the focus was when the dialog opened: it goes back there. */
let opener: HTMLElement | null = null

const clash = computed(() => props.taken(draft.value))
const refused = computed(() => (clash.value ? t('layers.save.taken') : problem.value))

watch(
  () => props.open,
  async (open) => {
    if (open) {
      draft.value = props.name
      problem.value = null
      // After the tick: a menu that opened the dialog gives the focus back to
      // its button first, and that button is where it must come back.
      await nextTick()
      opener = document.activeElement instanceof HTMLElement ? document.activeElement : null
      if (!dialog.value?.open) dialog.value?.showModal()
      field.value?.focus()
      field.value?.select()
    } else if (dialog.value?.open) {
      dialog.value.close()
    }
  },
  { immediate: true },
)

/** Closes, whatever asked: the button, Escape, a save done. */
function close(): void {
  if (dialog.value?.open) dialog.value.close()
  emit('close')
  const back = opener
  opener = null
  void nextTick(() => back?.focus())
}

/** Escape: the dialog's own `cancel`, turned into the parent's state. */
function onCancel(e: Event): void {
  e.preventDefault()
  close()
}

async function onSubmit(): Promise<void> {
  const name = savedName(draft.value)
  if (name === '' || clash.value || busy.value) return
  busy.value = true
  problem.value = null
  try {
    await props.submit(name)
    close()
  } catch (e) {
    problem.value = message(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <dialog ref="dialog" class="dialog" :aria-labelledby="`${uid}-title`" @cancel="onCancel">
    <form class="body" @submit.prevent="onSubmit">
      <h2 :id="`${uid}-title`">{{ title }}</h2>

      <label class="name">
        <span>{{ t('layers.save.name') }}</span>
        <input
          ref="field"
          v-model="draft"
          type="text"
          spellcheck="false"
          autocomplete="off"
          :maxlength="SAVED_NAME_MAX"
          :aria-invalid="refused !== null"
          :aria-describedby="refused ? `${uid}-refused` : undefined"
        />
      </label>
      <p v-if="refused" :id="`${uid}-refused`" class="refused" role="alert">{{ refused }}</p>

      <div v-if="items.length" class="saved">
        <p :id="`${uid}-list`" class="list-title">{{ t('layers.save.whatIsSaved') }}</p>
        <ul class="list" :aria-labelledby="`${uid}-list`">
          <li v-for="item in items" :key="item.label" class="item">
            <svg class="tick" viewBox="0 0 12 12" width="12" height="12" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
              <path d="M2 6.5 4.8 9 10 3" />
            </svg>
            <span class="key">{{ item.label }}</span>
            <span class="value">
              <span>{{ item.value }}</span>
              <span v-if="item.note" class="sub">{{ item.note }}</span>
            </span>
          </li>
        </ul>
      </div>

      <p v-if="note" class="note">{{ note }}</p>

      <div class="buttons">
        <button type="button" class="ghost" :disabled="busy" @click="close">
          {{ t('layers.save.cancel') }}
        </button>
        <button
          type="submit"
          class="solid"
          :disabled="busy || savedName(draft) === '' || clash"
        >
          {{ confirm }}
        </button>
      </div>
    </form>
  </dialog>
</template>

<style scoped>
.dialog {
  width: 460px;
  max-width: calc(100vw - 32px);
  padding: 0;
  background: var(--raised);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-lg);
  box-shadow: var(--shadow);
  color: var(--text);
}

.dialog::backdrop {
  background: rgb(8 7 12 / 55%);
}

.body {
  display: flex;
  flex-direction: column;
  gap: var(--gap-3);
  padding: var(--gap-4);
}

h2 {
  font-size: 17px;
  overflow-wrap: anywhere;
}

.name {
  display: flex;
  flex-direction: column;
  gap: 6px;
  color: var(--text-muted);
  font-size: 13px;
}

.name input {
  min-height: 34px;
  padding: 4px 10px;
  background: var(--ground);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  color: var(--text);
  font: inherit;
}

.name input[aria-invalid='true'] {
  border-color: var(--bad);
}

.refused {
  color: var(--bad);
  font-size: 12px;
}

.saved {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
}

.list-title,
.note {
  color: var(--text-muted);
  font-size: 13px;
}

.note {
  font-size: 12px;
}

.list {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  margin: 0;
  padding: 10px var(--gap-3);
  list-style: none;
  border: 1px solid var(--line);
  border-radius: var(--r-md);
}

.item {
  display: grid;
  grid-template-columns: 16px 72px minmax(0, 1fr);
  gap: var(--gap-2);
  align-items: baseline;
  font-size: 13px;
}

.tick {
  color: var(--ok);
}

.key {
  color: var(--text-muted);
}

.value {
  display: flex;
  flex-direction: column;
  gap: 2px;
  overflow-wrap: anywhere;
}

.sub {
  color: var(--text-muted);
  font-size: 12px;
}

.buttons {
  display: flex;
  justify-content: flex-end;
  gap: var(--gap-2);
}

.solid {
  min-height: 34px;
  padding: 6px 14px;
  background: var(--accent);
  border-radius: var(--r-md);
  color: var(--accent-ink);
  font-weight: 600;
}

.ghost {
  min-height: 34px;
  padding: 6px 14px;
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  color: var(--text-muted);
}

.solid:disabled,
.ghost:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
</style>
