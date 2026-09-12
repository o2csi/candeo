/**
 * Les réglages d'un effet : ce qu'on retient, et ce qu'on envoie.
 *
 * Trois destinations pour un même geste — bouger un curseur —, et elles n'ont
 * ni la même cadence ni la même durée de vie :
 *
 * 1. **La mémoire de la fenêtre**, immédiate : c'est elle que le formulaire
 *    affiche, et changer d'effet puis revenir la relit ;
 * 2. **la boucle de rendu**, à chaud, quelques dizaines de fois par seconde au
 *    plus (`set_effect_params`) ;
 * 3. **`settings.json`**, quand le curseur s'arrête (`remember_effect_params`).
 *
 * ## Pourquoi le disque et pas la seule session
 *
 * Ce formulaire sert celui qui n'écrira jamais d'effet. Il règle « la vague,
 * mais plus lente » **une fois** ; le lui refaire régler à chaque lancement
 * reviendrait à livrer un réglage qu'on ne peut pas garder, c'est-à-dire une
 * démonstration. Celui qui écrit du code itère et n'a rien à retenir — c'est
 * l'autre public qui paie une mémoire de session, et c'est justement celui que
 * l'issue #28 vise.
 *
 * L'adoption d'un appareil est déjà persistante pour la même raison : une
 * décision prise une fois ne se redemande pas.
 *
 * ## Ce qui est retenu, et ce qui ne l'est pas
 *
 * **Uniquement ce qui diffère de ce que l'effet déclare.** Un paramètre laissé à
 * sa valeur de départ n'est pas écrit, et suivra donc le manifeste si une
 * version ultérieure de l'effet en change le défaut. Même économie que les
 * décisions d'adoption, qui n'écrivent que ce qui s'écarte du défaut.
 *
 * État au niveau du module, comme `useDevice` et `useEffects` : passer à
 * l'éditeur détruit la vue, et les réglages en vol ne doivent pas partir avec.
 */

import { readonly, ref } from 'vue'
import type { ParamSpec, ParamValue, Rgb } from '@candeo/effects-api'

import * as api from '../api/candeo'
import type { EffectParams } from '../api/candeo'
import type { DeviceRef } from '../api/types'

/**
 * Cadence maximale des envois à chaud, en millisecondes.
 *
 * Un glissement de souris produit des dizaines d'événements par seconde, et le
 * `pointermove` d'un écran à 144 Hz bien davantage. La boucle, elle, relit les
 * paramètres **à chaque image** — 60 fois par seconde. Envoyer plus vite qu'elle
 * ne lit, c'est remplacer un JSON que personne n'a encore regardé.
 *
 * 40 ms, soit 25 envois par seconde au plus : en dessous de la cadence de rendu,
 * donc invisible à l'œil, et un ordre de grandeur sous ce qu'un curseur produit.
 */
const HOT_PERIOD = 40

/**
 * Repos du curseur avant l'écriture disque, en millisecondes.
 *
 * C'est un filet, pas le chemin nominal : l'écriture part normalement à la **fin
 * du geste** — `change` sur un curseur, c'est-à-dire au relâchement — et ce
 * repos ne sert qu'aux cas où cet événement n'arrive pas.
 */
const DISK_DELAY = 600

/**
 * Écart minimal entre deux écritures d'une même paire, en millisecondes.
 *
 * La fin du geste n'est pas toujours rare : une flèche du clavier maintenue
 * enfoncée sur un curseur émet `change` **à chaque répétition**, soit une
 * trentaine par seconde. Écrire sans condition ferait donc trente écritures
 * disque par seconde, exactement ce que la temporisation évitait.
 *
 * Passé ce délai, la temporisation reprend la main et écrit le dernier état à la
 * relâche — on ne perd rien, on décale.
 */
const DISK_PERIOD = 250

/** Ce qui diffère du manifeste, par appareil et par effet. Clé `vid:pid/effet`. */
const remembered = ref<Record<string, EffectParams>>({})

/** Ce qui a empêché de lire, d'ajuster ou de retenir. Déjà lisible. */
const error = ref<string | null>(null)

/** La lecture **en vol**, partagée : deux écrans qui montent ensemble ne lisent qu'une fois. */
let reading: Promise<void> | null = null

