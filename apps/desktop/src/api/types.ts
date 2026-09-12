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

/**
 * Une touche, telle que le simulateur doit la dessiner.
 *
 * Deux systèmes de coordonnées cohabitent, et ils ne disent pas la même chose :
 * `row`/`col` situent la LED dans la matrice, donc son rang dans une image ;
 * `x`/`y`/`w`/`h` donnent le rectangle physique. Le second ne se déduit pas du
 * premier — le périphérique ne déclare aucune dimension.
 */
export interface KeyInfo {
  index: number
  row: number
  col: number
  /** Nom gravé, variante French (ISO). */
  name: string
  /** En unités de pas de clavier : 1 u = une touche alphabétique. */
  x: number
  y: number
  w: number
  h: number
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
   *
   * ⚠️ Une touche ≠ une LED, dans les deux sens : l'Entrée ISO apparaît **deux
   * fois** sous le même `name` (index 57 et 79, les deux bras du L), et la barre
   * d'espace une seule malgré ses 6,25 u.
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
