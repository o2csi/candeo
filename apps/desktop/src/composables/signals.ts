// What the window computes about signals and their API (#108), apart from
// drawing them.
//
// Pure, so how a value reads, how a port is accepted, which interfaces are
// listed and how the time left reads are tested without Rust or a network.

import type { HeldSignal, NetworkInterface, SignalValue } from '../api/candeo'

/**
 * A value as a rule compares it, `Scalar::as_text` in Rust: a sender's `1` and
 * `2.0` read `1` and `2`, a flag `true` or `false`. Only numbers written with an
 * exponent read differently, and nobody types those into a rule.
 */
export function signalText(value: SignalValue): string {
  return String(value)
}

/** Below 1024, Linux asks for privileges no desktop application has: Rust refuses them. */
export const PORT_MIN = 1024
export const PORT_MAX = 65535

/** Whether a port can be asked of Rust; anything else is refused before it is sent. */
export function validPort(port: number): boolean {
  return Number.isInteger(port) && port >= PORT_MIN && port <= PORT_MAX
}

/** Where a sender on this computer posts its values. */
export function signalsAddress(port: number): string {
  return `http://127.0.0.1:${port}/signals`
}

/** An interface as Settings lists it. */
export interface InterfaceRow {
  name: string
  /** Its first address, IPv4 first; `null` for one ticked that is not up now. */
  address: string | null
  ticked: boolean
}

/**
 * The interfaces to list: those up, then those ticked that are not up now.
 *
 * A ticked interface stays listed while it is down, since the choice is kept by
 * name and comes back with it: otherwise nobody could untick it until it is up.
 */
export function interfaceRows(
  up: readonly NetworkInterface[],
  ticked: readonly string[],
): InterfaceRow[] {
  const rows: InterfaceRow[] = up.map((i) => ({
    name: i.name,
    address: i.addresses[0] ?? null,
    ticked: ticked.includes(i.name),
  }))
  for (const name of ticked) {
    if (!up.some((i) => i.name === name)) rows.push({ name, address: null, ticked: true })
  }
  return rows
}

/** The interfaces ticked once `name` is ticked or not, in the order they were ticked. */
export function ticking(ticked: readonly string[], name: string, on: boolean): string[] {
  const others = ticked.filter((n) => n !== name)
  return on ? [...others, name] : others
}

/** The signals still held at `now`: Rust drops the expired ones within a second. */
export function alive(held: readonly HeldSignal[], now: number): HeldSignal[] {
  return held.filter((s) => s.expires === null || s.expires > now)
}

/** How long a signal has left, as the list reads it. */
export interface TimeLeft {
  key:
    | 'settings.signals.untilErased'
    | 'settings.signals.leftSeconds'
    | 'settings.signals.leftMinutes'
    | 'settings.signals.leftHours'
  n?: number
}

/**
 * The time left before a signal expires.
 *
 * Seconds under two minutes, so the default lifetime of 60 s counts down one by
 * one; minutes under two hours, rounded up so it never reads less than is left;
 * whole hours beyond, since a lifetime that long was chosen, not counted.
 */
export function timeLeft(expires: number | null, now: number): TimeLeft {
  if (expires === null) return { key: 'settings.signals.untilErased' }
  const seconds = Math.max(0, Math.ceil((expires - now) / 1000))
  if (seconds < 120) return { key: 'settings.signals.leftSeconds', n: seconds }
  if (seconds < 7200) return { key: 'settings.signals.leftMinutes', n: Math.ceil(seconds / 60) }
  return { key: 'settings.signals.leftHours', n: Math.floor(seconds / 3600) }
}
