<script setup lang="ts">
/**
 * Settings: what concerns the application as a whole, and no device.
 *
 * The language, the log and the configuration reset.
 *
 * The reset stays **out of the library**: the neighbouring gesture there deletes
 * hand-written code.
 */

import { onMounted, ref } from 'vue'

import {
  diagnostic,
  getJournal,
  getLanguage,
  getLaunchAtLogin,
  getSettings,
  openLogDir,
  openRelease,
  resetSettings,
  setLanguage,
  setLaunchAtLogin,
  setLogFilesKept,
  setLogLevel,
  setResumeEffects,
  type JournalStatus,
  type LanguageSetting,
  type LanguageStatus,
  type LaunchAtLogin,
  type LogLevel,
} from '../api/candeo'
import { erreur, message } from '../api/journal'
import FailureNote from '../components/FailureNote.vue'
import { useDevice } from '../composables/useDevice'
import { useEffects } from '../composables/useEffects'
import { useSettings } from '../composables/useSettings'
import { useTheme } from '../composables/useTheme'
import { useUpdateCheck, type Found } from '../composables/useUpdateCheck'
import { showIn, t } from '../i18n'

const { busy, refresh } = useDevice()
const { dropAll } = useSettings()
const { forgetPosed } = useEffects()
const { load: loadTheme } = useTheme()

// ---------------------------------------------------------------- language

const language = ref<LanguageStatus | null>(null)
const languageProblem = ref<string | null>(null)

/** Each language named in itself: whoever cannot read the current one finds theirs. */
const LANGUAGES = ['en', 'fr'] as const

async function readLanguage(): Promise<void> {
  language.value = await getLanguage()
}

async function chooseLanguage(event: Event): Promise<void> {
  languageProblem.value = null
  try {
    language.value = await setLanguage((event.target as HTMLSelectElement).value as LanguageSetting)
    showIn(language.value.language)
  } catch (e) {
    languageProblem.value = message(e)
    await readLanguage()
  }
}

// ---------------------------------------------------------------- startup

/** Whether a device that opens starts its applied effect again; `null` until read. */
const resume = ref<boolean | null>(null)
const startupProblem = ref<string | null>(null)

async function readResume(): Promise<void> {
  try {
    resume.value = (await getSettings()).preferences.resumeEffects ?? true
  } catch (e) {
    startupProblem.value = message(e)
  }
}

async function chooseResume(event: Event): Promise<void> {
  startupProblem.value = null
  const on = (event.target as HTMLInputElement).checked
  try {
    await setResumeEffects(on)
    resume.value = on
  } catch (e) {
    startupProblem.value = message(e)
    await readResume()
  }
}

// ---------------------------------------------------------------- version

const {
  status: update,
  found,
  // `asking` is taken in this view: it is the reset confirmation.
  asking: checking,
  start: startUpdateCheck,
  checkNow,
  choose: chooseUpdateCheck,
} = useUpdateCheck()

/** What the check found, in one line. */
function foundNote(state: Found): string {
  switch (state.state) {
    case 'upToDate':
      return t('settings.version.upToDate')
    case 'newer':
      return t('settings.version.newer', { version: state.release.version })
    case 'unreadable':
      return t('settings.version.unreadable', { version: state.release.version })
    default:
      return t('settings.version.failed')
  }
}

async function chooseCheck(event: Event): Promise<void> {
  versionProblem.value = null
  try {
    await chooseUpdateCheck((event.target as HTMLInputElement).checked)
  } catch (e) {
    versionProblem.value = message(e)
  }
}

async function open(url: string): Promise<void> {
  versionProblem.value = null
  try {
    await openRelease(url)
  } catch (e) {
    versionProblem.value = message(e)
  }
}

const versionProblem = ref<string | null>(null)

/** The system's login entry; `null` until read. Not in `settings.json`: see `autostart.rs`. */
const login = ref<LaunchAtLogin | null>(null)

async function readLogin(): Promise<void> {
  try {
    login.value = await getLaunchAtLogin()
  } catch (e) {
    startupProblem.value = message(e)
  }
}

/** What the note under the setting says, and why no entry can be written. */
function loginNote(status: LaunchAtLogin): string {
  if (status.available) {
    return t('settings.startup.loginDetail')
  }
  switch (status.refused) {
    case 'unsupported':
      return t('settings.startup.loginUnsupported')
    default:
      return t('settings.startup.loginUnavailable')
  }
}

