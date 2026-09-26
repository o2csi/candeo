import type { Text } from '@candeo/effects-api'

/**
 * TypeScript mirror of the types serialized by the Tauri layer.
 *
 * The reference is `apps/desktop/src-tauri/src/lib.rs`: any gap here is a bug
 * that will only show at run time. The field names follow Rust's — serde does
 * not rename them.
 */

/**
 * Decision made for a device, remembered in `settings.json`.
 *
 * `detected` is the default: **a device never seen is not controlled**. Writing
 * to a USB device that is poorly understood is not harmless, and adopting by
 * default is the way to break someone's hardware.
 */
export type DeviceState = 'detected' | 'adopted' | 'ignored'

/**
 * Designates a device, and nothing else.
 *
 * VID and PID, as adoption identifies them: it is the key of the table of open
 * devices on the Rust side, and that of the render loops. Every command that
 * acts on **one** device takes one — there is no implicit device any more.
 */
export interface DeviceRef {
  vid: number
  pid: number
}

/**
 * A command's failure as Rust sends it: a key under `errors.` and its parameters,
 * for the window to say in its language. See `api/journal.ts`, `message`.
 */
export interface Failure {
  code: string
  params: Record<string, string>
}

export interface DeviceInfo {
  name: string
  vid: number
  pid: number
  /** True if the device is actually plugged in. */
  present: boolean
  /**
   * What the user decided — independent of {@link present}. A controlled device
   * can be unplugged, a plugged-in device can be ignored.
   */
  state: DeviceState
  /** True if it is **this** device that is open right now. */
  open: boolean
  /**
   * Last opening failure **of this device**.
   *
   * Each carries its own: an opening that fails does not prevent the others from
   * working, and does not make them carry its message.
   */
  error: Failure | null
  /**
   * Firmware against which the layout was surveyed, `v1.5`. Known without
   * opening anything: it is a piece of the layout's data.
   */
  surveyedFirmware: string
  /**
   * Firmware **read** on opening. `null` when the device is not open, or when
   * the read failed — which {@link warnings} then says.
   */
  firmware: string | null
  /**
   * What the inspection on opening found that deserves to be seen.
   *
   * **Empty means "nothing to report", not "compatible"**: the device confirms
   * that a command exists, never that its arguments are right. None of these
   * warnings blocks anything.
   *
   * `readonly`: the list of devices is exposed read-only by `useDevice`, and a
   * device is passed as it is to the adoption commands.
   *
   * Already in the interface language: Rust renders them when listing.
   */
  warnings: readonly string[]
  /**
   * Whose definition it is known by: built in — reviewed, and replayed by the
   * tests — or a file of yours (`docs/design/device-sdk.md` §9).
   */
  origin: 'builtIn' | 'yours'
  /** The file of yours it is known by, for *Open*; `null` for a built-in one. */
  file: string | null
  /** Every definition of it, to choose from: the built-in one first. */
  definitions: readonly { file: string; origin: 'builtIn' | 'yours' }[]
  /** The file of yours chosen for it when another drives it: it does not load. */
  unloadedChoice: string | null
  /** What its lights are, and how many. */
  lights: 'keys' | 'zones'
  lightCount: number
}

/**
 * A key, as the simulator must draw it.
 *
 * Two coordinate systems coexist, and they do not say the same thing:
 * `row`/`col` locate the LED in the matrix, hence its rank in a frame;
 * `x`/`y`/`w`/`h` give the physical rectangle. The second cannot be derived
 * from the first — the device declares no dimension.
 */
export interface KeyInfo {
  index: number
  row: number
  col: number
  /**
   * What the keyboard sends for this key, in PS/2 set 1 (`0xE0..` for an extended
   * key). It names the physical key whatever its legend. Absent for Fn.
   */
  scancode?: number
  /** The key's name in the system's keyboard layout, when the system gives one. */
  label?: string
  /** In keyboard pitch units: 1 u = one letter key. */
  x: number
  y: number
  w: number
  h: number
  /**
   * How the simulator draws it inside that rectangle. Absent: the rectangle, as
   * every key is. The rectangle stays what effects measure with.
   */
  shape?: KeyShape
}

/** Mirror of `candeo_device::Shape`, less the rectangle, which goes unsaid. */
export type KeyShape = 'disc' | 'archUp' | 'archDown'

/** A part of a device that lights nothing, drawn under its lights. Mirror of `OutlineInfo`. */
export interface OutlinePart {
  x: number
  y: number
  w: number
  h: number
  /** Corner radius. */
  r: number
}

/** Mirror of `LayoutInfo`, in `src-tauri/src/lib.rs`. */
export interface LayoutInfo {
  name: string
  rows: number
  cols: number
  /**
   * Size of a frame: **all** the cells of the matrix, gaps included.
   * 132 on the DeathStalker.
   */
  frameLen: number
  /**
   * Only the cells carrying an LED. 106 on the DeathStalker.
   *
   * `keys.length` and {@link frameLen} differ, and that is intended: a frame
   * must cover `frameLen` positions, not `keys.length`. Sending fewer leaves
   * the last rows frozen on their previous value.
   *
   * ⚠️ A key ≠ an LED, in both directions: the ISO Enter appears **twice**
   * under the same `name` (index 57 and 79, the two arms of the L), and the
   * space bar only once despite its 6.25 u.
   */
  keys: KeyInfo[]
  /**
   * The effects this device's **firmware** runs on its own, by their gallery
   * ids — `hardware:spectrumCycle`, `hardware:wave`.
   *
   * *Off* is never listed and stays offered everywhere: a device that draws
   * nothing of its own still goes dark, on a black frame. The gallery offers
   * these only, so that nobody picks a mode the device would refuse.
   */
  firmwareEffects: FirmwareEffectInfo[]
  /**
   * What this device's lights are.
   *
   * `keys`: keys, which can be pressed. `zones`: a ring, a logo, a strip — lit,
   * never typed on. The gallery uses it so that a surface nobody types on is
   * never offered an effect reading key presses: it would never see one.
   */
  lights: 'keys' | 'zones'
  /** What the simulator draws under the lights — a lid, a base, ports. Empty for a keyboard. */
  outline: OutlinePart[]
}

/**
 * An effect run by the firmware, as the gallery needs it.
 *
 * `colours` says how many colors it paints with: zero for those that have their
 * own palette — a spectrum, a rainbow — and the gallery then asks for nothing,
 * rather than offering a setting with no effect.
 */
export interface FirmwareEffectInfo {
  id: string
  colours: number
  /**
   * What its definition calls it and says it shows: a string, the same in every
   * language, or one per language. `null` where it says nothing.
   */
  name: Text | null
  summary: Text | null
  /** The pattern its legend is drawn with (`keyboard/illustration.ts`), `null` for none. */
  looks: string | null
}

export type Effect =
  | { kind: 'off' }
  | { kind: 'spectrumCycle' }
  | { kind: 'wave'; direction: number; speed: number }
  | { kind: 'custom' }

/** A color, components in RGB order. */
export type Rgb = readonly [r: number, g: number, b: number]
