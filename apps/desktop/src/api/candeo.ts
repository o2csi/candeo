/**
 * Seul point de passage vers le Rust.
 *
 * Aucun composant n'appelle `invoke` directement : tout transite par ici, pour
 * que la surface soit typée en un seul endroit et que le jour où une commande
 * change de forme, le compilateur désigne les appelants.
 */

import { Channel, invoke } from '@tauri-apps/api/core'
import type { ParamSpec, ParamValue } from '@candeo/effects-api'

import type { DeviceInfo, Effect, LayoutInfo, Rgb } from './types'

/** Liste les gabarits connus, branchés ou non. */
export function listDevices(): Promise<DeviceInfo[]> {
  return invoke('list_devices')
}

export function connect(vid: number, pid: number): Promise<LayoutInfo> {
  return invoke('connect', { vid, pid })
}

export function disconnect(): Promise<void> {
  return invoke('disconnect')
}

export function isConnected(): Promise<boolean> {
  return invoke('is_connected')
}

export function getLayout(): Promise<LayoutInfo> {
  return invoke('get_layout')
}

/**
 * Gabarit de repli, quand rien n'est connecté.
 *
 * `getLayout` refuse hors connexion, et c'est le cas qu'il faut servir : on
 * dessine le clavier et on écrit un effet avant d'avoir branché quoi que ce
 * soit. La géométrie reste ainsi définie **au seul endroit** où elle est
 * testée, dans `crates/candeo-device`.
 */
export function getDefaultLayout(): Promise<LayoutInfo> {
  return invoke('get_default_layout')
}

/** `level` de 0 à 255. */
export function setBrightness(level: number): Promise<void> {
  return invoke('set_brightness', { level })
}

/**
 * Bascule l'effet.
 *
 * Tout sauf `custom` est exécuté par le **micrologiciel** : coût processeur
 * nul, et l'effet survit à la fermeture de l'application.
 */
export function setEffect(effect: Effect): Promise<void> {
  return invoke('set_effect', { effect })
}

/**
 * Pousse une image complète.
 *
 * `frame` doit compter exactement `layout.frameLen` couleurs — **toutes** les
 * cases de la matrice, y compris celles sans LED. En envoyer moins laisse les
 * dernières rangées figées sur leur valeur précédente.
 */
export function present(frame: readonly Rgb[]): Promise<void> {
  const flat = new Array<number>(frame.length * 3)
  for (let i = 0; i < frame.length; i++) {
    flat[i * 3] = frame[i][0]
    flat[i * 3 + 1] = frame[i][1]
    flat[i * 3 + 2] = frame[i][2]
  }
  return invoke('present', { frame: flat })
}

/** Écrit un segment de rangée, sans toucher au reste. */
export function writeRow(row: number, colStart: number, colors: readonly Rgb[]): Promise<void> {
  const flat = colors.flatMap((c) => [c[0], c[1], c[2]])
  return invoke('write_row', { row, colStart, colors: flat })
}

// ---------------------------------------------------------------- bibliothèque

/**
 * Version de l'API d'effets que cette application sait servir.
 *
 * Miroir de `EFFECTS_API_VERSION` dans `src-tauri/src/storage.rs`, au même
 * titre que les types de `api/types.ts` le sont de `lib.rs`. La divergence n'est
 * pas silencieuse : un manifeste qui annonce une version plus récente que celle
 * du Rust est **refusé à l'installation**, avec un message qui le dit.
 *
 * Elle ne vient pas de `@candeo/effects-api` : ce module décrit l'API, il ne se
 * numérote pas lui-même — et tout ce qu'il exporte doit exister dans le jumeau
 * `src-tauri/src/runtime/api.js`, ce qu'une constante de version n'a aucune
 * raison de faire.
 */
export const EFFECTS_API_VERSION = 1

/**
 * Ce qui est écrit dans `manifest.json`, à côté de l'effet.
 *
 * `params` garde la forme de `ParamSpec` telle que déclarée en TypeScript : le
 * Rust ne les interprète pas, les retyper là-bas créerait une seconde source de
 * vérité.
 */
export interface EffectManifest {
  name: string
  description?: string
  params?: Record<string, ParamSpec>
  apiVersion: number
}

