// Aurora — curtains of colour swaying over the keyboard like northern lights,
// moved by the sound playing.
//
// Value noise stretched upright makes the curtains (`inputs: ['audio']`). The
// music moves them, on its rhythm: each beat throws them forward and flashes
// them, and they slow down until the next, so they surge in time with the
// music; between beats they drift at a pace set by the tempo, measured from the
// gaps between the last beats, and by the level. How sensitive to beats it is
// is set once for everything, in Settings › Sound. The bottom rows swell with the bass, a beat sends a
// sheen over them, and their colour slides from the low colour to the high one
// as the music sits lower or higher across the bands.
//
// With nothing playing, which is also how the swatch is sampled, the curtains
// drift slowly and dimly.

import { bounds, center, defineEffect, hsv } from '@candeo/effects-api'

const LOW = { r: 20, g: 255, b: 140 }
const HIGH = { r: 200, g: 60, b: 255 }
/** The lowest bands, below about 180 Hz: where the bass is. */
const BASS_BANDS = 4
/** How far a beat throws the curtains, in curtain widths per second. */
const KICK = 2.5
/** How long a surge takes to die down, in seconds. */
const SURGE = 0.35
/** The tempo the pace is set against: 120 beats a minute, half a second apart. */
const GAP_AT_120 = 0.5
/** Gaps between beats taken as a tempo, 30 to 240 a minute; others are missed or doubled beats. */
const GAP_MIN = 0.25
const GAP_MAX = 2

// A value in [0, 1[ for a point of the integer lattice, the same on every run.
function lattice(x = 0, y = 0, z = 0) {
  const n = Math.sin(x * 127.1 + y * 311.7 + z * 74.7) * 43758.5453
  return n - Math.floor(n)
}

// Smoothstep: the gradient is zero at lattice points, so cells do not show.
function smooth(t = 0) {
  return t * t * (3 - 2 * t)
}

function noise(x = 0, y = 0, z = 0) {
  const x0 = Math.floor(x)
  const y0 = Math.floor(y)
  const z0 = Math.floor(z)
  const fx = smooth(x - x0)
  const fy = smooth(y - y0)
  const fz = smooth(z - z0)
  const plane = (zi = 0) => {
    const top = lattice(x0, y0, zi) + (lattice(x0 + 1, y0, zi) - lattice(x0, y0, zi)) * fx
    const bottom =
      lattice(x0, y0 + 1, zi) + (lattice(x0 + 1, y0 + 1, zi) - lattice(x0, y0 + 1, zi)) * fx
    return top + (bottom - top) * fy
  }
  const near = plane(z0)
  return near + (plane(z0 + 1) - near) * fz
}

/** Where the sound sits, 0 all low to 1 all high: its energy's centre across the bands. */
function toneOf(bands = [0]) {
  const total = bands.reduce((sum, b) => sum + b, 0)
  if (total === 0) return 0.5
  return bands.reduce((sum, b, i) => sum + b * i, 0) / total / (bands.length - 1)
}

/** A colour's hue, in degrees. */
function hueOf(c = { r: 0, g: 0, b: 0 }) {
  const max = Math.max(c.r, c.g, c.b)
  const d = max - Math.min(c.r, c.g, c.b)
  if (d === 0) return 0
  const h = max === c.r ? ((c.g - c.b) / d) % 6 : max === c.g ? (c.b - c.r) / d + 2 : (c.r - c.g) / d + 4
  return (h * 60 + 360) % 360
}

/**
 * Between two hues the short way round the wheel, `t` from 0 to 1. Mixing the
 * colours themselves passes through grey halfway: green and violet made a dull
 * blue-grey.
 */
function hueBetween(from = 0, to = 0, t = 0) {
  const turn = ((to - from + 540) % 360) - 180
  return from + turn * t
}

/** `from` moved toward `to`, as far as `seconds` of easing allows in `dt`. */
function ease(from = 0, to = 0, dt = 0, seconds = 1) {
  return from + (to - from) * (1 - Math.exp(-dt / seconds))
}

/** The middle value: a missed or doubled beat does not move it. */
function median(values = [0]) {
  const sorted = [...values].sort((a, b) => a - b)
  return sorted[Math.floor(sorted.length / 2)]
}

