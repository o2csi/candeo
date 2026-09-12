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

export interface EffectContext {
  readonly layout: Layout
  /** Secondes écoulées depuis le démarrage de l'effet. */
  readonly time: number
  /** Numéro d'image, incrémenté à chaque rendu. */
  readonly frameIndex: number
  readonly frame: Frame
  /** Paramètres déclarés par l'effet, tels que réglés dans l'interface. */
  readonly params: Readonly<Record<string, ParamValue>>
}

/** Un effet rend une image à chaque appel. */
export type Effect = (ctx: EffectContext) => void

/**
 * Ce que vaut un paramètre réglé dans l'interface.
 *
 * Exactement l'ensemble des `default` que {@link ParamSpec} peut porter, `Rgb`
 * compris : un paramètre de couleur arrive dans `params` sous forme d'objet,
 * pas de nombre. Sans ce cas, l'éditeur promettrait un type que le moteur ne
 * livre pas — l'écart ne se verrait qu'à la première image.
 */
export type ParamValue = number | string | boolean | Rgb

/** Déclaration d'un paramètre réglable, pour que l'interface le présente. */
export type ParamSpec =
  | { kind: 'number'; label: string; min: number; max: number; step?: number; default: number }
  | { kind: 'color'; label: string; default: Rgb }
  | { kind: 'boolean'; label: string; default: boolean }
  | { kind: 'choice'; label: string; options: readonly string[]; default: string }

export interface EffectModule {
  readonly name: string
  readonly description?: string
  readonly params?: Readonly<Record<string, ParamSpec>>
  readonly render: Effect
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
