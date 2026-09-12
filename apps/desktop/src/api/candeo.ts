/**
 * Seul point de passage vers le Rust.
 *
 * Aucun composant n'appelle `invoke` directement : tout transite par ici, pour
 * que la surface soit typée en un seul endroit et que le jour où une commande
 * change de forme, le compilateur désigne les appelants.
 */

import { invoke } from '@tauri-apps/api/core'

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
