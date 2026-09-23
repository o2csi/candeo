<script setup lang="ts">
/**
 * Effect editor, as a full-window mode.
 *
 * `meta.full` hides the navigation bar: the editor is a **mode**, not a tab
 * (`docs/design/studio.md` §2). Code on the left, simulator on the right, at
 * all times.
 *
 * ## Saving previews, applying commits the keyboard
 *
 * The gallery's rule (§8), for the same reason: writing an effect must neither
 * take over the lighting in use nor require owning a keyboard.
 *
 * - Save chains the error check, `save_effect_source` (the file named
 *   after the effect), `transpileModule()` and `cache_effect`, then restarts the
 *   effect in the **preview loop**, on the current device's layout. Not a byte
 *   reaches a keyboard.
 * - Apply starts it on the current device for real, saving first
 *   when the code differs from the saved version, with the parameters the
 *   gallery would use.
 *
 * The save steps fail differently, and each says why: the language service
 * rejects code that does not compile, Rust rejects a name that cannot be a file
 * name, and a module that does not load is saved but reported with its error.
 * Those messages are written to be read, and shown as they are.
 *
 * ## The name is the file name
 *
 * It is edited in the header, never in the code. For an effect that exists,
 * changing it renames the file and moves its settings and draft; for a new
 * effect or a copy, it is the name the first save creates.
 *
 * ## Built-ins are read, not edited
 *
 * Their code and name are read-only, and Duplicate takes the place of Save:
 * the copy is the user's to change, and the built-in keeps
 * receiving updates (`docs/design/effects-library.md` §4). A draft left from
 * before goes with the copy.
 *
 * ## What the simulator shows
 *
 * The device's frames when the device runs exactly the saved version, the
 * preview otherwise — `useSimulatorFeed`, shared with the gallery. Both loops
 * load `effect.js` from disk, so the simulator never shows unsaved code.
 *
 * ## Leaving stops the preview, not the applied effect
 *
 * Nobody is left to watch the preview. The applied effect runs in a Rust thread
 * independent of the window and keeps feeding the keyboard, window closed
 * included.
 *
 * ## One device
 *
 * The engine runs one loop per device. The editor takes `current` without
 * asking: a single device is the common case, and choosing every time would be
 * one more ceremony. The gallery's devices column is where the choice is made.
 */

import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import type { ParamSpec } from '@candeo/effects-api'

import {
  cacheEffect,
  duplicateEffect,
  engineStatus,
  getDefaultLayout,
  legacyEffectIds,
  listEffects,
  readEffectSource,
  renameEffect,
  saveEffectSource,
  startEffect,
  stopEffect,
  type EngineReport,
} from '../api/candeo'
import { effectName, userKey } from '../api/effectKey'
import { error, message } from '../api/journal'
import CodeEditor from '../components/CodeEditor.vue'
import DeviceStatusDot from '../components/DeviceStatusDot.vue'
import FailureNote from '../components/FailureNote.vue'
import KeyboardSimulator from '../components/KeyboardSimulator.vue'
import { useDevice } from '../composables/useDevice'
import { useSettings } from '../composables/useSettings'
import { clearDraft, migrateDrafts, moveDraft, readDraft, writeDraft } from '../editor/draft'
import { transpile } from '../editor/effect'
import { refreshLibrary } from '../editor/library'
import { errors } from '../editor/monaco'
import { t } from '../i18n'
import { NEW_EFFECT } from '../editor/template'
import type { LayoutView } from '../keyboard/layout'
import { useSimulatorFeed } from '../keyboard/simulatorFeed'

/** Idle delay before saving the draft. */
const DRAFT_DELAY = 400
/** Engine polling period, in milliseconds. */
const STATUS_PERIOD = 1000

const route = useRoute()
const router = useRouter()
const { devices, layout, current, refresh } = useDevice()
const { load: loadSettings, reload: reloadSettings, valuesFor, bindingsFor } = useSettings()