/** Vrai dès qu'une lecture a abouti. C'est lui, et lui seul, qui évite de relire à chaque montage. */
let loaded = false

/**
 * Numéro de la dernière lecture lancée.
 *
 * `reload` peut partir pendant qu'une lecture est déjà en vol, et rien ne
 * garantit que les deux reviennent dans l'ordre où elles sont parties. Sans ce
 * compte, la plus ancienne pourrait atterrir en dernier et réinstaller
 * exactement l'état qu'on venait de partir remplacer.
 */
let generation = 0

const key = (d: DeviceRef, effect: string) => `${d.vid}:${d.pid}/${effect}`

/** Les erreurs remontées par Rust sont déjà lisibles : on les affiche telles quelles. */
function message(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : String(e)
}

// ---------------------------------------------------------------- valeurs

/** Vrai si cette valeur est une couleur, au sens de `ParamSpec`. */
function isRgb(v: unknown): v is Rgb {
  if (typeof v !== 'object' || v === null) return false
  const c = v as Record<string, unknown>
  return typeof c.r === 'number' && typeof c.g === 'number' && typeof c.b === 'number'
}

/**
 * Vrai si la valeur relue correspond à ce que l'effet déclare **aujourd'hui**.
 *
 * `settings.json` est un fichier, donc il se modifie à la main, et un effet
 * réenregistré peut avoir changé la sorte d'un paramètre. Une valeur qui ne
 * correspond plus retombe sur le défaut, plutôt que de faire produire à un
 * curseur un `NaN` ou à une liste une option qui n'existe pas.
 */
function fits(spec: ParamSpec, v: ParamValue): boolean {
  switch (spec.kind) {
    case 'number':
      return typeof v === 'number' && Number.isFinite(v)
    case 'color':
      return isRgb(v)
    case 'boolean':
      return typeof v === 'boolean'
    case 'choice':
      return typeof v === 'string' && spec.options.includes(v)
  }
}

/** Égalité de valeurs de paramètre, couleurs comprises. */
export function sameValue(a: ParamValue, b: ParamValue): boolean {
  if (isRgb(a) && isRgb(b)) return a.r === b.r && a.g === b.g && a.b === b.b
  return a === b
}

/**
 * Les valeurs complètes d'un effet : son manifeste, recouvert par ce qu'on a
 * retenu.
 *
 * Bornée aux paramètres **déclarés** : un réglage retenu pour un paramètre que
 * l'effet n'a plus disparaît de lui-même, au lieu de voyager indéfiniment vers
 * une boucle qui ne le lit plus.
 */
function merge(specs: Record<string, ParamSpec>, kept: EffectParams): EffectParams {
  const out: EffectParams = {}
  for (const [id, spec] of Object.entries(specs)) {
    const v = kept[id]
    out[id] = v !== undefined && fits(spec, v) ? v : spec.default
  }
  return out
}

/** Ce qui s'écarte du manifeste, et rien d'autre — c'est ce qui part sur disque. */
function apart(specs: Record<string, ParamSpec>, values: EffectParams): EffectParams {
  const out: EffectParams = {}
  for (const [id, spec] of Object.entries(specs)) {
    const v = values[id]
    if (v !== undefined && !sameValue(v, spec.default)) out[id] = v
  }
  return out
}

// ------------------------------------------------------------ envois à chaud

/**
 * Ce qu'il reste à envoyer à la boucle d'un appareil.
 *
 * Un seul envoi en vol à la fois, et jamais deux à moins de {@link HOT_PERIOD}
 * d'intervalle : les mouvements intermédiaires sont **écrasés**, pas empilés.
 * C'est la bonne façon de les perdre — la boucle ne lit que le dernier état, une
 * file d'attente ne ferait que le lui livrer en retard.
 *
 * `pending` garantit qu'aucune valeur finale ne se perd : le dernier état
 * demandé repart toujours, une fois l'envoi en cours revenu.
 */
interface Sender {
  /** Dernier état demandé, pas encore parti. `null` si tout est à jour. */
  pending: EffectParams | null
  /** Un envoi est en vol : à son retour, on repart si `pending` a bougé. */
  inFlight: boolean
  /** Date du dernier départ, pour tenir la cadence. */
  last: number
  /** Minuterie d'attente de cadence, `0` s'il n'y en a pas. */
  timer: number
}

