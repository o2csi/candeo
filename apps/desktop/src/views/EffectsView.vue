<script lang="ts">
import { ref } from 'vue'

/**
 * L'état des deux colonnes repliables, au niveau du module.
 *
 * Passer à l'éditeur détruit cette vue : depuis `setup()`, la colonne qu'on
 * vient de replier se rouvrirait au retour, et la bande morte qu'on voulait
 * supprimer reviendrait à chaque aller-retour. Même motif que `useDevice` —
 * l'état d'écran survit à la navigation, il ne se sérialise pas pour autant.
 *
 * Rangé dans un objet, et repris nommément dans `<script setup>` : seules les
 * liaisons de ce bloc-là sont exposées au patron.
 */
const shut = { devices: ref(false), effects: ref(false) }
</script>

<script setup lang="ts">
/**
 * Studio — trois colonnes : appareils, effets, réglages.
 *
 * La hiérarchie est celle dans laquelle on pense : on choisit un appareil, puis
 * son effet, puis ses réglages. **L'affectation n'est plus une case à cocher en
 * bas de panneau, c'est la structure de l'écran.**
 *
 * ## Deux colonnes se replient, la troisième non
 *
 * Avec un seul appareil piloté, une colonne entière serait une bande morte
 * permanente, et c'est l'aperçu qui a besoin de la largeur. La troisième ne se
 * replie pas : c'est le contenu, il ne resterait rien. Le détail du repliement
 * est dans la feuille de style, là où le piège se trouve.
 *
 * ## Un seul effet « actif »
 *
 * Celui de l'appareil **sélectionné**, et lui seul. Marquer actifs les effets de
 * tous les appareils dans une liste qui décrit ce que fait *un* appareil n'est
 * pas une simplification, c'est une information fausse.
 *
 * ## L'aperçu suit l'appareil
 *
 * Le simulateur vient dans le panneau de droite : liste à gauche / rendu à
 * droite ici, code à gauche / rendu à droite dans l'éditeur. Même grammaire, et
 * **un seul dessin** — `KeyboardSimulator` est le même composant des deux côtés,
 * il n'y a pas deux tracés à tenir d'accord.
 *
 * Il est alimenté par le canal d'images du moteur, celles-là mêmes qui partent
 * vers le clavier (`docs/design/studio.md` §3). D'où une différence assumée avec
 * la maquette : sans appareil piloté, l'aperçu **ne tourne pas**. Il n'y a pas
 * de boucle à faire tourner, et en animer une sur un appareil que l'utilisateur
 * n'a pas autorisé serait exactement ce que l'adoption interdit.
 */

import { computed, onBeforeUnmount, onMounted, watch } from 'vue'
import { useRouter } from 'vue-router'
import type { ParamSpec, ParamValue } from '@candeo/effects-api'

import {
  engineStatus,
  getDefaultLayout,
  getLayout,
  listEffects,
  startEffect,
  stopEffect,
  type DeviceEngineStatus,
  type EffectEntry,
} from '../api/candeo'
import type { DeviceRef } from '../api/types'
import EffectParamsForm from '../components/EffectParamsForm.vue'
import EffectSwatch from '../components/EffectSwatch.vue'
import KeyboardSimulator from '../components/KeyboardSimulator.vue'
import { useDevice } from '../composables/useDevice'
import { useEffectParams } from '../composables/useEffectParams'
import { hardwareEffects, useEffects, type HardwareEffect } from '../composables/useEffects'
import { useEngineFrames } from '../keyboard/engineFrames'
import type { LayoutView } from '../keyboard/layout'

/** Période d'interrogation du moteur, en millisecondes. */
const STATUS_PERIOD = 1000

const shutDevices = shut.devices
const shutEffects = shut.effects

const router = useRouter()
const { devices, current, select, busy, refresh } = useDevice()
// `apply` ne lève pas : il range son échec dans `applyError`, qu'il faut donc
// afficher — sans quoi un mode matériel refusé par l'appareil ne dirait rien.
const { appliedOn, apply, error: applyError } = useEffects()
const {
  load: loadParams,
  valuesFor,
  adjust,
  forget,
  flush: flushParams,
  error: paramsError,
} = useEffectParams()

/** Les erreurs remontées par Rust sont déjà lisibles : on les affiche telles quelles. */
function message(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : String(e)
}

// ---------------------------------------------------------------- appareils

/**
 * La colonne liste les appareils **pilotés**, et rien d'autre.
 *
 * L'adoption reste dans la vue Périphériques : choisir ce qu'on configure et
 * choisir ce que candeo a le droit de piloter sont deux gestes différents, et
 * les fondre ferait d'un clic de sélection une prise de contrôle.
 */
const piloted = computed(() => devices.value.filter((d) => d.state === 'adopted'))

/**
 * L'appareil que la colonne montre comme choisi.
 *
 * Le choix courant de l'application s'il est piloté, sinon le premier de la
 * liste : `current` peut désigner un gabarit connu mais non adopté, qui n'a
 * aucune ligne ici.
 */
