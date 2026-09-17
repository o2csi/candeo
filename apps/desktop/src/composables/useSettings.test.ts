import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ParamSpec } from '@candeo/effects-api'

import type { Settings } from '../api/candeo'
import { stubWindow } from '../test/window'

const api = vi.hoisted(() => ({
  BRIGHTNESS_DEFAULT: 255,
  getSettings: vi.fn(),
  rememberEffectParams: vi.fn(),
  setEffectParams: vi.fn(),
  setPreviewParams: vi.fn(),
  setBrightness: vi.fn(),
  rememberBrightness: vi.fn(),
}))
vi.mock('../api/candeo', () => api)

const keyboard = { vid: 1, pid: 2 }
const other = { vid: 1, pid: 3 }

const specs: Record<string, ParamSpec> = {
  speed: { kind: 'number', label: 'Speed', min: 0, max: 2, default: 1 },
  color: { kind: 'color', label: 'Color', default: { r: 255, g: 0, b: 0 } },
  mode: { kind: 'choice', label: 'Mode', options: ['calm', 'wild'], default: 'calm' },
  reverse: { kind: 'boolean', label: 'Reverse', default: false },
}

const defaults = { speed: 1, color: { r: 255, g: 0, b: 0 }, mode: 'calm', reverse: false }

function settings(part: Partial<Settings> = {}): Settings {
  return {
    version: 2,
    shippedEffects: {},
    preferences: {},
    devices: [],
    activeEffects: [],
    effectParams: [],
    rules: [],
    ...part,
  }
}

/** A promise settled from outside, to hold a call in flight. */
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((r) => (resolve = r))
  return { promise, resolve }
}

/** The module keeps its state between calls: each test starts from a fresh one. */
async function fresh(part: Partial<Settings> = {}) {
  api.getSettings.mockResolvedValue(settings(part))
  const { useSettings } = await import('./useSettings')
  const s = useSettings()
  await s.load()
  return s
}

beforeEach(() => {
  vi.resetModules()
  vi.useFakeTimers()
  stubWindow()
  for (const fn of Object.values(api)) if (typeof fn === 'function') fn.mockReset()
  api.rememberEffectParams.mockResolvedValue(undefined)
  api.setEffectParams.mockResolvedValue(undefined)
  api.setPreviewParams.mockResolvedValue(undefined)
  api.setBrightness.mockResolvedValue(undefined)
  api.rememberBrightness.mockResolvedValue(undefined)
})

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

describe('values', () => {
  it('lays what is remembered over what the effect declares', async () => {
    const s = await fresh({
      effectParams: [{ ...keyboard, effect: 'Wave', values: { speed: 1.5 } }],
    })

    expect(s.valuesFor(keyboard, 'Wave', specs)).toEqual({ ...defaults, speed: 1.5 })
    expect(s.keptFor(keyboard, 'Wave')).toBe(true)
    expect(s.valuesFor(other, 'Wave', specs)).toEqual(defaults)
    expect(s.valuesFor(null, 'Wave', specs)).toEqual(defaults)
    expect(s.keptFor(null, 'Wave')).toBe(false)
  })

  it('falls back to the default for values that no longer fit, and drops undeclared ones', async () => {
    const s = await fresh({
      effectParams: [
        {
          ...keyboard,
          effect: 'Wave',
          // The file is edited by hand, or written for an older version of the effect.
          values: JSON.parse(
            '{"speed": "fast", "color": {"r": 1, "g": 2}, "mode": "gone", "reverse": 1, "removed": 3}',
          ),
        },
      ],
    })

    expect(s.valuesFor(keyboard, 'Wave', specs)).toEqual(defaults)
  })
})

