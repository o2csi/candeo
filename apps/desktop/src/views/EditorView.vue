<script setup lang="ts">
/**
 * Éditeur d'effets — mode plein cadre.
 *
 * `meta.full` fait disparaître la barre de navigation : l'éditeur est un
 * **mode**, pas un onglet (`docs/design/studio.md` §2). À gauche le code, à
 * droite le simulateur, en permanence.
 *
 * ## Le cycle complet, en un bouton
 *
 * « Valider et lancer » enchaîne `transpileModule()`, `install_effect` et
 * `start_effect`. C'est le seul chemin vers le disque et vers le clavier : tant
 * qu'on n'a pas validé, on modifie un texte, rien d'autre.
 *
 * Les trois étapes échouent différemment, et chacune dit pourquoi :
 * le service de langage refuse un code qui ne compile pas, le relevé du
 * manifeste refuse un nom calculé, et le Rust refuse un effet écrit pour une
 * version de l'API qu'il ne connaît pas. Ces messages sont écrits pour être
 * lus : ils sont affichés tels quels.
 *
 * ## Les deux sorties sont indépendantes
 *
 * Le simulateur est alimenté par le **canal d'images** du moteur, celles-là
 * mêmes qui partent vers le clavier. La bascule « envoyer au clavier » coupe
 * l'écriture HID sans rien changer à l'aperçu : c'est ce qui permet d'écrire un
 * effet sans posséder le clavier.
 *
 * ## Quitter n'arrête pas l'effet
 *
 * Fermer l'éditeur libère le canal, donc le flux d'images. La boucle, elle,
 * tourne dans un fil Rust indépendant de la fenêtre et continue d'alimenter le
 * clavier — y compris l'application fermée.
 *
 * ## Un effet se lance sur **un** appareil
 *
 * Le moteur porte une boucle par appareil : lancer, arrêter, régler et suivre
 * les images désignent tous celui qui est visé. L'éditeur prend `current`, sans
 * rien demander — un seul appareil reste le cas courant, et choisir en
 * permanence serait une cérémonie de plus. L'écran à trois colonnes (issue #27)
 * rendra le choix explicite.
 */

import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import {
  engineStatus,
  getDefaultLayout,
  installEffect,
  listEffects,
  readEffectSource,
  setOutputToKeyboard,
  startEffect,
  stopEffect,
  type DeviceEngineStatus,
} from '../api/candeo'
import type { DeviceRef } from '../api/types'
import CodeEditor from '../components/CodeEditor.vue'
import DevicePill from '../components/DevicePill.vue'
import KeyboardSimulator from '../components/KeyboardSimulator.vue'
import { useDevice } from '../composables/useDevice'
import { clearDraft, readDraft, writeDraft } from '../editor/draft'
import { compile, nameInSource, renameInSource } from '../editor/effect'
import { errors } from '../editor/monaco'
import { NEW_EFFECT } from '../editor/template'
import { useEngineFrames } from '../keyboard/engineFrames'
import type { LayoutView } from '../keyboard/layout'

/** Délai d'inactivité avant d'enregistrer le brouillon. */
const DRAFT_DELAY = 400
/** Période d'interrogation du moteur, en millisecondes. */
const STATUS_PERIOD = 1000

const route = useRoute()
const router = useRouter()
const { layout, current, refresh } = useDevice()

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
/** Ce qui a empêché de valider, côté fenêtre. Déjà lisible. */
const problem = ref<string | null>(null)

/**
 * L'état du moteur, appareil par appareil — l'éditeur ne regarde que le sien.
 *
 * Tout est relu d'un coup plutôt qu'appareil par appareil : c'est un seul
 * aller-retour par seconde, et l'écran à trois colonnes (issue #27) aura de
 * toute façon besoin des autres lignes.
 */
const statuses = ref<DeviceEngineStatus[]>([])

const status = computed(() => {
  const device = current.value
  if (!device) return null
  return (
    statuses.value.find((s) => s.device.vid === device.vid && s.device.pid === device.pid) ?? null
  )
})

/**
 * Bascule « envoyer au clavier ». Par défaut : on envoie.
 *
 * Elle n'est pas initialisée depuis `engine_status()` sans discernement : hors
 * effet en cours, l'état rendu est celui d'un moteur vide, pas un choix de
 * l'utilisateur. On ne reprend donc l'état du moteur que s'il tourne.
 */
const toKeyboard = ref(true)

/**
 * Gabarit de repli, demandé au Rust plutôt que recopié ici : on écrit un effet
 * avant d'avoir branché quoi que ce soit, ou sans posséder le clavier.
 */