async function chooseLogin(event: Event): Promise<void> {
  startupProblem.value = null
  try {
    login.value = await setLaunchAtLogin((event.target as HTMLInputElement).checked)
  } catch (e) {
    startupProblem.value = message(e)
    await readLogin()
  }
}

// ---------------------------------------------------------------- log

/** The five levels, said by what they bring rather than by their technical name. */
const LEVELS: readonly LogLevel[] = ['error', 'warn', 'info', 'debug', 'trace']

const journal = ref<JournalStatus | null>(null)
/** What kept the log from being read or changed. */
const journalProblem = ref<string | null>(null)
/** The diagnostic, once asked for. Shown even when copying worked. */
const report = ref<string | null>(null)
/** What copying gave, in one line. */
const copied = ref<string | null>(null)

async function readJournal(): Promise<void> {
  try {
    journal.value = await getJournal()
  } catch (e) {
    journalProblem.value = message(e)
  }
}

/**
 * Changes the level without restarting. Rust returns the resulting state: when
 * `CANDEO_LOG` imposes the level, the setting is written but not applied, and
 * only Rust can say so.
 */
async function chooseLevel(event: Event): Promise<void> {
  journalProblem.value = null
  try {
    journal.value = await setLogLevel((event.target as HTMLSelectElement).value as LogLevel)
  } catch (e) {
    journalProblem.value = message(e)
    // The menu now shows a level that was not kept.
    await readJournal()
  }
}

/** How many daily files to keep; `0` keeps them all. Rust deletes the extra ones at once. */
async function chooseFilesKept(event: Event): Promise<void> {
  journalProblem.value = null
  const keep = Math.trunc(Number((event.target as HTMLInputElement).value))
  if (!Number.isFinite(keep) || keep < 0) {
    await readJournal()
    return
  }
  try {
    journal.value = await setLogFilesKept(keep)
  } catch (e) {
    journalProblem.value = message(e)
    await readJournal()
  }
}

async function showLogs(): Promise<void> {
  journalProblem.value = null
  try {
    await openLogDir()
  } catch (e) {
    journalProblem.value = message(e)
  }
}

/**
 * The diagnostic, copied **and** shown: the web view may refuse the clipboard,
 * and a text believed copied is found missing when pasting into a bug report.
 */
async function copyDiagnostic(): Promise<void> {
  journalProblem.value = null
  copied.value = null
  try {
    const text = await diagnostic()
    report.value = text
    try {
      await navigator.clipboard.writeText(text)
      copied.value = t('settings.log.copied')
    } catch (e) {
      copied.value = t('settings.log.copyRefused')
      erreur('diagnostic', `clipboard unavailable: ${message(e, 'en')}`, e)
    }
  } catch (e) {
    journalProblem.value = message(e)
  }
}

// ---------------------------------------------------------------- reset

/** The question is asked and waits for its answer. */
const asking = ref(false)
const working = ref(false)
/** What kept the reset from happening, as Rust says it. */
const problem = ref<string | null>(null)

/**
 * Resets `settings.json`. Rust stops the loops, turns the backlight off and
 * closes the devices before writing, and touches no effect.
 */
async function reset(): Promise<void> {
  problem.value = null
  working.value = true
  try {
    await resetSettings()
    // The window keeps what it read: without these, the next slider move would
    // write back the settings just erased, and the gallery would mark as applied
    // a hardware effect Rust just switched off.
    dropAll()
    forgetPosed()
    asking.value = false
  } catch (e) {
    problem.value = message(e)
  } finally {
    working.value = false
    // Every device changed state at once, and the log level and the language went
    // back to their defaults.
    await refresh()
    await readJournal()
    await readLanguage()
    await readResume()
    await loadTheme()
    if (language.value) showIn(language.value.language)
  }
}

onMounted(() => {
  void readLanguage()
  void readResume()
  void readLogin()
  void readJournal()
  // Reads the setting again; the check itself already ran at startup, and runs
  // once per window.
  void startUpdateCheck()
})
</script>

