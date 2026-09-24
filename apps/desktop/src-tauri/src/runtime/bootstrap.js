// Glue between the Rust host and the user's module.
//
// Evaluated once when an effect starts. It imports the user's module under the
// name `effect`, builds the render context, and installs
// `globalThis.__candeo_render`, which the Rust loop calls on every frame.
//
// The host sets two globals beforehand: `__candeo_frame_len` and
// `__candeo_layout` (JSON). Passing them this way avoids building the context
// on every frame.

// A namespace import, not `import effect from 'effect'`: the latter fails at
// module **linking** when the default export is missing, with a QuickJS message
// nobody can relate to their code. Here the import always succeeds, and we are
// the ones saying what is wrong.
import * as module from 'effect'

const FRAME_LEN = globalThis.__candeo_frame_len
const layout = JSON.parse(globalThis.__candeo_layout)

const effect = module.default

if (typeof effect?.render !== 'function') {
  throw new TypeError(
    'the effect must have a default export with a `render` function, ' +
      'for example: export default defineEffect({ render(ctx) { … } })',
  )
}

// The manifest as the module declares it, set once at load. The library reads it
// when it compiles an effect, and keeps it in the cache, so that listing runs no
// engine.
globalThis.__candeo_manifest = JSON.stringify({
  name: effect.name ?? '',
  // The version of the effects API the module says it was written against;
  // `null` when it says nothing, which the library reads as the first one.
  apiVersion: effect.apiVersion ?? null,
  description: effect.description ?? '',
  params: effect.params ?? {},
  inputs: Array.isArray(effect.inputs) ? effect.inputs : [],
})

// Read by the render loop once loaded: only an effect that declares keys gets
// presses, and makes the engine read them (`docs/design/key-input.md` §3).
globalThis.__candeo_reads_keys = Array.isArray(effect.inputs) && effect.inputs.includes('keys')

// Read once: an effect that does not declare the clock is given a zeroed one,
// and no `Date` is built for it (`docs/design/inputs-and-automations.md` §2.1).
const READS_CLOCK = Array.isArray(effect.inputs) && effect.inputs.includes('clock')

// Read by the render loop too: only an effect that declares signals is handed
// every value held (§2.3.1). Values bound to its parameters need no declaration.
const READS_SIGNALS = Array.isArray(effect.inputs) && effect.inputs.includes('signals')
globalThis.__candeo_reads_signals = READS_SIGNALS

// And the sound playing: the loop leases the capture only for an effect that
// declares it (§2.2, #107).
const READS_AUDIO = Array.isArray(effect.inputs) && effect.inputs.includes('audio')
globalThis.__candeo_reads_audio = READS_AUDIO

// The declared parameters, against which a bound value is converted.
const SPECS = effect.params ?? {}

// Frozen and shared: an effect that declares no keys receives this one list on
// every frame, with nothing to allocate or to modify.
const NO_PRESSES = Object.freeze([])

// Likewise for the clock: one frozen object, never rebuilt.
const NO_CLOCK = Object.freeze({
  year: 0,
  month: 0,
  day: 0,
  weekday: 0,
  hours: 0,
  minutes: 0,
  seconds: 0,
  ms: 0,
})

// Likewise for signals, for an effect that does not declare them.
const NO_SIGNALS = Object.freeze({})

// And for the sound: silence, which is also what an effect reading it gets
// when nothing plays or nothing can be captured.
const NO_AUDIO = Object.freeze({
  level: 0,
  peak: 0,
  bands: Object.freeze(new Array(16).fill(0)),
  beat: false,
})

// Buffer reused from one frame to the next: allocating it 30 times per second
// would make the garbage collector work for nothing. The rate went down, not
// the argument — QuickJS's garbage collector triggers on the allocated volume,
// and 30 arrays of 396 entries per second are still 30 too many when one is
// enough.
const buf = new Array(FRAME_LEN * 3).fill(0)

// Clamped here, and not only in `rgb()`: nothing forces an effect to go through
// the API, it can build `{r, g, b}` by hand. An out-of-range value or NaN must
// become a valid byte, otherwise it is the conversion on the Rust side that
// fails — far from the cause.
function byte(v) {
  const n = Math.round(v)
  if (!(n >= 0)) return 0
  return n > 255 ? 255 : n
}

function put(index, c) {
  // A position outside the frame is ignored rather than failing the effect: a
  // layout can change, the user's code cannot.
  if (!(index >= 0) || index >= FRAME_LEN) return
  const i = index * 3
  buf[i] = byte(c.r)
  buf[i + 1] = byte(c.g)
  buf[i + 2] = byte(c.b)
}

const frame = {
  set(key, color) {
    put(key?.index, color ?? { r: 0, g: 0, b: 0 })
  },
  fill(color) {
    // On **all** the matrix positions, not only the keys: a frame covers 132 of
    // them, not 106.
    for (let i = 0; i < FRAME_LEN; i++) put(i, color)
  },
}

