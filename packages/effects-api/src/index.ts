/**
 * API d'écriture d'effets.
 *
 * Un effet est une fonction pure du temps et de la position vers une couleur.
 * C'est ce qui rend YAML inadapté : on décrirait une configuration, pas un
 * comportement. Ici l'effet *est* du code.
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
  readonly params: Record<string, number | string | boolean>
}

/** Un effet rend une image à chaque appel. */
export type Effect = (ctx: EffectContext) => void

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

// ---------------------------------------------------------------- exemple

/**
 * Onde circulaire partant du centre du clavier.
 *
 * Sert d'exemple de référence : un effet tient en quelques lignes, et se lit
 * comme ce qu'il fait.
 */
export const ripple: EffectModule = {
  name: 'Onde',
  description: 'Une onde de teinte se propage depuis le centre',
  params: {
    speed: { kind: 'number', label: 'Vitesse', min: 0, max: 400, default: 120 },
    scale: { kind: 'number', label: 'Échelle', min: 1, max: 60, default: 18 },
  },
  render({ layout, time, frame, params }) {
    const cx = (layout.cols - 1) / 2
    const cy = (layout.rows - 1) / 2
    const speed = Number(params.speed ?? 120)
    const scale = Number(params.scale ?? 18)
    for (const key of layout.keys) {
      const d = Math.hypot(key.col - cx, key.row - cy)
      frame.set(key, hsv(time * speed + d * scale, 1, 1))
    }
  },
}