const selectedDevice = computed(() => {
  const list = piloted.value
  const c = current.value
  return list.find((d) => c !== null && d.vid === c.vid && d.pid === c.pid) ?? list[0] ?? null
})

/** Identité stable d'un appareil, pour comparer sans dépendre de l'objet. */
const key = (d: DeviceRef | null) => (d ? `${d.vid}:${d.pid}` : null)

const deviceKey = computed(() => key(selectedDevice.value))

/**
 * Ce que la colonne désigne devient le choix de toute l'application.
 *
 * C'est ce qui fait qu'« ouvrir dans l'éditeur » travaille sur l'appareil qu'on
 * regardait : l'éditeur n'a pas de colonne, il reprend `current`.
 */
watch(
  deviceKey,
  () => {
    const d = selectedDevice.value
    if (d && key(current.value) !== key(d)) select({ vid: d.vid, pid: d.pid })
  },
  { immediate: true },
)

function choose(d: { vid: number; pid: number }) {
  select({ vid: d.vid, pid: d.pid })
}

/**
 * L'avertissement attend la fin de la recherche lancée au démarrage. Sans ce
 * `busy`, il s'afficherait le temps de l'énumération puis disparaîtrait :
 * annoncer une absence qu'on n'a pas encore vérifiée.
 */
const noDevice = computed(() => piloted.value.length === 0 && !busy.value)

// ---------------------------------------------------------------- bibliothèque

type Nature = 'builtin' | 'user' | 'hardware'

/**
 * Un effet, quelle que soit sa nature.
 *
 * Les trois listes n'ont ni la même origine ni la même forme — `list_effects`
 * pour les deux premières, un catalogue écrit pour le matériel — mais la colonne
 * les affiche de la même façon. On les ramène donc à une seule forme ici plutôt
 * que de tenir trois gabarits de gabarit dans le patron.
 */
interface Choice {
  id: string
  name: string
  nature: Nature
  description: string
  /**
   * Repère de couleurs **prélevé en exécutant l'effet**, côté Rust (issue #29).
   *
   * Vide pour le matériel, et c'est la seule réponse honnête : ces effets sont
   * exécutés par le micrologiciel, l'application ne voit jamais leurs images.
   * `EffectSwatch` montre alors une pastille sourde — inventer quatre couleurs
   * plausibles serait décrire un effet qu'on n'a pas regardé.
   */
  swatch: string[]
  params: Record<string, ParamSpec>
  /** Renseigné pour la seule nature qui ne passe pas par le moteur. */
  hardware: HardwareEffect | null
}

const library = ref<EffectEntry[]>([])
/** Déjà lisible : les messages du Rust s'affichent tels quels. */
const listError = ref<string | null>(null)
/** Ce qui a empêché d'appliquer ou d'arrêter. Déjà lisible aussi. */
const problem = ref<string | null>(null)

function fromEntry(e: EffectEntry): Choice {
  return {
    id: e.id,
    name: e.name,
    nature: e.kind,
    description: e.description ?? 'Sans description.',
    swatch: e.swatch,
    params: e.params ?? {},
    hardware: null,
  }
}

function fromHardware(e: HardwareEffect): Choice {
  return {
    id: e.id,
    name: e.name,
    nature: 'hardware',
    description: e.summary,
    swatch: [],
    params: {},
    hardware: e,
  }
}

const choices = computed<Choice[]>(() => [
  ...library.value.filter((e) => e.kind === 'builtin').map(fromEntry),
  ...library.value.filter((e) => e.kind === 'user').map(fromEntry),
  ...hardwareEffects.map(fromHardware),
])

/**
 * Groupé par nature. Le coût de chacune est dit dans le panneau de droite, pas
 * seulement suggéré par l'ordre : un effet matériel ne coûte aucun temps
 * processeur et survit à la fermeture, et cela se lit en toutes lettres.
 */
const GROUPS: readonly { nature: Nature; title: string }[] = [
  { nature: 'builtin', title: 'intégrés' },
  { nature: 'user', title: 'à vous' },
  { nature: 'hardware', title: 'matériel — dans le clavier' },
]

const grouped = computed(() =>
  GROUPS.map((g) => ({ ...g, items: choices.value.filter((c) => c.nature === g.nature) })),
)

const NATURES: Record<Nature, string> = {
  builtin: 'intégré',
  user: 'à vous',
  hardware: 'matériel',
}

/** Le coût réel, en clair : c'est ce qui départage les trois natures. */
const COSTS: Record<Nature, string> = {
  builtin: 'boucle hôte · tourne fenêtre fermée',
  user: 'boucle hôte · tourne fenêtre fermée',
  hardware: 'micrologiciel · aucun temps processeur · survit à tout',
}

const chosenEffect = ref<string | null>(null)

const selectedEffect = computed<Choice | null>(
  () => choices.value.find((c) => c.id === chosenEffect.value) ?? choices.value[0] ?? null,
)

