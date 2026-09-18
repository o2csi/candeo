import { describe, expect, it } from 'vitest'

import type { Rgb } from '../api/types'
import { illustrate, illustrates } from './illustration'

/** Twenty columns of one row: enough for anything that travels sideways. */
const COLS = 20
const LEN = 20

const frame = (id: string, seconds: number) => illustrate(id, seconds, COLS, LEN)

describe('illustrating a firmware effect', () => {
  it('covers every cell of the matrix, gaps included', () => {
    expect(frame('hardware:m18-01', 0)).toHaveLength(LEN)
  })

  it('holds one colour everywhere, for the steady one', () => {
    const still = frame('hardware:m18-01', 3.7)
    expect(new Set(still.map((c) => c.join()))).toEqual(new Set(['255,0,0']))
  })

  it('throbs between full and dark, for the pulse', () => {
    expect(frame('hardware:m18-02', 0)[0]).toEqual([255, 0, 0])
    expect(frame('hardware:m18-02', 1)[0]).toEqual([0, 0, 0])
  })

  it('spreads hues across the keys, for the wave', () => {
    const wave = frame('hardware:m18-03', 0)
    expect(wave[0]).not.toEqual(wave[COLS - 1])
  })

  /** What tells the fade from the breath: one goes through black, the other not. */
  it('goes through black for the breath, and never for the fade', () => {
    const breath = Array.from({ length: 40 }, (_, i) => frame('hardware:m18-08', i / 4)[0])
    const fade = Array.from({ length: 40 }, (_, i) => frame('hardware:m18-09', i / 4)[0])
    const dark = (c: readonly number[]) => c.every((v) => v === 0)
    expect(breath.some(dark)).toBe(true)
    expect(fade.some(dark)).toBe(false)
  })

  it('moves a lit band along the keys, for the sweep', () => {
    const start = frame('hardware:m18-0a', 0)
    const later = frame('hardware:m18-0a', 0.75)
    const brightest = (f: readonly Rgb[]) => f.indexOf(f.reduce((a, b) => (a[0] > b[0] ? a : b)))
    expect(brightest(later)).toBeGreaterThan(brightest(start))
  })

  it('lights the whole surface in one hue at a time, for the spectrum', () => {
    const spectrum = frame('hardware:m18-0e', 1.2)
    expect(new Set(spectrum.map((c) => c.join())).size).toBe(1)
    expect(spectrum[0]).not.toEqual(frame('hardware:m18-0e', 2.4)[0])
  })

  it('draws a dark keyboard for off, and for anything it does not know', () => {
    expect(frame('hardware:off', 2)[0]).toEqual([0, 0, 0])
    expect(frame('shipped:Clock', 2)[0]).toEqual([0, 0, 0])
  })

  it('says which ids it draws, so that nothing else claims an illustration', () => {
    expect(illustrates('hardware:m18-0a')).toBe(true)
    expect(illustrates('hardware:m18-00')).toBe(false)
    expect(illustrates('shipped:Clock')).toBe(false)
  })
})
