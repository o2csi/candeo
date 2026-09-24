/**
 * What `settings.json` remembers, and how the window writes it.
 *
 * Three kinds of decisions live there, and they have neither the same shape nor
 * the same path to disk:
 *
 * - an **effect's settings**, per device / effect pair;
 * - a device's **brightness**, reapplied on every plug-in;
 * - the **applied effect** on a device, written by Rust at startup.
 *
 * The module used to carry the name of the first one only — `useEffectParams` —
 * and it was the same flaw of shape as the three single-device fields removed
 * from `settings.json`: a name that describes one part and a content that
 * covers three. A **single** read of the file feeds all three, which is also the
 * reason not to split them into three composables.
 *
 * ## An effect's settings: three destinations for one gesture
 *
 * Moving a slider writes to three places, and they have neither the same pace
 * nor the same lifetime:
 *
 * 1. **The window's memory**, immediate: it is what the form displays, and
 *    switching effects then coming back reads it again;
 * 2. **the render loop**, live, a few dozen times per second at most
 *    (`set_effect_params`, `set_preview_params`);
 * 3. **`settings.json`**, when the slider stops (`remember_effect_params`).
 *
 * ## Why disk and not the session alone
 *
 * This form serves the person who will never write an effect. They set "the
 * wave, but slower" **once**; making them set it again at every launch would
 * amount to shipping a setting that cannot be kept, that is, a demo. The person
 * who writes code iterates and has nothing to remember — it is the other
 * audience that pays for a session-only memory, and it is precisely the one
 * issue #28 targets.
 *
 * Adopting a device is already persistent for the same reason: a decision made
 * once is not asked again.
 *
 * ## What is remembered, and what is not
 *
 * **Only what differs from what the effect declares.** A parameter left at its
 * starting value is not written, and will therefore follow the manifest if a
 * later version of the effect changes its default. Same economy as the adoption
 * decisions, which only write what departs from the default.
 *
 * ## Bindings: the same pair, another table
 *
 * A parameter can read a signal instead of its value
 * (`docs/design/inputs-and-automations.md` §2.3.1). Its value stays, and is what
 * the effect shows while the signal is absent. Bindings go to the same three
 * places, but a binding changes on a click, not a drag: it is sent and written
 * at once, with neither pace nor debounce (`set_effect_bindings`,
 * `set_preview_bindings`, `remember_effect_bindings`).
 *
 * Module-level state, like `useDevice` and `useEffects`: moving to the editor
 * destroys the view, and settings in flight must not go with it.
 */

import { computed, readonly, ref } from 'vue'
import type { ParamSpec, ParamValue, Rgb } from '@candeo/effects-api'

import * as api from '../api/candeo'
import type { Bindings, EffectParams } from '../api/candeo'
import { message } from '../api/journal'
import type { DeviceRef } from '../api/types'
import { declaredBindings, rebound } from './bindings'

/**
 * Maximum pace of live sends, in milliseconds.
 *
 * A mouse drag produces dozens of events per second, and the `pointermove` of a
 * 144 Hz screen far more. The loop, for its part, reads the parameters again
 * **on every frame** — 30 times per second. Sending faster than it reads means
 * replacing a JSON nobody has looked at yet.
 *
 * 40 ms, that is 25 sends per second at most: always under the render pace,
 * therefore invisible to the eye, and an order of magnitude under what a slider
 * produces.
 *
 * ⚠️ **The margin tightened with the pace.** Against the 16.7 ms of a loop at
 * 60, these 40 ms left a factor of two; against the 33.3 ms of a loop at 30,
 * only 7 ms remain. The invariant holds — we always write less often than the
 * loop reads — but it holds narrowly: **lowering this value under 34 ms would
 * break it**, and we would go back to replacing unread JSON. It is the floor,
 * not a comfort setting.
 */
const HOT_PERIOD = 40

/**
 * Slider rest before the disk write, in milliseconds.
 *
 * It is a safety net, not the nominal path: the write normally goes out at the
 * **end of the gesture** — `change` on a slider, that is, on release — and this
 * rest only serves the cases where that event does not arrive.
 */
const DISK_DELAY = 600