<template>
  <section class="page">
    <header class="head">
      <h1>{{ t('settings.title') }}</h1>
    </header>

    <section v-if="language" class="block" aria-labelledby="language-title">
      <h2 id="language-title">{{ t('settings.language.title') }}</h2>

      <FailureNote v-if="languageProblem" class="err" @close="languageProblem = null">{{ languageProblem }}</FailureNote>

      <div class="level">
        <label for="language">{{ t('settings.language.label') }}</label>
        <select id="language" :value="language.setting" @change="chooseLanguage">
          <option value="system">
            {{
              t('settings.language.system', {
                language: t(`settings.language.names.${language.system}`),
              })
            }}
          </option>
          <option v-for="code in LANGUAGES" :key="code" :value="code">
            {{ t(`settings.language.names.${code}`) }}
          </option>
        </select>
      </div>
    </section>

    <section v-if="resume !== null" class="block" aria-labelledby="startup-title">
      <h2 id="startup-title">{{ t('settings.startup.title') }}</h2>

      <FailureNote v-if="startupProblem" class="err" @close="startupProblem = null">{{ startupProblem }}</FailureNote>

      <label class="level">
        <input type="checkbox" :checked="resume" @change="chooseResume" />
        {{ t('settings.startup.resume') }}
      </label>
      <p class="note">{{ t('settings.startup.resumeDetail') }}</p>

      <template v-if="login">
        <label class="level">
          <input
            id="launch-at-login"
            type="checkbox"
            :checked="login.enabled"
            :disabled="!login.available"
            @change="chooseLogin"
          />
          {{ t('settings.startup.login') }}
        </label>
        <p class="note">
          {{ loginNote(login) }}
        </p>
      </template>
    </section>

    <section v-if="update" class="block" aria-labelledby="version-title">
      <h2 id="version-title">{{ t('settings.version.title', { version: update.version }) }}</h2>

      <FailureNote v-if="versionProblem" class="err" @close="versionProblem = null">{{ versionProblem }}</FailureNote>

      <label class="level">
        <input
          type="checkbox"
          :checked="update.enabled"
          :disabled="!update.available"
          @change="chooseCheck"
        />
        {{ t('settings.version.check') }}
      </label>
      <p class="note">
        {{ update.available ? t('settings.version.checkDetail') : t('settings.version.store') }}
      </p>

      <template v-if="update.available">
        <p class="level">
          <button type="button" class="ghost" :disabled="checking" @click="checkNow">
            {{ t('settings.version.checkNow') }}
          </button>
          <span v-if="checking" class="note">{{ t('settings.version.asking') }}</span>
          <span v-else-if="found" class="note">{{ foundNote(found) }}</span>
        </p>
        <p v-if="found && 'release' in found" class="note">
          <button type="button" class="ghost" @click="open(found.release.url)">
            {{ t('settings.version.openRelease') }}
          </button>
        </p>
      </template>
    </section>

    <section v-if="journal" class="block" aria-labelledby="journal-title">
      <h2 id="journal-title">{{ t('settings.log.title') }}</h2>

      <FailureNote v-if="journalProblem" class="err" @close="journalProblem = null">{{ journalProblem }}</FailureNote>

      <div class="level">
        <label for="log-level">{{ t('settings.log.level') }}</label>
        <select
          id="log-level"
          :value="journal.setting ?? 'info'"
          :disabled="busy"
          @change="chooseLevel"
        >
          <option v-for="level in LEVELS" :key="level" :value="level">
            {{ t(`settings.log.levels.${level}`) }}
          </option>
        </select>
      </div>

      <div class="level">
        <label for="log-files">{{ t('settings.log.filesKept') }}</label>
        <input
          id="log-files"
          class="count"
          type="number"
          min="0"
          max="3650"
          :value="journal.filesKept"
          @change="chooseFilesKept"
        />
        <span class="note">{{ t('settings.log.filesKeptDetail') }}</span>
      </div>

      <!--
        A high level carries per-frame records and survives restarts: left on
        and forgotten, it fills the disk, since rotation caps the number of files,
        not the size of today's.
      -->
      <p v-if="journal.verbose" class="warn" role="status">{{ t('settings.log.verbose') }}</p>

      <!-- The environment variable always wins: say so rather than let a setting look applied. -->
      <p v-if="journal.forcedByEnv" class="note" role="status">
        {{ t('settings.log.forcedByEnv', { level: journal.level ?? t('settings.log.requested') }) }}
      </p>

      <div class="actions">
        <button class="ghost" :disabled="!journal.dir" @click="showLogs">
          {{ t('settings.log.openFolder') }}
        </button>
        <button class="ghost" @click="copyDiagnostic">{{ t('settings.log.copyDiagnostic') }}</button>
      </div>

      <p v-if="journal.dir" class="mono path">{{ journal.dir }}</p>
      <p v-else class="err">{{ t('settings.log.noFolder') }}</p>

      <p v-if="copied" class="note" role="status">{{ copied }}</p>
      <pre v-if="report" class="report">{{ report }}</pre>
    </section>

    <section class="block" aria-labelledby="config-title">
      <h2 id="config-title">{{ t('settings.config.title') }}</h2>

      <FailureNote v-if="problem" class="err" @close="problem = null">{{ problem }}</FailureNote>

      <!--
        The button stays in place and enabled while the question is asked:
        hiding it would take keyboard focus away right where it must reach the
        answer, which follows in the document.
      -->
      <button class="ghost danger" :disabled="busy || working" @click="asking = true">
        {{ t('settings.config.reset') }}
      </button>

      <div v-if="asking" class="confirm" role="group" aria-labelledby="confirm-reset">
        <p id="confirm-reset" class="confirm-title" role="alert">
          {{ t('settings.config.confirmTitle') }}
        </p>
        <!-- What goes, listed, and what does not, said as plainly: the last line matters most. -->
        <ul class="what">
          <li>{{ t('settings.config.detected') }}</li>
          <li>{{ t('settings.config.stopped') }}</li>
          <li>{{ t('settings.config.forgetDevices') }}</li>
          <li>{{ t('settings.config.forgetParams') }}</li>
          <li>
            <strong>{{ t('settings.config.effectsKept') }}</strong>
            {{ t('settings.config.effectsKeptDetail') }}
          </li>
        </ul>
        <div class="confirm-actions">
          <button class="solid danger" :disabled="working" @click="reset">
            {{ t('settings.config.confirm') }}
          </button>
          <button class="ghost" :disabled="working" @click="asking = false">
            {{ t('settings.config.cancel') }}
          </button>
        </div>
      </div>
    </section>
  </section>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  gap: var(--gap-4);
  padding: var(--gap-4);
  max-width: 720px;
}