// ---------------------------------------------------------------- moteur

/**
 * L'état du moteur, appareil par appareil.
 *
 * Tout est relu d'un coup : c'est un seul aller-retour par seconde, et la
 * colonne des appareils a besoin de chaque ligne pour dire ce que chacun fait
 * tourner.
 */
const statuses = ref<DeviceEngineStatus[]>([])

function statusOf(d: { vid: number; pid: number } | null): DeviceEngineStatus | null {
  if (!d) return null
  return statuses.value.find((s) => s.device.vid === d.vid && s.device.pid === d.pid) ?? null
}

/**
 * L'effet qu'un appareil fait tourner.
 *
 * La boucle hôte l'emporte sur le mode matériel : tant qu'elle pousse des
 * images, c'est elle qu'on voit sur les LED, quel que soit le mode posé avant.
 */
function runningOn(d: { vid: number; pid: number } | null): string | null {
  const s = statusOf(d)
  if (s?.running === true && s.effectId !== null) return s.effectId
  return appliedOn(d ? { vid: d.vid, pid: d.pid } : null)
}

function effectName(id: string | null): string | null {
  if (id === null) return null
  return choices.value.find((c) => c.id === id)?.name ?? id
}

/** Le seul effet marqué actif : celui de l'appareil sélectionné. */
const activeId = computed(() => runningOn(selectedDevice.value))

const status = computed(() => statusOf(selectedDevice.value))
const runningHere = computed(() => status.value?.running === true)

async function refreshStatus(): Promise<void> {
  try {
    statuses.value = await engineStatus()
  } catch (e) {
    problem.value = message(e)
  }
}

// ---------------------------------------------------------------- aperçu

/**
 * Gabarit de repli, demandé au Rust : il faut bien dessiner quelque chose avant
 * qu'un appareil soit ouvert.
 */
const fallback = ref<LayoutView | null>(null)
/** Gabarit de l'appareil sélectionné, quand il est réellement ouvert. */
const opened = ref<LayoutView | null>(null)
const board = computed<LayoutView | null>(() => opened.value ?? fallback.value)

/**
 * Le dessin suit l'appareil sélectionné.
 *
 * Un seul gabarit est connu aujourd'hui, mais il vient de l'appareil et non
 * d'une constante : `get_layout` pour celui qui est ouvert, le gabarit par
 * défaut sinon. Le jour où un second modèle arrive, cette vue n'a rien à
 * apprendre.
 */
watch(
  [deviceKey, () => selectedDevice.value?.open === true],
  async ([, ouvert]) => {
    const d = selectedDevice.value
    if (!d || !ouvert) {
      opened.value = null
      return
    }
    // Un gabarit qu'on n'obtient pas n'est pas une panne : le repli dessine.
    opened.value = await getLayout({ vid: d.vid, pid: d.pid }).catch(() => null)
  },
  { immediate: true },
)

const { frame, listen, stop: stopFrames } = useEngineFrames(() => board.value)

/**
 * Le canal d'images suit la sélection : changer d'appareil ferme l'un et ouvre
 * l'autre, sans quoi deux flux alimenteraient le même simulateur.
 */
watch(
  [deviceKey, runningHere],
  ([, running]) => {
    const d = selectedDevice.value
    if (d && running) void listen({ vid: d.vid, pid: d.pid })
    else stopFrames()
  },
  { immediate: true },
)

/**
 * Ce que le simulateur montre, dit en toutes lettres plutôt que deviné.
 *
 * L'aperçu suit **l'appareil**, pas la sélection : il affiche les images que le
 * moteur produit pour lui. Sélectionner un effet sans l'appliquer ne change donc
 * rien au dessin — et le dire vaut mieux que de laisser croire le contraire, ce
 * qu'un aperçu calculé dans la fenêtre ferait au prix d'un second moteur
 * (`docs/design/studio.md` §3).
 */
const previewNote = computed(() => {
  const c = selectedEffect.value
  if (!selectedDevice.value) {
    return "Aucun appareil piloté : il n'y a pas de boucle à alimenter, donc rien à animer."
  }
  if (runningHere.value) {
    return c !== null && activeId.value !== c.id
      ? `Images du moteur : c'est « ${effectName(activeId.value)} » qui tourne sur cet appareil.`
      : 'Images du moteur, exactement celles qui partent vers le clavier.'
  }
  if (c?.hardware) {
    return "Exécuté par le micrologiciel : l'application ne reçoit pas ses images, il n'y a rien à animer ici."
  }
  return "Aucun effet en cours sur cet appareil — « Appliquer » lance celui-ci."
})

// ---------------------------------------------------------------- actions

const working = ref(false)

const applied = computed(
  () => selectedEffect.value !== null && activeId.value === selectedEffect.value.id,
)

