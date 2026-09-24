<script setup lang="ts">
/**
 * Settings › Signals (#108, `docs/design/inputs-and-automations.md` §2.3):
 * whether other software can send Candeo named values, where it sends them, with
 * which token, and what is held now.
 *
 * Off, the block is its switch alone. On, it holds everything a sender needs, the
 * signals held with the time each has left and what reads each, the names
 * something reads that nothing sends now, and a test signal, so that a rule can
 * be tried before any sender exists.
 *
 * Its root is a `.block` of the Settings page, which gives it its place and its
 * separator; the rest is styled here.
 */
import { computed, onBeforeUnmount, onMounted, ref, useId } from 'vue'
import type { UnlistenFn } from '@tauri-apps/api/event'

import {
  eraseSignal,
  getSettings,
  getSignalsApi,
  listEffects,
  listNetworkInterfaces,
  listSignals,
  onSignalsChanged,
  renewSignalsToken,
  sendSignal,
  setSignalsApi,
  type EffectEntry,
  type HeldSignal,
  type NetworkInterface,
  type SignalsApi,
} from '../api/candeo'
import { message, warn } from '../api/journal'
import {
  PORT_MAX,
  PORT_MIN,
  alive,
  interfaceRows,
  signalText,
  signalsAddress,
  ticking,
  timeLeft,
  validPort,
} from '../composables/signals'
import {
  expectedSignals,
  readerText,
  signalReaders,
  switchedOff,
  type ReadingSettings,
  type SignalReader,
} from '../composables/signalReaders'
import { useDevice } from '../composables/useDevice'
import { t } from '../i18n'
import FailureNote from './FailureNote.vue'

const uid = useId()
const { devices } = useDevice()

/** The token hidden: a fixed length, so the mask says nothing of the token. */
const MASK = '•'.repeat(16)

/** Rust tries a refused port again every 10 s (`FOLLOW`): while it is taken, this follows. */
const PORT_RETRY_MS = 10_000

const api = ref<SignalsApi | null>(null)
const up = ref<NetworkInterface[]>([])
const held = ref<HeldSignal[]>([])
/** What kept a gesture on the API from happening, as Rust says it. */
const problem = ref<string | null>(null)
/** What kept the test signal from being sent. */
const sendProblem = ref<string | null>(null)
/** What copying gave, in one line. */
const copied = ref<string | null>(null)
const tokenShown = ref(false)
/** New token was asked for, and waits for its confirmation. */
const renewing = ref(false)
/**
 * Whether an interface was ticked here. The warning that other machines can send
 * is said then, once, below the list, rather than at each visit.
 */
const ticked = ref(false)
/** The window's clock, for the countdowns. */
const now = ref(Date.now())

/**
 * The bindings and rules, read when the block opens: they change on other
 * screens, and this one is mounted again each time Settings is shown.
 */
const reading = ref<ReadingSettings>({ effectParams: [], rules: [] })
/** The library, to name effects and their settings. */
const library = ref<EffectEntry[]>([])

const testName = ref('')
const testValue = ref('')

const rows = computed(() => (api.value ? interfaceRows(up.value, api.value.interfaces) : []))
const shown = computed(() => alive(held.value, now.value))
const readers = computed(() => signalReaders(reading.value, devices.value, library.value))
const expected = computed(() => expectedSignals(readers.value, shown.value))
const networkWarning = computed(() => ticked.value && (api.value?.interfaces.length ?? 0) > 0)

function readersOf(name: string): SignalReader[] {
  return readers.value.get(name) ?? []
}

function left(signal: HeldSignal): string {
  const time = timeLeft(signal.expires, now.value)
  return t(time.key, { n: time.n ?? 0 })
}

// ---------------------------------------------------------------- reading

async function readApi(): Promise<void> {
  try {
    api.value = await getSignalsApi()
  } catch (e) {
    problem.value = message(e)
  }
}