/**
 * Minimum gap between two writes of the same pair, in milliseconds.
 *
 * The end of the gesture is not always rare: a keyboard arrow held down on a
 * slider emits `change` **on every repeat**, about thirty per second. Writing
 * unconditionally would therefore make thirty disk writes per second, exactly
 * what the debounce avoided.
 *
 * Past this delay, the debounce takes over again and writes the last state on
 * release — nothing is lost, it is shifted.
 */
const DISK_PERIOD = 250

/** What differs from the manifest, per device and per effect. Key `vid:pid/effect`. */
const remembered = ref<Record<string, EffectParams>>({})

/**
 * Which parameters read a signal, per device and per effect, same key. An empty
 * table is kept once someone unbinds, rather than removed: a read landing
 * before the write must find it (see {@link read}).
 */
const bound = ref<Record<string, Bindings>>({})

/**
 * The effect **applied** on each device, as the file remembers it.
 * Key `vid:pid`.
 *
 * Read back, never written from here: it is Rust that remembers it, at the
 * moment the effect really starts. The window would have no honest way to do
 * it — the notification area icon starts effects without it.
 *
 * What depends on it: saying that an idle device **remembers** its last effect,
 * rather than showing "no effect" and suggesting that nothing was remembered.
 */
const applied = ref<Record<string, string>>({})

/** The brightness remembered per device, key `vid:pid`. Absent = the default. */
const brightness = ref<Record<string, number>>({})

/** What prevented reading, adjusting or remembering. Already readable. */
const error = ref<string | null>(null)

/** The read **in flight**, shared: two screens mounting together read only once. */
let reading: Promise<void> | null = null

/** True as soon as a read has succeeded. It, and it alone, avoids reading again on every mount. */
let loaded = false

/**
 * Number of the last read started.
 *
 * `reload` can start while a read is already in flight, and nothing guarantees
 * that both come back in the order they left. Without this count, the older one
 * could land last and reinstall exactly the state we had just set out to
 * replace.
 */
let generation = 0

const deviceKey = (d: DeviceRef) => `${d.vid}:${d.pid}`
const key = (d: DeviceRef, effect: string) => `${deviceKey(d)}/${effect}`

/** Errors raised by Rust are already readable: they are shown as they are. */
// ---------------------------------------------------------------- values

/** True if this value is a color, in the sense of `ParamSpec`. */
function isRgb(v: unknown): v is Rgb {
  if (typeof v !== 'object' || v === null) return false
  const c = v as Record<string, unknown>
  return typeof c.r === 'number' && typeof c.g === 'number' && typeof c.b === 'number'
}

/**
 * True if the value read back matches what the effect declares **today**.
 *
 * `settings.json` is a file, so it gets edited by hand, and a re-saved effect
 * may have changed the kind of a parameter. A value that no longer matches falls
 * back to the default, rather than making a slider produce a `NaN` or a list an
 * option that does not exist.
 */
function fits(spec: ParamSpec, v: ParamValue): boolean {
  switch (spec.kind) {
    case 'number':
      return typeof v === 'number' && Number.isFinite(v)
    case 'color':
      return isRgb(v)
    case 'boolean':
      return typeof v === 'boolean'
    case 'choice':
      return typeof v === 'string' && spec.options.some((o) => (typeof o === 'string' ? o : o.value) === v)
    case 'text':
      return typeof v === 'string' && Array.from(v).length <= textLimit(spec)
  }
}

/**
 * How many characters a `text` parameter takes: its `maxLength`, 64 unless it
 * says, and never more than 256, a signal's longest value. `textLimit` in the
 * bootstrap cuts a bound value at the same length.
 */
export function textLimit(spec: { maxLength?: number }): number {
  const declared = Number.isInteger(spec.maxLength) && spec.maxLength! > 0 ? spec.maxLength! : 64
  return Math.min(declared, 256)
}

/** Equality of parameter values, colors included. */
export function sameValue(a: ParamValue, b: ParamValue): boolean {
  if (isRgb(a) && isRgb(b)) return a.r === b.r && a.g === b.g && a.b === b.b
  return a === b
}

