// Equalizer — the sound playing as bars, by frequency.
//
// Two ways to lay the bars out (`inputs: ['audio']`):
// - Columns: the sixteen bands spread across the keyboard's physical width, low
//   notes on the left, so a bar keeps its width with or without a numeric
//   keypad. Each rises from the bottom row by its band's strength. Mirrored, the
//   low notes are in the middle and each band shows on both sides, as music
//   visualisers draw it.
// - Rows: a band per row, low notes at the bottom, each growing from the left.
//   Fewer bands, as many as rows, but bars about twenty keys long rather than
//   six high: the level reads finer.
// The key at the end of a bar is lit by how far the bar reaches into it, so
// bars grow smoothly rather than a key at a time. Colours run from the low
// level's to the high level's along each bar, as a VU meter's do.
//
// With nothing playing, which is also how the swatch is sampled, only the
// background shows.

import { bounds, center, defineEffect, mix } from '@candeo/effects-api'

const LOW = { r: 40, g: 220, b: 90 }
const HIGH = { r: 255, g: 40, b: 60 }
const BACKGROUND = { r: 4, g: 8, b: 14 }

export default defineEffect({
  description: {
    en: 'The sound playing as bars, by frequency',
    fr: 'Le son joué en barres, par fréquence',
  },
  kinds: ['keyboard'],
  inputs: ['audio'],
  params: {
    bars: {
      kind: 'choice',
      label: { en: 'Bars', fr: 'Barres' },
      options: [
        {
          value: 'columns',
          label: { en: 'Columns: sixteen bands, rising', fr: 'Colonnes : seize bandes, qui montent' },
        },
        {
          value: 'mirror',
          label: {
            en: 'Mirrored columns: low notes in the middle',
            fr: 'Colonnes en miroir : graves au centre',
          },
        },
        {
          value: 'rows',
          label: {
            en: 'Rows: a band per row, growing to the right',
            fr: 'Lignes : une bande par rangée, vers la droite',
          },
        },
      ],
      default: 'columns',
    },
    low: { kind: 'color', label: { en: 'Low level', fr: 'Niveau bas' }, default: LOW },
    high: { kind: 'color', label: { en: 'High level', fr: 'Niveau haut' }, default: HIGH },
    background: { kind: 'color', label: { en: 'Background', fr: 'Fond' }, default: BACKGROUND },
  },
  render({ layout, audio, frame, params }) {
    if (layout.keys.length === 0) return
    const low = params.low ?? LOW
    const high = params.high ?? HIGH
    const background = params.background ?? BACKGROUND

    const area = bounds(layout)
    const rows = Math.max(...layout.keys.map((key) => key.row)) + 1
    const count = audio.bands.length
    // How loud a band reads is set once for everything, in Settings › Sound.
    const strength = (value = 0) => Math.min(1, value)

    if (params.bars === 'rows') {
      // The bands grouped by row, the loudest of each group: a peak reads
      // better than an average on so few bars.
      const grouped = Array.from({ length: rows }, (_, group) => {
        const from = Math.floor((group * count) / rows)
        const to = Math.max(from + 1, Math.floor(((group + 1) * count) / rows))
        return Math.max(...audio.bands.slice(from, to))
      })
      for (const key of layout.keys) {
        const length = strength(grouped[rows - 1 - key.row]) * area.w
        const width = key.w ?? 1
        const start = center(key).x - width / 2 - area.x
        const filled = Math.min(1, Math.max(0, (length - start) / width))
        const shade = mix(low, high, (center(key).x - area.x) / area.w)
        frame.set(key, mix(background, shade, filled))
      }
      return
    }

    const mirrored = params.bars === 'mirror'
    for (const key of layout.keys) {
      // How far across the keyboard, 0 to 1: from the left edge, or mirrored
      // from the middle out to either edge.
      const across = mirrored
        ? Math.abs(center(key).x - (area.x + area.w / 2)) / (area.w / 2)
        : (center(key).x - area.x) / area.w
      const band = Math.min(count - 1, Math.floor(across * count))
      const height = strength(audio.bands[band]) * rows
      // Rows counted from the bottom: the bottom row is 0.
      const fromBottom = rows - 1 - key.row
      const filled = Math.min(1, Math.max(0, height - fromBottom))
      const shade = mix(low, high, rows > 1 ? fromBottom / (rows - 1) : 0)
      frame.set(key, mix(background, shade, filled))
    }
  },
})
