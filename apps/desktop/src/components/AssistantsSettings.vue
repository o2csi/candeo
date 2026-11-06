<script setup lang="ts">
/**
 * Settings › Assistants (#384, `docs/design/mcp-server.md` §8): whether
 * assistants on this computer may connect through MCP, which are connected, and
 * the text that connects one.
 *
 * Off, the block is its switch alone. Turning it off is the revocation: Rust
 * closes the pipe and drops every assistant connected.
 *
 * Its root is a `.block` of the Settings page, which gives it its place and its
 * separator; the rest is styled here.
 */
import { computed, onBeforeUnmount, onMounted, ref, useId } from 'vue'
import type { UnlistenFn } from '@tauri-apps/api/event'

import { getMcp, onMcpChanged, setMcp, type McpStatus } from '../api/candeo'
import { message, warn } from '../api/journal'
import { ASSISTANTS, snippet, type Assistant } from '../composables/assistants'
import { t } from '../i18n'
import FailureNote from './FailureNote.vue'

const uid = useId()

const status = ref<McpStatus | null>(null)
/** What kept the switch from being read or turned, as Rust says it. */
const problem = ref<string | null>(null)
/** What copying gave, in one line. */
const copied = ref<string | null>(null)
const chosen = ref<Assistant>('claudeDesktop')

const text = computed(() =>
  status.value?.command ? snippet(chosen.value, status.value.command) : null,
)

async function read(): Promise<void> {
  try {
    status.value = await getMcp()
  } catch (e) {
    problem.value = message(e)
  }
}

async function toggle(event: Event): Promise<void> {
  const box = event.target as HTMLInputElement
  const on = box.checked
  problem.value = null
  copied.value = null
  try {
    status.value = await setMcp(on)
  } catch (e) {
    problem.value = message(e)
    box.checked = !on
    await read()
  }
}

/**
 * Copies the text to paste. When the web view refuses the clipboard, the text
 * stays shown to be selected. It holds a path on this computer: never logged.
 */
async function copy(): Promise<void> {
  if (!text.value) return
  copied.value = null
  try {
    await navigator.clipboard.writeText(text.value)
    copied.value = t('settings.assistants.copied')
  } catch (e) {
    copied.value = t('settings.assistants.copyRefused')
    warn('assistants', `clipboard unavailable: ${message(e, 'en')}`)
  }
}

let unlisten: UnlistenFn | null = null
let gone = false

onMounted(async () => {
  await read()
  unlisten = await onMcpChanged(() => void read()).catch(() => null)
  // Left before the listener was in place: nobody else will remove it.
  if (gone) unlisten?.()
})

onBeforeUnmount(() => {
  gone = true
  unlisten?.()
})
</script>

<template>
  <section v-if="status" class="block" aria-labelledby="assistants-title">
    <h2 id="assistants-title">{{ t('settings.assistants.title') }}</h2>

    <FailureNote v-if="problem" class="err" @close="problem = null">{{ problem }}</FailureNote>

    <label class="level">
      <input type="checkbox" :checked="status.enabled" @change="toggle" />
      {{ t('settings.assistants.allow') }}
    </label>
    <p class="note">{{ t('settings.assistants.allowDetail') }}</p>

    <template v-if="status.enabled">
      <FailureNote v-if="status.listening === 'taken'" class="err" :closable="false">
        {{ t('settings.assistants.taken') }}
      </FailureNote>
      <FailureNote v-else-if="status.listening === 'failed'" class="err" :closable="false">
        {{ t('settings.assistants.failed') }}
      </FailureNote>
      <template v-else>
        <template v-if="status.assistants.length">
          <h3>{{ t('settings.assistants.connected') }}</h3>
          <ul class="connected" role="status">
            <li v-for="(name, i) in status.assistants" :key="i">
              {{ name || t('settings.assistants.unnamed') }}
            </li>
          </ul>
        </template>
        <p v-else class="note" role="status">{{ t('settings.assistants.none') }}</p>
      </template>

      <h3>{{ t('settings.assistants.connect') }}</h3>
      <div class="level">
        <label :for="`${uid}-assistant`">{{ t('settings.assistants.assistant') }}</label>
        <select :id="`${uid}-assistant`" v-model="chosen" @change="copied = null">
          <option v-for="assistant in ASSISTANTS" :key="assistant" :value="assistant">
            {{ t(`settings.assistants.names.${assistant}`) }}
          </option>
        </select>
      </div>
      <template v-if="text">
        <p class="note">{{ t(`settings.assistants.where.${chosen}`) }}</p>
        <pre class="snippet">{{ text }}</pre>
        <button type="button" class="ghost" @click="copy">
          {{ t('settings.assistants.copy') }}
        </button>
        <p v-if="copied" class="note" role="status">{{ copied }}</p>
      </template>
      <p v-else class="note">{{ t('settings.assistants.noCommand') }}</p>
    </template>
  </section>
</template>

<style scoped>
.block {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--gap-2);
}

h3 {
  margin-top: var(--gap-2);
}

.level {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--gap-2);
}

.level select {
  padding: 5px var(--gap-2);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  background: var(--raised);
  color: var(--text);
  font-size: 13px;
}

.connected {
  margin: 0;
  padding-left: var(--gap-4);
  font-size: 13px;
}

/* Selectable: the fallback when the clipboard refuses, and what is pasted. */
.snippet {
  align-self: stretch;
  margin: 0;
  padding: var(--gap-3);
  overflow: auto;
  background: var(--raised-2);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  font-family: var(--font-mono);
  font-size: 12px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  user-select: text;
}

.ghost {
  padding: 6px var(--gap-3);
  border-radius: var(--r-md);
  font-size: 13px;
  border: 1px solid var(--line-strong);
  color: var(--text-muted);
}

.ghost:hover:not(:disabled) {
  color: var(--text);
  background: var(--raised-2);
}

.note {
  margin: 0;
  color: var(--text-faint);
  font-size: 12px;
}

.err {
  margin: 0;
  color: var(--bad);
  font-size: 12px;
}
</style>
