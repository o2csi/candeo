/**
 * The single way through to Rust.
 *
 * No component calls `invoke` directly: everything goes through here, so that
 * the surface is typed in a single place and, the day a command changes shape,
 * the compiler points at the callers.
 */

import { Channel, invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { ParamSpec, ParamValue, Text } from '@candeo/effects-api'

import type {
  DeviceInfo,
  DeviceRef,
  DeviceState,
  Failure,
  FirmwareEffectInfo,
  LayoutInfo,
  Rgb,
} from './types'

/** Lists the known layouts, plugged in or not, with the state of each. */
export function listDevices(): Promise<DeviceInfo[]> {
  return invoke('list_devices')
}

/**
 * Remembers "controlled" for this device, and opens it if it is there.
 *
 * The decision is written **before** opening and holds even if opening fails:
 * the following startups will replay it. Returns the layout when the device was
 * opened, `null` when it is adopted but unplugged — it is not an error.
 */
export function adoptDevice(vid: number, pid: number): Promise<LayoutInfo | null> {
  return invoke('adopt_device', { vid, pid })
}

/** Remembers "ignored", and closes the device if it was open. */
export function ignoreDevice(vid: number, pid: number): Promise<void> {
  return invoke('ignore_device', { vid, pid })
}

/**
 * One-off opening, without deciding anything.
 *
 * Does not touch `settings.json`, so does not survive a restart — unlike
 * {@link adoptDevice}.
 */
export function connect(vid: number, pid: number): Promise<LayoutInfo> {
  return invoke('connect', { vid, pid })
}

/**
 * Closes **one** device. The others are not touched.
 *
 * With {@link connect}, the pair that opens and closes **without deciding** —
 * whereas {@link adoptDevice} and {@link ignoreDevice} write to `settings.json`.
 * `useDevice` wraps both; no screen calls them yet.
 */
export function disconnect(device: DeviceRef): Promise<void> {
  return invoke('disconnect', { device })
}

/** Layout of an open device. Fails if it is not open. */
export function getLayout(device: DeviceRef): Promise<LayoutInfo> {
  return invoke('get_layout', { device })
}

/**
 * Fallback layout, when nothing is connected.
 *
 * `getLayout` refuses when disconnected, and that is the case to serve: the
 * keyboard is drawn and an effect written before anything is plugged in. The
 * geometry thus stays defined **in the one place** where it is tested, in
 * `crates/candeo-device`.
 */
export function getDefaultLayout(): Promise<LayoutInfo> {
  return invoke('get_default_layout')
}

/**
 * **Full** brightness: the default of a keyboard just plugged in.
 *
 * Mirror of `DEFAULT_BRIGHTNESS` in `src-tauri/src/storage.rs`. It is also the
 * value `settings.json` does not write — remembering it amounts to removing the
 * entry.
 */
export const BRIGHTNESS_DEFAULT = 255

/**
 * Writes the brightness **to the keyboard**, and nothing else. `level` from 0 to
 * 255.
 *
 * Remembers nothing: that is {@link rememberBrightness}. Same separation as
 * {@link setEffectParams} and {@link rememberEffectParams}, for the same reason —
 * a slider being dragged produces dozens of HID writes and a single disk write,
 * when it stops.
 */
export function setBrightness(device: DeviceRef, level: number): Promise<void> {
  return invoke('set_brightness', { device, level })
}

/**
 * Remembers this device's brightness, without touching the keyboard.
 *
 * It is reapplied on adoption and at startup: a remembered level that is not
 * reapplied on plug-in would be of no use, and the recorded protocol can write
 * the brightness but not read it back.
 *
 * {@link BRIGHTNESS_DEFAULT} **erases** the entry, as an empty parameters table
 * erases an effect's settings.
 */
export function rememberBrightness(device: DeviceRef, level: number): Promise<void> {
  return invoke('remember_brightness', { device, level })
}

/** Mirror of `DEFAULT_FLOOR` in `src-tauri/src/runtime/dimming.rs`. */
export const DIMMING_FLOOR_DEFAULT = 20

/**
 * Makes a device's brightness follow the sound or a signal, `null` for nothing,
 * live. The hardware brightness is not touched, and nothing is remembered: that
 * is {@link rememberDimming}, same split as for the brightness itself.
 */
export function setDimming(device: DeviceRef, dimming: Dimming | null): Promise<void> {
  return invoke('set_dimming', { device, dimming })
}

/** Remembers what a device's brightness follows, `null` forgetting it. */
export function rememberDimming(device: DeviceRef, dimming: Dimming | null): Promise<void> {
  return invoke('remember_dimming', { device, dimming })
}

/**
 * Every firmware effect Candeo knows, whatever is plugged in, as their
 * definitions name them.
 */
export function firmwareEffects(): Promise<FirmwareEffectInfo[]> {
  return invoke('firmware_effects')
}

/**
 * Puts a firmware effect on a device, by the id the gallery uses.
 *
 * Run by the **firmware**: no processor time, and it outlives the application.
 * Which ids a device runs is its layout's business — `firmwareEffects` — and the
 * family that owns it turns the id into bytes.
 */
export function setEffect(device: DeviceRef, effect: string, colours: number[] = []): Promise<void> {
  return invoke('set_effect', { device, effect, colours })
}

/**
 * Pushes a complete frame.
 *
 * `frame` must hold exactly `layout.frameLen` colors — **all** the cells of the
 * matrix, including those without an LED. Sending fewer leaves the last rows
 * frozen on their previous value.
 *
 * ⚠️ **The window does not send frames.** It is the Rust loop that produces and
 * writes them; the simulator **receives** them over a channel. This wrapper and
 * {@link writeRow} are the low-level primitives that served to establish the
 * protocol, kept so it can be done again — the why is written on the commands,
 * in `src-tauri/src/lib.rs`. Calling them from a screen would compete with the
 * loop over the same HID handle.
 */
export function present(device: DeviceRef, frame: readonly Rgb[]): Promise<void> {
  const flat = new Array<number>(frame.length * 3)
  for (let i = 0; i < frame.length; i++) {
    flat[i * 3] = frame[i][0]
    flat[i * 3 + 1] = frame[i][1]
    flat[i * 3 + 2] = frame[i][2]
  }
  return invoke('present', { device, frame: flat })
}

/** Writes a row segment, without touching the rest. */
export function writeRow(
  device: DeviceRef,
  row: number,
  colStart: number,
  colors: readonly Rgb[],
): Promise<void> {
  const flat = colors.flatMap((c) => [c[0], c[1], c[2]])
  return invoke('write_row', { device, row, colStart, colors: flat })
}

// ---------------------------------------------------------------- library

/**
 * Version of the effects API this application can serve.
 *
 * Mirror of `EFFECTS_API_VERSION` in `src-tauri/src/storage.rs`, just as the
 * types of `api/types.ts` are of `lib.rs`. The divergence is not silent: a
 * manifest announcing a version more recent than Rust's is **refused at
 * install**, with a message that says so.
 *
 * It does not come from `@candeo/effects-api`: that module describes the API, it
 * does not number itself — and everything it exports must exist in its twin
 * `src-tauri/src/runtime/api.js`, which a version constant has no reason to do.
 */
export const EFFECTS_API_VERSION = 1

/**
 * What an effect declares, as the library lists it: its name is the file name,
 * the rest is what the module exports, read by Rust when it is compiled.
 *
 * `params` keeps the shape of `ParamSpec` as declared in TypeScript: the Rust
 * side does not interpret them, and typing them there would create a second
 * source of truth.
 */
export interface EffectManifest {
  name: string
  /** A string or a map of languages: see `localized` in `i18n/text.ts`. */
  description?: Text
  params?: Record<string, ParamSpec>
  apiVersion: number
  /** Declares `inputs: ['keys']`: key presses are read while it runs. */
  readsKeys?: boolean
  /** Declares `inputs: ['clock']`: it is given the wall-clock time. */
  readsClock?: boolean
  /**
   * Declares `inputs: ['signals']`: it is given every signal held. A parameter
   * bound to a signal needs no declaration, and is not what this says.
   */
  readsSignals?: boolean
  /** Declares `inputs: ['audio']`: the sound playing is captured while it runs. */
  readsAudio?: boolean
}

/**
 * Writes one of the user's effects, `user:<name>`, to `<name>.ts` in the user's
 * folder, and returns its SHA-256, to hand back to {@link cacheEffect} with the
 * JavaScript compiled from it.
 *
 * `create` says which gesture this is: creating never overwrites an effect of
 * that name, in any case, and saving again never creates one. A built-in is
 * refused.
 */
export function saveEffectSource(key: string, source: string, create: boolean): Promise<string> {
  return invoke('save_effect_source', { key, source, create })
}

/**
 * Records the JavaScript compiled from the version of the file that has `hash`.
 *
 * Rust runs the module once to read what it declares and samples its swatch.
 * A module that does not load comes back `broken`, with its error; a file that
 * changed since `hash` is refused.
 */
export function cacheEffect(key: string, hash: string, js: string): Promise<EffectEntry> {
  return invoke('cache_effect', { key, hash, js })
}

/**
 * Renames one of the user's effects to the name `to`, moves its settings, and
 * returns its new key. A running effect keeps running under it.
 */
export function renameEffect(from: string, to: string): Promise<string> {
  return invoke('rename_effect', { from, to })
}

/**
 * What the effects were called before this run's migration, and the keys they
 * became, when it migrated some. Only for renaming the editor's drafts.
 */
export function legacyEffectIds(): Promise<Record<string, string>> {
  return invoke('legacy_effect_ids')
}

/** The TypeScript source of an installed effect, to reopen it in the editor. */
export function readEffectSource(id: string): Promise<string> {
  return invoke('read_effect_source', { id })
}

/**
 * Deletes one of the user's effects: its file, its cache, and the settings kept
 * for it on every device. There is no undo, so the caller asks first.
 *
 * A built-in is refused, as renaming and saving over one are: see
 * {@link restoreBuiltin}.
 *
 * Rust stops the loops running the effect, on every device, before erasing
 * anything: a loop left running would carry on with code whose file is gone.
 */
export function deleteEffect(id: string): Promise<void> {
  return invoke('delete_effect', { id })
}

/**
 * Copies an effect into the user's folder — under its own name when that folder
 * holds none, then `<name> (2)`, `(3)`… — and returns the copy's key. The copy
 * is ready at once, and it is the user's.
 */
export function duplicateEffect(id: string): Promise<string> {
  return invoke('duplicate_effect', { id })
}

/** The names of the shipped effects their folder no longer holds, to offer them back. */
export function missingBuiltins(): Promise<string[]> {
  return invoke('missing_builtins')
}

/**
 * Writes a shipped effect's file again, by name: a missing one comes back, a
 * modified one is overwritten, and it receives updates again.
 *
 * The application does not delete, rename or save over a built-in: an edited
 * copy would stop receiving updates, a renamed or deleted one would never come
 * back. Duplicating makes an editable copy, in the user's folder.
 */
export function restoreBuiltin(name: string): Promise<void> {
  return invoke('restore_builtin', { name })
}

/** Opens the user's effects folder in the system file manager. */
export function openEffectsDir(): Promise<void> {
  return invoke('open_effects_dir')
}

/** A file of yours that drives nothing, and why: in English, for its author. */
export interface DefinitionProblem {
  file: string
  reason: string
}

/**
 * Copies a built-in device's definition into your folder, where it replaces the
 * built-in one; the file's name comes back. One already there is not overwritten.
 */
export function copyDeviceDefinition(file: string): Promise<string> {
  return invoke('copy_device_definition', { file })
}

/**
 * Chooses which definition drives a device: a file of yours, or `null` for the
 * default. An open device opens again on it.
 */
export function chooseDeviceDefinition(device: DeviceRef, file: string | null): Promise<void> {
  return invoke('choose_device_definition', { device, file })
}

/** A device definition's text, built in or yours. */
export function readDeviceDefinition(origin: 'builtIn' | 'yours', file: string): Promise<string> {
  return invoke('read_device_definition', { origin, file })
}

/**
 * Saves a file of yours; comes back with why it drives nothing, or `null` once
 * it does. The device it defines, if open, opens again on it.
 */
export function saveDeviceDefinition(file: string, text: string): Promise<string | null> {
  return invoke('save_device_definition', { file, text })
}

/** Your files that drive nothing, as last read. */
export function deviceDefinitionProblems(): Promise<DefinitionProblem[]> {
  return invoke('device_definition_problems')
}

/** Opens the folder of your device definitions, created if needed. */
export function openDevicesDir(): Promise<void> {
  return invoke('open_devices_dir')
}

/**
 * Reads the folder of your device definitions again; the files that drive
 * nothing come back with their reason. A device already open keeps the
 * definition it opened with until it opens again.
 */
export function reloadDeviceDefinitions(): Promise<DefinitionProblem[]> {
  return invoke('reload_device_definitions')
}

/**
 * Forgets the settings kept for an effect the folder no longer holds: its
 * parameters on every device, and where it was applied.
 */
export function forgetEffectSettings(id: string): Promise<void> {
  return invoke('forget_effect_settings', { id })
}

/**
 * Whether an effect can run, as far as its cache says.
 *
 * - `ready`: compiled for the file's current bytes;
 * - `stale`: never compiled, or changed since — see `refreshLibrary`;
 * - `broken`: compiled, and it does not load — `error` says why.
 */
export type EffectState = 'ready' | 'stale' | 'broken'

/**
 * A library effect: its manifest, plus what is not part of it.
 *
 * Every effect is a file; `builtin` marks one of the shipped folder, which the
 * application does not delete, rename or save over.
 */
export interface EffectEntry extends EffectManifest {
  /** The effect's key, `shipped:<name>` or `user:<name>`: see `effectKey.ts`. */
  id: string
  kind: 'builtin' | 'user'
  state: EffectState
  /** Why a `broken` effect does not load. */
  error?: string
  /** SHA-256 of the source file, for {@link cacheEffect}. */
  hash?: string
  /** A built-in whose file was edited outside the application. */
  modified: boolean
  /**
   * Color swatch, **sampled by running the effect** — never declared.
   *
   * A few `#rrggbb` colors, rendered at different moments and at different
   * places on the keyboard: a uniform effect and a spatial effect therefore
   * cannot look alike. Rust computes them at install and stores them next to
   * the manifest; they arrive with the list, without a second call.
   *
   * **The array can be empty** — an effect that throws during sampling is
   * installed anyway. The interface then shows a neutral dot.
   *
   * Its length is not guaranteed: the day the gallery wants animated
   * thumbnails, it will be the same field, with more frames.
   */
  swatch: string[]
}

/** Built-in effects and the files, in one list, in a stable order. */
export function listEffects(): Promise<EffectEntry[]> {
  return invoke('list_effects')
}

/**
 * An effect's starting values: the `default` of each declared parameter.
 *
 * Here, with the manifest that carries them, and not in the editor: the gallery
 * now starts an effect, and it has no reason to load the syntax analysis module
 * to copy four default values.
 *
 * ⚠️ **The same rule is written in Rust**, in `storage::starting_params`: the
 * notification area icon starts an effect without a window, so it cannot borrow
 * anything from here. What both must say the same way: the manifest's defaults,
 * overlaid with what was remembered (`merge`, in `useSettings`), and **limited
 * to the declared parameters**. Letting them disagree would give two different
 * lightings for the same effect depending on where it was started from.
 */
export function startingParams(declaring: Pick<EffectManifest, 'params'>): EffectParams {
  const out: EffectParams = {}
  for (const [key, spec] of Object.entries(declaring.params ?? {})) out[key] = spec.default
  return out
}

// ---------------------------------------------------------------- settings

export type EffectParams = Record<string, ParamValue>

/** An adoption decision, as `settings.json` remembers it. */
export interface DeviceRecord {
  vid: number
  pid: number
  /** Absent when the system declares none — not `null`. */
  serial?: string
  state: DeviceState
  /**
   * Brightness remembered for **this** device. Absent = {@link BRIGHTNESS_DEFAULT}.
   *
   * Here and not at the root: `set_brightness(device, level)` has taken a
   * `DeviceRef` since day one, and the protocol makes it a device command
   * separate from the current effect. Two keyboards have no reason to share a
   * level.
   */
  brightness?: number
  /** What this device's brightness follows. Absent: nothing, the slider's level. */
  dimming?: Dimming
}

/**
 * What a device's brightness follows, the sound or a signal: a factor from
 * `floor` to 100 % on its frames, under the slider's level
 * (`docs/design/inputs-and-automations.md` §2.2.2). Mirror of `Dimming` in
 * `src-tauri/src/runtime/dimming.rs`.
 */
export interface Dimming {
  /** Named as a setting's binding names it: `sound:bass`, `signal:lux`. */
  source: string
  /** The percent the brightness never goes under. */
  floor: number
}

/**
 * The effect **applied** on a device — the one driving its LEDs.
 *
 * An indexed list, not a scalar: the engine runs one effect per device, and a
 * single field could not describe that.
 *
 * **The preview never writes here.** Looking at an effect does not remember it;
 * it is "Apply" that decides.
 */
export interface ActiveEffectRecord {
  vid: number
  pid: number
  effect: string
}

/**
 * What holds for the whole application, and for no device in particular.
 *
 * A separate object: everything that depends on a keyboard lives in an indexed
 * list, what does not lives here. It is the arrangement that prevents the
 * confusion `settings.json` has just come out of — and the language, when it
 * arrives, will have nothing to arbitrate.
 */
export interface Preferences {
  /** The log level, when someone changed it. Absent = the default. */
  logLevel?: LogLevel
  /** The interface language someone chose. Absent = the system's. */
  language?: LanguageSetting
  /** A device that opens starts its applied effect again. Absent = on. */
  resumeEffects?: boolean
  /** Log files kept, one per day; `0` keeps them all. Absent = 7. */
  logFilesKept?: number
  /** Light or dark interface. Absent = the system's. */
  theme?: ThemeSetting
  /** Whether Candeo asks GitHub, once per launch, for a newer version. Absent = on. */
  checkForUpdates?: boolean
  /** Pause automations: no rule interrupts any device. Absent = off. */
  automationsPaused?: boolean
}

/** Turns resuming applied effects on or off. */
export function setResumeEffects(on: boolean): Promise<void> {
  return invoke('set_resume_effects', { on })
}

/** Whether Candeo launches at login, and whether this build can change it. */
/** Why no entry can be written: see `autostart.rs`. */
export type LoginRefused = 'development' | 'unsupported'

export interface LaunchAtLogin {
  enabled: boolean
  /**
   * False in a development build, from an MSIX package, and on a system Candeo
   * writes no entry for.
   */
  available: boolean
  /** What `available: false` is about; null when it is true. */
  refused: LoginRefused | null
}

export function getLaunchAtLogin(): Promise<LaunchAtLogin> {
  return invoke('get_launch_at_login')
}

/** Writes or removes the system's login entry, hidden in the notification area. */
export function setLaunchAtLogin(on: boolean): Promise<LaunchAtLogin> {
  return invoke('set_launch_at_login', { on })
}

/** The version running, and what may be asked about newer ones. */
export interface UpdateCheck {
  version: string
  /** False in the Microsoft Store version, which the Store updates. */
  available: boolean
  /** Whether the check runs once per launch. */
  enabled: boolean
  /** Where to ask for the latest release, built from the repository this came from. */
  latest: string
}

export function getUpdateCheck(): Promise<UpdateCheck> {
  return invoke('get_update_check')
}

/** Turns the version check on or off. */
export function setCheckForUpdates(on: boolean): Promise<void> {
  return invoke('set_check_for_updates', { on })
}

/** Opens a release's page in the browser; refused for any other address. */
export function openRelease(url: string): Promise<void> {
  return invoke('open_release', { url })
}

/** What someone chose for the interface theme. */
export type ThemeSetting = 'system' | 'light' | 'dark'

/** Saves the interface theme; the window applies it itself. */
export function setTheme(theme: ThemeSetting): Promise<void> {
  return invoke('set_theme', { theme })
}

/** What someone chose for the interface language. */
export type LanguageSetting = 'system' | 'en' | 'fr'

/** A language the interface is written in. */
export type Language = 'en' | 'fr'

/** The setting, the language it resolves to, and the system's. */
export interface LanguageStatus {
  setting: LanguageSetting
  language: Language
  system: Language
}

/** The interface language; the system's when the settings cannot be read. */
export function getLanguage(): Promise<LanguageStatus> {
  return invoke('get_language')
}

/** Changes the interface language and saves it. */
export function setLanguage(setting: LanguageSetting): Promise<LanguageStatus> {
  return invoke('set_language', { setting })
}

/**
 * The settings remembered for an effect, on a device.
 *
 * `values` carries only what **differs** from what the effect declares: a
 * parameter left at its starting value does not appear, and will therefore
 * follow the manifest if a later version of the effect changes its default.
 *
 * The key is the device / effect pair, **without a serial number**: every
 * engine command targets a `DeviceRef`, two units of the same model already
 * share their render loop, and telling them apart here would promise a
 * separation the rest of the application does not keep.
 */
export interface EffectParamsRecord {
  vid: number
  pid: number
  effect: string
  values: EffectParams
  /** Parameters reading a signal instead of their value: `{ colour: 'signal:status' }`. */
  bindings?: Bindings
}

/** Parameters bound to a live value, by parameter: `{ colour: 'signal:status' }`. */
export type Bindings = Record<string, string>

/**
 * Mirror of `Settings`, in `src-tauri/src/storage.rs`.
 *
 * A global preference in `preferences`, everything that depends on a keyboard in
 * an indexed list. Each carries only what **differs from the default**: a
 * device absent from `devices` is detected and at full brightness, and a device
 * absent from `activeEffects` has had no effect applied.
 */
export interface Settings {
  /** Shape of the file: 3 since every effect is referenced by its key, `<source>:<name>`. */
  version: number
  /**
   * The shipped effects copied into the folder: the hash of the version copied,
   * or `null` when a file of that name was already there. Kept when the file is
   * deleted or renamed, so that it is not copied again.
   */
  shippedEffects: Record<string, string | null>
  preferences: Preferences
  devices: DeviceRecord[]
  activeEffects: ActiveEffectRecord[]
  effectParams: EffectParamsRecord[]
  /**
   * Automation rules, in their order, which is their priority. Kept here even by
   * code that does not read them: writing `Settings` back without them would
   * erase them.
   */
  rules: Rule[]
}

/**
 * A rule interrupting the effect applied on a device for a while (#106). Mirror
 * of `Rule`, in `src-tauri/src/automations/resolver.rs`.
 */
export interface Rule {
  id: string
  name: string
  /** Off until someone switches it on. */
  enabled: boolean
  devices: DeviceRef[]
  when: CronTrigger | IdleTrigger | SignalTrigger
  show: RuleShow
  /** Named `for` in the file, as the sentence reads: show Clock *for* 10 seconds. */
  for: RuleDuration
}

/**
 * An occurrence starts each time the expression matches the local time: five
 * fields, `minute hour day month weekday`, or six with seconds first.
 */
export interface CronTrigger {
  kind: 'cron'
  expr: string
}

/**
 * Under way once nobody has used the computer for `minutes`, until someone does
 * (#179). The rule's `for` only applies to Try.
 */
export interface IdleTrigger {
  kind: 'idle'
  minutes: number
}

/**
 * Under way while the signal `name` equals `equals`, compared as text: a
 * sender's `1` equals `"1"` (#108). Held while it does, like idleness, and the
 * rule's `for` only applies to Try; with `hold` off, a flash for `for` from each
 * receipt of that value.
 */
export interface SignalTrigger {
  kind: 'signal'
  name: string
  equals: string
  /** Absent from a rule written by hand: it holds, as Rust reads it. */
  hold?: boolean
}

/** The effect a rule shows, with its own settings rather than the device's. */
export interface RuleShow {
  effect: string
  params: EffectParams
  bindings?: Bindings
}

export interface RuleDuration {
  seconds: number
}

/**
 * Reads `settings.json`.
 *
 * On first launch there is no file: it is the **defaults** that arrive, it is
 * not an error.
 */
export function getSettings(): Promise<Settings> {
  return invoke('get_settings')
}

/**
 * Resets `settings.json` to the default, and puts the devices down.
 *
 * What goes: the adoption decisions — everything goes back to `detected` —, each
 * device's remembered brightness, the effect applied on each, and the settings
 * remembered per device / effect pair. Rust first stops the running loops,
 * turns the backlight off and closes the devices: resetting the devices table
 * while an effect runs would leave loops that no decision designates any more.
 *
 * **No effect is touched.** Written effects live in the data folder, not in
 * `settings.json`; removing them is another action, one per effect
 * ({@link deleteEffect}). Mixing them up would lose hand-written code for
 * someone who only wanted to un-adopt a keyboard.
 *
 * The window, for its part, keeps what it had read: it is up to the caller to
 * forget the settings held in memory, otherwise the first slider move would
 * write them back.
 */
export function resetSettings(): Promise<void> {
  return invoke('reset_settings')
}

/**
 * Remembers an effect's settings for a device, without touching the rest.
 *
 * Not to be confused with {@link setEffectParams}, which adjusts the running
 * loop: this one writes to disk, and changes nothing in what is running. The two
 * have neither the same pace nor the same destination.
 *
 * A dedicated command rather than a `set_settings`: Rust reads, modifies and
 * writes back in one go. Sending the whole file back from the window would
 * overwrite in passing an adoption decided in the meantime.
 *
 * An **empty** table erases the entry: it is "restore the declared values".
 */
export function rememberEffectParams(
  device: DeviceRef,
  effect: string,
  params: EffectParams,
): Promise<void> {
  return invoke('remember_effect_params', { device, effect, params })
}

/**
 * Remembers which parameters of an effect read a signal on a device, as
 * {@link rememberEffectParams} remembers their values: on disk only, the running
 * loop is {@link setEffectBindings}. The tray, resuming and automations start the
 * effect with them.
 *
 * An **empty** table unbinds every parameter. A source that names no signal is
 * refused (`bindingInvalid`).
 */
export function rememberEffectBindings(
  device: DeviceRef,
  effect: string,
  bindings: Bindings,
): Promise<void> {
  return invoke('remember_effect_bindings', { device, effect, bindings })
}

// ---------------------------------------------------------------- engine

export interface EngineStatus {
  running: boolean
  effectId: string | null
  /** Error coming from the effect's code. Already readable: to be shown as it is. */
  error: string | null
  /**
   * Write failure towards the keyboard — unrelated to the effect's code.
   *
   * The two are distinct because they have neither the same cause nor the same
   * remedy: a flawless effect may reach no LED.
   */
  deviceError: Failure | null
  /**
   * True if the frames actually reach a keyboard.
   *
   * False with output cut, but also — and it is the tricky case — when no
   * device is connected: the simulator animates, the "send" box stays ticked,
   * and the keyboard keeps its frame. That reads as "only the first frame got
   * through".
   */
  reachingKeyboard: boolean
  toKeyboard: boolean
  /** The rule interrupting this device, when one does (#106). */
  interruption?: InterruptionStatus
}

/** A rule interrupting a device, as the engine reports it. */
export interface InterruptionStatus {
  /** The rule's id. */
  rule: string
  /** The rule's name; empty when it has none. */
  name: string
  /** The effect it shows. */
  effect: string
  /** When it ends, in epoch milliseconds; absent for a rule that never stops. */
  until?: number
}

/** Ends the interruption on a device now, and gives it its effect back. */
export function resumeDevice(device: DeviceRef): Promise<void> {
  return invoke('resume_device', { device })
}

/** Pauses automations, or turns them back on. */
export function setAutomationsPaused(paused: boolean): Promise<void> {
  return invoke('set_automations_paused', { paused })
}

/** Replaces the rules, in the order given, which is their priority. */
export function setRules(rules: Rule[]): Promise<void> {
  return invoke('set_rules', { rules })
}

/** Runs a rule once, now, for its duration, whatever its switch and the pause. */
export function tryRule(id: string): Promise<void> {
  return invoke('try_rule', { id })
}

/** Whether this system says how long the computer has been idle (#179, #183). */
export function idleAvailable(): Promise<boolean> {
  return invoke('idle_available')
}

/** A device's state, and whom it belongs to. */
export interface DeviceEngineStatus extends EngineStatus {
  device: DeviceRef
}

/**
 * What the window **looks at**, and which reaches no keyboard.
 *
 * A distinct type, in a distinct field: it is the fourth time in this project
 * that a lying state costs a diagnosis session, and a flag to filter filters
 * poorly. Here there is nothing to filter — the preview is not in the list of
 * devices, and nobody can find it there by mistake.
 *
 * It carries neither `toKeyboard`, nor `reachingKeyboard`, nor `deviceError`: a
 * preview loop has no hardware output, and those fields set to false would read
 * as a failure where there is only a choice.
 */
export interface PreviewStatus {
  /**
   * The device whose layout the preview **borrows**.
   *
   * It is neither controlled nor necessarily plugged in: it is a geometry, not a
   * destination.
   */
  layoutOf: DeviceRef
  running: boolean
  effectId: string | null
  /** Error coming from the effect's code. Already readable: to be shown as it is. */
  error: string | null
}

/**
 * Everything the engine knows, **arranged so as not to be confused**.
 *
 * `devices` describes what runs on the hardware — it is what the notification
 * area icon lists. `preview` describes what is being looked at.
 */
export interface EngineReport {
  devices: DeviceEngineStatus[]
  /** `null` when nothing is previewed — including as soon as the window is folded away. */
  preview: PreviewStatus | null
  /**
   * Whether the sound playing is captured: `idle` while no effect reads it,
   * `unavailable` when one does and it cannot be (#107). Mirror of
   * `SoundState`, in `src-tauri/src/audio/mod.rs`.
   */
  sound: 'idle' | 'capturing' | 'unavailable'
}

/**
 * Starts an installed effect **on a device**.
 *
 * The engine runs in one Rust thread per device, independent of the window:
 * closing the application does not turn the effect off. Starting here touches
 * no other device — each carries its effect and its settings.
 *
 * Targeting an unplugged device is not an error: the loop runs, the simulator
 * animates, and `reachingKeyboard` stays false until the device opens. It is
 * what makes it possible to write an effect **without owning the keyboard**.
 *
 * `bindings` are not read from the file: an effect started without them reads
 * no signal, whatever is remembered for it.
 */
export function startEffect(
  device: DeviceRef,
  id: string,
  params: EffectParams = {},
  bindings: Bindings = {},
): Promise<void> {
  return invoke('start_effect', { device, id, params, bindings })
}

export function stopEffect(device: DeviceRef): Promise<void> {
  return invoke('stop_effect', { device })
}

/**
 * Starts the preview of an effect, **without touching the keyboard or the
 * disk**.
 *
 * The exact counterpart of {@link startEffect}, minus everything that commits:
 * no hardware output, nothing written to `settings.json`, and above all **no
 * device loop stopped**. It is what makes it possible to browse the gallery
 * while an effect runs on the keyboard: without this separate loop, selecting
 * an effect would turn off the current lighting.
 *
 * `device` designates the device whose **layout is borrowed**; `null` falls
 * back on the default layout, to preview without owning a keyboard.
 *
 * There is only one preview: calling again **replaces** the previous one. Each
 * call builds a QuickJS context and destroys one, hence the debounce on the
 * caller's side — bounding it here would force a choice between making the
 * last selection wait and losing it.
 */
export function startPreview(
  device: DeviceRef | null,
  id: string,
  params: EffectParams = {},
  bindings: Bindings = {},
): Promise<void> {
  return invoke('start_preview', { device, id, params, bindings })
}

/** Stops the preview. No device effect is touched. */
export function stopPreview(): Promise<void> {
  return invoke('stop_preview')
}

/** Adjusts the preview's parameters live, without restarting its loop. */
export function setPreviewParams(params: EffectParams): Promise<void> {
  return invoke('set_preview_params', { params })
}

/** Binds the preview's parameters to signals live, as {@link setEffectBindings} does for a device. */
export function setPreviewBindings(bindings: Bindings): Promise<void> {
  return invoke('set_preview_bindings', { bindings })
}

/**
 * Opens the preview's frame stream to the simulator.
 *
 * A channel distinct from the devices' one, and it is what makes it possible to
 * look at an effect while another runs on the keyboard: both streams exist at
 * the same time, and the window chooses which one it draws.
 */
export function subscribePreviewFrames(
  onFrame: (frame: Uint8Array) => void,
): Promise<() => void> {
  const channel = new Channel<ArrayBuffer | number[]>()
  channel.onmessage = (m) => {
    onFrame(m instanceof ArrayBuffer ? new Uint8Array(m) : Uint8Array.from(m))
  }
  return invoke<void>('subscribe_preview_frames', { channel }).then(
    () => () => void invoke('unsubscribe_preview_frames'),
  )
}

/** Adjusts the parameters live, without restarting the loop. */
export function setEffectParams(device: DeviceRef, params: EffectParams): Promise<void> {
  return invoke('set_effect_params', { device, params })
}

/**
 * Binds parameters of the effect running on a device to signals, live: the loop
 * reads them from its next frame, without restarting. The whole table, as
 * {@link setEffectParams} sends every value. Remembering them is
 * {@link rememberEffectBindings}.
 */
export function setEffectBindings(device: DeviceRef, bindings: Bindings): Promise<void> {
  return invoke('set_effect_bindings', { device, bindings })
}

/**
 * Turns writing to a device on or off, **without** touching the simulator.
 *
 * It is what makes it possible to write an effect without owning the keyboard.
 * The cut is specific to the targeted device: the others keep being fed.
 */
export function setOutputToKeyboard(device: DeviceRef, on: boolean): Promise<void> {
  return invoke('set_output_to_keyboard', { device, on })
}

/**
 * Opens the frame stream of **one device** to the simulator.
 *
 * Each message is a raw frame: `frameLen × 3` bytes, in RGB order. A channel,
 * and not a global event — the scope is explicit and the binary goes through
 * without a detour through a JSON array of integers.
 *
 * The simulator follows the selected device: changing it means closing this
 * channel and subscribing elsewhere. Closing the channel stops the stream
 * **without stopping the effect**, which keeps feeding the keyboard.
 */
export function subscribeFrames(
  device: DeviceRef,
  onFrame: (frame: Uint8Array) => void,
): Promise<() => void> {
  const channel = new Channel<ArrayBuffer | number[]>()
  channel.onmessage = (m) => {
    onFrame(m instanceof ArrayBuffer ? new Uint8Array(m) : Uint8Array.from(m))
  }
  return invoke<void>('subscribe_frames', { device, channel }).then(
    () => () => void invoke('unsubscribe_frames', { device }),
  )
}

/**
 * Engine state: what runs **on the devices**, and what is **being looked at**.
 *
 * Polled rather than pushed: an error that occurred while the window was closed
 * must be readable on reopening, which a one-off event does not allow.
 *
 * `devices` carries one entry per device targeted since startup, not only per
 * open device: a device without a row is a device we know nothing about, which
 * is not the same thing as a device that does nothing.
 *
 * `preview` is separate — see {@link PreviewStatus}.
 */
export function engineStatus(): Promise<EngineReport> {
  return invoke('engine_status')
}

// ------------------------------------------------------- changes outside the window

/**
 * The state changed **without the window**.
 *
 * Written here *and* in `src-tauri/src/tray.rs`, which compares the two in a
 * test: nothing links these two strings at compile time, and letting them
 * disagree would give a window that never resynchronizes again, without an
 * error anywhere.
 */
const STATE_CHANGED = 'candeo://etat-change'

/**
 * Tells when the notification area icon has commanded something, or when the
 * window comes back after being folded away.
 *
 * # Why an event, here, when everything else is polled
 *
 * What **can be polled** has stayed so: the engine state is read again every
 * second, precisely because an error that occurred while the window was closed
 * must be readable on reopening. What cannot is what the window reads only
 * **once**, on mount — the list of devices, `settings.json` — because until now
 * it was their only source.
 *
 * It no longer is, and above all it no longer dies: closing the window folds it
 * away, so its snapshot can age for days while the menu controls the effects.
 * Polling the disk in a loop for that would mean paying every second for what
 * happens a few times per session.
 *
 * Returns the means to unsubscribe, like {@link subscribeFrames}.
 */
export function onStateChanged(handler: () => void): Promise<UnlistenFn> {
  return listen(STATE_CHANGED, () => {
    handler()
  })
}

// ---------------------------------------------------------------- signals

/**
 * The signals API as Settings shows it (#108). Mirror of `SignalsApi`, in
 * `src-tauri/src/signals/mod.rs`.
 */
export interface SignalsApi {
  enabled: boolean
  port: number
  /** Made the first time the API is turned on; empty before. */
  token: string
  /** The interfaces listened on besides loopback, by name. */
  interfaces: string[]
  /** The addresses listened on now, such as `127.0.0.1:7317`. */
  listening: string[]
  /** Something else holds the port on loopback: nothing answers there. */
  portInUse: boolean
}

// ---------------------------------------------------------------- sound

/**
 * How the sound playing is heard, for every effect and every setting following
 * it: Settings › Sound (#107). Mirror of `Tuning`, in
 * `src-tauri/src/audio/analysis.rs`.
 */
export interface SoundTuning {
  /** A factor on what is heard, 0.5 to 3. */
  gain: number
  /** 0 to 1, 0.5 by default: higher counts softer hits as beats. */
  sensitivity: number
}

export function getSoundSettings(): Promise<SoundTuning> {
  return invoke('get_sound_settings')
}

/** Sets and keeps it; Rust clamps it to what Settings offers. */
export function setSoundSettings(gain: number, sensitivity: number): Promise<SoundTuning> {
  return invoke('set_sound_settings', { gain, sensitivity })
}

/** Holds the capture on for the meter while Settings shows it, or lets it go. */
export function watchSound(on: boolean): Promise<void> {
  return invoke('watch_sound', { on })
}

/** What the meter shows: each source a setting can follow, 0..1. Mirror of `SoundNow`. */
export interface SoundNow {
  state: 'idle' | 'capturing' | 'unavailable'
  volume: number
  beat: number
  bass: number
  mids: number
  highs: number
  tone: number
}

export function soundNow(): Promise<SoundNow> {
  return invoke('sound_now')
}

export function getSignalsApi(): Promise<SignalsApi> {
  return invoke('get_signals_api')
}

/**
 * Turns the API on or off, and says where it listens besides loopback. The
 * interfaces are named, not addressed: an address changes with DHCP, another
 * Wi-Fi or a dock, and Rust follows the addresses of the names ticked.
 *
 * The token is made the first time the API is turned on. A port below 1024 is
 * refused.
 */
export function setSignalsApi(
  enabled: boolean,
  port: number,
  interfaces: string[],
): Promise<SignalsApi> {
  return invoke('set_signals_api', { enabled, port, interfaces })
}

/** A new token: every sender holding the old one is refused from now on. */
export function renewSignalsToken(): Promise<SignalsApi> {
  return invoke('renew_signals_token')
}

/**
 * An interface that is up, loopback aside, which the API can listen on. IPv4
 * addresses come first: that is the one someone copies into another machine.
 */
export interface NetworkInterface {
  name: string
  addresses: string[]
}

export function listNetworkInterfaces(): Promise<NetworkInterface[]> {
  return invoke('list_network_interfaces')
}

/** A signal's value, flat, as its sender sent it. */
export type SignalValue = string | number | boolean

/** A signal held now. Mirror of `SignalView`, in `src-tauri/src/signals/store.rs`. */
export interface HeldSignal {
  name: string
  value: SignalValue
  /** When it was last received, in epoch milliseconds. */
  received: number
  /** When it expires, in epoch milliseconds; `null` until erased. */
  expires: number | null
}

/** The signals held now, by name. */
export function listSignals(): Promise<HeldSignal[]> {
  return invoke('list_signals')
}

/**
 * Sets a value as a sender would, for the default lifetime: a rule can be tried
 * before any sender exists. An empty value erases the signal, as it does over
 * HTTP.
 */
export function sendSignal(name: string, value: string): Promise<void> {
  return invoke('send_signal', { name, value })
}

/** Erases a signal whatever its lifetime: the way out of one sent "until erased". */
export function eraseSignal(name: string): Promise<void> {
  return invoke('erase_signal', { name })
}

/**
 * Emitted by Rust when the signals held change, expiry included.
 *
 * Written here and as `CHANGED` in `src-tauri/src/signals/mod.rs`; nothing links
 * the two at compile time, so renaming one side only leaves a list that stops
 * following, without an error.
 */
const SIGNALS_CHANGED = 'candeo://signals-changed'

/** Tells when the signals held change, so a list follows without polling. */
export function onSignalsChanged(handler: () => void): Promise<UnlistenFn> {
  return listen(SIGNALS_CHANGED, () => {
    handler()
  })
}

// ---------------------------------------------------------------- log

/** Mirror of `LogLevel`, in `src-tauri/src/journal.rs`. */
export type LogLevel = 'error' | 'warn' | 'info' | 'debug' | 'trace'

/**
 * The levels the window can produce.
 *
 * Without `trace`: the per-frame logging comes from the engine, not from here,
 * and a level that cannot be written has no business being accepted as an
 * argument.
 */
export type WebviewLevel = Exclude<LogLevel, 'trace'>

/** Mirror of `JournalStatus`, in `src-tauri/src/journal.rs`. */
export interface JournalStatus {
  /**
   * The level applied. `null` when `CANDEO_LOG` carries a directive no level
   * sums up — to be shown as it is rather than inventing one.
   */
  level: LogLevel | null
  /** The level remembered in `settings.json`. `null` = the default. */
  setting: LogLevel | null
  /**
   * True if `CANDEO_LOG` imposes the level. The setting is then **written but
   * not applied**: it will hold at the next launch without the variable.
   */
  forcedByEnv: boolean
  /** The log folder, `null` if the log does not write to disk. */
  dir: string | null
  /**
   * True if the active level carries **per-frame** logging.
   *
   * ⚠️ It is what makes visible that a high level is active: left in place and
   * forgotten, it fills the disk silently.
   */
  verbose: boolean
  /** Log files kept, one per day; `0` keeps them all. */
  filesKept: number
}

/** The log's state: level applied, level remembered, folder. */
export function getJournal(): Promise<JournalStatus> {
  return invoke('get_journal')
}

/**
 * Changes the level **without restarting**, and remembers it.
 *
 * The flaw being hunted may not survive a restart — a keyboard that drops out
 * after two hours, a device that disappears intermittently: "restart in verbose
 * mode" amounts to asking to reproduce what has just been observed.
 *
 * It **survives a restart**, and that is a choice: a flaw that only occurs at
 * launch exists. Hence `verbose`, which serves to say so.
 */
export function setLogLevel(level: LogLevel): Promise<JournalStatus> {
  return invoke('set_log_level', { level })
}

/** Changes how many log files are kept, and deletes those beyond it now. */
export function setLogFilesKept(keep: number): Promise<JournalStatus> {
  return invoke('set_log_files_kept', { keep })
}

/** Opens the log folder in the system file manager. */
export function openLogDir(): Promise<void> {
  return invoke('open_log_dir')
}

/**
 * The diagnostic, ready to paste into a bug report: version, system, devices,
 * engine state.
 *
 * The serial number does not appear in it — a stable fingerprint replaces it,
 * which tells two units of the same model apart without disclosing which.
 */
export function diagnostic(): Promise<string> {
  return invoke('diagnostic')
}

/**
 * Records an entry coming from the window in Rust's log.
 *
 * To be called only by the facade in `api/journal.ts`: it alone knows that a
 * logging failure must be swallowed.
 */
export function logFromWebview(
  level: WebviewLevel,
  source: string,
  message: string,
): Promise<void> {
  return invoke('log_from_webview', { level, source, message })
}