const START = { at: 0, level: 0, bass: 0, tone: 0.5, flow: 0, velocity: 0, sheen: 0, beat: -Infinity }

// What the music did lately, eased; how far and how fast the curtains move; and
// the gaps between the last beats, for the tempo.
let heard = START
let gaps = [GAP_AT_120]

export default defineEffect({
  description: {
    en: 'Curtains of colour swaying like northern lights, moved by the music',
    fr: 'Des voiles de couleur qui ondulent comme une aurore boréale, portés par la musique',
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
      default: 0.04,
    },
    speed: {
      kind: 'number',
      label: { en: 'Speed', fr: 'Vitesse' },
      min: 0.25,
      max: 3,
      step: 0.05,
      default: 1,
    },
    size: {
      kind: 'number',
      label: { en: 'Curtain width (keys)', fr: 'Largeur des voiles (touches)' },
      min: 2,
      max: 12,
      step: 0.5,
      default: 3.5,
    },
  },
  render({ layout, time, audio, frame, params }) {
    if (layout.keys.length === 0) return
    const low = params.low ?? LOW
    const high = params.high ?? HIGH
    const rest = Number(params.rest ?? 0.04)
    const size = Math.max(0.5, Number(params.size ?? 3.5))
    const speed = Number(params.speed ?? 1)
    const lowHue = hueOf(low)
    const highHue = hueOf(high)

    // A time before the last frame is a restart: the preview starts from zero.
    if (time < heard.at) {
      heard = START
      gaps = [GAP_AT_120]
    }
    const dt = time - heard.at
    const bass = Math.max(...audio.bands.slice(0, BASS_BANDS))
    const level = ease(heard.level, audio.level, dt, 0.4)

    const beat = audio.beat
    if (beat) {
      const gap = time - heard.beat
      if (gap >= GAP_MIN && gap <= GAP_MAX) gaps = [...gaps, gap].slice(-8)
    }
    // Faster for a fast song, slower for a slow one, within half and twice.
    const tempo = Math.min(2, Math.max(0.5, GAP_AT_120 / median(gaps)))
    const pace = (0.12 + 0.5 * level) * tempo * speed
    // A beat throws the curtains forward; the surge dies down to the pace.
    const thrown = beat ? heard.velocity + KICK * tempo * speed : heard.velocity
    const velocity = pace + (thrown - pace) * Math.exp(-dt / SURGE)

    heard = {
      at: time,
      level,
      bass: ease(heard.bass, bass, dt, 0.12),
      tone: ease(heard.tone, audio.level > 0 ? toneOf([...audio.bands]) : heard.tone, dt, 1.2),
      flow: heard.flow + velocity * dt,
      velocity,
      sheen: beat ? 1 : heard.sheen * Math.exp(-dt / 0.2),
      beat: beat ? time : heard.beat,
    }

    const area = bounds(layout)
    for (const key of layout.keys) {
      const c = center(key)
      const x = (c.x - area.x) / size
      const y = (c.y - area.y) / Math.max(1, area.h)
      // Stretched upright, the noise makes curtains; a second, finer one
      // shades their colour.
      const drift = noise(x + heard.flow, y * 0.6, heard.flow * 0.3)
      const curtain = smooth(Math.min(1, Math.max(0, (drift - 0.4) / 0.35)))
      const shade = noise(x * 0.7 - heard.flow * 0.5, y, 7 + heard.flow * 0.2)
      const along = Math.min(1, Math.max(0, heard.tone + (shade - 0.5) * 0.7))
      // The bottom rows swell with the bass.
      const swell = heard.bass * Math.max(0, y - 0.3) * 1.2
      const light =
        rest + (1 - rest) * curtain * (0.2 + 0.8 * heard.level) + swell + 0.8 * heard.sheen * curtain
      // Squared: a key's LED gives light in proportion to its value, where a
      // screen darkens low values, so the dark between curtains stays dark on
      // the keyboard as in the preview.
      const shown = Math.min(1, light) ** 2
      frame.set(key, hsv(hueBetween(lowHue, highHue, along), 1, shown))
    }
  },
})