/**
 * An effect's complete values: its manifest, overlaid with what was
 * remembered.
 *
 * Limited to the **declared** parameters: a setting remembered for a parameter
 * the effect no longer has disappears on its own, instead of travelling
 * indefinitely to a loop that no longer reads it.
 */
function merge(specs: Record<string, ParamSpec>, kept: EffectParams): EffectParams {
  const out: EffectParams = {}
  for (const [id, spec] of Object.entries(specs)) {
    const v = kept[id]
    out[id] = v !== undefined && fits(spec, v) ? v : spec.default
  }
  return out
}

/** What departs from the manifest, and nothing else — it is what goes to disk. */
function apart(specs: Record<string, ParamSpec>, values: EffectParams): EffectParams {
  const out: EffectParams = {}
  for (const [id, spec] of Object.entries(specs)) {
    const v = values[id]
    if (v !== undefined && !sameValue(v, spec.default)) out[id] = v
  }
  return out
}

// ------------------------------------------------------------ live sends

/**
 * What remains to send to **one** render loop.
 *
 * A single send in flight at a time, and never two less than {@link HOT_PERIOD}
 * apart: intermediate moves are **overwritten**, not stacked. It is the right
 * way to lose them — the loop only reads the last state, a queue would only
 * deliver it late.
 *
 * `pending` guarantees that no final value is lost: the last requested state
 * always goes out again, once the current send has come back.
 */
interface Sender {
  /** Last requested state, not yet sent. `null` if everything is up to date. */
  pending: EffectParams | null
  /** A send is in flight: on its return, we go again if `pending` moved. */
  inFlight: boolean
  /** Time of the last departure, to keep the pace. */
  last: number
  /** Pace wait timer, `0` if there is none. */
  timer: number
  /** Where these values go. See {@link hot}. */
  send: (params: EffectParams) => Promise<void>
}

/**
 * One sender per loop: two keyboards adjusted one after the other do not get in
 * each other's way, and the preview has its own.
 *
 * The preview's key cannot collide with a device's: `vid:pid` is made of two
 * numbers.
 */
const senders = new Map<string, Sender>()

/** The key of the preview's sender, which is no device's loop. */
const PREVIEW = 'preview'

/**
 * Pushes values to a loop, at a bounded pace.
 *
 * `send` is provided by the caller rather than derived from a `DeviceRef`: the
 * preview has no device, and inventing a fake device identifier for it would
 * have put back into this module the confusion the engine has just taken out of
 * it.
 */
function hot(k: string, send: Sender['send'], params: EffectParams): void {
  let s = senders.get(k)
  if (!s) {
    s = { pending: null, inFlight: false, last: 0, timer: 0, send }
    senders.set(k, s)
  }
  // The preview changes loop on every selection: the send must follow the
  // latest, not the one that lived when the sender was created.
  s.send = send
  s.pending = params
  pump(s)
}

function pump(s: Sender): void {
  // Nothing to send, or someone already handles it: the return of the current
  // send, or the timer expiring, will call this function again.
  if (s.pending === null || s.inFlight || s.timer !== 0) return

  const wait = HOT_PERIOD - (Date.now() - s.last)
  if (wait > 0) {
    s.timer = window.setTimeout(() => {
      s.timer = 0
      pump(s)
    }, wait)
    return
  }

  const params = s.pending
  s.pending = null
  s.inFlight = true
  s.last = Date.now()

  void s
    .send(params)
    .catch((e: unknown) => {
      error.value = message(e)
    })
    .finally(() => {
      s.inFlight = false
      pump(s)
    })
}

// ------------------------------------------------------------ disk writes

/** A deferred write, and the means to trigger it right away. */
interface Write {
  timer: number
  run: () => void
}

const writes = new Map<string, Write>()

/** Time of the last write sent, per pair. See {@link DISK_PERIOD}. */
const written = new Map<string, number>()