describe('adjust', () => {
  it('leaves the device alone when the effect is only previewed', async () => {
    const s = await fresh()

    const values = s.adjust(keyboard, 'Wave', specs, 'speed', 1.5, false)

    expect(values).toEqual({ ...defaults, speed: 1.5 })
    expect(s.valuesFor(keyboard, 'Wave', specs)).toEqual(values)
    await vi.runAllTimersAsync()
    expect(api.setEffectParams).not.toHaveBeenCalled()
    // Still remembered for the pair: it applies at the next start.
    expect(api.rememberEffectParams).toHaveBeenCalledWith(keyboard, 'Wave', { speed: 1.5 })
  })

  it('sends every value to the loop running the effect', async () => {
    const s = await fresh()

    s.adjust(keyboard, 'Wave', specs, 'mode', 'wild', true)

    expect(api.setEffectParams).toHaveBeenCalledWith(keyboard, { ...defaults, mode: 'wild' })
  })

  it('sends at most once per 40 ms, and the last move always goes out', async () => {
    const s = await fresh()

    s.adjust(keyboard, 'Wave', specs, 'speed', 1.1, true)
    s.adjust(keyboard, 'Wave', specs, 'speed', 1.2, true)
    s.adjust(keyboard, 'Wave', specs, 'speed', 1.3, true)
    expect(api.setEffectParams).toHaveBeenCalledTimes(1)

    await vi.advanceTimersByTimeAsync(40)

    expect(api.setEffectParams.mock.calls.map(([, p]) => p.speed)).toEqual([1.1, 1.3])
  })

  it('does not send while the previous update is still in flight', async () => {
    const s = await fresh()
    const flight = deferred<void>()
    api.setEffectParams.mockReturnValueOnce(flight.promise)

    s.adjust(keyboard, 'Wave', specs, 'speed', 1.1, true)
    s.adjust(keyboard, 'Wave', specs, 'speed', 1.2, true)
    await vi.advanceTimersByTimeAsync(200)
    expect(api.setEffectParams).toHaveBeenCalledTimes(1)

    flight.resolve()
    await vi.advanceTimersByTimeAsync(0)
    expect(api.setEffectParams).toHaveBeenLastCalledWith(keyboard, { ...defaults, speed: 1.2 })
  })

  it('keeps one sender per loop', async () => {
    const s = await fresh()

    s.adjust(keyboard, 'Wave', specs, 'speed', 1.1, true)
    s.adjust(other, 'Wave', specs, 'speed', 1.2, true)

    expect(api.setEffectParams).toHaveBeenCalledTimes(2)
  })

  it('writes only what differs from the declaration, once the control rests', async () => {
    const s = await fresh()

    s.adjust(keyboard, 'Wave', specs, 'speed', 1.5, false)
    s.adjust(keyboard, 'Wave', specs, 'reverse', true, false)
    s.adjust(keyboard, 'Wave', specs, 'speed', 1, false)
    await vi.advanceTimersByTimeAsync(599)
    expect(api.rememberEffectParams).not.toHaveBeenCalled()

    await vi.advanceTimersByTimeAsync(1)
    expect(api.rememberEffectParams).toHaveBeenCalledTimes(1)
    expect(api.rememberEffectParams).toHaveBeenCalledWith(keyboard, 'Wave', { reverse: true })
  })

  it('says what failed, until the next attempt', async () => {
    const s = await fresh()
    api.setEffectParams.mockRejectedValueOnce('device unplugged')

    s.adjust(keyboard, 'Wave', specs, 'speed', 1.1, true)
    await vi.advanceTimersByTimeAsync(0)
    expect(s.error.value).toBe('device unplugged')

    s.adjust(keyboard, 'Wave', specs, 'speed', 1.2, false)
    expect(s.error.value).toBeNull()
  })
})

describe('adjustPreview', () => {
  it('goes to the preview loop, at the same pace, and never to disk', async () => {
    const s = await fresh()

    s.adjustPreview({ speed: 1.1 })
    s.adjustPreview({ speed: 1.2 })
    await vi.runAllTimersAsync()

    expect(api.setPreviewParams.mock.calls).toEqual([[{ speed: 1.1 }], [{ speed: 1.2 }]])
    expect(api.rememberEffectParams).not.toHaveBeenCalled()
  })
})

