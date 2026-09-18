/**
 * What a firmware effect looks like, drawn by the application.
 *
 * # This is an illustration, and the screen says so
 *
 * The simulator's rule is that it shows **the frames actually sent to the
 * device** (`docs/design/studio.md` §8). A firmware effect has none: the
 * surveyed protocols can write an effect, not read back what it draws. So the
 * gallery used to show a still, black keyboard for every one of them — which
 * says nothing about seven effects that differ.
 *
 * What is drawn here is therefore not a preview and must never be presented as
 * one: it is a legend, the way a colour swatch is, animated because the thing it
 * describes moves. It is written from what the effects were seen doing on the
 * hardware, and a device that shows something else is right and this is wrong.
 *
 * Pure on purpose: one function of a time, testable without a window, and no
 * engine context for a drawing nobody sends anywhere.
 */

import type { Rgb } from '../api/types'

/** What an effect paints with when its settings say nothing. */
const ONE: Rgb = [0xff, 0x00, 0x00]
const TWO: Rgb = [0x00, 0x00, 0xff]

const BLACK: Rgb = [0, 0, 0]

/** A hue of the wheel, full saturation and value, as the device's palettes are. */
function hue(degrees: number): Rgb {
  const sector = (((degrees % 360) + 360) % 360) / 60
  const fall = Math.round(255 * (1 - Math.abs((sector % 2) - 1)))
  switch (Math.floor(sector)) {
    case 0:
      return [255, fall, 0]
    case 1:
      return [fall, 255, 0]
    case 2:
      return [0, 255, fall]
    case 3:
      return [0, fall, 255]
    case 4:
      return [fall, 0, 255]
    default:
      return [255, 0, fall]
  }
}

function dim(colour: Rgb, level: number): Rgb {
  return [
    Math.round(colour[0] * level),
    Math.round(colour[1] * level),
    Math.round(colour[2] * level),
  ]
}

/** Between two colours, `at` from 0 to 1. */
function between(from: Rgb, to: Rgb, at: number): Rgb {
  return [
    Math.round(from[0] + (to[0] - from[0]) * at),
    Math.round(from[1] + (to[1] - from[1]) * at),
    Math.round(from[2] + (to[2] - from[2]) * at),
  ]
}

/** The ids this module draws — every other one has nothing to say. */
export const ILLUSTRATED = [
  'hardware:off',
  'hardware:spectrumCycle',
  'hardware:wave',
  'hardware:m18-01',
  'hardware:m18-02',
  'hardware:m18-03',
  'hardware:m18-08',
  'hardware:m18-09',
  'hardware:m18-0a',
  'hardware:m18-0e',
] as const

export function illustrates(id: string): boolean {
  return (ILLUSTRATED as readonly string[]).includes(id)
}

/**
 * One frame of the illustration of `id`, at `seconds` since it started, for a
 * matrix of `cols` columns and `frameLen` cells.
 *
 * Cells outside the drawing — the gaps of a matrix — are black, as they are in
 * any frame: the simulator only draws the ones carrying a key.
 */
export function illustrate(
  id: string,
  seconds: number,
  cols: number,
  frameLen: number,
  /**
   * The colours the effect was given, so that the drawing says what the device
   * will show. An effect painting its own palette ignores them, here as there.
   */
  colours: readonly Rgb[] = [],
): Rgb[] {
  const one = colours[0] ?? ONE
  const two = colours[1] ?? TWO

  const cell = (position: number): Rgb => {
    const column = cols > 0 ? position % cols : 0
    const across = cols > 1 ? column / (cols - 1) : 0

    switch (id) {
      // One colour, held.
      case 'hardware:m18-01':
        return one

      // The same colour, throbbing: full, dark, full.
      case 'hardware:m18-02':
        return dim(one, (1 + Math.cos(seconds * Math.PI)) / 2)

      // A rainbow crossing the keys, and the Razer's wave beside it. Five sixths
      // of the wheel, not the whole of it: a full turn puts the same red at both
      // ends, and the seam reads as a mistake.
      case 'hardware:m18-03':
      case 'hardware:wave':
        return hue(across * 300 - seconds * 180)

      // Two colours following one another, black in between.
      case 'hardware:m18-08': {
        const phase = (seconds / 2) % 2
        const fading = phase % 1
        const colour = phase < 1 ? one : two
        return dim(colour, fading < 0.5 ? 1 - fading * 2 : (fading - 0.5) * 2)
      }

      // The same, never going dark.
      case 'hardware:m18-09': {
        const phase = (seconds / 2) % 2
        return phase < 1 ? between(one, two, phase) : between(two, one, phase - 1)
      }

      // A lit band sweeping across the keys and back.
      case 'hardware:m18-0a': {
        // Starts at the left edge, as the keyboard's own does.
        const sweep = 1 - Math.abs(((seconds / 1.5) % 2) - 1)
        const distance = Math.abs(across - sweep)
        return dim(one, Math.max(0, 1 - distance * 4))
      }

      // Hues cycling over the whole surface, and the Razer's spectrum beside it.
      case 'hardware:m18-0e':
      case 'hardware:spectrumCycle':
        return hue(seconds * 120)

      // Off is a keyboard that is off.
      default:
        return BLACK
    }
  }

  return Array.from({ length: frameLen }, (_, position) => cell(position))
}