// `pressesJson` is `[{ "k": <position in layout.keys>, "at": <seconds> }]`, or
// empty when the effect reads no keys. Positions, so that the effect receives the
// layout's own `Key` objects rather than copies.
function presses(pressesJson) {
  if (!pressesJson) return NO_PRESSES
  return JSON.parse(pressesJson)
    .map((p) => ({ key: layout.keys[p.k], at: p.at }))
    .filter((p) => p.key !== undefined)
}

// `clockMs` is the host's wall clock, in milliseconds since the epoch. The host
// reads it, so a test can hand the effect any instant; the split into fields is
// here, where `Date` already knows the machine's time zone.
function clock(clockMs) {
  if (!READS_CLOCK) return NO_CLOCK
  const d = new Date(clockMs)
  return {
    year: d.getFullYear(),
    // 1 is January: a clock face counts months from one, `Date` from zero.
    month: d.getMonth() + 1,
    day: d.getDate(),
    weekday: d.getDay(),
    hours: d.getHours(),
    minutes: d.getMinutes(),
    seconds: d.getSeconds(),
    ms: d.getMilliseconds(),
  }
}

// `signalsJson` is every value held, by name, or empty when the effect does not
// declare signals.
function signals(signalsJson) {
  if (!READS_SIGNALS || !signalsJson) return NO_SIGNALS
  return Object.freeze(JSON.parse(signalsJson))
}

// `audioJson` is the latest analysis of the sound playing, or empty.
function audio(audioJson) {
  if (!READS_AUDIO || !audioJson) return NO_AUDIO
  const heard = JSON.parse(audioJson)
  Object.freeze(heard.bands)
  return Object.freeze(heard)
}

// A bound value, raw as a sender sent it, converted against the parameter's
// spec: `undefined` when it does not fit, and the configured value then stays.
// Here and not in Rust, which keeps parameter values untyped on purpose — the
// specs are only here (§2.3.1).
function converted(spec, raw) {
  switch (spec?.kind) {
    case 'number': {
      const n = typeof raw === 'string' ? (raw.trim() === '' ? NaN : Number(raw)) : raw
      if (typeof n !== 'number' || !Number.isFinite(n)) return undefined
      return Math.min(spec.max, Math.max(spec.min, n))
    }
    case 'color':
      return typeof raw === 'string' ? hexColor(raw) : undefined
    case 'boolean':
      if (raw === true || raw === 'true' || raw === 1 || raw === '1') return true
      if (raw === false || raw === 'false' || raw === 0 || raw === '0') return false
      return undefined
    case 'choice': {
      const value = String(raw)
      const known = spec.options.some((o) => (typeof o === 'string' ? o : o.value) === value)
      return known ? value : undefined
    }
    // Any scalar as its text, `1` or `true` included, cut where the setting
    // stops: a signal holds up to 256 characters, a setting may take fewer.
    case 'text':
      if (!['string', 'number', 'boolean'].includes(typeof raw)) return undefined
      return Array.from(String(raw)).slice(0, textLimit(spec)).join('')
    default:
      return undefined
  }
}

// What a `text` setting takes: its `maxLength`, 64 unless it says, 256 at most.
function textLimit(spec) {
  const declared = Number.isInteger(spec.maxLength) && spec.maxLength > 0 ? spec.maxLength : 64
  return Math.min(declared, 256)
}

// `#rrggbb` or `#rgb`, the `#` optional: what a sender computing a colour writes.
function hexColor(text) {
  const hex = text.trim().replace(/^#/, '')
  const full = /^[0-9a-f]{3}$/i.test(hex) ? hex.replace(/./g, (c) => c + c) : hex
  if (!/^[0-9a-f]{6}$/i.test(full)) return undefined
  const n = parseInt(full, 16)
  return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 }
}

// The configured parameters, with each bound value that converts in its place.
// Absent, expired or nonsense, a bound value leaves the configured one: nothing
// goes dark, and the preview and the swatch draw at rest.
function params(paramsJson, boundJson) {
  const configured = JSON.parse(paramsJson)
  if (!boundJson) return configured
  for (const [name, raw] of Object.entries(JSON.parse(boundJson))) {
    const value = converted(SPECS[name], raw)
    if (value !== undefined) configured[name] = value
  }
  return configured
}

globalThis.__candeo_render = (
  time,
  frameIndex,
  paramsJson,
  pressesJson,
  clockMs,
  boundJson,
  signalsJson,
  audioJson,
) => {
  // Every frame starts again from black: a frame is complete by definition, and
  // an effect that writes only part of the keyboard must not silently inherit
  // what was there before.
  buf.fill(0)

  effect.render({
    layout,
    time,
    frameIndex,
    frame,
    params: params(paramsJson, boundJson),
    presses: presses(pressesJson),
    clock: clock(clockMs),
    signals: signals(signalsJson),
    audio: audio(audioJson),
  })

  return buf
}
