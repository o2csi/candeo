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
 * - "Enregistrer" chains the error check, `transpileModule()` and
 *   `install_effect`, then restarts the effect in the **preview loop**, on the
 *   current device's layout. Not a byte reaches a keyboard.
 * - "Appliquer sur …" starts it on the current device for real, saving first
 *   when the code differs from the saved version, with the parameters the
 *   gallery would use.
 *
 * The save steps fail differently, and each says why: the language service
 * rejects code that does not compile, the manifest reader rejects a computed
 * name, and Rust rejects an effect written for an API version it does not
 * know. Those messages are written to be read, and shown as they are.
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
  engineStatus,
  getDefaultLayout,
  installEffect,
  listEffects,
  readEffectSource,
  startEffect,
  stopEffect,
  type EngineReport,
} from '../api/candeo'
import { erreur } from '../api/journal'
import CodeEditor from '../components/CodeEditor.vue'
import DeviceStatusDot from '../components/DeviceStatusDot.vue'
import KeyboardSimulator from '../components/KeyboardSimulator.vue'
import { useDevice } from '../composables/useDevice'
import { useSettings } from '../composables/useSettings'
import { clearDraft, readDraft, writeDraft } from '../editor/draft'
import { compile, nameInSource, renameInSource } from '../editor/effect'
import { errors } from '../editor/monaco'
import { NEW_EFFECT } from '../editor/template'
import type { LayoutView } from '../keyboard/layout'
import { useSimulatorFeed } from '../keyboard/simulatorFeed'

/** Délai d'inactivité avant d'enregistrer le brouillon. */
const DRAFT_DELAY = 400
/** Période d'interrogation du moteur, en millisecondes. */
const STATUS_PERIOD = 1000

const route = useRoute()
const router = useRouter()
const { devices, layout, current, refresh } = useDevice()
const { load: loadSettings, reload: reloadSettings, valuesFor } = useSettings()

/** `/editor` sans identifiant = nouvel effet ; avec = effet installé. */
const id = computed<string | null>(() => {
  const raw = route.params.id
  return typeof raw === 'string' && raw !== '' ? raw : null
})

const source = ref('')
/** La dernière version connue sur disque, pour pouvoir y revenir. */
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

/** Les erreurs remontées par Rust sont déjà lisibles : on les affiche telles quelles. */
function message(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : String(e)
}

// ---------------------------------------------------------------- ouverture

/**
 * Un effet **intégré** s'ouvre comme une copie.
 *
 * Son identifiant est réservé : l'installer sous le même nom est refusé, et le
 * moteur chargerait de toute façon le code livré. Sans cette duplication, on
 * modifiait un effet pendant de longues minutes pour se heurter au refus à la
 * validation — le mur arrivait après le travail.
 *
 * Le nom est donc changé **dans la source**, à l'ouverture. Pas de « enregistrer
 * sous » : une question posée au moment où l'on veut juste essayer, et à
 * laquelle on répond une fois pour toutes. Ici la décision est déjà prise quand
 * on arrive, et elle est lisible dans le code. Qui vient seulement lire ne
 * valide pas, et rien ne s'est produit.
 */
const derivedFrom = ref<string | null>(null)

async function open(): Promise<void> {
  loading.value = true
  derivedFrom.value = null
  const draft = readDraft(id.value)
  try {
    let disk = id.value === null ? NEW_EFFECT : await readEffectSource(id.value)

    if (id.value !== null) {
      const entry = (await listEffects()).find((e) => e.id === id.value)
      savedSpecs.value = entry?.params ?? {}
      if (entry?.kind === 'builtin') {
        derivedFrom.value = entry.name
        disk = await renameInSource(disk, `${entry.name} (copie)`)
      }
    }

    saved.value = disk
    restored.value = draft !== null && draft !== disk
    source.value = restored.value && draft !== null ? draft : disk
  } catch (e) {
    // Un effet illisible ne doit pas laisser un éditeur vide : s'il reste un
    // brouillon, c'est lui qu'on montre, et on dit ce qui a échoué.
    problem.value = message(e)
    saved.value = draft ?? ''
    source.value = draft ?? ''
    restored.value = draft !== null
  } finally {
    loading.value = false
    await refreshName()
  }
}

/** Reprend la version enregistrée et jette le brouillon. */
function discard(): void {
  source.value = saved.value
  restored.value = false
  clearDraft(id.value)
}

// ---------------------------------------------------------------- nom

/**
 * Le nom, modifiable sans toucher au code.
 *
 * **La source reste la vérité** : le champ l'affiche et la réécrit, il ne
 * double pas la donnée. C'est ce qui évite qu'un nom changé dans le code et un
 * nom changé dans le champ finissent par se contredire — et c'est aussi le
 * premier pas vers les métadonnées en formulaire.
 *
 * La réécriture a lieu à la validation du champ, pas à chaque touche : remplacer
 * le contenu de Monaco pendant la frappe déplacerait le curseur.
 */