/** `/editor` without an identifier = new effect; with one = installed effect. */
const id = computed<string | null>(() => {
  const raw = route.params.id
  return typeof raw === 'string' && raw !== '' ? raw : null
})

const source = ref('')
/** The last version known on disk, to be able to go back to it. */
const saved = ref('')
const loading = ref(true)
const restored = ref(false)
const busy = ref(false)
/** What stopped the last action, already readable. */
const problem = ref<string | null>(null)

/**
 * The parameters the saved version declares.
 *
 * Kept from the library listing at open and from each save, so that applying
 * an unchanged effect does not load the compiler again just to read them.
 */
const savedSpecs = ref<Record<string, ParamSpec>>({})

/** A shipped effect: read-only here, see the header comment. */
const builtin = ref(false)

/** What this screen is about: the effect being written, or that it is new. */
const heading = computed(() => (name.value.trim() ? name.value : t('editor.new')))

/** Errors coming up from Rust are already readable: they are shown as they are. */
// ---------------------------------------------------------------- opening

/** Opens the effect in place; a built-in opens read-only. */
async function open(): Promise<void> {
  loading.value = true
  name.value = id.value === null ? '' : effectName(id.value)
  builtin.value = false
  await migrateDrafts(legacyEffectIds)
  const draft = readDraft(id.value)
  try {
    const disk = id.value === null ? NEW_EFFECT : await readEffectSource(id.value)

    if (id.value !== null) {
      const entry = (await listEffects()).find((e) => e.id === id.value)
      savedSpecs.value = entry?.params ?? {}
      builtin.value = entry?.kind === 'builtin'
    }

    saved.value = disk
    restored.value = draft !== null && draft !== disk
    source.value = restored.value && draft !== null ? draft : disk
  } catch (e) {
    // An unreadable effect must not leave an empty editor: if a draft is left,
    // that is what is shown, along with what failed.
    problem.value = message(e)
    saved.value = draft ?? ''
    source.value = draft ?? ''
    restored.value = draft !== null
  } finally {
    loading.value = false
  }
}

/** Takes the saved version back and throws the draft away. */
function discard(): void {
  source.value = saved.value
  restored.value = false
  clearDraft(id.value)
}

// ---------------------------------------------------------------- name

/**
 * The effect's name, which is its file name.
 *
 * Edited here and nowhere else: sources no longer declare it. Committed when the
 * field is validated, not on every keystroke, since for an existing effect it
 * renames a file.
 */
const name = ref('')

/** True while the effect has no file yet. */
const creating = computed(() => id.value === null)

/**
 * Renames an existing effect's file, with its settings and its draft. A new
 * effect only keeps the name for its first save.
 */
async function rename(): Promise<void> {
  const wanted = name.value.trim()
  const current = id.value
  if (creating.value || current === null) {
    name.value = wanted
    return
  }
  if (wanted === effectName(current)) return
  if (wanted === '') {
    name.value = effectName(current)
    return
  }
  await act(async () => {
    let renamed: string
    try {
      renamed = await renameEffect(current, wanted)
    } catch (e) {
      name.value = effectName(current)
      throw e
    }
    moveDraft(current, renamed)
    await router.replace({ name: 'editor', params: { id: renamed } })
    // The settings moved on disk: the gallery reads them back rather than guess.
    await reloadSettings()
  })
}

// ---------------------------------------------------------------- draft

let draftTimer = 0

/**
 * Continuous saving, after a pause in typing.
 *
 * **A text identical to the saved version is not a draft**: it is then cleared
 * instead of written. Without this rule, going back to the version on disk, or
 * simply undoing one's changes, would leave a ghost draft that would resurface
 * at the next opening, and the editor would announce a restore that restores
 * nothing.
 */