/**
 * « Appliquer » ouvre la sortie vers l'appareil sélectionné.
 *
 * Deux chemins, parce que les deux natures ne passent pas par le même endroit :
 * un effet matériel est un mode posé sur le micrologiciel, un effet hôte est une
 * boucle qu'on démarre. Poser un mode matériel arrête d'abord la boucle : sans
 * cela elle continuerait d'écrire par-dessus, et le mode resterait invisible.
 */
async function applyEffect(): Promise<void> {
  const c = selectedEffect.value
  const d = selectedDevice.value
  if (!c || !d) return

  const device = { vid: d.vid, pid: d.pid }
  problem.value = null
  working.value = true
  try {
    if (c.hardware) {
      if (statusOf(d)?.running === true) await stopEffect(device)
      stopFrames()
      await apply(device, c.hardware)
    } else {
      // Les réglages retenus pour **cette paire**, et non les valeurs déclarées :
      // un effet réglé puis quitté doit repartir comme on l'avait laissé, sans
      // quoi il faudrait rebouger chaque curseur après chaque « Appliquer ».
      //
      // Rien à abonner ici : le canal vit dans l'état de la boucle, et c'est la
      // surveillance plus haut qui l'ouvre dès que le moteur dit « en cours ».
      // Un abonnement de plus, posé ici, en ferait deux pour un seul flux.
      await startEffect(device, c.id, paramValues.value)
    }
  } catch (e) {
    problem.value = message(e)
  } finally {
    working.value = false
    await refreshStatus()
  }
}

/**
 * Arrête la boucle. La dernière image reste affichée comme elle reste sur le
 * clavier : arrêter un effet n'éteint pas les LED.
 */
async function halt(): Promise<void> {
  const d = selectedDevice.value
  if (!d) return

  problem.value = null
  working.value = true
  try {
    await stopEffect({ vid: d.vid, pid: d.pid })
    stopFrames()
  } catch (e) {
    problem.value = message(e)
  } finally {
    working.value = false
    await refreshStatus()
  }
}

// ---------------------------------------------------------------- réglages

/** Les paramètres déclarés par l'effet regardé. */
const specs = computed<Record<string, ParamSpec>>(() => selectedEffect.value?.params ?? {})

/**
 * Les valeurs sur lesquelles cet effet tourne — ou tournerait — sur cet
 * appareil : son manifeste, recouvert par ce qu'on a retenu pour cette paire.
 */
const paramValues = computed(() =>
  valuesFor(selectedDevice.value, selectedEffect.value?.id ?? '', specs.value),
)

/**
 * Pourquoi les contrôles sont inertes, ou `null` s'ils sont vivants.
 *
 * **Un réglage n'a de sens que sur l'effet en cours sur l'appareil.** La boucle
 * est le seul endroit où un paramètre change quelque chose ; bouger un curseur
 * pour un effet qu'on n'a pas appliqué ne pourrait rien produire.
 *
 * D'où le choix : figer et le dire, plutôt qu'appliquer l'effet au premier
 * mouvement de curseur. Lancer une boucle sur un clavier est un geste qu'on
 * décide — c'est tout le sens de « Appliquer », et de l'adoption avant lui.
 * Qu'un glissement de souris s'en charge à la place ferait d'un réglage une
 * prise de contrôle.
 *
 * Les valeurs restent **visibles** et retenues : ce sont celles avec lesquelles
 * « Appliquer » lancera l'effet.
 */
const frozen = computed<string | null>(() => {
  if (!selectedDevice.value) {
    return "Aucun appareil piloté : un réglage agit sur la boucle d'un appareil, et il n'y en a aucune."
  }
  if (!applied.value) {
    return "Ces réglages agissent sur l'effet en cours sur l'appareil. « Appliquer » lance celui-ci avec les valeurs ci-dessous, et ils redeviennent réglables."
  }
  return null
})

/** Un effet sans paramètre le dit — et il ne le dit pas de la même façon selon sa nature. */
const noParams = computed(() =>
  selectedEffect.value?.hardware
    ? "Exécuté par le micrologiciel : il n'expose aucun réglage à l'application."
    : "Cet effet n'en déclare aucun : il fait la même chose à chaque lancement.",
)

function onParamChange(id: string, value: ParamValue): void {
  const d = selectedDevice.value
  const c = selectedEffect.value
  if (!d || !c) return
  adjust({ vid: d.vid, pid: d.pid }, c.id, specs.value, id, value)
}

function onParamReset(): void {
  const d = selectedDevice.value
  const c = selectedEffect.value
  if (!d || !c) return
  forget({ vid: d.vid, pid: d.pid }, c.id, specs.value)
}

// ---------------------------------------------------------------- cycle de vie

let statusTimer = 0
/** Faux dès la destruction : l'ouverture enchaîne des allers-retours au Rust. */
let alive = true