const name = ref('')

async function refreshName(): Promise<void> {
  const found = await nameInSource(source.value)
  if (found !== null) name.value = found
}

async function rename(): Promise<void> {
  const wanted = name.value.trim()
  if (wanted === '' || wanted === (await nameInSource(source.value))) return
  source.value = await renameInSource(source.value, wanted)
}

// ---------------------------------------------------------------- brouillon

let draftTimer = 0

/**
 * Enregistrement continu, après une pause de frappe.
 *
 * **Un texte identique à la version enregistrée n'est pas un brouillon** : on
 * efface alors au lieu d'écrire. Sans cette règle, revenir à la version du
 * disque — ou simplement défaire ses modifications — laisserait un brouillon
 * fantôme qui ressurgirait à la prochaine ouverture, et l'éditeur annoncerait
 * une restauration qui ne restaure rien.
 */
watch(source, (value) => {
  window.clearTimeout(draftTimer)
  draftTimer = window.setTimeout(() => {
    if (value === saved.value) clearDraft(id.value)
    else writeDraft(id.value, value)
    // Le nom a pu changer dans le code : le champ suit.
    void refreshName()
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
 * True when "Appliquer" must save first.
 *
 * A device loop loads `effect.js` from disk, so only installed code can run. A
 * new effect and the copy of a built-in have nothing installed under their own
 * id yet, even when their text still matches what was opened.
 */
const unsaved = computed(
  () => id.value === null || derivedFrom.value !== null || source.value !== saved.value,
)

/**
 * This effect is the one the current device runs.
 *
 * "Arrêter" stops whatever the device runs, so it is offered only then. The
 * copy of a built-in is not the built-in, even though it still carries its id.
 */
const runsHere = computed(
  () =>
    id.value !== null &&
    derivedFrom.value === null &&
    status.value?.running === true &&
    status.value.effectId === id.value,
)

/**
 * The saved text the device loop was started from, as far as this screen knows.
 *
 * The engine reports which effect a device runs, not which version: the loop
 * keeps the `effect.js` it loaded at start. Without this, saving an applied
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
 * Gabarit de repli, demandé au Rust plutôt que recopié ici : on écrit un effet
 * avant d'avoir branché quoi que ce soit, ou sans posséder le clavier.
 */
const fallback = ref<LayoutView | null>(null)
const board = computed<LayoutView | null>(() => layout.value ?? fallback.value)

const { frame, restartPreview } = useSimulatorFeed({
  layout: () => board.value,
  device: () => current.value,
  showsDevice: () => showsDevice.value,
  // A new effect has no `effect.js` on disk to preview until its first save.
  previewed: () => (ready.value ? id.value : null),
  params: () => valuesFor(current.value, id.value ?? '', savedSpecs.value),
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
  if (showsDevice.value) return `Images de ${deviceName.value ?? "l'appareil"}`
  const p = report.value.preview
  if (p?.running === true && p.effectId === id.value) return 'Aperçu'
  if (previewError.value !== null) return 'Aperçu arrêté'
  return id.value === null ? 'Aucun aperçu' : 'Aperçu en préparation…'
})

// ---------------------------------------------------------------- actions

/**
 * Checks, transpiles and installs the source, and returns the id Rust gave it.
 *
 * Three refusals are possible, in this order, and none runs anything: the
 * language service rejects code that does not compile, the manifest reader
 * rejects a computed name or parameter, and `install_effect` rejects an
 * `apiVersion` this app does not know.
 *
 * The text is read once, up front: what is typed while the install is in
 * flight was not installed and must not be marked as saved.
 */
async function install(): Promise<string> {
  const text = source.value
  const found = await errors()
  if (found.length > 0) {
    const first = found[0]
    throw new Error(
      `${found.length} erreur(s) dans l'effet — ligne ${first.line} : ${first.message}`,
    )
  }

  const { js, manifest } = await compile(text)
  const installedId = await installEffect(text, js, manifest)
  clearDraft(id.value)
  restored.value = false
  saved.value = text
  savedSpecs.value = manifest.params ?? {}
  // The effect now exists in its own right: it is no longer a built-in's copy.
  derivedFrom.value = null

  // Rust derives the id from the name. Carrying it in the route is what makes
  // reopening this screen read this effect back.
  if (id.value !== installedId) await router.replace(`/editor/${installedId}`)
  return installedId
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
    erreur('editor', `${id.value ?? 'new effect'}: ${problem.value}`, e)
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
    // The gallery's rule, through the same helper: manifest defaults overridden
    // by what is remembered for this device. Two rules would light one effect
    // differently depending on the screen it was applied from.
    await startEffect(device, effectId, valuesFor(device, effectId, savedSpecs.value))
    appliedSource.value = { device: `${device.vid}:${device.pid}`, text: saved.value }
    // Rust has just remembered the applied effect. The gallery reads this shared
    // state back rather than guessing it.
    await reloadSettings()
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
    <header class="head">
      <button class="ghost" @click="router.push('/')">Retour</button>

      <!--
        Le nom se change ici, sans toucher au code — mais la source reste la
        vérité : ce champ la réécrit, il ne double pas la donnée.
      -->
      <label class="name">
        <span class="sr-only">Nom de l'effet</span>
        <input
          v-model="name"
          type="text"
          :disabled="loading"
          placeholder="Nom de l'effet"
          @change="rename"
          @keyup.enter="rename"
        />
      </label>

      <p class="what">
        {{ derivedFrom ? `copie de ${derivedFrom}` : (id ?? 'nouvel effet') }}
      </p>

      <span class="spacer" />

      <!--
        The editor has no devices column: the target device's dot is the only
        place here that shows it dropped. Same component as the gallery's cards.
      -->
      <DeviceStatusDot v-if="targetDevice" :device="targetDevice" />

      <!-- Stops what the device runs, whichever effect it is: offered only for this one. -->
      <button class="ghost" :disabled="busy || !runsHere" @click="halt">Arrêter</button>
      <!--
        The device goes in the tooltip and the accessible name, not the label: a
        product name is long enough to wrap the header onto a second row.
      -->
      <button
        v-if="deviceName"
        class="ghost"
        :disabled="busy || loading || applied"
        :title="applied ? `Appliqué sur ${deviceName}` : `Appliquer sur ${deviceName}`"
        :aria-label="applied ? `Appliqué sur ${deviceName}` : `Appliquer sur ${deviceName}`"
        @click="applyToDevice"
      >
        {{ applied ? 'Appliqué' : 'Appliquer' }}
      </button>
      <button class="solid" :disabled="busy || loading" @click="save">
        {{ busy ? 'Un instant…' : 'Enregistrer' }}
      </button>
    </header>

    <div class="split">
      <div class="pane code-pane">
        <p v-if="runsHere && status?.deviceError" class="notice warn" role="alert">
          Écriture vers le clavier impossible : {{ status.deviceError }}
        </p>

        <!--
          Dit d'emblée ce qui vient de se passer. L'identifiant d'un effet
          intégré est réservé : sans cette copie, on découvrirait le refus à la
          validation, c'est-à-dire après le travail.
        -->
        <p v-if="derivedFrom" class="notice" role="status">
          Copie de « {{ derivedFrom }} » — l'original reste intact. Le nom a été changé dans le
          code ; modifiez-le à votre guise.
        </p>

        <p v-if="restored" class="notice" role="status">
          Brouillon restauré — cette version n'a pas été enregistrée.
          <button class="link" @click="discard">Revenir à la version enregistrée</button>
        </p>

        <CodeEditor v-model="source" :disabled="loading" class="code" />

        <!--
          One place for every failure. What was just attempted comes first: it
          describes the last gesture, whereas a loop error describes something
          that keeps running.
        -->
        <p v-if="problem" class="failure" role="alert">{{ problem }}</p>
        <p v-else-if="effectError" class="failure" role="alert">
          Erreur de l'effet, à l'image en cours : {{ effectError }}
        </p>
        <p v-else class="hint">
          Le code est transpilé ici, exécuté côté Rust : un effet ne voit ni le DOM, ni
          l'application. <code>console</code> n'existe pas dans ce moteur — l'autocomplétion ne le
          propose pas.
        </p>
      </div>

      <div class="pane sim">
        <div class="sim-head">
          <h2>Simulateur</h2>
          <p class="sim-note">{{ simNote }}</p>
          <p v-if="!layout" class="sim-note">
            Aucun clavier connecté : dessin d'après le gabarit par défaut.
          </p>
        </div>

        <!--
          Le gabarit vient du Rust : il est nul le temps d'un aller-retour.
          On ne dessine pas un clavier vide en attendant.
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
  /* Assez large pour un nom, sans pousser le reste de la barre. */
  width: 22ch;
}

/* La bordure n'apparaît qu'au survol ou à la saisie : au repos, c'est un titre. */
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

/* `min-width` et `min-height` à zéro : sans eux, une cellule de grille refuse
   de descendre sous la taille intrinsèque de son contenu et déborde la
   fenêtre — ce que Monaco, qui mesure son conteneur, amplifierait. */
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
 * Un écart entre ce qu'on a demandé et ce qui se passe — pas une simple
 * information. La couleur ne porte pas seule : le texte le dit aussi.
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
