/**
 * Ce que `settings.json` retient, et comment la fenêtre l'écrit.
 *
 * Trois sortes de décisions y vivent, et elles n'ont ni la même forme ni le même
 * chemin vers le disque :
 *
 * - les **réglages d'un effet**, par paire appareil / effet ;
 * - la **luminosité** d'un appareil, réappliquée à chaque branchement ;
 * - l'**effet appliqué** sur un appareil, écrit par le Rust au lancement.
 *
 * Le module portait le nom du premier seul — `useEffectParams` — et c'était le
 * même défaut de forme que les trois champs mono-appareil retirés de
 * `settings.json` : un nom qui décrit une part et un contenu qui en couvre
 * trois. Une **seule** lecture du fichier alimente les trois, ce qui est aussi
 * la raison de ne pas les séparer en trois composables.
 *
 * ## Les réglages d'un effet : trois destinations pour un même geste
 *
 * Bouger un curseur écrit à trois endroits, et ils n'ont ni la même cadence ni
 * la même durée de vie :
 *
 * 1. **La mémoire de la fenêtre**, immédiate : c'est elle que le formulaire
 *    affiche, et changer d'effet puis revenir la relit ;
 * 2. **la boucle de rendu**, à chaud, quelques dizaines de fois par seconde au
 *    plus (`set_effect_params`, `set_preview_params`) ;
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

/**
 * L'effet **appliqué** sur chaque appareil, tel que le fichier le retient.
 * Clé `vid:pid`.
 *
 * Relu, jamais écrit d'ici : c'est le Rust qui le retient, au moment où l'effet
 * démarre pour de bon. La fenêtre n'aurait pas de quoi le faire honnêtement —
 * l'icône de zone de notification lance des effets sans elle.
 *
 * Ce qui en dépend : dire qu'un appareil au repos **se souvient** de son dernier
 * effet, plutôt que d'afficher « aucun effet » et laisser croire que rien n'a
 * été retenu.
 */
const applied = ref<Record<string, string>>({})

/** La luminosité retenue par appareil, clé `vid:pid`. Absente = le défaut. */
const brightness = ref<Record<string, number>>({})

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

const deviceKey = (d: DeviceRef) => `${d.vid}:${d.pid}`
const key = (d: DeviceRef, effect: string) => `${deviceKey(d)}/${effect}`

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
 * Ce qu'il reste à envoyer à **une** boucle de rendu.
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
  /** Où ces valeurs vont. Voir {@link hot}. */
  send: (params: EffectParams) => Promise<void>
}

/**
 * Un émetteur par boucle : deux claviers réglés à la suite ne se gênent pas, et
 * l'aperçu a le sien.
 *
 * La clé de l'aperçu ne peut pas entrer en collision avec celle d'un appareil :
 * `vid:pid` est fait de deux nombres.
 */
const senders = new Map<string, Sender>()

/** La clé de l'émetteur de l'aperçu, qui n'est la boucle d'aucun appareil. */
const PREVIEW = 'apercu'

/**
 * Pousse des valeurs vers une boucle, à cadence bornée.
 *
 * `send` est fourni par l'appelant plutôt que déduit d'un `DeviceRef` : l'aperçu
 * n'a pas d'appareil, et lui inventer un identifiant d'appareil factice aurait
 * remis dans ce module la confusion que le moteur vient d'en sortir.
 */
function hot(k: string, send: Sender['send'], params: EffectParams): void {
  let s = senders.get(k)
  if (!s) {
    s = { pending: null, inFlight: false, last: 0, timer: 0, send }
    senders.set(k, s)
  }
  // L'aperçu change de boucle à chaque sélection : l'envoi doit suivre la
  // dernière, pas celle qui vivait quand l'émetteur a été créé.
  s.send = send
  s.pending = params
  pump(s)
}