onMounted(async () => {
  void getDefaultLayout().then((l) => {
    fallback.value = l
  })

  // Avant tout le reste : « Appliquer » part des valeurs retenues, et les lire
  // après coup laisserait une fenêtre où l'effet démarrerait sur ses défauts.
  await loadParams()

  await refresh()

  // Une seule alerte : la bibliothèque est lue d'un coup, elle échoue d'un coup.
  // Les effets matériels, eux, sont écrits ici : la colonne n'est jamais vide.
  try {
    library.value = await listEffects()
  } catch (e) {
    listError.value = message(e)
  }

  await refreshStatus()

  // On a pu quitter l'écran entre-temps : poser l'interrogation périodique
  // maintenant la laisserait tourner pour personne.
  if (alive) statusTimer = window.setInterval(() => void refreshStatus(), STATUS_PERIOD)
})

onBeforeUnmount(() => {
  alive = false
  window.clearInterval(statusTimer)
  // Le dernier mouvement d'un curseur ne doit pas dépendre du fait qu'on soit
  // resté devant le temps du repos d'écriture.
  flushParams()
})
</script>

<template>
  <section class="studio" :class="{ 'shut-1': shutDevices, 'shut-2': shutEffects }">
    <!-- ------------------------------------------------------- appareils -->
    <section class="col devices" :class="{ shut: shutDevices }" aria-label="Appareils">
      <!--
        Un intitulé, pas un titre de niveau : le seul `h1` de l'écran est le nom
        de l'effet qu'on configure, et il vient après dans le document. Chaque
        colonne est déjà nommée pour les lecteurs d'écran par son `aria-label`.
      -->
      <div class="col-head">
        <p class="col-title">Appareils</p>
        <button
          class="collapse"
          type="button"
          aria-controls="col-devices"
          :aria-expanded="!shutDevices"
          :aria-label="shutDevices ? 'Déplier la colonne des appareils' : 'Replier la colonne des appareils'"
          @click="shutDevices = !shutDevices"
        >
          {{ shutDevices ? '›' : '‹' }}
        </button>
      </div>

      <div id="col-devices" class="col-body">
        <button
          v-for="d in piloted"
          :key="`${d.vid}:${d.pid}`"
          class="entry"
          type="button"
          :aria-pressed="deviceKey === `${d.vid}:${d.pid}`"
          :aria-label="d.name"
          :title="d.name"
          @click="choose(d)"
        >
          <!--
            Un pictogramme de type, pas un logo de fabricant : ce sont des
            marques protégées, elles ne distinguent pas un clavier d'une souris,
            et le nom du produit porte déjà l'information. Le seul gabarit connu
            est un clavier ; le jour où le Rust déclarera un type, il viendra de
            là plutôt que d'être deviné sur le nom.
          -->
          <svg
            class="glyph"
            viewBox="0 0 24 16"
            width="18"
            height="12"
            aria-hidden="true"
            fill="none"
            stroke="currentColor"
            stroke-width="1.6"
          >
            <rect x="1" y="2" width="22" height="12" rx="2" />
            <path d="M6 11h12" stroke-linecap="round" />
            <path d="M5 6h1M9 6h1M13 6h1M17 6h1" stroke-linecap="round" />
          </svg>

          <span class="entry-text">
            <!-- Le nom du **produit**, pas une catégorie : c'est ce qui
                 distingue deux claviers de la même marque. Il passe à la ligne
                 plutôt que d'être tronqué. -->
            <span class="dev-name">{{ d.name }}</span>
            <span class="dev-fx">{{ effectName(runningOn(d)) ?? 'aucun effet' }}</span>
          </span>
        </button>

        <p v-if="!piloted.length" class="none">
          Aucun appareil piloté.
          <RouterLink to="/devices" class="link">Choisir un périphérique</RouterLink>
        </p>
      </div>
    </section>

    <!-- ---------------------------------------------------------- effets -->
    <section class="col effects" :class="{ shut: shutEffects }" aria-label="Effets">
      <div class="col-head">
        <p class="col-title">Effets</p>
        <button
          class="collapse"
          type="button"
          aria-controls="col-effects"
          :aria-expanded="!shutEffects"
          :aria-label="shutEffects ? 'Déplier la colonne des effets' : 'Replier la colonne des effets'"
          @click="shutEffects = !shutEffects"
        >
          {{ shutEffects ? '›' : '‹' }}
        </button>
      </div>

      <div id="col-effects" class="col-body">
        <template v-for="g in grouped" :key="g.nature">
          <p class="group">{{ g.title }}</p>
          <button
            v-for="c in g.items"
            :key="c.id"
            class="entry"
            type="button"
            :aria-pressed="selectedEffect?.id === c.id"
            :aria-label="activeId === c.id ? `${c.name} — actif` : c.name"
            :title="c.name"
            @click="chosenEffect = c.id"
          >
            <EffectSwatch class="mark" :colors="c.swatch" />
            <span class="fx-name">{{ c.name }}</span>
            <span v-if="activeId === c.id" class="fx-state">actif</span>
          </button>
        </template>

        <button class="new" type="button" @click="router.push('/editor')">
          <span class="plus" aria-hidden="true">＋</span>
          <span>Nouvel effet</span>
        </button>
      </div>
    </section>

    <!-- --------------------------------------------------------- réglages -->
    <section class="col detail" aria-label="Réglages">
      <p v-if="listError" class="failure" role="alert">{{ listError }}</p>
      <p v-if="problem" class="failure" role="alert">{{ problem }}</p>
      <p v-if="applyError" class="failure" role="alert">{{ applyError }}</p>
      <p v-if="paramsError" class="failure" role="alert">{{ paramsError }}</p>
      <p v-if="status?.error" class="failure" role="alert">
        Erreur de l'effet, à l'image en cours : {{ status.error }}
      </p>
      <p v-if="status?.deviceError" class="notice warn" role="alert">
        Écriture vers l'appareil impossible : {{ status.deviceError }}
      </p>

      <!--
        La bibliothèque se parcourt sans appareil : on doit pouvoir voir ce que
        l'application propose avant d'autoriser quoi que ce soit. Mais l'écran
        ne reste pas inerte, il dit ce qui manque et où aller.
      -->
      <p v-if="noDevice" class="notice" role="status">
        Aucun appareil piloté : la bibliothèque se parcourt, mais appliquer un effet demande un
        appareil que candeo a le droit de piloter.
        <RouterLink to="/devices" class="link">Choisir un périphérique</RouterLink>
      </p>

      <template v-if="selectedEffect">
        <header class="fx-head">
          <h1>{{ selectedEffect.name }}</h1>
          <span class="badge" :class="selectedEffect.nature">
            {{ NATURES[selectedEffect.nature] }}
          </span>
        </header>

        <p class="desc">{{ selectedEffect.description }}</p>
        <p class="cost">{{ COSTS[selectedEffect.nature] }}</p>

        <div class="preview">
          <p class="cost">{{ previewNote }}</p>
          <!--
            Le gabarit vient du Rust : il est nul le temps d'un aller-retour. On
            ne dessine pas un clavier vide en attendant.
          -->
          <KeyboardSimulator v-if="board" class="sim" :layout="board" :frame="frame" />
          <p v-if="board && !opened" class="cost">
            Dessin d'après le gabarit par défaut : aucun appareil ouvert.
          </p>
        </div>

        <!--
          Les réglages, engendrés depuis le manifeste. Le formulaire ne connaît
          aucun effet en particulier : il connaît les quatre sortes de
          `ParamSpec`, et rien d'autre.
        -->
        <EffectParamsForm
          :specs="specs"
          :values="paramValues"
          :frozen="frozen"
          :empty="noParams"
          @change="onParamChange"
          @reset="onParamReset"
        />

        <footer class="actions">
          <button
            class="solid"
            :disabled="!selectedDevice || working || applied"
            @click="applyEffect"
          >
            {{ applied ? 'Appliqué' : 'Appliquer' }}
          </button>

          <!--
            Arrête la boucle de **l'appareil**, pas celle de l'effet sélectionné :
            c'est elle qui écrit, quel que soit l'effet qu'on regarde.
          -->
          <button v-if="runningHere" class="ghost" :disabled="working" @click="halt">
            Arrêter
          </button>

          <!-- Un effet matériel n'a pas de code : le dire vaut mieux que de
               laisser cliquer dans le vide. -->
          <button
            class="ghost"
            :disabled="selectedEffect.hardware !== null"
            @click="router.push(`/editor/${selectedEffect.id}`)"
          >
            Modifier
          </button>

          <span class="spacer" />

          <p class="cost">
            {{
              selectedEffect.hardware
                ? "exécuté par l'appareil · rien à modifier"
                : "l'aperçu tourne dès le lancement · la sortie clavier se coupe dans l'éditeur"
            }}
          </p>
        </footer>
      </template>
    </section>
  </section>
