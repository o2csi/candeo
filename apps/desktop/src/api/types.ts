/**
 * Miroir TypeScript des types sérialisés par la couche Tauri.
 *
 * La référence est `apps/desktop/src-tauri/src/lib.rs` : tout écart ici est un
 * bug qui ne se verra qu'à l'exécution. Les noms de champs suivent ceux de
 * Rust — serde ne les renomme pas.
 */

/**
 * Décision prise pour un appareil, retenue dans `settings.json`.
 *
 * `detected` est le défaut : **un appareil jamais vu n'est pas piloté**. Écrire
 * sur un périphérique USB qu'on comprend mal n'est pas anodin, et adopter par
 * défaut est la façon de casser le matériel de quelqu'un.
 */
export type DeviceState = 'detected' | 'adopted' | 'ignored'

/**
 * Désigne un appareil, et rien d'autre.
 *
 * VID et PID, comme l'adoption les identifie : c'est la clé de la table des
 * appareils ouverts côté Rust, et celle des boucles de rendu. Toute commande qui
 * agit sur **un** appareil en prend un — il n'y a plus d'appareil implicite.
 */
export interface DeviceRef {
  vid: number
  pid: number
}

export interface DeviceInfo {
  name: string
  vid: number
  pid: number
  /** Vrai si le périphérique est effectivement branché. */
  present: boolean
  /**
   * Ce que l'utilisateur a décidé — indépendant de {@link present}. Un appareil
   * piloté peut être débranché, un appareil branché peut être ignoré.
   */
  state: DeviceState
  /** Vrai si c'est **cet** appareil qui est ouvert en ce moment. */
  open: boolean
  /**
   * Dernier échec d'ouverture **de cet appareil**.
   *
   * Chacun porte le sien : une ouverture qui échoue n'empêche pas les autres de
   * fonctionner, et ne leur fait pas porter son message.
   */
  error: string | null
  /**
   * Micrologiciel contre lequel le gabarit a été relevé, `v1.5`. Connu sans rien
   * ouvrir : c'est une donnée du gabarit.
   */
  surveyedFirmware: string
  /**
   * Micrologiciel **lu** à l'ouverture. `null` quand l'appareil n'est pas ouvert,
   * ou quand la lecture a échoué — ce que {@link warnings} dit alors.
   */
  firmware: string | null
  /**
   * Ce que l'inspection à l'ouverture a trouvé qui mérite d'être vu.
   *
   * **Vide veut dire « rien à signaler », pas « compatible »** : l'appareil
   * confirme qu'une commande existe, jamais que ses arguments sont bons. Aucun de
   * ces avertissements ne bloque quoi que ce soit.
   *
   * `readonly` : la liste des appareils est exposée en lecture seule par
   * `useDevice`, et un appareil se repasse tel quel aux commandes d'adoption.
   */
  warnings: readonly string[]
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
