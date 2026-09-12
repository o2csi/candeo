/**
 * Seul point de passage vers le Rust.
 *
 * Aucun composant n'appelle `invoke` directement : tout transite par ici, pour
 * que la surface soit typée en un seul endroit et que le jour où une commande
 * change de forme, le compilateur désigne les appelants.
 */

import { Channel, invoke } from '@tauri-apps/api/core'

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

// ---------------------------------------------------------------- moteur

export type EffectParams = Record<string, number | string | boolean>

export interface EngineStatus {
  running: boolean
  effectId: string | null
  /** Déjà lisible : à afficher tel quel. */
  error: string | null
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