async function readInterfaces(): Promise<void> {
  try {
    up.value = await listNetworkInterfaces()
  } catch (e) {
    problem.value = message(e)
  }
}

async function readHeld(): Promise<void> {
  try {
    held.value = await listSignals()
  } catch (e) {
    problem.value = message(e)
  }
}

/**
 * What reads signals, and the names to say it with. Without the library, effects
 * and settings go by their keys: the lines stay true, only less readable.
 */
async function readReaders(): Promise<void> {
  const [settings, effects] = await Promise.all([
    getSettings().catch((e: unknown) => {
      problem.value = message(e)
      return null
    }),
    listEffects().catch((e: unknown) => {
      warn('signals', `library not listed, effects named by their keys: ${message(e, 'en')}`)
      return null
    }),
  ])
  if (settings) reading.value = { effectParams: settings.effectParams, rules: settings.rules }
  if (effects) library.value = effects
}

// ---------------------------------------------------------------- the API

/** Asks Rust for these settings; it answers with where it listens now. Whether it took them. */
async function apply(enabled: boolean, port: number, interfaces: string[]): Promise<boolean> {
  problem.value = null
  try {
    api.value = await setSignalsApi(enabled, port, interfaces)
    return true
  } catch (e) {
    problem.value = message(e)
    await readApi()
    return false
  }
}

async function toggle(event: Event): Promise<void> {
  if (!api.value) return
  const box = event.target as HTMLInputElement
  const on = box.checked
  if (!(await apply(on, api.value.port, api.value.interfaces))) {
    box.checked = !on
    return
  }
  if (on) await Promise.all([readInterfaces(), readHeld(), readReaders()])
}

async function choosePort(event: Event): Promise<void> {
  if (!api.value) return
  const field = event.target as HTMLInputElement
  const port = Number(field.value)
  if (field.value !== '' && port === api.value.port) return
  if (!validPort(port)) {
    // Said with Rust's own words: past 65535 it could not even read the number.
    if (field.value !== '') {
      problem.value = t('errors.signalsPortInvalid', { port: field.value })
    }
    field.value = String(api.value.port)
    return
  }
  if (!(await apply(api.value.enabled, port, api.value.interfaces))) {
    field.value = String(api.value?.port ?? '')
  }
}

async function chooseInterface(name: string, event: Event): Promise<void> {
  if (!api.value) return
  const box = event.target as HTMLInputElement
  const on = box.checked
  if (!(await apply(api.value.enabled, api.value.port, ticking(api.value.interfaces, name, on)))) {
    box.checked = !on
    return
  }
  if (on) ticked.value = true
}

async function renew(): Promise<void> {
  problem.value = null
  try {
    api.value = await renewSignalsToken()
    renewing.value = false
  } catch (e) {
    problem.value = message(e)
  }
}

/**
 * Copies the address or the token. When the web view refuses the clipboard the
 * text is shown to be selected instead, the token included. Nothing copied is
 * ever logged: the token must not reach a log someone attaches to a bug report.
 */
async function copy(text: string, token: boolean): Promise<void> {
  copied.value = null
  try {
    await navigator.clipboard.writeText(text)
    copied.value = t('settings.signals.copied')
  } catch (e) {
    copied.value = t('settings.signals.copyRefused')
    if (token) tokenShown.value = true
    warn('signals', `clipboard unavailable: ${message(e, 'en')}`)
  }
}

// ---------------------------------------------------------------- signals held

async function erase(name: string): Promise<void> {
  problem.value = null
  try {
    await eraseSignal(name)
    await readHeld()
  } catch (e) {
    problem.value = message(e)
  }
}

async function send(): Promise<void> {
  sendProblem.value = null
  try {
    await sendSignal(testName.value.trim(), testValue.value)
    await readHeld()
  } catch (e) {
    sendProblem.value = message(e)
  }
}

// ---------------------------------------------------------------- lifecycle