</template>

<style scoped>
/*
 * Les deux largeurs repliables sont des **variables**, pas des règles
 * concurrentes : `.shut-1` et `.shut-2` écrivent chacune la sienne, et le point
 * de rupture redéfinit `grid-template-columns` une bonne fois. Aucune des trois
 * n'a à l'emporter sur les autres — il n'y a rien à départager.
 */
.studio {
  --col-devices: 212px;
  --col-effects: 244px;

  display: grid;
  grid-template-columns: var(--col-devices) var(--col-effects) minmax(0, 1fr);
  height: 100%;
  min-height: 0;
}

.studio.shut-1 {
  --col-devices: 40px;
}

.studio.shut-2 {
  --col-effects: 40px;
}

.col {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  background: var(--raised);
  border-right: 1px solid var(--line);
}

.col-head {
  display: flex;
  gap: var(--gap-2);
  align-items: center;
  padding: var(--gap-2) var(--gap-2) var(--gap-2) var(--gap-3);
  border-bottom: 1px solid var(--line);
}

.col-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  color: var(--text-faint);
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  white-space: nowrap;
}

.collapse {
  flex: none;
  width: 22px;
  height: 22px;
  border: 1px solid transparent;
  border-radius: var(--r-sm);
  color: var(--text-faint);
  font-size: 13px;
  line-height: 1;
}