/** Un émetteur par appareil : deux claviers réglés à la suite ne se gênent pas. */
const senders = new Map<string, Sender>()

function hot(device: DeviceRef, params: EffectParams): void {
  const k = `${device.vid}:${device.pid}`
  let s = senders.get(k)
  if (!s) {
    s = { pending: null, inFlight: false, last: 0, timer: 0 }
    senders.set(k, s)
  }
  s.pending = params
  pump(device, s)
}

function pump(device: DeviceRef, s: Sender): void {
  // Rien à envoyer, ou quelqu'un s'en charge déjà : le retour de l'envoi en
  // cours, ou l'expiration de la minuterie, rappellera cette fonction.
  if (s.pending === null || s.inFlight || s.timer !== 0) return

  const wait = HOT_PERIOD - (Date.now() - s.last)
  if (wait > 0) {
    s.timer = window.setTimeout(() => {
      s.timer = 0
      pump(device, s)
    }, wait)
    return
  }

  const params = s.pending
  s.pending = null
  s.inFlight = true
  s.last = Date.now()

  void api
    .setEffectParams(device, params)
    .catch((e: unknown) => {
      error.value = message(e)
    })
    .finally(() => {
      s.inFlight = false
      pump(device, s)
    })
}

// ------------------------------------------------------------ écriture disque

/** Une écriture différée, et de quoi la déclencher tout de suite. */
interface Write {
  timer: number
  run: () => void
}

const writes = new Map<string, Write>()

/** Date de la dernière écriture partie, par paire. Voir {@link DISK_PERIOD}. */
const written = new Map<string, number>()

/**
 * Écrit au plus tard après {@link DISK_DELAY} sans mouvement.
 *
 * Chaque nouvelle valeur remplace la précédente : un glissement de deux
 * secondes ne produit qu'une écriture, celle de la valeur à laquelle on
 * s'arrête. Le repos n'est pas une optimisation de confort — `settings.json`
 * s'écrit par fichier temporaire puis renommage, c'est un geste disque complet.
 */
function persist(device: DeviceRef, effect: string, values: EffectParams): void {
  const k = key(device, effect)
  const previous = writes.get(k)
  if (previous) window.clearTimeout(previous.timer)

  const run = () => {
    writes.delete(k)
    written.set(k, Date.now())
    api.rememberEffectParams(device, effect, values).catch((e: unknown) => {
      error.value = message(e)
    })
  }
  writes.set(k, { timer: window.setTimeout(run, DISK_DELAY), run })
}

/**
 * Déclenche l'écriture en attente pour cette paire, si elle peut partir.
 *
 * Trop tôt après la précédente, on ne fait rien : la temporisation armée par
 * `persist` est toujours là et écrira le dernier état. Rien ne se perd, l'ordre
 * est seulement décalé — voir {@link DISK_PERIOD}.
 *
 * `now` lève cet écart, pour les gestes qui ne se répètent pas : un clic sur
 * « Rétablir » n'a aucune raison d'attendre parce qu'un curseur vient d'être
 * relâché.
 */
function settleOne(device: DeviceRef, effect: string, now = false): void {
  const k = key(device, effect)
  const w = writes.get(k)
  if (!w) return
  if (!now && Date.now() - (written.get(k) ?? 0) < DISK_PERIOD) return

  window.clearTimeout(w.timer)
  w.run()
}

/**
 * Déclenche toutes les écritures en attente sans attendre le repos.
 *
 * L'itération porte sur un instantané : `run` se retire lui-même de la table.
 */
function flushAll(): void {
  for (const w of [...writes.values()]) {
    window.clearTimeout(w.timer)
    w.run()
  }
}

/**
 * Annule les écritures en attente dont la clé passe le crible, **sans les
 * exécuter**.
 *
 * L'inverse exact de {@link flushAll}, et le seul geste correct quand le Rust
 * vient de retirer ces entrées de `settings.json` : une temporisation qui
 * partirait après coup les y réécrirait, ressuscitant précisément ce qu'on
 * venait d'effacer.
 */
function cancelWrites(keep: (key: string) => boolean): void {
  for (const [k, w] of [...writes.entries()]) {
    if (keep(k)) continue
    window.clearTimeout(w.timer)
    writes.delete(k)
    written.delete(k)
  }
}

