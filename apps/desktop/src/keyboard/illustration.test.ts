import { describe, expect, it } from 'vitest'

import type { Rgb } from '../api/types'
import { illustrate, isLook, LOOKS } from './illustration'

/** Twenty columns of one row: enough for anything that travels sideways. */
const COLS = 20
const LEN = 20

const frame = (looks: string, seconds: number) => illustrate(looks, seconds, COLS, LEN)

/** The schema definitions are checked against, as the repository holds it. */
const SCHEMA = import.meta.glob<string>(
  '../../../../crates/candeo-device/devices/device-definition.schema.json',
  { query: '?raw', import: 'default', eager: true },
)

describe('illustrating a firmware effect', () => {
  it('covers every cell of the matrix, gaps included', () => {
    expect(frame('steady', 0)).toHaveLength(LEN)
  })

  it('holds one colour everywhere, for the steady one', () => {
    const still = frame('steady', 3.7)
    expect(new Set(still.map((c) => c.join()))).toEqual(new Set(['255,0,0']))
  })

  it('throbs between full and dark, for the pulse', () => {
    expect(frame('pulse', 0)[0]).toEqual([255, 0, 0])
    expect(frame('pulse', 1)[0]).toEqual([0, 0, 0])
  })

  it('spreads hues across the keys, for the wave', () => {
    const wave = frame('wave', 0)
    expect(wave[0]).not.toEqual(wave[COLS - 1])
  })

  /** What tells the breath from the morph: one goes through black, the other not. */
  it('goes through black for the breath, and never for the morph', () => {
    const breath = Array.from({ length: 40 }, (_, i) => frame('breathing', i / 4)[0])
    const morph = Array.from({ length: 40 }, (_, i) => frame('morph', i / 4)[0])
    const dark = (c: readonly number[]) => c.every((v) => v === 0)
    expect(breath.some(dark)).toBe(true)
    expect(morph.some(dark)).toBe(false)
  })

  it('breathes each colour out of black and back, changing colour in the dark', () => {
    const at = (seconds: number) => illustrate('breathing', seconds, COLS, LEN, [])[0]
    expect(at(0)).toEqual([0, 0, 0])
    expect(at(1)).toEqual([255, 0, 0])
    expect(at(2)).toEqual([0, 0, 0])
    expect(at(3)).toEqual([0, 0, 255])
    const green: Rgb = [0, 255, 0]
    expect(illustrate('breathing', 3, COLS, LEN, [green])[0]).toEqual(green)
  })

  it('moves a lit band along the keys, for the scanner', () => {
    const start = frame('scanner', 0)
    const later = frame('scanner', 0.75)
    const brightest = (f: readonly Rgb[]) => f.indexOf(f.reduce((a, b) => (a[0] > b[0] ? a : b)))
    expect(brightest(later)).toBeGreaterThan(brightest(start))
  })

  it('lights the whole surface in one hue at a time, for the spectrum', () => {
    const spectrum = frame('spectrum', 1.2)
    expect(new Set(spectrum.map((c) => c.join())).size).toBe(1)
    expect(spectrum[0]).not.toEqual(frame('spectrum', 2.4)[0])
  })

  it('draws a dark keyboard for off, and for a word it does not know', () => {
    expect(frame('off', 2)[0]).toEqual([0, 0, 0])
    expect(frame('sparkle', 2)[0]).toEqual([0, 0, 0])
  })

  it('says which words it draws, so that nothing else claims an illustration', () => {
    expect(isLook('scanner')).toBe(true)
    expect(isLook('sparkle')).toBe(false)
    expect(isLook(null)).toBe(false)
  })

  it('draws every pattern a definition may name, and no other', () => {
    const [schema] = Object.values(SCHEMA)
    const text = JSON.parse(schema)
    const effect = text.properties.firmware.properties.effects.items.properties
    expect([...effect.looks.enum].sort()).toEqual(LOOKS.filter((l) => l !== 'off').sort())
  })
})