describe('settle', () => {
  it('writes at the end of the gesture, without waiting for the rest', async () => {
    const s = await fresh()

    s.adjust(keyboard, 'Wave', specs, 'speed', 1.5, false)
    s.settle(keyboard, 'Wave')

    expect(api.rememberEffectParams).toHaveBeenCalledWith(keyboard, 'Wave', { speed: 1.5 })
    await vi.runAllTimersAsync()
    expect(api.rememberEffectParams).toHaveBeenCalledTimes(1)
  })

  it('writes gestures repeated within 250 ms once, with the last state', async () => {
    const s = await fresh()

    s.adjust(keyboard, 'Wave', specs, 'speed', 1.1, false)
    s.settle(keyboard, 'Wave')
    s.adjust(keyboard, 'Wave', specs, 'speed', 1.2, false)
    s.settle(keyboard, 'Wave')
    s.adjust(keyboard, 'Wave', specs, 'speed', 1.3, false)
    s.settle(keyboard, 'Wave')
    expect(api.rememberEffectParams).toHaveBeenCalledTimes(1)

    await vi.runAllTimersAsync()
    expect(api.rememberEffectParams.mock.calls.map(([, , v]) => v.speed)).toEqual([1.1, 1.3])
  })

  it('flush writes everything pending', async () => {
    const s = await fresh()

    s.adjust(keyboard, 'Wave', specs, 'speed', 1.1, false)
    s.adjust(other, 'Rain', specs, 'speed', 1.2, false)
    s.flush()

    expect(api.rememberEffectParams).toHaveBeenCalledTimes(2)
  })
})

describe('reload', () => {
  const onDisk = { effectParams: [{ ...keyboard, effect: 'Wave', values: { speed: 1.2 } }] }

  it('keeps a value the window has not written yet', async () => {
    const s = await fresh(onDisk)

    s.adjust(keyboard, 'Wave', specs, 'speed', 1.8, false)
    await s.reload()

    expect(s.valuesFor(keyboard, 'Wave', specs).speed).toBe(1.8)
  })

  it('keeps a value whose write is on its way', async () => {
    const s = await fresh(onDisk)
    api.rememberEffectParams.mockReturnValueOnce(deferred<void>().promise)

    s.adjust(keyboard, 'Wave', specs, 'speed', 1.8, false)
    s.settle(keyboard, 'Wave')
    await s.reload()

    expect(s.valuesFor(keyboard, 'Wave', specs).speed).toBe(1.8)
  })

  it('takes the file once the write landed', async () => {
    const s = await fresh(onDisk)

    s.adjust(keyboard, 'Wave', specs, 'speed', 1.8, false)
    s.settle(keyboard, 'Wave')
    await vi.advanceTimersByTimeAsync(0)
    api.getSettings.mockResolvedValue(
      settings({ effectParams: [{ ...keyboard, effect: 'Wave', values: { speed: 0.4 } }] }),
    )
    await s.reload()

    expect(s.valuesFor(keyboard, 'Wave', specs).speed).toBe(0.4)
  })

  it('ignores an older read that comes back last', async () => {
    const s = await fresh()
    const older = deferred<Settings>()
    const newer = deferred<Settings>()
    api.getSettings.mockReturnValueOnce(older.promise).mockReturnValueOnce(newer.promise)

    const first = s.reload()
    const second = s.reload()
    newer.resolve(settings({ effectParams: [{ ...keyboard, effect: 'Wave', values: { speed: 1.5 } }] }))
    await second
    older.resolve(settings({ effectParams: [{ ...keyboard, effect: 'Wave', values: { speed: 0.2 } }] }))
    await first

    expect(s.valuesFor(keyboard, 'Wave', specs).speed).toBe(1.5)
  })

  it('reads once for screens that load together, and not again after', async () => {
    const s = await fresh()

    await Promise.all([s.load(), s.load()])

    expect(api.getSettings).toHaveBeenCalledTimes(1)
  })

  it('reads again after a failed read', async () => {
    api.getSettings.mockRejectedValueOnce('settings.json unreadable')
    const { useSettings } = await import('./useSettings')
    const s = useSettings()

    await s.load()
    expect(s.error.value).toBe('settings.json unreadable')

    api.getSettings.mockResolvedValue(settings())
    await s.load()
    expect(api.getSettings).toHaveBeenCalledTimes(2)
  })
})

