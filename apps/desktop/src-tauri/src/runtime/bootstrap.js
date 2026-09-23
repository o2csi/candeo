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

globalThis.__candeo_render = (time, frameIndex, paramsJson, pressesJson, clockMs) => {
  // Every frame starts again from black: a frame is complete by definition, and
  // an effect that writes only part of the keyboard must not silently inherit
  // what was there before.
  buf.fill(0)

  effect.render({
    layout,
    time,
    frameIndex,
    frame,
    params: JSON.parse(paramsJson),
    presses: presses(pressesJson),
    clock: clock(clockMs),
  })

  return buf
}