let unlisten: UnlistenFn | null = null
let gone = false
const clock = window.setInterval(() => (now.value = Date.now()), 1000)
const retry = window.setInterval(() => {
  if (api.value?.enabled && api.value.portInUse) void readApi()
}, PORT_RETRY_MS)

onMounted(async () => {
  await readApi()
  if (api.value?.enabled) await Promise.all([readInterfaces(), readHeld(), readReaders()])
  unlisten = await onSignalsChanged(() => void readHeld()).catch(() => null)
  // Left before the listener was in place: nobody else will remove it.
  if (gone) unlisten?.()
})

onBeforeUnmount(() => {
  gone = true
  unlisten?.()
  window.clearInterval(clock)
  window.clearInterval(retry)
})
</script>

<template>
  <section v-if="api" class="block" aria-labelledby="signals-title">
    <h2 id="signals-title">{{ t('settings.signals.title') }}</h2>

    <FailureNote v-if="problem" class="err" @close="problem = null">{{ problem }}</FailureNote>

    <label class="level">
      <input type="checkbox" :checked="api.enabled" @change="toggle" />
      {{ t('settings.signals.receive') }}
    </label>
    <p class="note">{{ t('settings.signals.receiveDetail') }}</p>

    <template v-if="api.enabled">
      <div class="grid">
        <span class="label">{{ t('settings.signals.address') }}</span>
        <span class="row">
          <code class="value">{{ signalsAddress(api.port) }}</code>
          <button type="button" class="ghost" @click="copy(signalsAddress(api.port), false)">
            {{ t('settings.signals.copy') }}
          </button>
        </span>

        <label class="label" :for="`${uid}-port`">{{ t('settings.signals.port') }}</label>
        <span class="row">
          <input
            :id="`${uid}-port`"
            class="count"
            type="number"
            :min="PORT_MIN"
            :max="PORT_MAX"
            step="1"
            :value="api.port"
            @change="choosePort"
          />
        </span>

        <span class="label">{{ t('settings.signals.token') }}</span>
        <span class="row">
          <code v-if="tokenShown" class="value token">{{ api.token }}</code>
          <span v-else class="value">
            <span aria-hidden="true">{{ MASK }}</span>
            <span class="sr-only">{{ t('settings.signals.tokenHidden') }}</span>
          </span>
          <span class="buttons">
            <button type="button" class="ghost" @click="tokenShown = !tokenShown">
              {{ tokenShown ? t('settings.signals.hide') : t('settings.signals.show') }}
            </button>
            <button type="button" class="ghost" @click="copy(api.token, true)">
              {{ t('settings.signals.copy') }}
            </button>
            <!-- Stays in place while the question is asked: focus reaches the answer next. -->
            <button type="button" class="ghost" @click="renewing = true">
              {{ t('settings.signals.newToken') }}
            </button>
          </span>
        </span>
      </div>

      <FailureNote v-if="api.portInUse" class="err" :closable="false">
        {{ t('settings.signals.portInUse', { port: api.port }) }}
      </FailureNote>
      <p v-if="copied" class="note" role="status">{{ copied }}</p>

      <div v-if="renewing" class="confirm" role="group" :aria-labelledby="`${uid}-renew`">
        <p :id="`${uid}-renew`" class="confirm-title" role="alert">
          {{ t('settings.signals.renewTitle') }}
        </p>
        <p class="detail">{{ t('settings.signals.renewDetail') }}</p>
        <div class="actions">
          <button type="button" class="solid" @click="renew">
            {{ t('settings.signals.renewConfirm') }}
          </button>
          <button type="button" class="ghost" @click="renewing = false">
            {{ t('settings.signals.cancel') }}
          </button>
        </div>
      </div>

      <fieldset class="where">
        <legend>{{ t('settings.signals.listenOn') }}</legend>
        <!-- Loopback is always listened on: shown ticked, so the list says the whole truth. -->
        <label class="level">
          <input type="checkbox" checked disabled />
          {{ t('settings.signals.loopback') }}
        </label>
        <label v-for="row in rows" :key="row.name" class="level">
          <input type="checkbox" :checked="row.ticked" @change="chooseInterface(row.name, $event)" />
          {{
            row.address === null
              ? t('settings.signals.interfaceDown', { name: row.name })
              : t('settings.signals.interface', { name: row.name, address: row.address })
          }}
        </label>
      </fieldset>
      <p v-if="networkWarning" class="warn" role="status">
        {{ t('settings.signals.networkWarning') }}
      </p>

      <h3>{{ t('settings.signals.held') }}</h3>
      <table v-if="shown.length" class="held">
        <thead>
          <tr>
            <th scope="col">{{ t('settings.signals.name') }}</th>
            <th scope="col">{{ t('settings.signals.value') }}</th>
            <th scope="col">{{ t('settings.signals.timeLeft') }}</th>
            <td />
          </tr>
        </thead>
        <tbody>
          <template v-for="signal in shown" :key="signal.name">
            <tr>
              <td class="mono selectable">{{ signal.name }}</td>
              <td class="mono selectable">{{ signalText(signal.value) }}</td>
              <td class="left">{{ left(signal) }}</td>
              <td>
                <button type="button" class="ghost small" @click="erase(signal.name)">
                  {{ t('settings.signals.erase') }}
                </button>
              </td>
            </tr>
            <!-- A signal nothing reads has nothing under it: send it, see it, then bind it. -->
            <tr v-if="readersOf(signal.name).length" class="under">
              <td colspan="4">
                <ul class="readers">
                  <li v-for="(reader, i) in readersOf(signal.name)" :key="i">
                    {{ readerText(reader) }}
                    <span v-if="switchedOff(reader)" class="off">
                      {{ t('settings.signals.ruleOff') }}
                    </span>
                  </li>
                </ul>
              </td>
            </tr>
          </template>
        </tbody>
      </table>
      <p v-else class="note">{{ t('settings.signals.none') }}</p>

      <template v-if="expected.length">
        <h3>{{ t('settings.signals.expected') }}</h3>
        <ul class="expected">
          <li v-for="signal in expected" :key="signal.name">
            <span class="mono selectable">{{ signal.name }}</span>
            <ul class="readers">
              <li v-for="(reader, i) in signal.readers" :key="i">
                {{ readerText(reader) }}
                <span v-if="switchedOff(reader)" class="off">
                  {{ t('settings.signals.ruleOff') }}
                </span>
              </li>
            </ul>
          </li>
        </ul>
      </template>

      <h3>{{ t('settings.signals.test') }}</h3>
      <FailureNote v-if="sendProblem" class="err" @close="sendProblem = null">
        {{ sendProblem }}
      </FailureNote>
      <form class="level" @submit.prevent="send">
        <label :for="`${uid}-name`">{{ t('settings.signals.name') }}</label>
        <input
          :id="`${uid}-name`"
          v-model="testName"
          class="field mono"
          type="text"
          spellcheck="false"
          autocomplete="off"
          :list="`${uid}-names`"
        />
        <!-- The names something waits for too: a test signal is how a binding is tried. -->
        <datalist :id="`${uid}-names`">
          <option v-for="signal in shown" :key="signal.name" :value="signal.name" />
          <option v-for="signal in expected" :key="signal.name" :value="signal.name" />
        </datalist>
        <label :for="`${uid}-value`">{{ t('settings.signals.value') }}</label>
        <input
          :id="`${uid}-value`"
          v-model="testValue"
          class="field mono"
          type="text"
          spellcheck="false"
          autocomplete="off"
        />
        <button type="submit" class="ghost" :disabled="!testName.trim()">
          {{ t('settings.signals.send') }}
        </button>
      </form>
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

