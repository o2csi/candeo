<script setup lang="ts">
/**
 * Settings: what concerns the application as a whole, and no device.
 *
 * The log and the configuration reset lived at the bottom of the devices screen,
 * which made it a catch-all. Devices keep their screen; the language setting
 * (#73) will come here.
 *
 * The reset stays **out of the library**: the neighbouring gesture there deletes
 * hand-written code.
 */

import { onMounted, ref } from 'vue'

import {
  diagnostic,
  getJournal,
  openLogDir,
  resetSettings,
  setLogLevel,
  type JournalStatus,
  type LogLevel,
} from '../api/candeo'
import { erreur, message } from '../api/journal'
import { useDevice } from '../composables/useDevice'
import { useEffects } from '../composables/useEffects'
import { useSettings } from '../composables/useSettings'

const { busy, refresh } = useDevice()
const { dropAll } = useSettings()
const { forgetPosed } = useEffects()

// ---------------------------------------------------------------- log

/**
 * The five levels, said by what they bring rather than by their technical name.
 * A record, so that a sixth level cannot be forgotten here.
 */
const LEVELS: Record<LogLevel, string> = {
  error: 'Erreurs seules',
  warn: 'Avertissements',
  info: 'Cycle de vie (défaut)',
  debug: 'Détaillé — par image',
  trace: 'Tout — par image',
}

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
      copied.value = 'Copié dans le presse-papiers.'
    } catch (e) {
      copied.value = 'Copie refusée par le système — le texte est ci-dessous, à sélectionner.'
      erreur('diagnostic', `clipboard unavailable: ${message(e)}`, e)
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
    // Every device changed state at once, and the log level went back to the
    // default.
    await refresh()
    await readJournal()
  }
}

onMounted(readJournal)
</script>

<template>
  <section class="page">
    <header class="head">
      <h1>Réglages</h1>
    </header>

    <section v-if="journal" class="block" aria-labelledby="journal-title">
      <h2 id="journal-title">Journal</h2>

      <p v-if="journalProblem" class="err" role="alert">{{ journalProblem }}</p>

      <div class="level">
        <label for="log-level">Niveau</label>
        <select
          id="log-level"
          :value="journal.setting ?? 'info'"
          :disabled="busy"
          @change="chooseLevel"
        >
          <option v-for="(label, level) in LEVELS" :key="level" :value="level">
            {{ label }}
          </option>
        </select>
      </div>

      <!--
        A high level carries per-frame records and survives restarts: left on
        and forgotten, it fills the disk, since rotation caps the number of files,
        not the size of today's.
      -->
      <p v-if="journal.verbose" class="warn" role="status">
        Niveau détaillé actif : le journal grossit vite et <strong>reste actif après un
        redémarrage</strong>.
      </p>

      <!-- The environment variable always wins: say so rather than let a setting look applied. -->
      <p v-if="journal.forcedByEnv" class="note" role="status">
        CANDEO_LOG impose le niveau {{ journal.level ?? 'demandé' }} pour cette exécution. Le réglage
        ci-dessus s'appliquera au prochain lancement sans cette variable.
      </p>

      <div class="actions">
        <button class="ghost" :disabled="!journal.dir" @click="showLogs">
          Ouvrir le dossier des journaux
        </button>
        <button class="ghost" @click="copyDiagnostic">Copier le diagnostic</button>
      </div>

      <p v-if="journal.dir" class="mono path">{{ journal.dir }}</p>
      <p v-else class="err">Aucun fichier : le journal n'a pas pu ouvrir son dossier.</p>

      <p v-if="copied" class="note" role="status">{{ copied }}</p>
      <pre v-if="report" class="report">{{ report }}</pre>
    </section>

    <section class="block" aria-labelledby="config-title">
      <h2 id="config-title">Configuration</h2>

      <p v-if="problem" class="err" role="alert">{{ problem }}</p>

      <!--
        The button stays in place and enabled while the question is asked:
        hiding it would take keyboard focus away right where it must reach the
        answer, which follows in the document.
      -->
      <button class="ghost danger" :disabled="busy || working" @click="asking = true">
        Remettre la configuration au défaut
      </button>

      <div v-if="asking" class="confirm" role="group" aria-labelledby="confirm-reset">
        <p id="confirm-reset" class="confirm-title" role="alert">
          Repartir de la configuration par défaut ?
        </p>
        <!-- What goes, listed, and what does not, said as plainly: the last line matters most. -->
        <ul class="what">
          <li>
            Tous les appareils repassent en <strong>détecté</strong> : plus aucun n'est ouvert au
            démarrage.
          </li>
          <li>
            Les effets en cours s'arrêtent et le rétroéclairage s'éteint, plutôt que de rester figé
            sur la dernière image.
          </li>
          <li>
            L'effet appliqué et la luminosité retenue de chaque appareil sont oubliés : tout repart
            à pleine luminosité, sans effet.
          </li>
          <li>Les réglages retenus pour chaque effet, sur chaque appareil, sont oubliés.</li>
          <li>
            <strong>Vos effets ne sont pas touchés.</strong> Les supprimer est une autre action, une
            par effet, depuis la bibliothèque.
          </li>
        </ul>
        <div class="confirm-actions">
          <button class="solid danger" :disabled="working" @click="reset">
            Remettre au défaut
          </button>
          <button class="ghost" :disabled="working" @click="asking = false">Annuler</button>
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

.level select {
  padding: 5px var(--gap-2);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  background: var(--raised);
  color: var(--text);
  font-size: 13px;
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
