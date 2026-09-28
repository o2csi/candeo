// What the window says about the computer's sources (§2.4): their names, and
// which this computer offers, asked once.

import { systemOffers, type SystemOffers } from '../api/candeo'
import type { SystemSource } from './bindings'

/** Each source, as the lists name it. */
export const SYSTEM_LABELS = {
  cpu: 'effects.params.system.cpu',
  memory: 'effects.params.system.memory',
  gpu: 'effects.params.system.gpu',
  cpuTemp: 'effects.params.system.cpuTemp',
  gpuTemp: 'effects.params.system.gpuTemp',
  cpuPower: 'effects.params.system.cpuPower',
} as const satisfies Record<SystemSource, string>

/** Every source offered: what the lists assume until Rust has said. */
const EVERY: SystemOffers = {
  cpu: true,
  memory: true,
  gpu: true,
  cpuTemp: true,
  gpuTemp: true,
  cpuPower: true,
}

let asked: Promise<SystemOffers> | null = null

/**
 * Which sources this computer offers. Rust reads them once and keeps them, so
 * the window asks once too; a failure offers them all, and Settings › System
 * says what can be read.
 */
export function offeredSources(): Promise<SystemOffers> {
  asked ??= systemOffers().catch(() => EVERY)
  return asked
}

/** Whether `source` is offered, every one while nothing is known. */
export function offered(offers: SystemOffers | null, source: SystemSource): boolean {
  return offers === null || offers[source]
}
