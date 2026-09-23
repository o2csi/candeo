/**
 * The API for writing effects.
 *
 * An effect is a pure function from time and position to a color. That is
 * what makes YAML unsuitable: it would describe a configuration, not a
 * behavior. Here the effect *is* code.
 *
 * ## The contract
 *
 * An effect module **default-exports** an {@link EffectModule}. The engine
 * looks for nothing else:
 *
 * ```ts
 * import { hsv } from '@candeo/effects-api'
 *
 * export default {
 *   description: 'My effect',
 *   render({ layout, time, frame }) { … },
 * } satisfies EffectModule
 * ```
 *
 * ## This file has a twin
 *
 * ⚠️ It describes what the **editor** shows in autocompletion; what the engine
 * actually provides is written in
 * `apps/desktop/src-tauri/src/runtime/api.js`. If they diverge, the editor
 * promises a function that does not exist, and the error only shows at the
 * first frame. The Rust test `api_js_exports_match_the_typescript_surface`
 * fails if a name disappears from the twin — any change must touch both.
 */

export interface Rgb {
  r: number
  g: number
  b: number
}

/**
 * A matrix position that carries an LED.
 *
 * **Two spaces live here, and they do not say the same thing.**
 *
 * - `row`/`col` place the LED in the **matrix**. Neighborhood has a meaning
 *   there, but a cell is a cell: the space bar takes **a single one** despite its
 *   6.25 u, and matrix holes count as distance although they take no space.
 * - `x`/`y`/`w`/`h` give the **physical rectangle** of the keycap, the one the
 *   simulator draws.
 *
 * An effect that talks about distance must therefore choose what it measures:
 * among the shipped effects, "Radial wave" measures the keycaps, "Diagonal
 * wave" counts matrix steps.
 */
export interface Key {
  /** LED index in the frame. */
  readonly index: number
  readonly row: number
  readonly col: number
  /**
   * The key's name in the keyboard layout the system uses — "Z" or "ECHAP" on a
   * French Windows. For display: it changes with the layout, so find keys by
   * {@link scancode}. Absent when the system gives no name.
   */
  readonly label?: string
  /**
   * What the keyboard sends for this key, in PS/2 set 1: the make code, `0xE0` in
   * the high byte for an extended key (`0xE01D`, right Ctrl). It names the
   * physical key whatever its legend: the keys under the left hand of a gamer are
   * `[0x11, 0x1E, 0x1F, 0x20]`, engraved ZQSD on AZERTY and WASD on QWERTY. Both
   * LEDs of the ISO Enter are `0x1C`.
   *
   * Absent for a key that sends nothing (Fn), and when the layout does not say.
   */
  readonly scancode?: number
  /**
   * Left edge of the keycap, in **keyboard pitch units**: 1 u = the width of a
   * letter key. The origin is at the top left, `y` grows downwards, and the key
   * covers `[x, x + w[ × [y, y + h[`.
   *
   * ## Why `u`, and not a fraction of the keyboard
   *
   * The pitch is an **absolute** quantity — 19.05 mm on any full-size keyboard.
   * `1 u` therefore means the same distance on a full-size keyboard, a TKL or a
   * macro pad, and a scale set in `u` keeps its meaning from one device to
   * another: "a ring every six keys" stays a ring every six keys. Normalizing
   * on the footprint would do the opposite — the same `0.5` would be worth
   * eleven keys here and three elsewhere, and the effect would change its look
   * without a line changing.
   *
   * What a smaller device changes is the **number** of visible rings, not their
   * size. What an effect must not assume, on the other hand, is where the
   * center is: it is read from {@link bounds}, never from `rows`/`cols`.
   *
   * It is also the unit the Rust code carries
   * (`crates/candeo-device/src/layout.rs`): nothing is converted along the way,
   * so nothing can get it wrong there.
   *
   * ## ⚠️ Optional, and that is the heart of the matter
   *
   * The geometry is not read from the device — it only exposes its logical
   * grid — but transcribed by hand. Not every layout has it: it is the
   * `geometry` capability of `docs/design/device-sdk.md` §3.2.
   *
   * An effect that depends on it must therefore **say so by failing**, never
   * subtract `undefined`: the distance would be `NaN`, the color would be
   * clamped to zero, and the keyboard would stay black without a word.
   * {@link center} and {@link bounds} are there for that — they throw, naming
   * the key that has no rectangle.
   */
  readonly x?: number
  /** Top edge, in pitch units. See {@link x}. */
  readonly y?: number
  /** Width, in pitch units. See {@link x}. */
  readonly w?: number
  /** Height, in pitch units. See {@link x}. */
  readonly h?: number
}

