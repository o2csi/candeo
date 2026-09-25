// Waterfall — the sound playing as it goes by: what plays now comes in on the
// right and slides to the left.
//
// A spectrogram on the keys (`inputs: ['audio']`): a band per row, low notes at
// the bottom, and time across the keyboard's physical width, so a note held
// draws a line and a beat a column. Each key shows how loud its band was when
// that moment passed the right edge, from the background through the quiet
// colour to the loud one.
//
// With nothing playing, which is also how the swatch is sampled, only the
// background shows.

import { bounds, center, defineEffect, mix } from '@candeo/effects-api'

const QUIET = { r: 20, g: 60, b: 200 }
const LOUD = { r: 255, g: 230, b: 120 }
const BACKGROUND = { r: 2, g: 4, b: 12 }

// What each row heard, frame by frame, newest last: `{ at, rows }`.
let history = [{ at: 0, rows: [0] }]

/** The bands grouped into `rows`, low first: the loudest of each group. */
function grouped(bands = [0], rows = 1) {
  return Array.from({ length: rows }, (_, group) => {
    const from = Math.floor((group * bands.length) / rows)
    const to = Math.max(from + 1, Math.floor(((group + 1) * bands.length) / rows))
    return Math.max(...bands.slice(from, to))
  })
}

/** What was heard at `at`, or nothing when it is older than the history. */
function heardAt(at = 0) {
  for (let i = history.length - 1; i >= 0; i--) {
    if (history[i].at <= at) return history[i].rows
  }
  return []
}

export default defineEffect({
  description: {
    en: 'The sound playing as it goes by, sliding from right to left, a row per pitch',
    fr: 'Le son joué qui défile de droite à gauche, une rangée par hauteur',
  },
  kinds: ['keyboard'],
  inputs: ['audio'],
  params: {
    quiet: { kind: 'color', label: { en: 'Quiet', fr: 'Faible' }, default: QUIET },
    loud: { kind: 'color', label: { en: 'Loud', fr: 'Fort' }, default: LOUD },
    background: { kind: 'color', label: { en: 'Background', fr: 'Fond' }, default: BACKGROUND },
    speed: {
      kind: 'number',
      label: { en: 'Speed (keys per second)', fr: 'Vitesse (touches par seconde)' },
      min: 2,
      max: 30,
      step: 1,
      default: 10,
    },
    gain: {
      kind: 'number',
      label: { en: 'Gain', fr: 'Gain' },
      min: 0.5,
      max: 3,
      step: 0.1,
      default: 1.2,
    },
  },
  render({ layout, time, audio, frame, params }) {
    if (layout.keys.length === 0) return
    const quiet = params.quiet ?? QUIET
    const loud = params.loud ?? LOUD
    const background = params.background ?? BACKGROUND
    const speed = Number(params.speed ?? 10)
    const gain = Number(params.gain ?? 1.2)
    const area = bounds(layout)
    const rows = Math.max(...layout.keys.map((key) => key.row)) + 1

    // A time before the last frame is a restart: the preview starts from zero.
    if (time < history[history.length - 1].at) history = []
    history.push({ at: time, rows: grouped([...audio.bands], rows) })
    // Kept for as long as it takes to cross the keyboard.
    const span = area.w / speed + 0.5
    while (history.length > 1 && history[0].at < time - span) history.shift()

    for (const key of layout.keys) {
      // Measured from the key's right edge: the rightmost keys show what plays
      // now, not what played the time it takes to cross half a key.
      const right = center(key).x + (key.w ?? 1) / 2
      const age = (area.x + area.w - right) / speed
      const level = Math.min(1, (heardAt(time - age)[rows - 1 - key.row] ?? 0) * gain)
      const color = level < 0.5 ? mix(background, quiet, level * 2) : mix(quiet, loud, level * 2 - 1)
      frame.set(key, color)
    }
  },
})