/* Label, then what it names and its actions: the three settings a sender copies line up. */
.grid {
  display: grid;
  grid-template-columns: max-content 1fr;
  align-items: center;
  gap: var(--gap-2) var(--gap-3);
}

.label {
  color: var(--text-muted);
  font-size: 13px;
}

.row {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--gap-2);
  min-width: 0;
}

.value {
  font-family: var(--font-mono);
  font-size: 12px;
}

/* A token shown wraps in its own space; the buttons beside it stay together. */
.token {
  flex: 1 1 16em;
  min-width: 0;
}

.buttons {
  display: flex;
  flex: none;
  gap: var(--gap-2);
}

/* Selectable: the fallback when the clipboard refuses, and what a sender is set up with. */
.token,
.value,
.selectable {
  user-select: text;
  overflow-wrap: anywhere;
}

.count,
.field {
  padding: 5px var(--gap-2);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  background: var(--raised);
  color: var(--text);
  font-size: 13px;
}

.count {
  width: 7em;
}

.field {
  width: 12em;
}

.where {
  display: flex;
  flex-direction: column;
  gap: var(--gap-1);
  margin: 0;
  padding: 0;
  border: 0;
}

.where legend {
  margin-bottom: var(--gap-1);
  padding: 0;
  color: var(--text-muted);
  font-size: 13px;
}

