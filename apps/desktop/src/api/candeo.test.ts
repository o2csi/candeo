import { beforeEach, describe, expect, it, vi } from 'vitest'

// The one module that calls Tauri: what it sends is what Rust reads.
const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke, Channel: class {} }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }))

import {
  rememberDimming,
  rememberEffectBindings,
  rememberEffectParams,
  setDimming,
  setEffectBindings,
  setEffectParams,
  setSplit,
  startEffect,
  startPreview,
  stopEffect,
} from './candeo'

const device = { vid: 0x187c, pid: 0x0550 }
const lid = { ...device, region: 'lid' }

beforeEach(() => {
  invoke.mockReset()
  invoke.mockResolvedValue(undefined)
})

describe('a command naming a region', () => {
  it('sends the device alone and the region beside it', async () => {
    await startEffect(lid, 'shipped:Rain', { speed: 2 }, { hue: 'sound:tone' })
    expect(invoke).toHaveBeenCalledWith('start_effect', {
      device,
      region: 'lid',
      id: 'shipped:Rain',
      params: { speed: 2 },
      bindings: { hue: 'sound:tone' },
    })
  })

  it('sends no region for the whole device, whatever else the object carries', async () => {
    await stopEffect({ ...device, name: 'Alienware 17 R5', split: false } as typeof device)
    expect(invoke).toHaveBeenCalledWith('stop_effect', { device, region: null })
  })

  it('names the region in every command that takes one', async () => {
    await setEffectParams(lid, { speed: 1 })
    await setEffectBindings(lid, {})
    await rememberEffectParams(lid, 'shipped:Rain', { speed: 1 })
    await rememberEffectBindings(lid, 'shipped:Rain', {})
    await setDimming(lid, null)
    await rememberDimming(lid, { source: 'sound:bass', floor: 20 })
    for (const [, args] of invoke.mock.calls) {
      expect(args).toMatchObject({ device, region: 'lid' })
    }
    expect(invoke).toHaveBeenCalledTimes(6)
  })

  it('previews a region, a whole device, or the default layout', async () => {
    await startPreview(lid, 'shipped:Rain')
    await startPreview(device, 'shipped:Rain')
    await startPreview(null, 'shipped:Rain')
    const sent = invoke.mock.calls.map(([, args]) => [args.device, args.region])
    expect(sent).toEqual([
      [device, 'lid'],
      [device, null],
      [null, null],
    ])
  })

  it('splits a device by its model', async () => {
    await setSplit(device, true)
    expect(invoke).toHaveBeenCalledWith('set_split', { device, split: true })
  })
})