.collapse:hover {
  color: var(--accent);
  background: var(--raised-2);
  border-color: var(--line);
}

/* Conteneur de défilement : rien ne peut déborder latéralement d'une colonne
   repliée, quelle que soit l'erreur commise plus bas. */
.col-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: var(--gap-2) var(--gap-1) var(--gap-3);
}

.entry {
  display: flex;
  gap: var(--gap-2);
  align-items: center;
  width: 100%;
  padding: 6px var(--gap-2);
  border: 1px solid transparent;
  border-radius: var(--r-md);
  text-align: left;
}

.entry:hover {
  background: var(--raised-2);
}

/* Doublé de la marque « actif » pour les effets, et de la position dans la
   liste pour les appareils : la bordure ambrée ne porte rien seule. */
.entry[aria-pressed="true"] {
  background: var(--raised-2);
  border-color: var(--accent);
}

.glyph {
  flex: none;
  color: var(--text-muted);
}

.entry[aria-pressed="true"] .glyph {
  color: var(--accent);
}

.entry-text {
  display: block;
  min-width: 0;
}

.dev-name {
  display: block;
  font-weight: 500;

  /* Un nom de produit est long : il passe à la ligne plutôt que d'être
     tronqué — c'est lui qui distingue deux claviers de la même marque.
     `anywhere` couvre le cas d'une référence d'un seul tenant. */
  overflow-wrap: anywhere;
}