/**
 * Writes **sent to Rust and not yet confirmed**, per pair.
 *
 * It is the second piece of what {@link read} must protect, and it cannot be
 * derived from {@link writes}: `persist` removes the entry from the pending
 * writes table **before** the round trip, otherwise a `flushAll` or a
 * `settleOne` would send it a second time. Between that removal and Rust's
 * return, the pair therefore appears nowhere — and a read landing in that window
 * would return the value from before the write, that is, undoing before the
 * user's eyes the setting they just made.
 *
 * Unobservable as long as nothing called {@link reload}. The notification area
 * icon is the first to call it, and it does so precisely when something has just
 * moved — so at the worst moment.
 *
 * A **count** and not a flag: two writes of the same pair can overlap — the
 * debounce fires, a `settle` triggers another right away — and a flag lowered by
 * the first would reopen the window while the second is still in flight.
 */
const inflight = new Map<string, number>()

/**
 * Binding writes sent to Rust and not yet confirmed, per pair: the same
 * protection as {@link inflight}, for the other table. They leave at once, so
 * there is no pending table to go with it.
 */
const bindingsInflight = new Map<string, number>()

/** Records that a write is leaving, and the means to know when it has come back. */
function takeOff(k: string, table = inflight): void {
  table.set(k, (table.get(k) ?? 0) + 1)
}

function landed(k: string, table = inflight): void {
  const remaining = (table.get(k) ?? 1) - 1
  if (remaining > 0) table.set(k, remaining)
  else table.delete(k)
}

/**
 * Writes at the latest after {@link DISK_DELAY} without movement.
 *
 * Each new value replaces the previous one: a two-second drag produces only one
 * write, that of the value where it stops. The rest is not a comfort
 * optimisation — `settings.json` is written through a temporary file then a
 * rename, it is a complete disk operation.
 */
function persist(device: DeviceRef, effect: string, values: EffectParams): void {
  const k = key(device, effect)
  const previous = writes.get(k)
  if (previous) window.clearTimeout(previous.timer)

  const run = () => {
    // Removed first, so that a `flushAll` or a `settleOne` does not send it
    // again; counted as in flight right after, so that it does not disappear
    // from what {@link read} protects in between. See {@link inflight}.
    writes.delete(k)
    written.set(k, Date.now())
    takeOff(k)
    api
      .rememberEffectParams(device, effect, values)
      .catch((e: unknown) => {
        error.value = message(e)
      })
      .finally(() => {
        landed(k)
      })
  }
  writes.set(k, { timer: window.setTimeout(run, DISK_DELAY), run })
}

/**
 * Writes which parameters of an effect read a signal on a device, at once: a
 * binding changes on a click or a name typed, never thirty times a second.
 * Counted in flight until Rust answers, for {@link read}.
 */
function rememberBindings(device: DeviceRef, effect: string, bindings: Bindings): void {
  const k = key(device, effect)
  takeOff(k, bindingsInflight)
  api
    .rememberEffectBindings(device, effect, bindings)
    .catch((e: unknown) => {
      error.value = message(e)
    })
    .finally(() => {
      landed(k, bindingsInflight)
    })
}

/**
 * Triggers the pending write for this pair, if it can go.
 *
 * Too soon after the previous one, nothing happens: the debounce armed by
 * `persist` is still there and will write the last state. Nothing is lost, the
 * order is only shifted — see {@link DISK_PERIOD}.
 *
 * `now` lifts this gap, for gestures that do not repeat: a click on "Restore"
 * has no reason to wait because a slider has just been released.
 */
function settleOne(device: DeviceRef, effect: string, now = false): void {
  const k = key(device, effect)
  const w = writes.get(k)
  if (!w) return
  if (!now && Date.now() - (written.get(k) ?? 0) < DISK_PERIOD) return

  window.clearTimeout(w.timer)
  w.run()
}

/**
 * Triggers every pending write without waiting for the rest.
 *
 * The iteration runs over a snapshot: `run` removes itself from the table.
 */
function flushAll(): void {
  for (const w of [...writes.values()]) {
    window.clearTimeout(w.timer)
    w.run()
  }
}

/**
 * Cancels the pending writes whose key passes the filter, **without running
 * them**.
 *
 * The exact opposite of {@link flushAll}, and the only correct move when Rust
 * has just removed these entries from `settings.json`: a debounce firing
 * afterwards would write them back, resurrecting precisely what had just been
 * erased.
 *
 * Does not recall what has already left — nothing can, the call is in flight. A
 * write that lands just after a reset therefore writes its pair again; the
 * window, for its part, no longer remembers it ({@link inflight} only protects
 * what `remembered` still holds), and the next launch starts again from the
 * file.
 */
