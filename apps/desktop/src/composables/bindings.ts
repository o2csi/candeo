// What the window computes about bindings: a parameter reading a signal instead
// of its value (`docs/design/inputs-and-automations.md` §2.3.1).
//
// Pure, so which bindings hold, what the engine and the file are given, and what
// a bound row says about its signal are tested without Rust or a DOM.

import type { ParamSpec } from '@candeo/effects-api'

import type { Bindings, HeldSignal, Rule, RuleShow, SignalValue } from '../api/candeo'
import { validSignalName } from './rules'
import { signalText } from './signals'

/**
 * How a source names a signal. A source says its kind so that another per-frame
 * value, the sound level of §2.2, binds the same way; Rust reads it back with
 * `bound_signal`, in `src-tauri/src/signals/store.rs`.
 */
const SIGNAL = 'signal:'

/** The source that reads the signal `name`. */
export function signalSource(name: string): string {
  return `${SIGNAL}${name}`
}

/** The signal a source reads, or `null` when Rust would refuse it (`bindingInvalid`). */
export function boundSignal(source: string): string | null {
  if (!source.startsWith(SIGNAL)) return null
  const name = source.slice(SIGNAL.length)
  return validSignalName(name) ? name : null
}

/**
 * The bindings that hold for an effect: its declared parameters only, each to a
 * source Rust accepts.
 *
 * The engine and the file are both given this. Values leave out what equals the
 * effect's default; a binding has no default to equal, so nothing more goes. A
 * binding kept for a parameter the effect no longer declares goes as its value
 * does, and a source edited by hand into nonsense goes rather than making Rust
 * refuse the whole table at the next start.
 */
export function declaredBindings(specs: Record<string, ParamSpec>, kept: Bindings): Bindings {
  const out: Bindings = {}
  for (const [id, source] of Object.entries(kept)) {
    if (id in specs && boundSignal(source) !== null) out[id] = source
  }
  return out
}

/** `bindings` with `id` reading `source`, or holding its value again when `source` is `null`. */
export function rebound(bindings: Bindings, id: string, source: string | null): Bindings {
  const out = { ...bindings }
  if (source === null) delete out[id]
  else out[id] = source
  return out
}

/**
 * What a rule shows once `id` reads `source`, or its value again when `source`
 * is `null`. A rule carries its own bindings, as it carries its own values
 * (§2.3.1); no `bindings` once none is left, as Rust writes the file.
 */
export function withBinding(show: RuleShow, id: string, source: string | null): RuleShow {
  const { bindings: before, ...rest } = show
  const bindings = rebound(before ?? {}, id, source)
  return Object.keys(bindings).length > 0 ? { ...rest, bindings } : rest
}

/**
 * Whether a signal's value takes the place of a parameter's own.
 *
 * The conversion is `converted`, in `src-tauri/src/runtime/bootstrap.js`, and it
 * is done there, once per frame. This copy only decides what a row says beside
 * the field: disagreeing, it would mislabel a value, never change what the
 * effect shows.
 */
export function converts(spec: ParamSpec, raw: SignalValue): boolean {
  switch (spec.kind) {
    case 'number': {
      const n = typeof raw === 'string' ? (raw.trim() === '' ? NaN : Number(raw)) : raw
      return typeof n === 'number' && Number.isFinite(n)
    }
    case 'color':
      return typeof raw === 'string' && /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.test(raw.trim())
    case 'boolean':
      return [true, false, 1, 0, 'true', 'false', '1', '0'].includes(raw)
    case 'choice': {
      const value = String(raw)
      return spec.options.some((o) => (typeof o === 'string' ? o : o.value) === value)
    }
  }
}

/** What a bound row says about its signal. */
export type BindingState =
  /** No signal of that name is held: the parameter keeps its value. */
  | { kind: 'absent' }
  /** Held, and it takes the parameter's place. */
  | { kind: 'fits'; value: string }
  /** Held, and the parameter cannot take it: the parameter keeps its value. */
  | { kind: 'unfit'; value: string }

export function bindingState(
  spec: ParamSpec,
  name: string,
  held: readonly HeldSignal[],
): BindingState {
  const signal = held.find((s) => s.name === name)
  if (!signal) return { kind: 'absent' }
  const value = signalText(signal.value)
  return converts(spec, signal.value) ? { kind: 'fits', value } : { kind: 'unfit', value }
}

/**
 * Whether an effect reads a signal: a parameter bound to one, or all of them
 * through its declared `inputs: ['signals']`. Turning signals off leaves it
 * showing its own values, so the window says so where it is set up (#224).
 */
export function readsSignal(declared: boolean | undefined, bindings: Bindings): boolean {
  return declared === true || Object.keys(bindings).length > 0
}

/** Whether a rule needs signals: to start, or for the effect it shows. */
export function ruleReadsSignal(
  rule: Rule,
  declared: boolean | undefined,
  bindings: Bindings,
): boolean {
  return rule.when.kind === 'signal' || readsSignal(declared, bindings)
}