.dev-fx {
  display: block;
  overflow: hidden;
  color: var(--accent);
  font-size: 11px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.group {
  margin: var(--gap-3) 0 var(--gap-1);
  padding-left: var(--gap-2);
  color: var(--text-faint);
  font-size: 10px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.group:first-child {
  margin-top: 0;
}

.fx-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.fx-state {
  flex: none;
  color: var(--accent);
  font-size: 10px;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

/* Le repère est décoratif — il porte déjà `aria-hidden`. Le rendre transparent
   au pointeur laisse l'infobulle du bouton passer : une fois la colonne
   repliée, c'est le seul endroit où le nom de l'effet se lit encore. */
.mark {
  pointer-events: none;
}

.new {
  width: 100%;
  margin-top: var(--gap-3);
  padding: 8px;
  border: 1px dashed var(--line-strong);
  border-radius: var(--r-md);
  color: var(--text-muted);
  font-size: 12px;
}

.new:hover {
  color: var(--accent);
  border-color: var(--accent);
}

.plus {
  margin-right: var(--gap-1);
}

/*
 * ---------------------------------------------------------------- repliement
 *
 * Deux fois la même règle, au même endroit : **on masque tous les enfants, puis
 * on rétablit explicitement le seul qui reste**.
 *
 * Ce n'est pas un détail de style. Énumérer ce qu'on cache — « cacher le nom,
 * cacher l'effet » — a déjà produit ici une collision de spécificité : une règle
 * ajoutée ailleurs pour le nom l'emportait sur le masquage, et le texte revenait
 * déborder dans 40 px. Écrite ainsi, la règle survit à l'ajout d'un enfant ou
 * d'une classe :
 *
 * 1. le sélecteur universel couvre ce qui n'existe pas encore — un enfant ajouté
 *    demain est masqué sans que personne ait à y penser ;
 * 2. le rétablissement est plus spécifique que le masquage, et il est ici, à
 *    deux lignes de lui, pas dans un autre bloc ;
 * 3. toute règle qui pourrait les concurrencer est gardée par `:not(.shut)` :
 *    elle ne **s'applique pas** en état replié, au lieu de gagner ou perdre un
 *    arbitrage de spécificité ;
 * 4. `.col-body` est un conteneur de défilement : même une règle fautive ne
 *    pourrait pas faire déborder la colonne sur sa voisine.
 */
@media (width > 820px) {
  .col.shut .col-head {
    justify-content: center;
    padding-inline: var(--gap-1);
  }

  .col.shut .col-title {
    display: none;
  }

  .col.shut .col-body {
    padding-inline: var(--gap-1);
  }

  /* Premier niveau : le corps ne montre que des entrées, un filet de groupe et
     le bouton d'ajout. Tout le reste — messages, aides, ce qu'on ajoutera —
     disparaît sans avoir à être nommé. */
  .col.shut .col-body > * {
    display: none;
  }

  .col.shut .col-body > .entry {
    display: flex;
  }

  .col.shut .col-body > .group {
    display: block;
  }

  .col.shut .col-body > .new {
    display: block;
  }

  /* Second niveau : dans une entrée, un seul enfant survit — l'icône d'appareil
     ou le repère de couleurs. */
  .col.shut .entry > * {
    display: none;
  }

  .col.shut .entry > .glyph {
    display: block;
  }

  .col.shut .entry > .mark {
    display: flex;

    /* 34 px ne tiennent pas dans les 32 px utiles d'une colonne repliée : le
       repère se resserre plutôt que de faire défiler la colonne en largeur. */
    width: 26px;
  }

  .col.shut .entry {
    justify-content: center;
    align-items: center;
    gap: 0;
    padding-inline: 0;
  }

  /* Même forme pour le bouton d'ajout : tout masqué, le signe rétabli. */
  .col.shut .new > * {
    display: none;
  }

  .col.shut .new > .plus {
    display: inline;
    margin-right: 0;
  }

  .col.shut .new {
    padding-inline: 0;
  }

  /* Le titre de groupe devient un filet : la séparation reste, le texte part. */
  .col.shut .group {
    height: 0;
    margin: var(--gap-2) var(--gap-1);
    padding: 0;
    overflow: hidden;
    border-top: 1px solid var(--line);
    font-size: 0;
    line-height: 0;
  }

  .col.shut .group:first-child {
    margin-top: 0;
    border-top: none;
  }

  /*
   * Gardé par `:not(.shut)`, et c'est la clause qui compte.
   *
   * Le nom d'un appareil passe à la ligne, donc son entrée s'aligne en haut.
   * Sans cette garde, la règle **s'appliquerait** en état replié et il faudrait
   * qu'elle perde un arbitrage de spécificité contre le masquage — exactement le
   * piège dans lequel cette interface est déjà tombée.
   */
  .devices:not(.shut) .entry {
    align-items: flex-start;
  }

  .devices:not(.shut) .glyph {
    margin-top: 3px;
  }
}

/*
 * ------------------------------------------------------------------ réglages
 */
.detail {
  overflow-y: auto;
  padding: var(--gap-4);
  gap: var(--gap-3);
  background: var(--ground);
  border-right: none;
}

.fx-head {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-2);
  align-items: baseline;
}

.fx-head h1 {
  min-width: 0;
  overflow-wrap: anywhere;
}

.badge {
  flex: none;
  padding: 2px var(--gap-2);
  border-radius: 99px;
  background: var(--raised-2);
  color: var(--text-muted);
  font-size: 11px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.badge.user {
  background: var(--accent-soft);
  color: var(--accent);
}

.badge.hardware {
  background: color-mix(in srgb, var(--ok) 14%, transparent);
  color: var(--ok);
}

.desc {
  max-width: 68ch;
  color: var(--text-muted);
  font-size: 13px;
}

/* Le coût réel, les notes d'état et les aides : même voix, la plus discrète. */
.cost {
  max-width: 68ch;
  color: var(--text-faint);
  font-size: 12px;
}

.preview {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
}

/* Le dessin ne prend pas plus que sa part : la colonne porte aussi les réglages
   et les actions, et un clavier qui pousse le reste hors de l'écran ferait
   défiler pour trouver un bouton. Borné en **largeur** et non en hauteur — le
   SVG garde ses proportions, une hauteur maximale le ferait rogner. */
.sim {
  max-width: 760px;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-2);
  align-items: center;
  margin-top: auto;
  padding-top: var(--gap-3);
  border-top: 1px solid var(--line);
}

.spacer {
  flex: 1;
}

.solid {
  padding: 6px var(--gap-3);
  background: var(--accent);
  border-radius: var(--r-md);
  color: var(--accent-ink);
  font-size: 13px;
  font-weight: 500;
}

.ghost {
  padding: 6px var(--gap-3);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  color: var(--text-muted);
  font-size: 13px;
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

.notice,
.failure {
  padding: var(--gap-3);
  border-radius: var(--r-md);
  font-size: 13px;
}

.notice {
  background: var(--raised);
  border: 1px solid var(--line);
  color: var(--text-muted);
}

/* Un écart entre ce qu'on a demandé et ce qui se passe. La couleur ne porte pas
   seule : le texte le dit aussi. */
.notice.warn {
  background: color-mix(in srgb, var(--warn) 12%, var(--raised));
  border-color: var(--warn);
  color: var(--text);
}

.failure {
  background: color-mix(in srgb, var(--bad) 10%, transparent);
  border: 1px solid var(--bad);
}

.none {
  color: var(--text-faint);
  font-size: 12px;
}

.link {
  color: var(--accent);
  text-decoration: none;
  white-space: nowrap;
}

.link:hover {
  text-decoration: underline;
}

/*
 * Fenêtre étroite : les trois colonnes s'empilent. Le repliement n'y a plus de
 * sens — une colonne pleine largeur réduite à une bande d'icônes ne gagnerait
 * rien — donc ses règles sont **entièrement** dans le point de rupture large, et
 * le bouton disparaît plutôt que de basculer un état sans effet.
 */
@media (width <= 820px) {
  .studio {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: auto auto minmax(0, 1fr);
    overflow-y: auto;
  }

  .col {
    border-right: none;
    border-bottom: 1px solid var(--line);
  }

  .detail {
    border-bottom: none;
  }

  .collapse {
    display: none;
  }

  /* Les deux listes cèdent la place au contenu, sans disparaître. */
  .devices .col-body,
  .effects .col-body {
    max-height: 24vh;
  }
}
</style>