/**
 * Dernier filet : fermer la fenêtre détruit la vue web **sans passer par les
 * crochets de Vue**.
 *
 * `onBeforeUnmount` ne couvre que le changement d'écran ; or on ferme la fenêtre
 * pendant qu'un effet tourne, c'est même le mode d'emploi. Une minuterie de
 * 600 ms n'y survivrait pas.
 *
 * Ce n'est qu'un filet, et volontairement : rien ne garantit qu'un aller-retour
 * vers le Rust aboutisse pendant que la vue web s'éteint. Le chemin sûr est
 * ailleurs — l'écriture part **à la fin du geste**, au relâchement du curseur,
 * donc bien avant qu'on approche de la croix de fermeture.
 */
window.addEventListener('pagehide', flushAll)

// ------------------------------------------------------------------- lecture

/**
 * Lit `settings.json` et remplace ce qu'on retient.
 *
 * Un échec ne marque pas la lecture comme faite : il la laisse à retenter, pour
 * qu'un second écran ne se contente pas d'hériter d'un refus définitif.
 */
function read(): Promise<void> {
  const mine = ++generation

  const run = api
    .getSettings()
    .then((s) => {
      // Une lecture plus récente est passée devant : la nôtre est périmée, et
      // l'appliquer reviendrait à défaire ce qu'elle vient d'installer.
      if (mine !== generation) return

      const disque: Record<string, EffectParams> = Object.fromEntries(
        s.effectParams.map((r) => [key({ vid: r.vid, pid: r.pid }, r.effect), r.values]),
      )

      // Ce qui attend encore le disque est plus récent que le disque : l'écriture
      // ne part qu'au repos du curseur, et `reload` ne choisit pas son moment.
      // Reprendre le fichier tel quel ferait donc reculer un curseur sous la main
      // de celui qui le tient.
      //
      // Couvre ce qui attend, pas ce qui est déjà parti : `persist` retire
      // l'entrée de `writes` **avant** que le Rust n'ait écrit. Une relecture qui
      // tomberait dans cet aller-retour rendrait la valeur d'avant. Fenêtre
      // connue et sans conséquence tant que rien n'appelle `reload` — à traiter
      // avec #46, qui sera le premier à le faire.
      for (const k of writes.keys()) {
        const enVol = remembered.value[k]
        if (enVol !== undefined) disque[k] = enVol
      }

      remembered.value = disque
      loaded = true
    })
    .catch((e: unknown) => {
      if (mine !== generation) return
      error.value = message(e)
    })
    .finally(() => {
      // Pas d'effacement inconditionnel : une lecture plus récente a pu prendre
      // la place, et la retirer la rendrait invisible à qui appelle `load`.
      if (reading === run) reading = null
    })

  reading = run
  return run
}

