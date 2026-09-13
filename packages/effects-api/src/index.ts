/**
 * API d'écriture d'effets.
 *
 * Un effet est une fonction pure du temps et de la position vers une couleur.
 * C'est ce qui rend YAML inadapté : on décrirait une configuration, pas un
 * comportement. Ici l'effet *est* du code.
 *
 * ## Le contrat
 *
 * Un module d'effet **exporte par défaut** un {@link EffectModule}. Le moteur
 * ne cherche rien d'autre :
 *
 * ```ts
 * import { hsv } from '@candeo/effects-api'
 *
 * export default {
 *   name: 'Mon effet',
 *   render({ layout, time, frame }) { … },
 * } satisfies EffectModule
 * ```
 *
 * ## Ce fichier a un jumeau
 *
 * ⚠️ Il décrit ce que l'**éditeur** montre en autocomplétion ; ce que le moteur
 * fournit réellement est écrit dans
 * `apps/desktop/src-tauri/src/runtime/api.js`. S'ils divergent, l'éditeur
 * promet une fonction qui n'existe pas, et l'erreur ne se voit qu'à la première
 * image. Le test Rust `api_js_exports_match_the_typescript_surface` échoue si
 * un nom disparaît du jumeau — toute modification doit toucher les deux.
 */

export interface Rgb {
  r: number
  g: number
  b: number
}

/** Une position de la matrice portant une LED. */
export interface Key {
  /** Index de LED dans l'image. */
  readonly index: number
  readonly row: number
  readonly col: number
  /** Nom lisible, quand le périphérique le fournit. */
  readonly label?: string
}

export interface Layout {
  readonly name: string
  readonly rows: number
  readonly cols: number
  /** Uniquement les positions portant une LED. */
  readonly keys: readonly Key[]
}

export interface Frame {
  /** Écrit une couleur à une position. */
  set(key: Key, color: Rgb): void
  /** Écrit la même couleur partout. */
  fill(color: Rgb): void
}

export interface EffectContext<P = undefined> {
  readonly layout: Layout
  /**
   * Secondes écoulées depuis le démarrage de l'effet.
   *
   * **C'est l'horloge, et la seule.** Elle est prise sur le temps réel, pas
   * comptée en images : une image sautée ne ralentit donc pas l'animation, elle
   * l'échantillonne moins souvent. Animer sur `time` garde la même vitesse
   * quelle que soit la charge de la machine.
   */
  readonly time: number
  /**
   * Numéro d'image, incrémenté à chaque rendu.
   *
   * ⚠️ **Ce n'est pas une horloge.** La boucle vise 30 images par seconde mais
   * ne les garantit pas : une machine chargée en fait moins, et les images
   * manquées ne sont **pas** rattrapées. `frameIndex * 0.016` n'est donc pas
   * une durée, et un effet animé dessus **ralentit** au lieu de sauter — sans
   * rien signaler.
   *
   * Il sert à ce qui se compte en images et non en secondes : alterner une
   * image sur deux, semer un générateur pseudo-aléatoire, espacer un
   * rafraîchissement coûteux. Pour tout mouvement, c'est {@link time}.
   */
  readonly frameIndex: number
  readonly frame: Frame
  /**
   * Paramètres déclarés par l'effet, tels que réglés dans l'interface.
   *
   * `Rgb` fait partie de l'union parce qu'un {@link ParamSpec} de type `color`
   * a pour valeur une couleur, pas un nombre : l'omettre obligerait tout effet
   * paramétré par une couleur à passer par un transtypage, pour contourner une
   * déclaration fausse.
   */
  readonly params: ParamsOf<P>
}

/** Un effet rend une image à chaque appel. */
export type Effect = (ctx: EffectContext) => void

/**
 * Ce que vaut un paramètre réglé dans l'interface.
 *
 * Exactement l'ensemble des `default` que {@link ParamSpec} peut porter. Nommé
 * plutôt qu'écrit deux fois : l'éditeur s'en sert pour typer ce qu'il envoie à
 * `start_effect`, et les deux unions ne doivent pas pouvoir diverger.
 */
export type ParamValue = number | string | boolean | Rgb

/**
 * La valeur que porte un paramètre, déduite de sa déclaration.
 *
 * C'est ce qui évite d'écrire `params.couleur as Rgb` dans un effet — un
 * transtypage serait de toute façon impossible dans un effet intégré, qui est
 * du JavaScript exécuté tel quel par le moteur.
 */
