<script setup lang="ts">
/**
 * A device definition, in the editor: a built-in one read only, with *Copy to
 * yours* to change it; one of yours saved in place. The schema checks it as it
 * is typed (`editor/monaco.ts`); saving says whether it drives anything, and
 * opens its device again on it.
 */

import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import {
  copyDeviceDefinition,
  deviceDefinitionProblems,
  readDeviceDefinition,
  saveDeviceDefinition,
} from '../api/candeo'
import { message } from '../api/journal'
import CodeEditor from '../components/CodeEditor.vue'
import FailureNote from '../components/FailureNote.vue'
import { clearDraft, readDraft, writeDraft } from '../editor/draft'
import { t } from '../i18n'

const route = useRoute()
const router = useRouter()

const origin = computed(() => (route.params.origin === 'yours' ? 'yours' : 'builtIn'))
const file = computed(() => String(route.params.file))
const builtIn = computed(() => origin.value === 'builtIn')
/** Apart from effects' drafts: a file name is not an effect's key. */
const draftId = computed(() => `device:${file.value}`)

const text = ref('')
/** What the file holds on disk. */
const saved = ref('')
const loading = ref(true)
const busy = ref(false)
const restored = ref(false)
/** Why the file drives nothing, in English, as its author reads it. */
const reason = ref<string | null>(null)
/** What went wrong with the last action. */
const problem = ref<string | null>(null)

const unsaved = computed(() => !builtIn.value && text.value !== saved.value)

async function load(): Promise<void> {
  loading.value = true
  problem.value = null
  reason.value = null
  try {
    const disk = await readDeviceDefinition(origin.value, file.value)
    const draft = builtIn.value ? null : readDraft(draftId.value)
    saved.value = disk
    restored.value = draft !== null && draft !== disk
    text.value = restored.value && draft !== null ? draft : disk
    if (!builtIn.value) {
      const problems = await deviceDefinitionProblems()
      reason.value = problems.find((p) => p.file === file.value)?.reason ?? null
    }
  } catch (e) {
    problem.value = message(e)
  } finally {
    loading.value = false
  }
}

watch(() => route.fullPath, load, { immediate: true })

// A draft after a pause, as for effects (`editor/draft.ts`): leaving without
// saving loses nothing.
let pending: number | undefined
watch(text, (value) => {
  if (loading.value || builtIn.value) return
  window.clearTimeout(pending)
  pending = window.setTimeout(() => {
    if (value === saved.value) clearDraft(draftId.value)
    else writeDraft(draftId.value, value)
  }, 400)
})
onBeforeUnmount(() => window.clearTimeout(pending))

function discard(): void {
  clearDraft(draftId.value)
  text.value = saved.value
  restored.value = false
}

async function act(action: () => Promise<void>): Promise<void> {
  busy.value = true
  problem.value = null
  try {
    await action()
  } catch (e) {
    problem.value = message(e)
  } finally {
    busy.value = false
  }
}

const save = () =>
  act(async () => {
    const value = text.value
    reason.value = await saveDeviceDefinition(file.value, value)
    saved.value = value
    window.clearTimeout(pending)
    clearDraft(draftId.value)
    restored.value = false
  })

/** The copy opens in place of the built-in one, ready to change. */
const copy = () =>
  act(async () => {
    const copied = await copyDeviceDefinition(file.value)
    await router.replace({ name: 'definition', params: { origin: 'yours', file: copied } })
  })
</script>

<template>
  <section class="page">
    <h1 class="sr-only">{{ t('definition.title') }}</h1>

    <header class="head">
      <button class="ghost" @click="router.push({ name: 'devices' })">{{ t('editor.back') }}</button>
      <span class="file">{{ file }}</span>
      <p v-if="builtIn" class="what">{{ t('editor.builtinReadOnly') }}</p>
      <span class="spacer" />
      <button
        v-if="builtIn"
        class="solid"
        :disabled="busy || loading"
        :title="t('devices.copyTitle')"
        @click="copy"
      >
        {{ busy ? t('editor.wait') : t('devices.copy') }}
      </button>
      <button v-else class="solid" :disabled="busy || loading || !unsaved" @click="save">
        {{ busy ? t('editor.wait') : t('editor.save') }}
      </button>
    </header>

    <div class="body">
      <p v-if="restored" class="notice" role="status">
        {{ t('editor.draftRestored') }}
        <button class="link" @click="discard">{{ t('editor.discard') }}</button>
      </p>

      <CodeEditor
        v-model="text"
        language="json"
        :disabled="loading || builtIn"
        class="code"
        @save="busy || !unsaved || save()"
      />

      <FailureNote v-if="problem" class="failure" @close="problem = null">{{ problem }}</FailureNote>
      <FailureNote v-else-if="reason" class="failure" :closable="false">
        {{ t('definition.drivesNothing', { reason }) }}
      </FailureNote>
    </div>
  </section>
</template>

<style scoped>
.page {
  display: grid;
  grid-template-rows: auto 1fr;
  height: 100%;
}

.head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--gap-3);
  padding: var(--gap-3) var(--gap-4);
  border-bottom: 1px solid var(--line);
}

.file {
  font-family: var(--font-mono);
  font-weight: 600;
}

.what {
  color: var(--text-faint);
  font-family: var(--font-mono);
  font-size: 12px;
}

.spacer {
  flex: 1;
}

.ghost {
  padding: 4px var(--gap-3);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  color: var(--text-muted);
  font-size: 13px;
}

.solid {
  padding: 5px var(--gap-3);
  background: var(--accent);
  color: var(--accent-ink);
  border-radius: var(--r-md);
  font-size: 13px;
  font-weight: 500;
}

.ghost:disabled,
.solid:disabled {
  opacity: 0.45;
  cursor: default;
}

/* Zero minimums: Monaco measures its container, and a grid cell would
   otherwise grow with it. */
.body {
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: var(--ground);
}

.code {
  flex: 1;
  min-height: 0;
}

.notice,
.failure {
  padding: var(--gap-2) var(--gap-3);
  font-size: 12px;
}

.notice {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-2);
  background: var(--raised);
  border-bottom: 1px solid var(--line);
  color: var(--text-muted);
}

.link {
  color: var(--accent);
  font-size: 12px;
  text-decoration: underline;
}

.failure {
  background: color-mix(in srgb, var(--bad) 10%, transparent);
  border-top: 1px solid var(--bad);
  color: var(--text);
  overflow-wrap: anywhere;
}
</style>