.held {
  border-collapse: collapse;
  font-size: 13px;
}

.held th {
  color: var(--text-muted);
  font-weight: 500;
  text-align: left;
}

.held th,
.held td {
  padding: 3px var(--gap-3) 3px 0;
}

/* Wide enough that the columns stay put as signals come and go. */
.held th {
  min-width: 7em;
}

/* Digits of one width, so a countdown does not jitter. */
.left {
  font-variant-numeric: tabular-nums;
}

/* What reads a signal sits under its row, closer to it than to the next signal. */
.held .under td {
  padding-top: 0;
  padding-bottom: var(--gap-2);
}

.readers {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin: 0;
  padding: 0 0 0 var(--gap-3);
  list-style: none;
  color: var(--text-muted);
  font-size: 12px;
}

.readers li {
  overflow-wrap: anywhere;
}

.expected {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  margin: 0;
  padding: 0;
  list-style: none;
  font-size: 13px;
}

.expected .readers {
  margin-top: 2px;
}

/* A rule switched off reads nothing until someone switches it on. */
.off {
  margin-left: var(--gap-1);
  padding: 0 var(--gap-1);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  color: var(--text-faint);
  font-size: 11px;
  white-space: nowrap;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-2);
}

.solid,
.ghost {
  padding: 6px var(--gap-3);
  border-radius: var(--r-md);
  font-size: 13px;
}

.ghost.small {
  padding: 2px var(--gap-2);
  font-size: 12px;
}

.solid {
  background: var(--accent);
  color: var(--accent-ink);
  font-weight: 500;
}

.ghost {
  border: 1px solid var(--line-strong);
  color: var(--text-muted);
}

.ghost:hover:not(:disabled) {
  color: var(--text);
  background: var(--raised-2);
}

.solid:disabled,
.ghost:disabled {
  opacity: 0.45;
  cursor: not-allowed;
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

/* Surprising rather than broken: other machines can now reach Candeo. */
.warn {
  margin: 0;
  align-self: stretch;
  padding: var(--gap-2) var(--gap-3);
  border: 1px solid var(--warn);
  border-radius: var(--r-md);
  background: color-mix(in srgb, var(--warn) 10%, transparent);
  font-size: 13px;
}

.confirm {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  align-self: stretch;
  padding: var(--gap-3);
  background: color-mix(in srgb, var(--warn) 8%, var(--raised));
  border: 1px solid var(--warn);
  border-radius: var(--r-md);
}

.confirm-title {
  font-weight: 600;
}

.detail {
  color: var(--text-muted);
  font-size: 13px;
}
</style>
