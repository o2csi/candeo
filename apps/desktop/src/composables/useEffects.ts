/**
 * Hardware effects, and the effect set by this session.
 *
 * Module-level state, like `useDevice`: moving to the Devices screen and coming
 * back must not erase what was just applied.
 */

import { readonly, ref } from 'vue'

import * as api from '../api/candeo'
import { message, warn } from '../api/journal'
import type { ParamSpec } from '@candeo/effects-api'

import type { DeviceRef, FirmwareEffectInfo } from '../api/types'
import { t } from '../i18n'
import { localized } from '../i18n/text'

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

/**
 * Every firmware effect Candeo knows, by id: what names one outside the
 * gallery — a rule's, a signal reader's — whatever device is current. Read at
 * startup and again when the definitions are; {@link named} shows the id until
 * then.
 */
const known = ref(new Map<string, FirmwareEffectInfo>())

export async function refreshFirmwareEffects(): Promise<void> {
  try {
    known.value = new Map((await api.firmwareEffects()).map((e) => [e.id, e]))
  } catch (e) {
    warn('effects', `firmware effect names not read: ${message(e, 'en')}`, e)
  }
}

/**
 * A firmware effect's name and summary come from its device's definition
 * (`device-sdk.md` §7), in the interface's language: one family's modes are not
 * another's, and whoever describes a kind names it. Only *Off*, which every
 * device offers, is the application's. A kind nobody named shows its id rather
 * than an invented name.
 */
export function named(id: string, colours = 0, own?: FirmwareEffectInfo): HardwareEffect {
  if (id === OFF) {
    return {
      id,
      colours,
      name: t('effects.hardwareEffects.off.name'),
      summary: t('effects.hardwareEffects.off.summary'),
    }
  }
  const declared = own ?? known.value.get(id)
  return {
    id,
    colours,
    name: localized(declared?.name ?? undefined) || id,
    summary: localized(declared?.summary ?? undefined),
  }
}

/**
 * Those a given device runs, **Off included**.
 *
 * Without a layout — no device chosen — only *Off* comes back: naming what an
 * unknown device runs would be inventing it.
 */
export function hardwareEffectsFor(
  layout: { firmwareEffects?: FirmwareEffectInfo[] } | null | undefined,
): readonly HardwareEffect[] {
  const offered = layout?.firmwareEffects ?? []
  return [...offered.map((e) => named(e.id, e.colours, e)), named(OFF)]
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