const fallback = ref<LayoutView | null>(null)
const board = computed<LayoutView | null>(() => layout.value ?? fallback.value)

const { frame, listen, stop: stopFrames } = useEngineFrames(() => board.value)

const running = computed(() => status.value?.running === true)

/**
 * L'effet tourne, on veut l'envoyer, et rien n'arrive au clavier.
 *
 * Le cas se produit surtout sans périphérique connecté — et il ne se signalait
 * d'aucune façon : le simulateur s'animait, la case restait cochée, le clavier
 * gardait son image. Symptôme rapporté tel quel : « comme s'il n'y avait que la
 * première image ».
 */
const silencieux = computed(
  () => running.value && toKeyboard.value && status.value?.reachingKeyboard === false,
)

/** Les erreurs remontées par Rust sont déjà lisibles : on les affiche telles quelles. */
function message(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : String(e)
}

/**
 * L'appareil visé, ou un refus lisible.
 *
 * Il n'y a plus d'appareil implicite côté Rust : chaque commande du moteur en
 * désigne un. `null` ne se produit que si la liste des gabarits connus est vide,
 * donc jamais en pratique — mais le dire vaut mieux qu'un `invoke` qui échoue en
 * parlant de désérialisation.
 */
function target(): DeviceRef {
  const device = current.value
  if (!device) throw new Error('aucun appareil connu : impossible de lancer un effet')
  return device
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

// ---------------------------------------------------------------- moteur

async function refreshStatus(): Promise<void> {
  try {
    statuses.value = await engineStatus()
  } catch (e) {
    problem.value = message(e)
  }
}

/**
 * Transpile, installe, démarre.
 *
 * Trois refus possibles, dans cet ordre, et aucun n'exécute quoi que ce soit :
 * le service de langage refuse un code qui ne compile pas, le relevé du
 * manifeste refuse un nom ou un paramètre calculés, et `install_effect` refuse
 * un `apiVersion` que cette application ne connaît pas. Ce n'est qu'après que
 * le moteur charge le `.js` — qui doit donc être sur disque d'abord.
 */
async function store(run: boolean): Promise<void> {
  busy.value = true
  problem.value = null
  try {
    const found = await errors()
    if (found.length > 0) {
      const first = found[0]
      throw new Error(
        `${found.length} erreur(s) dans l'effet — ligne ${first.line} : ${first.message}`,
      )
    }

    const { js, manifest, params } = await compile(source.value)
    const installedId = await installEffect(source.value, js, manifest)
    clearDraft(id.value)
    restored.value = false
    saved.value = source.value
    // L'effet a désormais une existence propre : ce n'est plus la copie d'un
    // intégré, c'est le sien.
    derivedFrom.value = null

    if (run) {
      const device = target()
      await startEffect(device, installedId, params)
      // `start_effect` repart d'un état neuf, dont la sortie clavier est
      // active. Sans cette ligne, « ne pas envoyer » serait oublié à chaque
      // lancement.
      if (!toKeyboard.value) await setOutputToKeyboard(device, false)
      // Le canal vit dans l'état de la boucle de cet appareil : un nouveau
      // départ, un nouvel abonnement.
      await listen(device)
    }

    // L'identifiant est dérivé du nom par le Rust. Le porter dans la route,
    // c'est ce qui fait que rouvrir cet écran relit bien cet effet.
    if (id.value !== installedId) await router.replace(`/editor/${installedId}`)
  } catch (e) {
    problem.value = message(e)
  } finally {
    busy.value = false
    await refreshStatus()
  }
}

/** Enregistre sans lancer — on met de côté un effet qu'on ne veut pas voir tourner. */
const save = () => store(false)

/** Enregistre puis lance. */
const validate = () => store(true)

/**
 * Arrête la boucle. La dernière image reste au simulateur comme elle reste sur
 * le clavier : arrêter un effet n'éteint pas les LED.
 */
async function halt(): Promise<void> {
  busy.value = true
  try {
    await stopEffect(target())
    stopFrames()
  } catch (e) {
    problem.value = message(e)
  } finally {
    busy.value = false
    await refreshStatus()
  }
}

async function toggleOutput(): Promise<void> {
  toKeyboard.value = !toKeyboard.value
  try {
    // Sans effet en cours sur cet appareil, le moteur n'a nulle part où poser ce
    // choix : il sera réappliqué au prochain lancement.
    await setOutputToKeyboard(target(), toKeyboard.value)
  } catch (e) {
    problem.value = message(e)
  }
  await refreshStatus()
}

// ---------------------------------------------------------------- cycle de vie

let statusTimer = 0
/** Faux dès la destruction : l'ouverture enchaîne des allers-retours au Rust. */
let alive = true

onMounted(async () => {
  void getDefaultLayout().then((l) => {
    fallback.value = l
  })

  // L'éditeur peut être la première vue affichée — un lien direct vers
  // `/editor/:id`. Sans cette relecture, aucun appareil ne serait désigné et
  // « Valider et lancer » n'aurait rien à viser.
  await refresh()
  await open()
  await refreshStatus()

  // Un effet peut déjà tourner sur cet appareil : lancé à la session
  // précédente, ou depuis la galerie. On reprend alors son flux d'images et
  // l'état réel de sa sortie.
  const encours = status.value
  if (encours?.running === true) {
    toKeyboard.value = encours.toKeyboard
    await listen(encours.device)
  }

  // On a pu quitter l'écran entre-temps : poser l'interrogation périodique
  // maintenant la laisserait tourner pour personne, hors de portée du
  // nettoyage qui a déjà eu lieu.
  if (alive) statusTimer = window.setInterval(() => void refreshStatus(), STATUS_PERIOD)
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
        L'éditeur n'a pas de colonne des appareils : la pastille est ici le seul
        endroit qui signale une perte. Même composant que dans la barre de
        navigation — un seul libellé, pas deux à tenir d'accord.
      -->
      <DevicePill />

      <!--
        Une vraie case à cocher : elle se pilote au clavier et porte son état
        sans qu'on ait à l'annoncer autrement.
      -->
      <label class="toggle">
        <input type="checkbox" :checked="toKeyboard" @change="toggleOutput" />
        Envoyer au clavier
      </label>

      <button class="ghost" :disabled="busy || !running" @click="halt">Arrêter</button>
      <!-- Enregistrer sans lancer : on met de côté un effet en chantier. -->
      <button class="ghost" :disabled="busy || loading" @click="save">Enregistrer</button>
      <button class="solid" :disabled="busy || loading" @click="validate">
        {{ busy ? 'Un instant…' : 'Valider et lancer' }}
      </button>
    </header>

    <div class="split">
      <div class="pane code-pane">
        <!--
          Dit d'emblée ce qui vient de se passer. L'identifiant d'un effet
          intégré est réservé : sans cette copie, on découvrirait le refus à la
          validation, c'est-à-dire après le travail.
        -->
        <!--
          Dire que rien n'atteint le clavier, plutôt que de laisser la case
          cochée le sous-entendre. `alert` et non `status` : c'est un écart
          entre ce qu'on a demandé et ce qui se passe.
        -->
        <p v-if="silencieux" class="notice warn" role="alert">
          L'effet tourne, mais <strong>aucune image n'atteint le clavier</strong> — il n'y en a
          probablement aucun de connecté.
          <button class="link" @click="router.push('/devices')">Choisir un périphérique</button>
        </p>

        <p v-if="status?.deviceError" class="notice warn" role="alert">
          Écriture vers le clavier impossible : {{ status.deviceError }}
        </p>

        <p v-if="derivedFrom" class="notice" role="status">
          Copie de « {{ derivedFrom }} » — l'original reste intact. Le nom a été changé dans le
          code ; modifiez-le à votre guise.
        </p>

        <p v-if="restored" class="notice" role="status">
          Brouillon restauré — cette version n'a pas été validée.
          <button class="link" @click="discard">Revenir à la version enregistrée</button>
        </p>

        <CodeEditor v-model="source" :disabled="loading" class="code" />

        <!--
          Trois sources d'échec, une seule place pour les dire. L'erreur du
          moteur passe en second : elle décrit un effet qui tourne, alors qu'un
          refus de validation décrit ce qu'on vient de tenter.
        -->
        <p v-if="problem" class="failure" role="alert">{{ problem }}</p>
        <p v-else-if="status?.error" class="failure" role="alert">
          Erreur de l'effet, à l'image en cours : {{ status.error }}
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
          <p class="sim-note">
            <template v-if="running">
              Images du moteur, exactement celles qui partent vers le clavier.
              <span v-if="!toKeyboard">Sortie clavier coupée : seul l'aperçu est alimenté.</span>
            </template>
            <template v-else>
              Aucun effet en cours. Validez pour lancer — quitter cet écran n'arrêterait pas
              l'effet, il continuerait d'alimenter le clavier.
            </template>
            <span v-if="!layout"> Aucun clavier connecté : dessin d'après le gabarit par défaut. </span>
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

.toggle {
  display: flex;
  align-items: center;
  gap: var(--gap-2);
  color: var(--text-muted);
  font-size: 13px;
}

.toggle input {
  accent-color: var(--accent);
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