type ValueOfSpec<S> = S extends { kind: 'number' }
  ? number
  : S extends { kind: 'color' }
    ? Rgb
    : S extends { kind: 'boolean' }
      ? boolean
      : S extends { kind: 'choice' }
        ? string
        : ParamValue

/**
 * Les paramètres tels que `render` les reçoit.
 *
 * Sans déclaration — un effet qui n'en a pas — on retombe sur la forme large,
 * ce qui laisse l'effet fonctionner sans rien déclarer.
 */
export type ParamsOf<P> = P extends Record<string, ParamSpec>
  ? { readonly [K in keyof P]: ValueOfSpec<P[K]> }
  : Readonly<Record<string, ParamValue>>

/** Déclaration d'un paramètre réglable, pour que l'interface le présente. */
export type ParamSpec =
  | { kind: 'number'; label: string; min: number; max: number; step?: number; default: number }
  | { kind: 'color'; label: string; default: Rgb }
  | { kind: 'boolean'; label: string; default: boolean }
  | { kind: 'choice'; label: string; options: readonly string[]; default: string }

export interface EffectModule<P = undefined> {
  readonly name: string
  readonly description?: string
  readonly params?: P
  /** `ctx.params` est typé d'après `params` ci-dessus. */
  readonly render: (ctx: EffectContext<P>) => void
}

/**
 * Déclare un effet.
 *
 * Ne fait **rien** à l'exécution — elle rend son argument tel quel. Son seul
 * rôle est de donner un type contextuel à l'objet littéral, ce qui type les
 * paramètres de `render` :
 *
 * ```ts
 * export default defineEffect({
 *   name: 'Mon effet',
 *   render({ layout, time, frame }) { … },   // typés, sans annotation
 * })
 * ```
 *
 * Sans cette enveloppe — ou sans `satisfies EffectModule` — un objet littéral
 * n'a aucun type contextuel : `layout`, `time`, `frame` et `params` sont alors
 * implicitement `any`, et `strict` les refuse. Quatre erreurs, sur la façon la
 * plus naturelle d'écrire un effet ; c'est précisément ce que cette fonction
 * évite.
 */
export function defineEffect<
  const P extends Readonly<Record<string, ParamSpec>> | undefined = undefined,
>(effect: EffectModule<P>): EffectModule<P> {
  return effect
}

// ---------------------------------------------------------------- utilitaires

export const rgb = (r: number, g: number, b: number): Rgb => ({
  r: clampByte(r),
  g: clampByte(g),
  b: clampByte(b),
})

export const BLACK: Rgb = { r: 0, g: 0, b: 0 }

function clampByte(v: number): number {
  return Math.max(0, Math.min(255, Math.round(v)))
}

/** Teinte 0-360, saturation et valeur 0-1. */
export function hsv(h: number, s: number, v: number): Rgb {
  const c = v * s
  const hp = (((h % 360) + 360) % 360) / 60
  const x = c * (1 - Math.abs((hp % 2) - 1))
  const [r, g, b] =
    hp < 1 ? [c, x, 0] :
    hp < 2 ? [x, c, 0] :
    hp < 3 ? [0, c, x] :
    hp < 4 ? [0, x, c] :
    hp < 5 ? [x, 0, c] :
             [c, 0, x]
  const m = v - c
  return rgb((r + m) * 255, (g + m) * 255, (b + m) * 255)
}

export const lerp = (a: number, b: number, t: number): number => a + (b - a) * t

export function mix(a: Rgb, b: Rgb, t: number): Rgb {
  return rgb(lerp(a.r, b.r, t), lerp(a.g, b.g, t), lerp(a.b, b.b, t))
}

// L'exemple de référence vit dans `example.ts` : ce fichier décrit l'API, il
// n'exporte pas d'effet. Un export par défaut ici ferait de la bibliothèque
// elle-même un effet, ce qu'elle n'est pas.
//
// Les effets **livrés avec l'application** sont ailleurs encore, dans
// `apps/desktop/src-tauri/src/builtins/` : ils sont compilés dans le binaire et
// écrits en JavaScript, contre cette même API. C'est la meilleure lecture
// disponible de ce qu'on peut écrire ici.