export function useEffectParams() {
  /**
   * S'assure que `settings.json` a été lu — une fois par session, pas une fois
   * par montage.
   *
   * Revenir sur cet écran ne relit pas, et c'est voulu : le disque ne bouge pas
   * du seul fait qu'on navigue, et c'est la fenêtre elle-même qui l'écrit, donc
   * elle en sait déjà plus que lui. Le jour où ce n'est plus vrai, c'est
   * {@link reload} qu'il faut appeler — pas cette économie qu'il faut retirer.
   */
  function load(): Promise<void> {
    if (loaded) return Promise.resolve()
    return reading ?? read()
  }

  /**
   * Relit `settings.json`, mémoïsation comprise.
   *
   * Pour ce qui écrit les réglages **hors de la fenêtre** — l'icône de zone de
   * notification (#46) est le premier cas attendu. Sans ce point d'entrée,
   * `load` ne relirait jamais après un premier succès, et la fenêtre
   * travaillerait indéfiniment sur l'instantané de son démarrage.
   *
   * Ne rejoint pas une lecture déjà en vol : celle-ci a pu partir **avant**
   * l'écriture qu'on vient d'apprendre, et rendrait alors le contenu même qu'on
   * cherche à remplacer.
   */
  function reload(): Promise<void> {
    loaded = false
    return read()
  }

  /**
   * Les valeurs sur lesquelles cet effet tourne — ou tournerait — sur cet
   * appareil.
   *
   * Sans appareil, ce que l'effet déclare : il faut bien montrer quelque chose,
   * et ces valeurs-là sont vraies pour n'importe quel appareil.
   */
  function valuesFor(
    device: DeviceRef | null,
    effect: string,
    specs: Record<string, ParamSpec>,
  ): EffectParams {
    const kept = device ? remembered.value[key(device, effect)] : undefined
    return merge(specs, kept ?? {})
  }

  /**
   * Change **un** réglage : mémoire, boucle, disque.
   *
   * Le tout est envoyé à la boucle, pas le seul champ modifié : `set_params`
   * remplace le JSON des paramètres, il ne le fusionne pas.
   */
  function adjust(
    device: DeviceRef,
    effect: string,
    specs: Record<string, ParamSpec>,
    id: string,
    value: ParamValue,
  ): void {
    // Comme `useEffects.apply` : l'échec précédent s'efface à la tentative
    // suivante. Sans cela un incident passager laisserait un bandeau rouge
    // jusqu'à la fermeture, longtemps après que tout est rentré dans l'ordre.
    error.value = null

    const complete = { ...valuesFor(device, effect, specs), [id]: value }
    const kept = apart(specs, complete)

    // Remplacement plutôt que mutation, comme dans `useEffects` : la réactivité
    // ne dépend plus de la présence de la clé.
    remembered.value = { ...remembered.value, [key(device, effect)]: kept }
    hot(device, complete)
    persist(device, effect, kept)
  }

  /**
   * Le geste est terminé — relâchement d'un curseur, case cochée, option
   * choisie : on écrit maintenant.
   *
   * C'est **le** chemin nominal vers le disque. Sans lui, la seule garantie
   * serait une minuterie de 600 ms, que fermer la fenêtre emporterait — or on
   * ferme la fenêtre pendant qu'un effet tourne, c'est le mode d'emploi.
   */
  function settle(device: DeviceRef, effect: string): void {
    settleOne(device, effect)
  }

  /**
   * Rétablit ce que l'effet déclare, et **oublie** — l'entrée disparaît de
   * `settings.json` au lieu d'y garder une copie des défauts.
   *
   * Écrit sans attendre : c'est un clic, pas un glissement, il n'y a rien à
   * regrouper.
   */
  function forget(device: DeviceRef, effect: string, specs: Record<string, ParamSpec>): void {
    error.value = null
    remembered.value = { ...remembered.value, [key(device, effect)]: {} }
    hot(device, merge(specs, {}))
    persist(device, effect, {})
    settleOne(device, effect, true)
  }

  /**
   * Oublie ce qu'on retenait pour un effet, sur **tous** les appareils.
   *
   * Le pendant, en mémoire, de ce que `delete_effect` fait dans `settings.json`.
   * Sans lui, la fenêtre garderait des réglages désignant un identifiant que plus
   * rien ne nomme, et un effet réenregistré sous le même nom **dans la même
   * session** en hériterait — exactement ce que la purge côté Rust évite d'un
   * lancement à l'autre.
   *
   * Rien n'est envoyé au Rust : il a déjà oublié. Ce qui est en vol est annulé,
   * pas déclenché.
   */
  function dropEffect(effect: string): void {
    // Un identifiant d'effet ne contient ni `/` ni `:` — la liste blanche du Rust
    // n'accepte que `a-z`, `0-9` et le tiret. Le suffixe ne peut donc pas
    // désigner la mauvaise paire.
    const suffix = `/${effect}`
    const autres = (k: string) => !k.endsWith(suffix)
    cancelWrites(autres)
    remembered.value = Object.fromEntries(
      Object.entries(remembered.value).filter(([k]) => autres(k)),
    )
  }

  /**
   * Oublie **tout**, comme la remise à zéro de la configuration vient de le faire
   * sur disque.
   *
   * Sans cela le premier mouvement de curseur réécrirait les réglages qu'on
   * venait d'effacer : la fenêtre les tient en mémoire, et n'envoie au Rust que
   * ce qui diffère du manifeste — c'est-à-dire ce qu'elle croit savoir.
   */
  function dropAll(): void {
    cancelWrites(() => false)
    remembered.value = {}
  }

  return {
    load,
    reload,
    valuesFor,
    adjust,
    settle,
    forget,
    dropEffect,
    dropAll,
    flush: flushAll,
    error: readonly(error),
  }
}
