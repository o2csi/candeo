/**
 * Hardware effects, and the effect set by this session.
 *
 * Module-level state, like `useDevice`: moving to the Devices screen and coming
 * back must not erase what was just applied.
 */

import { readonly, ref } from 'vue'

import * as api from '../api/candeo'
import { message } from '../api/journal'
import type { ParamSpec } from '@candeo/effects-api'

import type { DeviceRef } from '../api/types'
import { t } from '../i18n'

export interface HardwareEffect {
  id: string
  /** Its name, as the gallery shows it. */
  name: string
  /** One sentence on what it does, when we know. */
  summary: string
  /** How many colours it paints with — none for one with its own palette. */
  colours: number
}

/**
 * The one effect every device offers: a firmware that draws nothing of its own
 * still goes dark, on a black frame.
 */
export const OFF = 'hardware:off'

/**
 * What a firmware effect declares as settings: its colours, and nothing else.
 *
 * **A firmware effect is an effect with settings**, so it says so the same way
 * the others do. The form that draws them, the values kept for a device, the
 * ones a rule carries: all of it already works on a declaration, and none of it
 * had to learn what a firmware effect is.
 */
export function hardwareParams(e: HardwareEffect): Record<string, ParamSpec> {
  const specs: Record<string, ParamSpec> = {}
  if (e.colours >= 1) {
    specs.colour = {
      kind: 'color',
      label: t('effects.hardwareEffects.pickColour'),
      default: { r: 0xff, g: 0x00, b: 0x00 },
    }
  }
  if (e.colours >= 2) {
    specs.colour2 = {
      kind: 'color',
      label: t('effects.hardwareEffects.pickColour2'),
      default: { r: 0x00, g: 0x00, b: 0xff },
    }
  }
  return specs
}

/**
 * The colours out of those values, flat, in the order the effect takes them —
 * the shape the command wants, and the one a frame crosses in.
 */
export function colourBytes(values: Record<string, unknown>): number[] {
  return ['colour', 'colour2'].flatMap((key) => {
    const colour = values[key] as { r?: number; g?: number; b?: number } | undefined
    if (!colour || typeof colour.r !== 'number') return []
    return [colour.r, colour.g ?? 0, colour.b ?? 0]
  })
}

/** What an Alienware keyboard's own effects are called, before their number. */
const ALIENWARE = 'hardware:m18-'

/**
 * What each of that keyboard's kinds shows, watched one by one on the hardware.
 * The protocol says nothing about it: the numbers answer, the names were read
 * off the keyboard.
 *
 * **The words are the maker's own**, as its software lists them — *Couleur*,
 * *Respiration*, *Spectre*, *Onde arc-en-ciel*, *Scanner* — so that someone
 * coming from it finds what they know. The two it does not offer, `02` and `09`,
 * keep the names of the lighting API they belong to.
 */
const ALIENWARE_NAMES = {
  '01': 'colour',
  '02': 'pulse',
  '03': 'rainbowWave',
  '08': 'breathing',
  '09': 'morph',
  '0a': 'scanner',
  '0e': 'spectrum',
} as const

/**
 * A firmware effect's name comes from its id, not from a catalogue written here.
 *
 * **One family's modes are not another's.** The layout says which ids a device
 * runs; this only puts words on them. The Alienware keyboard's sixteen kinds are
 * named after their number until someone says what each one shows — which takes
 * eyes on a keyboard, not code.
 */
export function named(id: string, colours = 0): HardwareEffect {
  if (id === OFF) {
    return {
      id,
      colours,
      name: t('effects.hardwareEffects.off.name'),
      summary: t('effects.hardwareEffects.off.summary'),
    }
  }
  if (id === 'hardware:spectrumCycle' || id === 'hardware:wave') {
    const key = id === 'hardware:wave' ? 'wave' : 'spectrumCycle'
    return {
      id,
      colours,
      name: t(`effects.hardwareEffects.${key}.name`),
      summary: t(`effects.hardwareEffects.${key}.summary`),
    }
  }
  const kind = id.slice(ALIENWARE.length) as keyof typeof ALIENWARE_NAMES
  const alienware = ALIENWARE_NAMES[kind]
  if (id.startsWith(ALIENWARE) && alienware) {
    return {
      id,
      colours,
      name: t(`effects.hardwareEffects.${alienware}.name`),
      summary: t(`effects.hardwareEffects.${alienware}.summary`),
    }
  }
  // An id nobody named: shown as it is rather than invented. The gallery only
  // offers what a layout lists, so this is the sign of a layout gone ahead of
  // the words for it.
  return { id, colours, name: id, summary: '' }
}

/**
 * Those a given device runs, **Off included**.
 *
 * Without a layout — no device chosen — only *Off* comes back: naming what an
 * unknown device runs would be inventing it.
 */
export function hardwareEffectsFor(
  layout: { firmwareEffects?: { id: string; colours: number }[] } | null | undefined,
): readonly HardwareEffect[] {
  const offered = layout?.firmwareEffects ?? []
  return [...offered.map((e) => named(e.id, e.colours)), named(OFF)]
}

/**
 * What this session has set, **device by device**, key "vid:pid".
 *
 * A single global field would mark the same effect active on every device, in a
 * list that describes what **one** device does: it is not a simplification, it
 * is false information from the second keyboard on.
 */
const applied = ref<Record<string, { id: string; colours: number[] }>>({})
const error = ref<string | null>(null)

const key = (d: DeviceRef) => `${d.vid}:${d.pid}`

/** Errors raised by Rust are already readable: they are shown as they are. */
export function useEffects() {
  /**
   * Sets a hardware effect **on a device**.
   *
   * What is remembered says what **this session** has set, not what the
   * keyboard shows: the recorded protocol can write an effect, not read it back.
   * Nothing is therefore marked at launch — a guess would be worse than
   * nothing, since it would be silently wrong after a restart.
   */
  async function apply(device: DeviceRef, e: HardwareEffect, colours: number[] = []) {
    error.value = null
    try {
      await api.setEffect(device, e.id, colours)
      // Replacement rather than mutation: `readonly()` forbids writing into the
      // exposed object, and reactivity no longer depends on the key being present.
      applied.value = { ...applied.value, [key(device)]: { id: e.id, colours } }
    } catch (err) {
      error.value = message(err)
    }
  }

  /** The hardware effect this session has set on this device, if there is one. */
  function appliedOn(device: DeviceRef | null): string | null {
    return device ? (applied.value[key(device)]?.id ?? null) : null
  }


  /**
   * Forgets what this session had set, on every device.
   *
   * Called after the configuration reset, which turns the backlight off and
   * closes the devices: what was remembered here described a mode no keyboard
   * shows any more. Keeping it would mark an effect that is off as "active" —
   * and this table does not correct itself, since the recorded protocol can
   * write an effect, not read it back.
   */
  function forgetApplied() {
    applied.value = {}
  }

  /** Closes the message: what failed is read, and the screen goes back to work. */
  function dismissError() {
    error.value = null
  }

  return {
    appliedOn,
    error: readonly(error),
    dismissError,
    apply,
    forgetApplied,
  }
}