export interface Layout {
  readonly name: string
  readonly rows: number
  readonly cols: number
  /** Only the positions that carry an LED. */
  readonly keys: readonly Key[]
}

export interface Frame {
  /** Writes a color at a position. */
  set(key: Key, color: Rgb): void
  /** Writes the same color everywhere. */
  fill(color: Rgb): void
}

export interface EffectContext<P = undefined> {
  readonly layout: Layout
  /**
   * Seconds elapsed since the effect started.
   *
   * **It is the clock, and the only one.** It is taken from real time, not
   * counted in frames: a skipped frame therefore does not slow the animation
   * down, it samples it less often. Animating on `time` keeps the same speed
   * whatever the load on the machine.
   */
  readonly time: number
  /**
   * Frame number, incremented at each render.
   *
   * ⚠️ **It is not a clock.** The loop aims at 30 frames per second but does
   * not guarantee them: a loaded machine renders fewer, and missed frames are
   * **not** made up. `frameIndex * 0.016` is therefore not a duration, and an
   * effect animated on it **slows down** instead of skipping — without
   * reporting anything.
   *
   * It is for what counts in frames and not in seconds: alternating every other
   * frame, seeding a pseudo-random generator, spacing out a costly refresh. For
   * any motion, it is {@link time}.
   */
  readonly frameIndex: number
  readonly frame: Frame
  /**
   * Parameters declared by the effect, as set in the interface.
   *
   * `Rgb` is part of the union because a {@link ParamSpec} of kind `color` has
   * a color as its value, not a number: leaving it out would force every effect
   * parameterized by a color through a cast, to work around a wrong
   * declaration.
   */
  readonly params: ParamsOf<P>
  /**
   * The keys pressed recently on this layout, oldest first. Always empty unless
   * the effect declares `inputs: ['keys']`.
   *
   * A press is a key going down: holding a key does not repeat it, and releases
   * are not reported. Only presses younger than 10 seconds, at most the last 32.
   * On a device, only that keyboard's presses; in the preview, any keyboard's.
   */
  readonly presses: readonly Press[]
  /**
   * The local wall-clock time, read once per frame. Zeroed unless the effect
   * declares `inputs: ['clock']`.
   *
   * {@link EffectContext.time} stays the seconds since the effect started, and
   * is what animations run on; `clock` is what a clock face needs.
   */
  readonly clock: Clock
  /**
   * Every signal other programs have sent and that has not expired, by name —
   * `{ build: 'failed', volume: 0.4 }`. Empty unless the effect declares
   * `inputs: ['signals']`.
   *
   * An author's tool, for an effect drawing many values at once. An effect
   * reading `signals.build` works only for whoever sends exactly `build`: to let
   * a signal drive one setting of any effect, the person binds that setting to
   * it in the gallery instead, and the effect reads it in `params` without
   * knowing (`docs/design/inputs-and-automations.md` §2.3.1).
   */
  readonly signals: Readonly<Record<string, string | number | boolean>>
}

/** A key going down. See {@link EffectContext.presses}. */
export interface Press {
  /** The key, as found in `layout.keys` by scancode. */
  readonly key: Key
  /** When it went down, on the clock of {@link EffectContext.time}. */
  readonly at: number
}

/** The local wall-clock time. See {@link EffectContext.clock}. */
export interface Clock {
  readonly year: number
  /** 1 is January — unlike `Date.prototype.getMonth()`, which starts at 0. */
  readonly month: number
  /** Day of the month, 1 to 31. */
  readonly day: number
  /** 0 is Sunday, as `Date.prototype.getDay()` has it. */
  readonly weekday: number
  /** 0 to 23: the hour as the clock shows it, not on 12. */
  readonly hours: number
  readonly minutes: number
  readonly seconds: number
  /** Milliseconds within the second, 0 to 999, for a second hand that moves. */
  readonly ms: number
}