function cancelWrites(keep: (key: string) => boolean): void {
  for (const [k, w] of [...writes.entries()]) {
    if (keep(k)) continue
    window.clearTimeout(w.timer)
    writes.delete(k)
    written.delete(k)
  }
}

/**
 * Last safety net: closing the window destroys the web view **without going
 * through Vue's hooks**.
 *
 * `onBeforeUnmount` only covers a change of screen; yet the window is closed
 * while an effect runs, it is even how the app is meant to be used. A 600 ms
 * timer would not survive it.
 *
 * It is only a safety net, and deliberately so: nothing guarantees that a round
 * trip to Rust completes while the web view shuts down. The safe path is
 * elsewhere — the write goes out **at the end of the gesture**, on slider
 * release, so well before anyone gets near the close button.
 */
window.addEventListener('pagehide', flushAll)

// ------------------------------------------------------------------- reading

/**
 * Reads `settings.json` and replaces what is remembered.
 *
 * A failure does not mark the read as done: it leaves it to be retried, so that
 * a second screen does not merely inherit a permanent refusal.
 */
function read(): Promise<void> {
  const mine = ++generation

  const run = api
    .getSettings()
    .then((s) => {
      // A more recent read got ahead: ours is stale, and applying it would undo
      // what that one has just installed.
      if (mine !== generation) return

      const onDisk: Record<string, EffectParams> = Object.fromEntries(
        s.effectParams.map((r) => [key({ vid: r.vid, pid: r.pid }, r.effect), r.values]),
      )
      const boundOnDisk: Record<string, Bindings> = Object.fromEntries(
        s.effectParams
          .filter((r) => r.bindings !== undefined)
          .map((r) => [key({ vid: r.vid, pid: r.pid }, r.effect), r.bindings as Bindings]),
      )

      // The two other tables are taken as they are: the window does not write
      // them — Rust remembers the applied effect at startup, and the
      // brightness goes out through its own command —, so there is nothing to
      // protect from a write in flight as for the settings below.
      applied.value = Object.fromEntries(
        s.activeEffects.map((r) => [deviceKey({ vid: r.vid, pid: r.pid }), r.effect]),
      )
      brightness.value = Object.fromEntries(
        s.devices
          .filter((r) => r.brightness !== undefined)
          .map((r) => [deviceKey({ vid: r.vid, pid: r.pid }), r.brightness as number]),
      )

      // What is still waiting for the disk is more recent than the disk: the
      // write only goes out when the slider rests, and `reload` does not choose
      // its moment. Taking the file as it is would therefore move a slider back
      // under the hand of the person holding it.
      //
      // **Both tables, and it is the fix #46 required**: what is waiting
      // ({@link writes}) and what has left without being confirmed
      // ({@link inflight}). `persist` removes the entry from the first before
      // the round trip; without the second, a read landing in that window would
      // return the value from before the write. Unobservable as long as nothing
      // called `reload` — the notification area icon calls it, and precisely
      // when something has just moved.
      for (const k of [...writes.keys(), ...inflight.keys()]) {
        const ours = remembered.value[k]
        if (ours !== undefined) onDisk[k] = ours
      }
      // Same for a binding whose write has not come back: the file would unbind
      // what was just bound, or the other way round.
      for (const k of bindingsInflight.keys()) {
        const ours = bound.value[k]
        if (ours !== undefined) boundOnDisk[k] = ours
      }

      remembered.value = onDisk
      bound.value = boundOnDisk
      loaded = true
    })
    .catch((e: unknown) => {
      if (mine !== generation) return
      error.value = message(e)
    })
    .finally(() => {
      // No unconditional clearing: a more recent read may have taken the place,
      // and removing it would make it invisible to whoever calls `load`.
      if (reading === run) reading = null
    })

  reading = run
  return run
}

