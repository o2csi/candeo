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

/** Repos du curseur avant l'écriture disque, en millisecondes. */
const DISK_DELAY = 600

/** Ce qui diffère du manifeste, par appareil et par effet. Clé `vid:pid/effet`. */
const remembered = ref<Record<string, EffectParams>>({})

/** Ce qui a empêché de lire, d'ajuster ou de retenir. Déjà lisible. */
const error = ref<string | null>(null)

/** La lecture initiale, partagée : elle n'a lieu qu'une fois par session. */
let reading: Promise<void> | null = null

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

/**
 * Écrit après {@link DISK_DELAY} sans mouvement.
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
    api.rememberEffectParams(device, effect, values).catch((e: unknown) => {
      error.value = message(e)
    })
  }
  writes.set(k, { timer: window.setTimeout(run, DISK_DELAY), run })
}

export function useEffectParams() {
  /**
   * Lit `settings.json` une fois par session.
   *
   * Un échec n'efface pas la promesse partagée : il la libère, pour qu'un
   * second écran retente au lieu d'hériter d'un refus définitif.
   */
  function load(): Promise<void> {
    reading ??= api
      .getSettings()
      .then((s) => {
        remembered.value = Object.fromEntries(
          s.effectParams.map((r) => [key({ vid: r.vid, pid: r.pid }, r.effect), r.values]),
        )
      })
      .catch((e: unknown) => {
        reading = null
        error.value = message(e)
      })
    return reading
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
   * Rétablit ce que l'effet déclare, et **oublie** — l'entrée disparaît de
   * `settings.json` au lieu d'y garder une copie des défauts.
   */
  function forget(device: DeviceRef, effect: string, specs: Record<string, ParamSpec>): void {
    error.value = null
    remembered.value = { ...remembered.value, [key(device, effect)]: {} }
    hot(device, merge(specs, {}))
    persist(device, effect, {})
  }

  /**
   * Déclenche les écritures en attente sans attendre le repos.
   *
   * Appelée en quittant l'écran : le dernier mouvement d'un curseur ne doit pas
   * dépendre du fait qu'on soit resté 600 ms de plus devant.
   */
  function flush(): void {
    for (const w of [...writes.values()]) {
      window.clearTimeout(w.timer)
      w.run()
    }
  }

  return { load, valuesFor, adjust, forget, flush, error: readonly(error) }
}