watch(source, (value) => {
  window.clearTimeout(draftTimer)
  draftTimer = window.setTimeout(() => {
    if (value === saved.value) clearDraft(id.value)
    else writeDraft(id.value, value)
  }, DRAFT_DELAY)
})

// ---------------------------------------------------------------- engine

/** The whole engine report, re-read at once: one round trip per second. */
const report = ref<EngineReport>({ devices: [], preview: null })

const status = computed(() => {
  const device = current.value
  if (!device) return null
  return (
    report.value.devices.find((s) => s.device.vid === device.vid && s.device.pid === device.pid) ??
    null
  )
})

const deviceKey = computed(() =>
  current.value ? `${current.value.vid}:${current.value.pid}` : null,
)

/** The target device as listed, for its product name and its state. */
const targetDevice = computed(() => {
  const device = current.value
  if (!device) return null
  return devices.value.find((d) => d.vid === device.vid && d.pid === device.pid) ?? null
})

/** The product name, as the gallery's devices column shows it. */
const deviceName = computed(() => targetDevice.value?.name ?? null)

async function refreshStatus(): Promise<void> {
  try {
    report.value = await engineStatus()
  } catch (e) {
    problem.value = message(e)
  }
}

/**
 * True when Apply must save first.
 *
 * A device loop runs the JavaScript compiled from the saved file, so a new
 * effect, which has no file yet, must be saved first.
 */
const unsaved = computed(() => creating.value || source.value !== saved.value)

/**
 * This effect is the one the current device runs.
 *
 * Stop stops whatever the device runs, so it is offered only then.
 */
const runsHere = computed(
  () =>
    id.value !== null &&
    status.value?.running === true &&
    status.value.effectId === id.value,
)

/**
 * The saved text the device loop was started from, as far as this screen knows.
 *
 * The engine reports which effect a device runs, not which version: the loop
 * keeps the JavaScript it loaded at start. Without this, saving an applied
 * effect would keep showing the device's stale frames instead of the new code.
 */
const appliedSource = ref<{ device: string; text: string } | null>(null)

const showsDevice = computed(
  () =>
    runsHere.value &&
    appliedSource.value !== null &&
    appliedSource.value.device === deviceKey.value &&
    appliedSource.value.text === saved.value,
)

/** Nothing left to apply: the device already runs this very code. */
const applied = computed(() => showsDevice.value && !unsaved.value)

/**
 * False until settings, devices, source and engine report are read. Previewing
 * earlier would build a QuickJS context the report may make useless at once.
 */
const ready = ref(false)

/**
 * Fallback layout, asked of Rust rather than copied here: an effect gets written
 * before anything is plugged in, or without owning the keyboard.
 */
const fallback = ref<LayoutView | null>(null)
const board = computed<LayoutView | null>(() => layout.value ?? fallback.value)

const { frame, restartPreview } = useSimulatorFeed({
  layout: () => board.value,
  device: () => current.value,
  showsDevice: () => showsDevice.value,
  // A new effect has no file to preview until its first save.
  previewed: () => (ready.value ? id.value : null),
  params: () => valuesFor(current.value, id.value ?? '', savedSpecs.value),
  bindings: () => bindingsFor(current.value, id.value ?? '', savedSpecs.value),
  onError: (e) => {
    problem.value = message(e)
  },
})

/** The preview's error, only while the preview is this effect's. */
const previewError = computed<string | null>(() => {
  const p = report.value.preview
  return p !== null && p.effectId === id.value ? p.error : null
})

/**
 * The error of the loop on screen. A device still running an older version can
 * fail where the saved code does not, and the other way round: the error shown
 * next to the code must be about what the simulator draws.
 */
const effectError = computed<string | null>(() =>
  showsDevice.value ? (status.value?.error ?? null) : previewError.value,
)

