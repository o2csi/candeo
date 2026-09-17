// Clock — the time, read on the number row.
//
// It reads the wall clock (`inputs: ['clock']`), which costs nothing to capture
// and reveals nothing: every effect asking for it gets it.
//
// The digits are found by **scancode**, not by label: `0x02`…`0x0B` are the keys
// engraved 1 to 9 then 0 on every layout, AZERTY and QWERTY alike. So 14:35
// lights 1 and 4 in the hours colour and 3 and 5 in the minutes colour; a digit
// both halves need — 12:23 — takes the two mixed, the only honest thing to draw
// on one key.
//
// With no clock, which is how the swatch is sampled, the time reads 00:00: the
// zero lights, and the thumbnail shows the effect's colours rather than black.

import { defineEffect, mix } from '@candeo/effects-api'

const HOURS = { r: 255, g: 176, b: 64 }
const MINUTES = { r: 64, g: 200, b: 255 }
const BACKGROUND = { r: 4, g: 6, b: 12 }

/** Scancodes of the number row, in digit order: `1`…`9`, then `0`. */
const DIGITS = [0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b]

/** The scancode of the key engraved with this digit — `0` sits last on the row. */
function scancodeOf(digit = 0) {
  return DIGITS[(digit + 9) % 10]
}

export default defineEffect({
  description: {
    en: 'The time, lit on the number row: hours in one colour, minutes in another',
    fr: "L'heure, allumée sur la rangée des chiffres : heures d'une couleur, minutes d'une autre",
  },
  kinds: ['keyboard'],
  inputs: ['clock'],
  params: {
    hours: { kind: 'color', label: { en: 'Hours', fr: 'Heures' }, default: HOURS },
    minutes: { kind: 'color', label: { en: 'Minutes', fr: 'Minutes' }, default: MINUTES },
    background: { kind: 'color', label: { en: 'Background', fr: 'Fond' }, default: BACKGROUND },
    hour12: {
      kind: 'boolean',
      label: { en: 'Show the hour on 12', fr: "Afficher l'heure sur 12" },
      default: false,
    },
    beat: {
      kind: 'boolean',
      label: { en: 'Beat every second', fr: 'Battre chaque seconde' },
      default: true,
    },
  },
  render({ layout, clock, frame, params }) {
    const hoursColor = params.hours ?? HOURS
    const minutesColor = params.minutes ?? MINUTES
    const background = params.background ?? BACKGROUND
    const both = mix(hoursColor, minutesColor, 0.5)

    // Midnight and noon read 12 on a twelve-hour face, never 0.
    const hour = (params.hour12 ?? false) ? clock.hours % 12 || 12 : clock.hours

    // A pulse fading over the second, so the keyboard shows a clock running
    // rather than one frozen on the minute.
    const beat = (params.beat ?? true) ? 1 - 0.35 * (clock.ms / 1000) : 1

    const firstHour = scancodeOf(Math.floor(hour / 10))
    const lastHour = scancodeOf(hour % 10)
    const firstMinute = scancodeOf(Math.floor(clock.minutes / 10))
    const lastMinute = scancodeOf(clock.minutes % 10)

    for (const key of layout.keys) {
      const code = key.scancode
      const isHour = code === firstHour || code === lastHour
      const isMinute = code === firstMinute || code === lastMinute

      if (!isHour && !isMinute) {
        frame.set(key, background)
        continue
      }
      const color = isHour && isMinute ? both : isHour ? hoursColor : minutesColor
      frame.set(key, mix(background, color, beat))
    }
  },
})
