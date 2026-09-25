// What the device card computes about a brightness following the sound or a
// signal (`docs/design/inputs-and-automations.md` §2.2.2). Pure, so what the
// switch shows and what goes to Rust are tested without a DOM.

import { DIMMING_FLOOR_DEFAULT, type Dimming } from '../api/candeo'
import { boundSignal, boundSound, signalSource, soundSource, type SoundSource } from './bindings'
import { validSignalName } from './rules'

/** What holds a device's brightness: the slider alone, a signal or the sound. */
export type DimmingMode = 'value' | 'signal' | 'sound'

/** The mode a stored dimming says; a source Rust would refuse reads as none. */
export function dimmingMode(dimming: Dimming | null): DimmingMode {
  if (dimming === null) return 'value'
  if (boundSound(dimming.source) !== null) return 'sound'
  if (boundSignal(dimming.source) !== null) return 'signal'
  return 'value'
}

/** What of the sound it follows, or `null`. */
export function dimmingSound(dimming: Dimming | null): SoundSource | null {
  return dimming === null ? null : boundSound(dimming.source)
}

/** The signal it follows, or `null`. */
export function dimmingSignal(dimming: Dimming | null): string | null {
  return dimming === null ? null : boundSignal(dimming.source)
}

/** The floor it keeps, or the default when it follows nothing yet. */
export function dimmingFloor(dimming: Dimming | null): number {
  return dimming?.floor ?? DIMMING_FLOOR_DEFAULT
}

/** Following `sound`, the floor kept from what it followed before. */
export function followingSound(before: Dimming | null, sound: SoundSource): Dimming {
  return { source: soundSource(sound), floor: dimmingFloor(before) }
}

/**
 * Following the signal `name`, the floor kept; `null` while the name is not one
 * a sender could use, and the brightness then stays as it was.
 */
export function followingSignal(before: Dimming | null, name: string): Dimming | null {
  const trimmed = name.trim()
  if (!validSignalName(trimmed)) return null
  return { source: signalSource(trimmed), floor: dimmingFloor(before) }
}

/** The same source with another floor, 0 to 100. */
export function withFloor(dimming: Dimming, floor: number): Dimming {
  return { ...dimming, floor: Math.min(100, Math.max(0, Math.round(floor))) }
}
