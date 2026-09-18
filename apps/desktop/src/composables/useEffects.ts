/**
 * Effets matériels, et effet posé par cette session.
 *
 * État au niveau du module, comme `useDevice` : passer sur l'écran
 * Périphériques et revenir ne doit pas effacer ce qu'on vient d'appliquer.
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
      label: t('effects.hardwareEffects.colour'),
      default: { r: 0xff, g: 0x00, b: 0x00 },
    }
  }
  if (e.colours >= 2) {
    specs.colour2 = {
      kind: 'color',
      label: t('effects.hardwareEffects.colour2'),
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
 * The protocol says nothing about it: the numbers answer, the names come from
 * eyes on a keyboard.
 */
const ALIENWARE_NAMES = {
  '01': 'static',
  '02': 'pulse',
  '03': 'wave',
  '08': 'breathing',
  '09': 'morph',
  '0a': 'scan',
  '0e': 'spectrumCycle',
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
 * Ce que cette session a posé, **appareil par appareil**, clé « vid:pid ».
 *
 * Un seul champ global marquerait le même effet actif sur tous les appareils,
 * dans une liste qui décrit ce que fait **un** appareil : ce n'est pas une
 * simplification, c'est une information fausse dès le second clavier.
 */
const posed = ref<Record<string, { id: string; colours: number[] }>>({})
const error = ref<string | null>(null)

const key = (d: DeviceRef) => `${d.vid}:${d.pid}`

/** Les erreurs remontées par Rust sont déjà lisibles : on les affiche telles quelles. */
export function useEffects() {
  /**
   * Pose un effet matériel **sur un appareil**.
   *
   * Ce qu'on retient dit ce que **cette session** a posé, pas ce que le clavier
   * affiche : le protocole relevé sait écrire un effet, pas le relire. Rien
   * n'est donc marqué au lancement — une supposition serait pire que le vide,
   * puisqu'elle se tromperait silencieusement après un redémarrage.
   */
  async function apply(device: DeviceRef, e: HardwareEffect, colours: number[] = []) {
    error.value = null
    try {
      await api.setEffect(device, e.id, colours)
      // Remplacement plutôt que mutation : `readonly()` interdit d'écrire dans
      // l'objet exposé, et la réactivité ne dépend plus de la clé déjà présente.
      posed.value = { ...posed.value, [key(device)]: { id: e.id, colours } }
    } catch (err) {
      error.value = message(err)
    }
  }

  /** L'effet matériel que cette session a posé sur cet appareil, s'il y en a un. */
  function appliedOn(device: DeviceRef | null): string | null {
    return device ? (posed.value[key(device)]?.id ?? null) : null
  }

  /**
   * True when the device is showing this effect **with these colours**.
   *
   * A firmware effect has no loop to adjust: the device holds what it was last
   * given, so a colour changed since then is a change waiting to be applied —
   * and the button says so instead of reading *Applied* over a keyboard showing
   * the previous colour.
   */
  function poseMatches(device: DeviceRef | null, id: string, colours: number[]): boolean {
    const pose = device ? posed.value[key(device)] : undefined
    return pose?.id === id && String(pose.colours) === String(colours)
  }

  /**
   * Oublie ce que cette session avait posé, sur tous les appareils.
   *
   * Appelé après la remise à zéro de la configuration, qui éteint le
   * rétroéclairage et referme les appareils : ce qu'on retenait ici décrivait un
   * mode que plus aucun clavier n'affiche. Le garder ferait marquer « actif » un
   * effet éteint — et cette table ne se corrige pas d'elle-même, puisque le
   * protocole relevé sait écrire un effet, pas le relire.
   */
  function forgetPosed() {
    posed.value = {}
  }

  /** Closes the message: what failed is read, and the screen goes back to work. */
  function dismissError() {
    error.value = null
  }

  return {
    appliedOn,
    poseMatches,
    error: readonly(error),
    dismissError,
    apply,
    forgetPosed,
  }
}
