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