/** What an effect reads besides time and its parameters. */
export type Input = 'keys' | 'clock' | 'signals'

/** An effect renders a frame at each call. */
export type Effect = (ctx: EffectContext) => void

/**
 * The value of a parameter set in the interface.
 *
 * Exactly the set of `default`s a {@link ParamSpec} can carry. Named rather
 * than written twice: the editor uses it to type what it sends to
 * `start_effect`, and the two unions must not be able to diverge.
 */
export type ParamValue = number | string | boolean | Rgb

/**
 * The value a parameter carries, inferred from its declaration.
 *
 * It is what avoids writing `params.color as Rgb` in an effect — a cast would
 * be impossible anyway in a built-in effect, which is JavaScript run as it is
 * by the engine.
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
 * The parameters as `render` receives them.
 *
 * Without a declaration — an effect that has none — it falls back to the wide
 * form, which lets the effect work without declaring anything.
 */
export type ParamsOf<P> = P extends Record<string, ParamSpec>
  ? { readonly [K in keyof P]: ValueOfSpec<P[K]> }
  : Readonly<Record<string, ParamValue>>

/**
 * Text shown to the user: a string, or the same text in several languages.
 *
 * ```ts
 * label: 'Vitesse'
 * label: { en: 'Speed', fr: 'Vitesse' }
 * ```
 *
 * The interface picks its own language, then English, then the first entry.
 * A plain string suits an effect written for one language.
 */
export type Text = string | { readonly [language: string]: string }

/**
 * An option of a `choice`: its value alone, shown as it is, or the value the effect
 * receives and the text shown for it.
 *
 * ```ts
 * options: ['calm', 'wild']
 * options: [{ value: 'calm', label: { en: 'Calm', fr: 'Calme' } }]
 * ```
 */
export type ChoiceOption = string | { readonly value: string; readonly label: Text }

/** Declaration of an adjustable parameter, for the interface to present it. */
export type ParamSpec =
  | { kind: 'number'; label: Text; min: number; max: number; step?: number; default: number }
  | { kind: 'color'; label: Text; default: Rgb }
  | { kind: 'boolean'; label: Text; default: boolean }
  | { kind: 'choice'; label: Text; options: readonly ChoiceOption[]; default: string }

/** A kind of device an effect can target. The list grows with the devices. */
export type DeviceKind = 'keyboard'

export interface EffectModule<P = undefined> {
  /**
   * @deprecated The file name is the effect's name: this property is ignored.
   * Kept so that effects written before still type-check.
   */
  readonly name?: string
  /** What the effect does, in one sentence. See {@link Text}. */
  readonly description?: Text
  /**
   * The version of the effects API this effect was written against. Absent
   * means the first one; Candeo refuses to load an effect written for a newer
   * version than it knows.
   */
  readonly apiVersion?: number
  /**
   * The kinds of device this effect is meant for. Only keyboards exist today, so
   * an effect that says nothing is read as `['keyboard']`; saying it is what
   * keeps the effect right the day a second kind arrives.
   */
  readonly kinds?: readonly DeviceKind[] | 'all'
  /**
   * What the effect reads besides time and its parameters. `['keys']` gives it
   * {@link EffectContext.presses}; key presses are read only while such an
   * effect runs, and the gallery says so (`docs/design/key-input.md`).
   * `['clock']` gives it {@link EffectContext.clock}: nothing is captured for
   * it, so it is read whenever the effect asks
   * (`docs/design/inputs-and-automations.md` §2.1). `['signals']` gives it
   * {@link EffectContext.signals}.
   */
  readonly inputs?: readonly Input[]
  readonly params?: P
  /** `ctx.params` is typed from `params` above. */
  readonly render: (ctx: EffectContext<P>) => void
}