/** Which source the simulator draws, as a state. */
const simNote = computed(() => {
  if (showsDevice.value) {
    return t('editor.simDevice', { device: deviceName.value ?? t('editor.theDevice') })
  }
  const p = report.value.preview
  if (p?.running === true && p.effectId === id.value) return t('editor.simPreview')
  if (previewError.value !== null) return t('editor.simStopped')
  return id.value === null ? t('editor.simNone') : t('editor.simStarting')
})

// ---------------------------------------------------------------- actions

/**
 * Checks the source, writes its file in the user's folder, compiles it and
 * records the result, and returns the effect's key.
 *
 * Refusals come in this order: the language service rejects code that does not
 * compile, Rust rejects a name that cannot be a file name or that is taken. A
 * module that does not load is **saved anyway** and reported with its error: the
 * file is the author's work, and it can only be fixed once it exists.
 *
 * The text is read once, up front: what is typed while saving was not saved and
 * must not be marked as such.
 */
async function install(): Promise<string> {
  const text = source.value
  const target = name.value.trim()
  if (target === '') throw new Error(t('editor.nameRequired'))

  const found = await errors()
  if (found.length > 0) {
    const first = found[0]
    throw new Error(
      t(
        'editor.compileErrors',
        { n: found.length, line: first.line, message: first.message },
        found.length,
      ),
    )
  }

  const key = userKey(target)
  const hash = await saveEffectSource(key, text, creating.value)
  clearDraft(id.value)
  restored.value = false
  saved.value = text
  // Carrying the key in the route is what makes reopening this screen read this
  // effect back, and what the preview follows.
  if (id.value !== key) await router.replace({ name: 'editor', params: { id: key } })

  const entry = await cacheEffect(key, hash, await transpile(text))
  savedSpecs.value = entry.params ?? {}
  if (entry.state === 'broken') {
    throw new Error(entry.error ?? t('editor.savedButBroken'))
  }
  return key
}

/** Runs one action, shows what stopped it, and re-reads the engine either way. */
async function act(task: () => Promise<void>): Promise<void> {
  busy.value = true
  problem.value = null
  try {
    await task()
  } catch (e) {
    problem.value = message(e)
    // **The only place an effect compile error exists.** The transpiler lives
    // in the window: without this line the refusal is gone from the screen by
    // the time the log is opened. The effect id goes with it, since a log read
    // an hour later does not know what was displayed.
    error('editor', `${id.value ?? 'new effect'}: ${problem.value}`, e)
  } finally {
    busy.value = false
    await refreshStatus()
  }
}

/** Installs, then shows the saved code in the preview loop. */
const save = () =>
  act(async () => {
    await install()
    // Same id, new code on disk: nothing the feed watches has changed.
    restartPreview()
  })

/**
 * Starts this effect on the current device for real, saving first when needed.
 *
 * Nothing subscribes here: `showsDevice` turns true once the version is
 * recorded, and the feed opens the device channel then. Recording it only after
 * `start_effect` returns is what matters: the channel lives in the new loop's
 * state, and subscribing to the loop being replaced would freeze the simulator
 * without an error.
 */
const applyToDevice = () =>
  act(async () => {
    const device = current.value
    if (!device) return
    const effectId = (unsaved.value ? null : id.value) ?? (await install())
    // The gallery's rule, through the same helpers: manifest defaults overridden
    // by what is remembered for this device, and the settings bound to a signal.
    // Two rules would light one effect differently depending on the screen it
    // was applied from.
    await startEffect(
      device,
      effectId,
      valuesFor(device, effectId, savedSpecs.value),
      bindingsFor(device, effectId, savedSpecs.value),
    )
    appliedSource.value = { device: `${device.vid}:${device.pid}`, text: saved.value }
    // Rust has just remembered the applied effect. The gallery reads this shared
    // state back rather than guessing it.
    await reloadSettings()
  })

/**
 * Copies a built-in and opens the copy, carrying the text on screen: a draft
 * restored on a built-in cannot be saved there, and must not be lost.
 */
