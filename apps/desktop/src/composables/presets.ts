/**
 * Presets: named settings for one effect, offered on every device and region
 * (`docs/design/presets.md`).
 *
 * What a preset gives an effect today, and whether the settings shown are a
 * preset's. Pure: the list lives in `useSettings`, read with the rest of the
 * file.
 */

import type { ParamSpec } from '@candeo/effects-api'

import type { Bindings, EffectParams, PresetRecord } from '../api/candeo'
import { declaredBindings } from './bindings'
import { merge, sameValue } from './useSettings'

/** Longest name, in characters: Rust refuses more (`presetNameTooLong`). */
export const PRESET_NAME_MAX = 64

/** An effect's settings, as a preset gives them and as the form shows them. */
export interface PresetSettings {
  values: EffectParams
  bindings: Bindings
}

/**
 * The settings a preset gives an effect **as it declares its parameters now**
 * (§6): its value for each parameter still declared, the default where one no
 * longer fits or was added since, and its bindings on parameters that can still
 * follow them. The rule kept settings follow, so a preset saved for an older
 * version of an effect is picked like a setting kept for it.
 */
export function presetSettings(
  specs: Record<string, ParamSpec>,
  preset: Pick<PresetRecord, 'values' | 'bindings'>,
): PresetSettings {
  return {
    values: merge(specs, preset.values),
    bindings: declaredBindings(specs, preset.bindings ?? {}),
  }
}

/**
 * Whether the settings shown are this preset's: every declared value the same,
 * a bound parameter's included — it is what shows while its source is absent —
 * and the same bindings.
 *
 * What marks a preset in the list. Computed, never stored: a stored *preset in
 * use* would be stale at the first slider moved.
 */
export function holds(
  specs: Record<string, ParamSpec>,
  shown: PresetSettings,
  preset: Pick<PresetRecord, 'values' | 'bindings'>,
): boolean {
  const wanted = presetSettings(specs, preset)
  for (const [id, spec] of Object.entries(specs)) {
    if (!sameValue(shown.values[id] ?? spec.default, wanted.values[id])) return false
  }
  const bound = Object.entries(declaredBindings(specs, shown.bindings))
  return (
    bound.length === Object.keys(wanted.bindings).length &&
    bound.every(([id, source]) => wanted.bindings[id] === source)
  )
}

/** A name as Rust keeps it: trimmed. Empty means nothing to save yet. */
export function presetName(typed: string): string {
  return typed.trim()
}