/**
 * Écrit `source.ts`, `effect.js` et `manifest.json`, et rend l'`id` retenu.
 *
 * Les deux sources partent ensemble : sans le `.ts` l'effet ne serait plus
 * modifiable, sans le `.js` il ne pourrait plus démarrer sans ouvrir la fenêtre
 * — le transpileur vit ici, dans l'éditeur.
 *
 * L'`id` est **dérivé du nom** par le Rust, jamais repris tel quel. Deux
 * enregistrements sous le même nom mettent donc à jour le même effet.
 */
export function installEffect(
  sourceTs: string,
  js: string,
  manifest: EffectManifest,
): Promise<string> {
  return invoke('install_effect', { sourceTs, js, manifest })
}

/** La source TypeScript d'un effet installé, pour la rouvrir dans l'éditeur. */
export function readEffectSource(id: string): Promise<string> {
  return invoke('read_effect_source', { id })
}

/**
 * Un effet de la bibliothèque : son manifeste, plus ce qui n'en fait pas partie.
 *
 * Les intégrés sont compilés dans le binaire et n'ont pas de dossier ; `kind`
 * les distingue, pour que l'interface n'ait qu'une liste à afficher.
 */
export interface EffectEntry extends EffectManifest {
  id: string
  kind: 'builtin' | 'user'
}

/** Effets intégrés **et** installés, dans une seule liste, d'ordre stable. */
export function listEffects(): Promise<EffectEntry[]> {
  return invoke('list_effects')
}

// ---------------------------------------------------------------- moteur

export type EffectParams = Record<string, ParamValue>

export interface EngineStatus {
  running: boolean
  effectId: string | null
  /** Erreur venant du code de l'effet. Déjà lisible : à afficher telle quelle. */
  error: string | null
  /**
   * Échec d'écriture vers le clavier — sans rapport avec le code de l'effet.
   *
   * Les deux sont distincts parce qu'ils n'ont ni la même cause ni le même
   * remède : un effet impeccable peut n'atteindre aucune LED.
   */
  deviceError: string | null
  /**
   * Vrai si les images parviennent effectivement à un clavier.
   *
   * Faux avec la sortie coupée, mais aussi — et c'est le cas piégeux —
   * quand aucun périphérique n'est connecté : le simulateur s'anime, la case
   * « envoyer » reste cochée, et le clavier garde son image. On lit ça comme
   * « seule la première image est passée ».
   */
  reachingKeyboard: boolean
  toKeyboard: boolean
}

/**
 * Démarre un effet installé.
 *
 * Le moteur tourne dans un fil Rust indépendant de la fenêtre : fermer
 * l'application n'éteint pas l'effet.
 */
export function startEffect(id: string, params: EffectParams = {}): Promise<void> {
  return invoke('start_effect', { id, params })
}

export function stopEffect(): Promise<void> {
  return invoke('stop_effect')
}

/** Ajuste les paramètres à chaud, sans redémarrer la boucle. */
export function setEffectParams(params: EffectParams): Promise<void> {
  return invoke('set_effect_params', { params })
}

/**
 * Active ou coupe l'écriture vers le clavier, **sans** toucher au simulateur.
 *
 * C'est ce qui permet d'écrire un effet sans posséder le clavier.
 */
export function setOutputToKeyboard(on: boolean): Promise<void> {
  return invoke('set_output_to_keyboard', { on })
}

/**
 * Ouvre le flux d'images vers le simulateur.
 *
 * Chaque message est une image brute : `frameLen × 3` octets, dans l'ordre RVB.
 * Un canal, et non un événement global — la portée est explicite et le binaire
 * passe sans détour par un tableau JSON d'entiers.
 *
 * Fermer le canal arrête le flux **sans arrêter l'effet**, qui continue
 * d'alimenter le clavier.
 */
export function subscribeFrames(onFrame: (frame: Uint8Array) => void): Promise<() => void> {
  const channel = new Channel<ArrayBuffer | number[]>()
  channel.onmessage = (m) => {
    onFrame(m instanceof ArrayBuffer ? new Uint8Array(m) : Uint8Array.from(m))
  }
  return invoke<void>('subscribe_frames', { channel }).then(
    () => () => void invoke('unsubscribe_frames'),
  )
}

/**
 * État du moteur, erreur comprise.
 *
 * Interrogé plutôt que poussé : une erreur survenue fenêtre fermée doit se lire
 * à la réouverture, ce qu'un événement ponctuel ne permet pas.
 */
export function engineStatus(): Promise<EngineStatus> {
  return invoke('engine_status')
}