const duplicate = () =>
  act(async () => {
    const original = id.value
    if (original === null) return
    const copy = await duplicateEffect(original)
    if (source.value !== saved.value) writeDraft(copy, source.value)
    clearDraft(original)
    // The copy takes the original's cache, which is stale when the file changed
    // outside the application: compiled first, so that its preview starts.
    await refreshLibrary()
    await router.replace({ name: 'editor', params: { id: copy } })
    await open()
  })

/**
 * Stops the device loop. The last frame stays on the keyboard, since stopping
 * does not turn the LEDs off; the preview comes back once the report says so.
 */
const halt = () =>
  act(async () => {
    const device = current.value
    if (!device) return
    await stopEffect(device)
    // The stop forgot the applied effect on disk, for the same reason as above.
    await reloadSettings()
  })

// ---------------------------------------------------------------- lifecycle

let statusTimer = 0
/** False once unmounted: opening chains several round trips to Rust. */
let alive = true

onMounted(async () => {
  void getDefaultLayout().then((l) => {
    fallback.value = l
  })

  // First: applying and previewing start from the remembered parameters, and
  // reading them later would let an effect start on its defaults.
  await loadSettings()
  // The editor may be the first view shown, through a direct link to
  // `/editor/:id`: without this, no device would be targeted.
  await refresh()
  await open()
  await refreshStatus()
  if (!alive) return

  // The effect may already run on this device: started in an earlier session,
  // from the gallery or from the tray. It loaded what is on disk, which is the
  // saved version unless something was installed since without applying it.
  if (runsHere.value && deviceKey.value !== null) {
    appliedSource.value = { device: deviceKey.value, text: saved.value }
  }
  ready.value = true

  statusTimer = window.setInterval(() => void refreshStatus(), STATUS_PERIOD)
})

onBeforeUnmount(() => {
  alive = false
  window.clearInterval(statusTimer)
  window.clearTimeout(draftTimer)
})
</script>

