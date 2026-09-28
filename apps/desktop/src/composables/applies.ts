/**
 * Whether an effect is offered for a device (`docs/design/device-sdk.md` §2–§3):
 * its kind of lights one the effect makes sense on, what it needs there, and
 * keys to press when it reads presses. The same rule as `Manifest::applies_to`
 * in Rust, which the tray reads.
 */

/** What an effect says of where it goes. */
export interface Wants {
  /** `keys`, `zones`, or `all`. */
  kinds: readonly string[]
  /** `matrix`, `geometry`. */
  requires: readonly string[]
  readsKeys: boolean
}

/** What a device offers it. */
export interface Offers {
  lights: 'keys' | 'zones'
  capabilities: readonly string[]
}

export function applies(effect: Wants, device: Offers): boolean {
  const makesSense = effect.kinds.some((kind) => kind === 'all' || kind === device.lights)
  const hasAll = effect.requires.every((capability) => device.capabilities.includes(capability))
  return makesSense && hasAll && (!effect.readsKeys || device.lights === 'keys')
}

/** What a library effect's manifest says, what it leaves out read as Rust reads it. */
export function wants(manifest: {
  kinds?: readonly string[]
  requires?: readonly string[]
  readsKeys?: boolean
}): Wants {
  return {
    kinds: manifest.kinds ?? ['keys'],
    requires: manifest.requires ?? [],
    readsKeys: manifest.readsKeys ?? false,
  }
}

/**
 * The effects a picker offers for some devices: those that apply to each, and
 * the one already chosen, which the picker must still show. With no device
 * known, every one: nothing says what would not apply.
 */
export function offeredFor<E extends Parameters<typeof wants>[0] & { id: string }>(
  effects: readonly E[],
  devices: readonly Offers[],
  chosen?: string,
): E[] {
  return effects.filter((e) => e.id === chosen || devices.every((d) => applies(wants(e), d)))
}

/**
 * The devices an effect goes to: of those wanted, where it applies; none left,
 * every candidate where it does. An effect nobody knows goes where it was sent.
 */
export function devicesFor<D extends Offers>(
  effect: Wants | null,
  wanted: readonly D[],
  candidates: readonly D[],
): D[] {
  if (!effect) return [...wanted]
  const kept = wanted.filter((d) => applies(effect, d))
  return kept.length ? kept : candidates.filter((d) => applies(effect, d))
}
