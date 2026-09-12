/**
 * Miroir TypeScript des types sérialisés par la couche Tauri.
 *
 * La référence est `apps/desktop/src-tauri/src/lib.rs` : tout écart ici est un
 * bug qui ne se verra qu'à l'exécution. Les noms de champs suivent ceux de
 * Rust — serde ne les renomme pas.
 */

export interface DeviceInfo {
  name: string
  vid: number
  pid: number
  /** Vrai si le périphérique est effectivement branché. */
  present: boolean
}

export interface KeyInfo {
  index: number
  row: number
  col: number
}

export interface LayoutInfo {
  name: string
  rows: number
  cols: number
  /**
   * Taille d'une image : **toutes** les cases de la matrice, trous compris.
   * Vaut 132 sur le DeathStalker.
   */
  frameLen: number
  /**
   * Uniquement les cases portant une LED. 106 sur le DeathStalker.
   *
   * `keys.length` et {@link frameLen} diffèrent, et c'est voulu : une image
   * doit couvrir `frameLen` positions, pas `keys.length`. En envoyer moins
   * laisse les dernières rangées figées sur leur valeur précédente.
   */
  keys: KeyInfo[]
}

export type Effect =
  | { kind: 'off' }
  | { kind: 'spectrumCycle' }
  | { kind: 'wave'; direction: number; speed: number }
  | { kind: 'custom' }

/** Une couleur, composantes dans l'ordre RVB. */
export type Rgb = readonly [r: number, g: number, b: number]
