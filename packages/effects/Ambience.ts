// Ambience — the keyboard breathes with the bass of the sound playing, its
// colour drifting with where the music sits, low or high.
//
// A quiet background for working with music on (`inputs: ['audio']`). The
// brightness follows the lowest bands, eased so it swells rather than blinks;
// the colour moves from the low colour to the high one as the music's energy
// sits lower or higher across the bands, eased more slowly still. A slow wave
// across the keys keeps it alive when the music holds.
//
// With nothing playing, which is also how the swatch is sampled, it rests dimly
// halfway between its two colours.

import { bounds, center, defineEffect, mix } from '@candeo/effects-api'

const LOW = { r: 255, g: 90, b: 20 }
const HIGH = { r: 90, g: 60, b: 255 }
const BLACK = { r: 0, g: 0, b: 0 }
/** The lowest bands, below about 180 Hz: where the bass is. */
const BASS_BANDS = 4

// Eased values, kept from one frame to the next.
let eased = { at: 0, bass: 0, tone: 0.5 }

/** Where the sound sits, 0 all low to 1 all high: its energy's centre across the bands. */
function toneOf(bands = [0]) {
  const total = bands.reduce((sum, b) => sum + b, 0)
  if (total === 0) return 0.5
  return bands.reduce((sum, b, i) => sum + b * i, 0) / total / (bands.length - 1)
}

/** `from` moved toward `to`, as far as `seconds` of easing allows in `dt`. */
function ease(from = 0, to = 0, dt = 0, seconds = 1) {
  return from + (to - from) * (1 - Math.exp(-dt / seconds))
}

export default defineEffect({
  description: {
    en: 'The keyboard breathing with the bass, its colour drifting with the music, low or high',
    fr: 'Le clavier qui respire avec les graves, sa couleur glissant avec la musique, grave ou aiguë',
  },
  kinds: ['keyboard'],
  inputs: ['audio'],
  params: {
    low: { kind: 'color', label: { en: 'Low music', fr: 'Musique grave' }, default: LOW },
    high: { kind: 'color', label: { en: 'High music', fr: 'Musique aiguë' }, default: HIGH },
    rest: {
      kind: 'number',
      label: { en: 'Brightness at rest', fr: 'Luminosité au repos' },
      min: 0,
      max: 0.5,
      step: 0.05,
      default: 0.15,
    },
  },
  render({ layout, time, audio, frame, params }) {
    if (layout.keys.length === 0) return
    const low = params.low ?? LOW
    const high = params.high ?? HIGH
    const rest = Number(params.rest ?? 0.15)

    // A time before the last frame is a restart: the preview starts from zero.
    if (time < eased.at) eased = { at: 0, bass: 0, tone: 0.5 }
    const dt = time - eased.at
    const bass = Math.max(...audio.bands.slice(0, BASS_BANDS))
    const tone = audio.level > 0 ? toneOf([...audio.bands]) : eased.tone
    eased = {
      at: time,
      bass: ease(eased.bass, bass, dt, 0.15),
      tone: ease(eased.tone, tone, dt, 1.5),
    }

    const color = mix(low, high, eased.tone)
    const area = bounds(layout)
    for (const key of layout.keys) {
      const across = (center(key).x - area.x) / Math.max(1, area.w)
      const wave = 0.9 + 0.1 * Math.sin(time * 0.8 - across * Math.PI * 2)
      const brightness = Math.min(1, (rest + (1 - rest) * eased.bass) * wave)
      frame.set(key, mix(BLACK, color, brightness))
    }
  },
})