export function useSettings() {
  /**
   * Makes sure `settings.json` has been read — once per session, not once per
   * mount.
   *
   * Coming back to this screen does not read again, and that is intended: the
   * disk does not change merely because we navigate, and it is the window itself
   * that writes it, so it already knows more than the disk. The day this is no
   * longer true, it is {@link reload} that must be called — not this economy
   * that must be removed.
   */
  function load(): Promise<void> {
    if (loaded) return Promise.resolve()
    return reading ?? read()
  }

  /**
   * Reads `settings.json` again, memoisation included.
   *
   * For what moves **outside the window**: the notification area icon controls
   * effects without it, and the window now outlives it folded away — its
   * snapshot can therefore age for days. Without this entry point, `load` would
   * never read again after a first success.
   *
   * Does not join a read already in flight: that one may have left **before**
   * the write we have just learned about, and would then return the very
   * content we are trying to replace. What the window has not finished writing
   * is preserved by {@link read} — see {@link inflight}, which is the half of
   * this protection that calling `reload` made necessary.
   */
  function reload(): Promise<void> {
    loaded = false
    return read()
  }

  /**
   * The values this effect runs — or would run — with on this device.
   *
   * Without a device, what the effect declares: something has to be shown, and
   * those values hold for any device.
   */
  function valuesFor(
    device: DeviceRef | null,
    effect: string,
    specs: Record<string, ParamSpec>,
  ): EffectParams {
    const kept = device ? remembered.value[key(device, effect)] : undefined
    return merge(specs, kept ?? {})
  }

  /**
   * The parameters of this effect that read a signal on this device. Without a
   * device, none: nothing is kept for no device.
   */
  function bindingsFor(
    device: DeviceRef | null,
    effect: string,
    specs: Record<string, ParamSpec>,
  ): Bindings {
    const kept = device ? bound.value[key(device, effect)] : undefined
    return declaredBindings(specs, kept ?? {})
  }

  /**
   * True if something is **remembered** for this pair: a value, or a binding.
   *
   * Used to say so on screen. It was the real flaw of the persistence shipped
   * by issue #28: the settings held, and nothing hinted at it — you adjust, you
   * close, and you have no reason to believe it survived.
   */
  function keptFor(device: DeviceRef | null, effect: string): boolean {
    if (!device) return false
    const k = key(device, effect)
    return (
      Object.keys(remembered.value[k] ?? {}).length > 0 ||
      Object.keys(bound.value[k] ?? {}).length > 0
    )
  }

  /**
   * Changes **one** setting: memory, the device's loop, disk.
   *
   * The whole set is sent to the loop, not only the modified field: `set_params`
   * replaces the parameters JSON, it does not merge it.
   *
   * Returns the complete values, so that the caller can also push them to the
   * preview ({@link adjustPreview}). Sending them here unconditionally would
   * adjust a preview that may not be showing this effect — only the caller
   * knows what it is looking at.
   */
  function adjust(
    device: DeviceRef,
    effect: string,
    specs: Record<string, ParamSpec>,
    id: string,
    value: ParamValue,
    applied: boolean,
  ): EffectParams {
    // Like `useEffects.apply`: the previous failure is cleared on the next
    // attempt. Without this a passing incident would leave a red banner until
    // closing, long after everything is back in order.
    error.value = null

    const complete = { ...valuesFor(device, effect, specs), [id]: value }
    const kept = apart(specs, complete)

    // Replacement rather than mutation, as in `useEffects`: reactivity no
    // longer depends on the key being present.
    remembered.value = { ...remembered.value, [key(device, effect)]: kept }
    // ⚠️ To the device **only if it is this effect that runs there**.
    //
    // A device's loop has only one effect, and it reads a parameters JSON
    // without knowing which effect it comes from. Pushing the values of the
    // effect being adjusted to a loop that runs another one makes it apply
    // settings that are not its own: a parameter name that coincides — `color` —
    // changes the lighting before the user's eyes, and a shape that does not
    // fit makes rendering fail until it stops after
    // [`MAX_CONSECUTIVE_ERRORS`] frames.
    //
    // Adjusting an effect being previewed must therefore send nothing to the
    // keyboard — that is the whole point of the preview. It is still
    // remembered: the setting belongs to the device/effect pair, it will apply
    // at the next start.
    if (applied) hot(deviceKey(device), (p) => api.setEffectParams(device, p), complete)
    persist(device, effect, kept)
    return complete
  }

  /**
   * Pushes values to the **preview** loop, at the same pace.
   *
   * It is what makes a setting act live on what is being looked at, without
   * having applied it: the preview loop reads its JSON again on every frame,
   * like a device's. Nothing is written to disk here — it is {@link adjust}
   * that remembers, and it remembers for the device / effect pair, not for the
   * preview, which belongs to no device.
   */
  function adjustPreview(values: EffectParams): void {
    hot(PREVIEW, api.setPreviewParams, values)
  }

  /**
   * Binds **one** parameter to a signal, or gives it its value back when
   * `source` is `null`: memory, the device's loop, disk.
   *
   * To the device only when this effect runs there, for the reason
   * {@link adjust} gives: another effect's loop would read a binding that is not
   * its own. Always remembered for the pair.
   *
   * Returns every binding of the effect, for the caller to give the preview
   * ({@link bindPreview}), as {@link adjust} returns the values.
   */
  function bind(
    device: DeviceRef,
    effect: string,
    specs: Record<string, ParamSpec>,
    id: string,
    source: string | null,
    applied: boolean,
  ): Bindings {
    error.value = null
    const next = declaredBindings(specs, rebound(bindingsFor(device, effect, specs), id, source))
    bound.value = { ...bound.value, [key(device, effect)]: next }
    if (applied) {
      void api.setEffectBindings(device, next).catch((e: unknown) => {
        error.value = message(e)
      })
    }
    rememberBindings(device, effect, next)
    return next
  }

  /** Gives the **preview** loop its bindings, live. Nothing goes to disk: see {@link adjustPreview}. */
  function bindPreview(bindings: Bindings): void {
    void api.setPreviewBindings(bindings).catch((e: unknown) => {
      error.value = message(e)
    })
  }

  /** The brightness remembered for this device, or the default. */
  function brightnessOf(device: DeviceRef | null): number {
    if (!device) return api.BRIGHTNESS_DEFAULT
    return brightness.value[deviceKey(device)] ?? api.BRIGHTNESS_DEFAULT
  }

  /**
   * Changes a device's brightness: memory, keyboard, and disk if asked.
   *
   * Two commands and not one, as for effect settings: `setBrightness` writes to
   * the keyboard on every move, `rememberBrightness` only writes to disk at the
   * end of the gesture. `commit` says which of the two is being done — a drag
   * produces dozens of `setBrightness` and a single disk write.
   */
  function setBrightness(device: DeviceRef, level: number, commit: boolean): void {
    error.value = null
    const k = deviceKey(device)
    // The default is not remembered: the absence of an entry **is** the
    // default, here as in the file.
    const next = { ...brightness.value }
    if (level === api.BRIGHTNESS_DEFAULT) delete next[k]
    else next[k] = level
    brightness.value = next

    const fail = (e: unknown) => {
      error.value = message(e)
    }
    // The device may be closed — unplugged, ignored: the HID write then fails,
    // and that is no reason not to remember the level. It will be reapplied on
    // the next plug-in, which is the whole point of remembering it.
    void api.setBrightness(device, level).catch(fail)
    if (commit) void api.rememberBrightness(device, level).catch(fail)
  }

  /**
   * The effect `settings.json` remembers as applied on this device.
   *
   * It is **not** what is running — that is `engine_status`. It is what was
   * applied last time, and what lets an idle device say it remembers it rather
   * than showing "no effect".
   */
  function lastAppliedOn(device: DeviceRef | null): string | null {
    return device ? (applied.value[deviceKey(device)] ?? null) : null
  }

  /**
   * The gesture is over — slider released, box ticked, option chosen: write
   * now.
   *
   * It is **the** nominal path to disk. Without it, the only guarantee would be
   * a 600 ms timer, which closing the window would take away — yet the window
   * is closed while an effect runs, it is how the app is meant to be used.
   */
  function settle(device: DeviceRef, effect: string): void {
    settleOne(device, effect)
  }

  /**
   * Restores what the effect declares, and **forgets** — the entry disappears
   * from `settings.json` instead of keeping a copy of the defaults there.
   *
   * The effect declares values and no binding, so every parameter reading a
   * signal goes back to its value too: restoring half of the settings would
   * leave a bound colour looking like the default one while it follows a signal.
   *
   * Writes without waiting: it is a click, not a drag, there is nothing to
   * group.
   *
   * Returns the declared values, for the same reason {@link adjust} returns
   * its own: the preview must come back with them, and only the caller knows
   * what it is looking at. Its bindings are none: {@link bindPreview} with `{}`.
   */
  function forget(
    device: DeviceRef,
    effect: string,
    specs: Record<string, ParamSpec>,
    applied: boolean,
  ): EffectParams {
    error.value = null
    const declared = merge(specs, {})
    const k = key(device, effect)
    remembered.value = { ...remembered.value, [k]: {} }
    bound.value = { ...bound.value, [k]: {} }
    // Same condition as {@link adjust}, and for the same reason: restoring the
    // values of an effect being previewed has no reason to touch the keyboard,
    // which may be running something else.
    if (applied) {
      hot(deviceKey(device), (p) => api.setEffectParams(device, p), declared)
      void api.setEffectBindings(device, {}).catch((e: unknown) => {
        error.value = message(e)
      })
    }
    persist(device, effect, {})
    settleOne(device, effect, true)
    rememberBindings(device, effect, {})
    return declared
  }

  /**
   * Forgets what was remembered for an effect, on **every** device.
   *
   * The in-memory counterpart of what `delete_effect` does in `settings.json`.
   * Without it, the window would keep settings pointing to an identifier that
   * nothing names any more, and an effect re-saved under the same name **in the
   * same session** would inherit them — exactly what the purge on the Rust side
   * avoids from one launch to the next.
   *
   * Nothing is sent to Rust: it has already forgotten. What is in flight is
   * cancelled, not triggered.
   */
  function dropEffect(effect: string): void {
    // An effect name cannot contain `/`, which Windows forbids in file names: the
    // suffix cannot designate the wrong pair.
    const suffix = `/${effect}`
    const others = (k: string) => !k.endsWith(suffix)
    cancelWrites(others)
    remembered.value = Object.fromEntries(
      Object.entries(remembered.value).filter(([k]) => others(k)),
    )
    bound.value = Object.fromEntries(Object.entries(bound.value).filter(([k]) => others(k)))
    // The counterpart of what `Settings::forget_effect` has just done on disk:
    // the identifier no longer designates anything, and leaving it here would
    // make the devices column announce a "remembered" effect that the library
    // no longer knows.
    applied.value = Object.fromEntries(
      Object.entries(applied.value).filter(([, id]) => id !== effect),
    )
  }

  /**
   * Forgets **everything**, as the configuration reset has just done on disk.
   *
   * Without this the first slider move would write back the settings that had
   * just been erased: the window holds them in memory, and only sends Rust
   * what differs from the manifest — that is, what it believes it knows.
   */
  function dropAll(): void {
    cancelWrites(() => false)
    remembered.value = {}
    bound.value = {}
    // The two other tables described a file that has just been reset: keeping
    // them would make the screen say that an effect is still applied and a
    // brightness still remembered, while Rust has turned everything off and
    // closed everything.
    applied.value = {}
    brightness.value = {}
  }

  /**
   * Every effect the settings refer to, applied or tuned on any device.
   *
   * What the gallery compares with the library to tell someone that an effect
   * they tuned is no longer in the folder.
   */
  const referencedEffects = computed(() => {
    const names = new Set(Object.values(applied.value))
    for (const key of Object.keys(remembered.value)) names.add(key.slice(key.indexOf('/') + 1))
    for (const [key, bindings] of Object.entries(bound.value)) {
      if (Object.keys(bindings).length > 0) names.add(key.slice(key.indexOf('/') + 1))
    }
    return names
  })

  /** Closes the message: what failed is read, and the screen goes back to work. */
  function dismissError(): void {
    error.value = null
  }

  return {
    load,
    reload,
    referencedEffects,
    valuesFor,
    bindingsFor,
    keptFor,
    adjust,
    adjustPreview,
    bind,
    bindPreview,
    settle,
    forget,
    dropEffect,
    dropAll,
    lastAppliedOn,
    brightnessOf,
    setBrightness,
    flush: flushAll,
    error: readonly(error),
    dismissError,
  }
}