.block {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--gap-2);
}

.block + .block {
  padding-top: var(--gap-4);
  border-top: 1px solid var(--line);
}

.level {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--gap-2);
}

.level select,
.level .count {
  padding: 5px var(--gap-2);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  background: var(--raised);
  color: var(--text);
  font-size: 13px;
}

.level .count {
  width: 6em;
}

.actions,
.confirm-actions {
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

/* A warning, not an error: nothing is broken, but nothing will switch it off. */
.warn {
  margin: 0;
  align-self: stretch;
  padding: var(--gap-2) var(--gap-3);
  border: 1px solid var(--warn);
  border-radius: var(--r-md);
  background: color-mix(in srgb, var(--warn) 10%, transparent);
  font-size: 13px;
}

.path {
  margin: 0;
  color: var(--text-faint);
  font-size: 12px;
  overflow-wrap: anywhere;
}

/* Selectable: the fallback when the clipboard refuses, and a way to read what is about to be published. */
.report {
  align-self: stretch;
  max-height: 240px;
  margin: 0;
  padding: var(--gap-3);
  overflow: auto;
  background: var(--raised-2);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  font-family: var(--font-mono);
  font-size: 12px;
  white-space: pre-wrap;
  user-select: text;
}

/* A gesture with no undo: the label says it too, and the confirmation lists what goes. */
.danger {
  color: var(--bad);
  border-color: var(--bad);
}

.solid.danger {
  background: var(--bad);
  color: var(--accent-ink);
}

.ghost.danger:hover:not(:disabled) {
  color: var(--bad);
  background: color-mix(in srgb, var(--bad) 12%, transparent);
}

.confirm {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  align-self: stretch;
  padding: var(--gap-3);
  background: color-mix(in srgb, var(--bad) 8%, var(--raised));
  border: 1px solid var(--bad);
  border-radius: var(--r-md);
}

.confirm-title {
  font-weight: 600;
}

.what {
  margin: 0;
  padding-left: var(--gap-4);
  color: var(--text-muted);
  font-size: 13px;
}
</style>