<template>
  <section class="page">
    <!--
      The editor is a mode, not a tab: it replaces the window, and the name in
      the header is an input, not a title. Read aloud, the screen had nothing
      saying what it is about (#157).
    -->
    <h1 class="sr-only">{{ heading }}</h1>

    <header class="head">
      <button class="ghost" @click="router.push('/')">{{ t('editor.back') }}</button>

      <!-- The name is the file name: renaming here renames the file. -->
      <label class="name">
        <span class="sr-only">{{ t('editor.name') }}</span>
        <input
          v-model="name"
          type="text"
          :disabled="loading || busy || builtin"
          :placeholder="t('editor.name')"
          @change="rename"
          @keyup.enter="rename"
        />
      </label>

      <p v-if="creating" class="what">{{ t('editor.new') }}</p>
      <p v-else-if="builtin" class="what">{{ t('editor.builtinReadOnly') }}</p>

      <span class="spacer" />

      <!--
        The editor has no devices column: the target device's dot is the only
        place here that shows it dropped. Same component as the gallery's cards.
      -->
      <DeviceStatusDot v-if="targetDevice" :device="targetDevice" />

      <!-- Stops what the device runs, whichever effect it is: offered only for this one. -->
      <button class="ghost" :disabled="busy || !runsHere" @click="halt">{{ t('editor.stop') }}</button>
      <!--
        The device goes in the tooltip and the accessible name, not the label: a
        product name is long enough to wrap the header onto a second row.
      -->
      <button
        v-if="deviceName"
        class="ghost"
        :disabled="busy || loading || applied || (builtin && unsaved)"
        :title="t(applied ? 'editor.appliedOn' : 'editor.applyOn', { device: deviceName })"
        :aria-label="t(applied ? 'editor.appliedOn' : 'editor.applyOn', { device: deviceName })"
        @click="applyToDevice"
      >
        {{ applied ? t('editor.applied') : t('editor.apply') }}
      </button>
      <button v-if="builtin" class="solid" :disabled="busy || loading" @click="duplicate">
        {{ busy ? t('editor.wait') : t('editor.duplicate') }}
      </button>
      <button v-else class="solid" :disabled="busy || loading" @click="save">
        {{ busy ? t('editor.wait') : t('editor.save') }}
      </button>
    </header>

    <div class="split">
      <div class="pane code-pane">
        <p v-if="runsHere && status?.deviceError" class="notice warn" role="alert">
          {{ message(status.deviceError) }}
        </p>

        <p v-if="restored" class="notice" role="status">
          {{ builtin ? t('editor.draftRestoredBuiltin') : t('editor.draftRestored') }}
          <button class="link" @click="discard">{{ t('editor.discard') }}</button>
        </p>

        <CodeEditor v-model="source" :disabled="loading || builtin" class="code" />

        <!--
          One place for every failure. What was just attempted comes first: it
          describes the last gesture, whereas a loop error describes something
          that keeps running.
        -->
        <FailureNote v-if="problem" class="failure" @close="problem = null">
          {{ problem }}
        </FailureNote>
        <!-- The loop keeps raising: closing this would only hide what is happening. -->
        <FailureNote v-else-if="effectError" class="failure" :closable="false">
          {{ t('editor.effectError', { error: effectError }) }}
        </FailureNote>
        <p v-else class="hint">
          {{ t('editor.hint') }}
        </p>
      </div>

      <div class="pane sim">
        <div class="sim-head">
          <h2>{{ t('editor.simulator') }}</h2>
          <p class="sim-note">{{ simNote }}</p>
          <p v-if="!layout" class="sim-note">{{ t('editor.noKeyboard') }}</p>
        </div>

        <!--
          The layout comes from Rust: it is null for the length of a round trip.
          No empty keyboard is drawn in the meantime.
        -->
        <KeyboardSimulator v-if="board" class="sim-board" :layout="board" :frame="frame" />
      </div>
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

.name input {
  padding: 4px var(--gap-2);
  border: 1px solid transparent;
  border-radius: var(--r-md);
  background: none;
  color: var(--text);
  font: inherit;
  font-weight: 600;
  /* Wide enough for a name, without pushing the rest of the bar. */
  width: 22ch;
}

/* The border only appears on hover or while typing: at rest, it is a title. */
.name input:hover:not(:disabled),
.name input:focus {
  border-color: var(--line-strong);
  background: var(--raised-2);
}

.name input::placeholder {
  color: var(--text-faint);
  font-weight: 400;
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

.split {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 1px;
  background: var(--line);
  min-height: 0;
}

@media (width <= 900px) {
  .split {
    grid-template-columns: 1fr;
    grid-template-rows: 1fr 1fr;
  }
}

/* `min-width` and `min-height` at zero: without them, a grid cell refuses to
   shrink below its content's intrinsic size and overflows the window, which
   Monaco, measuring its container, would amplify. */
.pane {
  background: var(--ground);
  min-width: 0;
  min-height: 0;
}

.code-pane {
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.code {
  flex: 1;
  min-height: 0;
}

.sim {
  display: flex;
  flex-direction: column;
  gap: var(--gap-3);
  padding: var(--gap-4);
}

.sim-head {
  display: flex;
  flex-direction: column;
  gap: var(--gap-1);
}

.sim-note {
  max-width: 68ch;
  color: var(--text-faint);
  font-size: 12px;
}

.sim-board {
  flex: 1;
  min-height: 0;
}

.notice,
.failure,
.hint {
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

/*
 * A gap between what was asked and what is happening, not mere information.
 * Color does not carry it alone: the text says it too.
 */
.notice.warn {
  background: color-mix(in srgb, var(--warn) 12%, var(--raised));
  border-bottom-color: var(--warn);
  color: var(--text);
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
}

.hint {
  border-top: 1px solid var(--line);
  color: var(--text-faint);
}
</style>