describe('forget', () => {
  it('writes an empty table at once and hands back the declared values', async () => {
    const s = await fresh({
      effectParams: [{ ...keyboard, effect: 'Wave', values: { speed: 1.5 } }],
    })

    s.adjust(keyboard, 'Wave', specs, 'speed', 1.7, false)
    s.settle(keyboard, 'Wave')
    const values = s.forget(keyboard, 'Wave', specs, false)

    expect(values).toEqual(defaults)
    expect(api.rememberEffectParams).toHaveBeenLastCalledWith(keyboard, 'Wave', {})
    expect(s.keptFor(keyboard, 'Wave')).toBe(false)
    expect(api.setEffectParams).not.toHaveBeenCalled()
  })

  it('resets the loop only when the effect runs on the device', async () => {
    const s = await fresh()

    s.forget(keyboard, 'Wave', specs, true)

    expect(api.setEffectParams).toHaveBeenCalledWith(keyboard, defaults)
  })
})

describe('dropEffect', () => {
  it('cancels the pending writes of that effect only, and forgets it everywhere', async () => {
    const s = await fresh({ activeEffects: [{ ...keyboard, effect: 'Wave' }] })

    s.adjust(keyboard, 'Wave', specs, 'speed', 1.1, false)
    s.adjust(other, 'Wave', specs, 'speed', 1.2, false)
    s.adjust(keyboard, 'Rain', specs, 'speed', 1.3, false)
    s.dropEffect('Wave')
    await vi.runAllTimersAsync()

    expect(api.rememberEffectParams.mock.calls).toEqual([[keyboard, 'Rain', { speed: 1.3 }]])
    expect(s.lastAppliedOn(keyboard)).toBeNull()
    expect([...s.referencedEffects.value]).toEqual(['Rain'])
  })
})

describe('dropAll', () => {
  it('forgets settings, applied effects and brightness, and writes nothing back', async () => {
    const s = await fresh({
      devices: [{ ...keyboard, state: 'adopted', brightness: 80 }],
      activeEffects: [{ ...keyboard, effect: 'Wave' }],
    })

    s.adjust(keyboard, 'Wave', specs, 'speed', 1.1, false)
    s.dropAll()
    await vi.runAllTimersAsync()

    expect(api.rememberEffectParams).not.toHaveBeenCalled()
    expect(s.keptFor(keyboard, 'Wave')).toBe(false)
    expect(s.lastAppliedOn(keyboard)).toBeNull()
    expect(s.brightnessOf(keyboard)).toBe(255)
  })
})

describe('brightness', () => {
  it('reads each device level, the default when none is kept', async () => {
    const s = await fresh({ devices: [{ ...keyboard, state: 'adopted', brightness: 80 }] })

    expect(s.brightnessOf(keyboard)).toBe(80)
    expect(s.brightnessOf(other)).toBe(255)
    expect(s.brightnessOf(null)).toBe(255)
  })

  it('sends every move and writes only at the end of the gesture', async () => {
    const s = await fresh()

    s.setBrightness(keyboard, 120, false)
    s.setBrightness(keyboard, 100, true)

    expect(api.setBrightness.mock.calls).toEqual([
      [keyboard, 120],
      [keyboard, 100],
    ])
    expect(api.rememberBrightness.mock.calls).toEqual([[keyboard, 100]])
    expect(s.brightnessOf(keyboard)).toBe(100)
  })

  it('remembers the level even when the device cannot be written', async () => {
    const s = await fresh()
    api.setBrightness.mockRejectedValueOnce('device not open')

    s.setBrightness(keyboard, 100, true)
    await vi.advanceTimersByTimeAsync(0)

    expect(s.brightnessOf(keyboard)).toBe(100)
    expect(api.rememberBrightness).toHaveBeenCalledWith(keyboard, 100)
    expect(s.error.value).toBe('device not open')
  })
})

describe('referencedEffects', () => {
  it('names every effect applied or tuned on any device', async () => {
    const s = await fresh({
      activeEffects: [{ ...keyboard, effect: 'Radial wave' }],
      effectParams: [
        { ...keyboard, effect: 'Rain', values: { speed: 1.5 } },
        { ...other, effect: 'Radial wave', values: { speed: 0.5 } },
      ],
    })

    expect([...s.referencedEffects.value].sort()).toEqual(['Radial wave', 'Rain'])
  })
})
