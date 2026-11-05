import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest'
import type { ParamSpec } from '@candeo/effects-api'

import type { Bindings, EffectParams } from '../api/candeo'
import { stubWindow } from '../test/window'

// `presets.ts` reads values the way `useSettings` does, and that module talks
// to Rust and listens to the window as it loads: neither is wanted here.
vi.mock('../api/candeo', () => ({ logFromWebview: vi.fn() }))

let presets: typeof import('./presets')

beforeAll(async () => {
  stubWindow()
  presets = await import('./presets')
})

afterAll(() => {
  vi.unstubAllGlobals()
})

const specs: Record<string, ParamSpec> = {
  speed: { kind: 'number', label: 'Speed', min: 0, max: 2, default: 1 },
  colour: { kind: 'color', label: 'Colour', default: { r: 255, g: 0, b: 0 } },
  mode: { kind: 'choice', label: 'Mode', options: ['calm', 'wild'], default: 'calm' },
}

const defaults = { speed: 1, colour: { r: 255, g: 0, b: 0 }, mode: 'calm' }

describe('presetSettings', () => {
  it('gives the preset its values, and its bindings', () => {
    const preset = {
      values: { speed: 0.4, colour: { r: 0, g: 80, b: 255 }, mode: 'wild' },
      bindings: { speed: 'sound:volume' },
    }
    expect(presets.presetSettings(specs, preset)).toEqual({
      values: preset.values,
      bindings: { speed: 'sound:volume' },
    })
  })

  it('reads a preset saved for an older version of the effect as kept settings are read', () => {
    const preset = {
      // `rings` is gone, `mode` lost the option, `colour` was added since.
      values: JSON.parse('{"speed": 0.4, "rings": 3, "mode": "storm"}'),
      bindings: { rings: 'signal:count', speed: 'signal:tempo', mode: 'sound:beat' },
    }
    expect(presets.presetSettings(specs, preset)).toEqual({
      values: { ...defaults, speed: 0.4 },
      // A choice cannot follow the sound; a parameter no longer declared reads nothing.
      bindings: { speed: 'signal:tempo' },
    })
  })

  it('takes a preset without bindings as binding nothing', () => {
    expect(presets.presetSettings(specs, { values: {} })).toEqual({
      values: defaults,
      bindings: {},
    })
  })
})

describe('holds', () => {
  const preset = {
    values: { speed: 0.4, colour: { r: 0, g: 80, b: 255 }, mode: 'calm' },
    bindings: { speed: 'sound:volume' },
  }

  /** The settings shown: the preset's, with these values and bindings changed. */
  const shown = (values: EffectParams = {}, bindings: Bindings = preset.bindings) => ({
    values: { ...preset.values, ...values },
    bindings,
  })

  it('marks the preset whose values and bindings the settings are', () => {
    expect(presets.holds(specs, shown(), preset)).toBe(true)
    // A colour is compared by its channels, not by identity.
    expect(presets.holds(specs, shown({ colour: { r: 0, g: 80, b: 255 } }), preset)).toBe(true)
  })

  it('lets go of it once a value moves, a bound one included', () => {
    expect(presets.holds(specs, shown({ mode: 'wild' }), preset)).toBe(false)
    // The value a bound parameter keeps is what shows while its source is absent.
    expect(presets.holds(specs, shown({ speed: 0.5 }), preset)).toBe(false)
  })

  it('lets go of it once a binding changes', () => {
    expect(presets.holds(specs, shown({}, {}), preset)).toBe(false)
    expect(presets.holds(specs, shown({}, { speed: 'sound:bass' }), preset)).toBe(false)
    const more = { speed: 'sound:volume', mode: 'signal:mode' }
    expect(presets.holds(specs, shown({}, more), preset)).toBe(false)
  })

  it('reads both sides against what the effect declares', () => {
    const older = { values: JSON.parse('{"speed": 0.4, "rings": 3}') }
    const settled = { values: { ...defaults, speed: 0.4 }, bindings: {} }
    expect(presets.holds(specs, settled, older)).toBe(true)
    expect(presets.holds(specs, { values: { speed: 0.4 }, bindings: {} }, older)).toBe(true)
  })
})

describe('presetName', () => {
  it('is what Rust keeps: trimmed', () => {
    expect(presets.presetName('  Slow blue ')).toBe('Slow blue')
    expect(presets.presetName('   ')).toBe('')
  })
})