function pump(s: Sender): void {
  // Rien à envoyer, ou quelqu'un s'en charge déjà : le retour de l'envoi en
  // cours, ou l'expiration de la minuterie, rappellera cette fonction.
  if (s.pending === null || s.inFlight || s.timer !== 0) return

  const wait = HOT_PERIOD - (Date.now() - s.last)
  if (wait > 0) {
    s.timer = window.setTimeout(() => {
      s.timer = 0
      pump(s)
    }, wait)
    return
  }

  const params = s.pending
  s.pending = null
  s.inFlight = true
  s.last = Date.now()

  void s
    .send(params)
    .catch((e: unknown) => {
      error.value = message(e)
    })
    .finally(() => {
      s.inFlight = false
      pump(s)
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
 * Écritures **parties vers le Rust et pas encore confirmées**, par paire.
 *
 * C'est le second morceau de ce que {@link read} doit protéger, et il ne se
 * déduit pas de {@link writes} : `persist` retire l'entrée de la table des
 * écritures en attente **avant** l'aller-retour, sans quoi un `flushAll` ou un
 * `settleOne` la referait partir une seconde fois. Entre ce retrait et le retour
 * du Rust, la paire n'apparaît donc nulle part — et une relecture qui tomberait
 * dans cette fenêtre rendrait la valeur d'avant l'écriture, c'est-à-dire
 * défaisant sous les yeux de l'utilisateur le réglage qu'il vient de poser.
 *
 * Inobservable tant que rien n'appelait {@link reload}. L'icône de zone de
 * notification est le premier à l'appeler, et elle le fait précisément quand
 * quelque chose vient de bouger — donc au pire moment.
 *
 * Un **compte** et non un drapeau : deux écritures de la même paire peuvent se
 * chevaucher — la temporisation part, un `settle` en déclenche une autre aussitôt
 * — et un drapeau baissé par la première rouvrirait la fenêtre pendant que la
 * seconde vole encore.
 */
const inflight = new Map<string, number>()

/** Retient qu'une écriture part, et de quoi savoir quand elle est revenue. */
function takeOff(k: string): void {
  inflight.set(k, (inflight.get(k) ?? 0) + 1)
}

function landed(k: string): void {
  const reste = (inflight.get(k) ?? 1) - 1
  if (reste > 0) inflight.set(k, reste)
  else inflight.delete(k)
}

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
    // Retirée d'abord, pour qu'un `flushAll` ou un `settleOne` ne la refasse pas
    // partir ; comptée comme en vol dans la foulée, pour qu'elle ne disparaisse
    // pas de ce que {@link read} protège entre les deux. Voir {@link inflight}.
    writes.delete(k)
    written.set(k, Date.now())
    takeOff(k)
    api
      .rememberEffectParams(device, effect, values)
      .catch((e: unknown) => {
        error.value = message(e)
      })
      .finally(() => {
        landed(k)
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
 *
 * Ne rappelle pas ce qui est déjà parti — rien ne le peut, l'appel est en vol.
 * Une écriture qui atterrit juste après une remise à zéro réécrit donc sa paire ;
 * la fenêtre, elle, ne s'en souvient plus ({@link inflight} ne protège que ce que
 * `remembered` porte encore), et le prochain lancement repart du fichier.
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

      // Les deux autres tables sont reprises telles quelles : la fenêtre ne les
      // écrit pas — le Rust retient l'effet appliqué au lancement, et la
      // luminosité repart par sa propre commande —, il n'y a donc rien à
      // protéger d'une écriture en vol comme pour les réglages ci-dessous.
      applied.value = Object.fromEntries(
        s.activeEffects.map((r) => [deviceKey({ vid: r.vid, pid: r.pid }), r.effect]),
      )
      brightness.value = Object.fromEntries(
        s.devices
          .filter((r) => r.brightness !== undefined)
          .map((r) => [deviceKey({ vid: r.vid, pid: r.pid }), r.brightness as number]),
      )

      // Ce qui attend encore le disque est plus récent que le disque : l'écriture
      // ne part qu'au repos du curseur, et `reload` ne choisit pas son moment.
      // Reprendre le fichier tel quel ferait donc reculer un curseur sous la main
      // de celui qui le tient.
      //
      // **Les deux tables, et c'est la correction que #46 imposait** : ce qui
      // attend ({@link writes}) et ce qui est parti sans être confirmé
      // ({@link inflight}). `persist` retire l'entrée de la première avant
      // l'aller-retour ; sans la seconde, une relecture tombant dans cette
      // fenêtre rendrait la valeur d'avant l'écriture. Inobservable tant que rien
      // n'appelait `reload` — l'icône de zone de notification l'appelle, et
      // justement quand quelque chose vient de bouger.
      for (const k of [...writes.keys(), ...inflight.keys()]) {
        const aNous = remembered.value[k]
        if (aNous !== undefined) disque[k] = aNous
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

export function useSettings() {
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
   * Pour ce qui bouge **hors de la fenêtre** : l'icône de zone de notification
   * commande les effets sans elle, et la fenêtre lui survit désormais repliée —
   * son instantané peut donc vieillir des jours. Sans ce point d'entrée, `load`
   * ne relirait jamais après un premier succès.
   *
   * Ne rejoint pas une lecture déjà en vol : celle-ci a pu partir **avant**
   * l'écriture qu'on vient d'apprendre, et rendrait alors le contenu même qu'on
   * cherche à remplacer. Ce que la fenêtre n'a pas fini d'écrire est préservé
   * par {@link read} — voir {@link inflight}, qui est la moitié de cette
   * protection que l'appel de `reload` a rendue nécessaire.
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
   * Vrai si quelque chose est **retenu** pour cette paire.
   *
   * Sert à le dire à l'écran. C'était le défaut réel de la persistance livrée
   * par l'issue #28 : les réglages tenaient, et rien ne laissait le deviner — on
   * règle, on ferme, et on n'a aucune raison de croire que ça a survécu.
   */
  function keptFor(device: DeviceRef | null, effect: string): boolean {
    if (!device) return false
    return Object.keys(remembered.value[key(device, effect)] ?? {}).length > 0
  }

  /**
   * Change **un** réglage : mémoire, boucle de l'appareil, disque.
   *
   * Le tout est envoyé à la boucle, pas le seul champ modifié : `set_params`
   * remplace le JSON des paramètres, il ne le fusionne pas.
   *
   * Rend les valeurs complètes, pour que l'appelant puisse les pousser aussi
   * vers l'aperçu ({@link adjustPreview}). Les envoyer ici sans condition
   * ajusterait un aperçu qui ne montre peut-être pas cet effet-là — seul
   * l'appelant sait ce qu'il regarde.
   */
  function adjust(
    device: DeviceRef,
    effect: string,
    specs: Record<string, ParamSpec>,
    id: string,
    value: ParamValue,
  ): EffectParams {
    // Comme `useEffects.apply` : l'échec précédent s'efface à la tentative
    // suivante. Sans cela un incident passager laisserait un bandeau rouge
    // jusqu'à la fermeture, longtemps après que tout est rentré dans l'ordre.
    error.value = null

    const complete = { ...valuesFor(device, effect, specs), [id]: value }
    const kept = apart(specs, complete)

    // Remplacement plutôt que mutation, comme dans `useEffects` : la réactivité
    // ne dépend plus de la présence de la clé.
    remembered.value = { ...remembered.value, [key(device, effect)]: kept }
    hot(deviceKey(device), (p) => api.setEffectParams(device, p), complete)
    persist(device, effect, kept)
    return complete
  }

  /**
   * Pousse des valeurs vers la boucle d'**aperçu**, à la même cadence.
   *
   * C'est ce qui fait qu'un réglage agit à chaud sur ce qu'on regarde, sans
   * l'avoir appliqué : la boucle d'aperçu relit son JSON à chaque image, comme
   * celle d'un appareil. Rien n'est écrit sur disque ici — c'est {@link adjust}
   * qui retient, et il retient pour la paire appareil / effet, pas pour l'aperçu
   * qui n'appartient à aucun appareil.
   */
  function adjustPreview(values: EffectParams): void {
    hot(PREVIEW, api.setPreviewParams, values)
  }

  /** La luminosité retenue pour cet appareil, ou le défaut. */
  function brightnessOf(device: DeviceRef | null): number {
    if (!device) return api.BRIGHTNESS_DEFAULT
    return brightness.value[deviceKey(device)] ?? api.BRIGHTNESS_DEFAULT
  }

  /**
   * Change la luminosité d'un appareil : mémoire, clavier, et disque si demandé.
   *
   * Deux commandes et non une, comme pour les réglages d'effet : `setBrightness`
   * écrit sur le clavier à chaque mouvement, `rememberBrightness` n'écrit sur
   * disque qu'à la fin du geste. `commit` dit lequel des deux on est en train de
   * faire — un glissement produit des dizaines de `setBrightness` et une seule
   * écriture disque.
   */
  function setBrightness(device: DeviceRef, level: number, commit: boolean): void {
    error.value = null
    const k = deviceKey(device)
    // Le défaut ne se retient pas : l'absence d'entrée **est** le défaut, ici
    // comme dans le fichier.
    const suite = { ...brightness.value }
    if (level === api.BRIGHTNESS_DEFAULT) delete suite[k]
    else suite[k] = level
    brightness.value = suite

    const echoue = (e: unknown) => {
      error.value = message(e)
    }
    // L'appareil peut être fermé — débranché, ignoré : l'écriture HID échoue
    // alors, et ce n'est pas une raison de ne pas retenir le niveau. Il sera
    // réappliqué au branchement suivant, c'est tout l'objet de le retenir.
    void api.setBrightness(device, level).catch(echoue)
    if (commit) void api.rememberBrightness(device, level).catch(echoue)
  }

  /**
   * L'effet que `settings.json` retient comme appliqué sur cet appareil.
   *
   * Ce n'est **pas** ce qui tourne — ça, c'est `engine_status`. C'est ce qui a
   * été appliqué la dernière fois, et ce qui permet à un appareil au repos de
   * dire qu'il s'en souvient plutôt que d'afficher « aucun effet ».
   */
  function lastAppliedOn(device: DeviceRef | null): string | null {
    return device ? (applied.value[deviceKey(device)] ?? null) : null
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
   *
   * Rend les valeurs déclarées, pour la même raison qu'{@link adjust} rend les
   * siennes : l'aperçu doit revenir avec, et seul l'appelant sait ce qu'il
   * regarde.
   */
  function forget(
    device: DeviceRef,
    effect: string,
    specs: Record<string, ParamSpec>,
  ): EffectParams {
    error.value = null
    const declarees = merge(specs, {})
    remembered.value = { ...remembered.value, [key(device, effect)]: {} }
    hot(deviceKey(device), (p) => api.setEffectParams(device, p), declarees)
    persist(device, effect, {})
    settleOne(device, effect, true)
    return declarees
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
    // Le pendant de ce que `Settings::forget_effect` vient de faire sur disque :
    // l'identifiant ne désigne plus rien, et le laisser ici ferait annoncer par
    // la colonne des appareils un effet « retenu » que la bibliothèque ne
    // connaît plus.
    applied.value = Object.fromEntries(
      Object.entries(applied.value).filter(([, id]) => id !== effect),
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
    // Les deux autres tables décrivaient un fichier qui vient d'être remis à
    // plat : les garder ferait dire à l'écran qu'un effet reste appliqué et
    // qu'une luminosité reste retenue, alors que le Rust a tout éteint et tout
    // refermé.
    applied.value = {}
    brightness.value = {}
  }

  return {
    load,
    reload,
    valuesFor,
    keptFor,
    adjust,
    adjustPreview,
    settle,
    forget,
    dropEffect,
    dropAll,
    lastAppliedOn,
    brightnessOf,
    setBrightness,
    flush: flushAll,
    error: readonly(error),
  }
}