/**
 * Declares an effect.
 *
 * Does **nothing** at run time — it returns its argument as it is. Its only
 * role is to give the object literal a contextual type, which types the
 * parameters of `render`:
 *
 * ```ts
 * export default defineEffect({
 *   description: 'My effect',
 *   render({ layout, time, frame }) { … },   // typed, without annotations
 * })
 * ```
 *
 * Without this wrapper — or without `satisfies EffectModule` — an object
 * literal has no contextual type: `layout`, `time`, `frame` and `params` are
 * then implicitly `any`, and `strict` rejects them. Four errors, on the most
 * natural way to write an effect; that is precisely what this function avoids.
 */
export function defineEffect<
  const P extends Readonly<Record<string, ParamSpec>> | undefined = undefined,
>(effect: EffectModule<P>): EffectModule<P> {
  return effect
}

// ------------------------------------------------------------------ utilities

export const rgb = (r: number, g: number, b: number): Rgb => ({
  r: clampByte(r),
  g: clampByte(g),
  b: clampByte(b),
})

export const BLACK: Rgb = { r: 0, g: 0, b: 0 }

function clampByte(v: number): number {
  return Math.max(0, Math.min(255, Math.round(v)))
}

/** Hue 0-360, saturation and value 0-1. */
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

// ------------------------------------------------------------------- geometry
//
// Two functions with the same role: read a rectangle **or fail saying so**.
// That is all that separates a portable spatial effect from an effect that
// renders black on the layouts we do not have at hand. See {@link Key.x}.

/** A rectangle in keyboard pitch units. */
export interface Rect {
  readonly x: number
  readonly y: number
  readonly w: number
  readonly h: number
}

/**
 * The center of a key's keycap, in pitch units.
 *
 * The center and not the corner: that is where the LED is, and it is what puts
 * the space bar in the middle of its 6.25 u rather than at its left edge.
 *
 * @throws if the key has no rectangle — see {@link Key.x}.
 */
export function center(key: Key): { x: number; y: number } {
  const { x, y, w, h } = key
  if (x === undefined || y === undefined || w === undefined || h === undefined) {
    throw new TypeError(noRectangle(key))
  }
  return { x: x + w / 2, y: y + h / 2 }
}

/**
 * The physical footprint of the drawing, in pitch units.
 *
 * It is what replaces `rows`/`cols` as soon as distance is involved: the middle
 * of the keyboard is at `x + w / 2`, and it stays there on a layout without a
 * numeric keypad as on a full-size one. `(cols - 1) / 2` designates the middle
 * of the **matrix**, which is the middle of nothing visible.
 *
 * Same definition as the simulator's `viewBox`: the center of a wave is
 * therefore the center of what one looks at.
 *
 * A layout without any key returns an empty rectangle — there is nothing to
 * frame, and nothing to light either.
 *
 * @throws as soon as a key has no rectangle — see {@link Key.x}.
 */
export function bounds(layout: Layout): Rect {
  if (layout.keys.length === 0) return { x: 0, y: 0, w: 0, h: 0 }

  let x0 = Infinity
  let y0 = Infinity
  let x1 = -Infinity
  let y1 = -Infinity

  for (const key of layout.keys) {
    const { x, y, w, h } = key
    if (x === undefined || y === undefined || w === undefined || h === undefined) {
      throw new TypeError(noRectangle(key))
    }
    if (x < x0) x0 = x
    if (y < y0) y0 = y
    if (x + w > x1) x1 = x + w
    if (y + h > y1) y1 = y + h
  }

  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
}

/**
 * The sentence {@link center} and {@link bounds} throw when they fail.
 *
 * It names the faulty key: a contributed layout can be half drawn, and "a
 * rectangle is missing" without saying which one cannot be fixed.
 */
function noRectangle(key: Key): string {
  const which = key.label === undefined ? `position ${key.index}` : `“${key.label}”`
  return (
    `${which} has no rectangle: this layout has no surveyed geometry, ` +
    'and an effect that measures physical distances has nothing to measure on it.'
  )
}

// The reference example lives in `example.ts`: this file describes the API, it
// exports no effect. A default export here would make the library itself an
// effect, which it is not.
//
// The effects shipped with the application live in `packages/effects/`: plain
// `.ts` files against this same API, copied into the effects folder at startup.
// They are the best reading of what can be written here.
