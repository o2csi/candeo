import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { EffectEntry } from '../api/candeo'

const api = vi.hoisted(() => ({
  listEffects: vi.fn(),
  readEffectSource: vi.fn(),
  cacheEffect: vi.fn(),
}))
vi.mock('../api/candeo', () => api)

const transpile = vi.hoisted(() => vi.fn())
vi.mock('./effect', () => ({ transpile }))

const alerte = vi.hoisted(() => vi.fn())
vi.mock('../api/journal', () => ({ alerte, message: String }))

import { refreshLibrary } from './library'

function entry(id: string, state: EffectEntry['state'], hash?: string): EffectEntry {
  return { id, name: id, kind: 'user', state, hash, swatch: [], apiVersion: 1 }
}

beforeEach(() => {
  vi.resetAllMocks()
  api.readEffectSource.mockImplementation(async (id: string) => `source of ${id}`)
  transpile.mockImplementation(async (source: string) => `js of ${source}`)
  api.cacheEffect.mockImplementation(async (id: string) => entry(id, 'ready', 'recorded'))
})

describe('refreshLibrary', () => {
  it('compiles the stale files only, and lists what was recorded in their place', async () => {
    api.listEffects.mockResolvedValue([
      entry('Rain', 'ready', 'a'),
      entry('Wave', 'stale', 'b'),
      entry('Broken', 'broken', 'c'),
    ])

    const list = await refreshLibrary()

    expect(api.cacheEffect.mock.calls).toEqual([['Wave', 'b', 'js of source of Wave']])
    expect(list.map((e) => `${e.id}:${e.state}`)).toEqual(['Rain:ready', 'Wave:ready', 'Broken:broken'])
  })

  it('keeps compiling the others when one file fails', async () => {
    api.listEffects.mockResolvedValue([entry('Gone', 'stale', 'a'), entry('Wave', 'stale', 'b')])
    api.readEffectSource.mockRejectedValueOnce('file not found')

    const list = await refreshLibrary()

    expect(list.map((e) => `${e.id}:${e.state}`)).toEqual(['Gone:stale', 'Wave:ready'])
    expect(alerte).toHaveBeenCalledWith('library', 'Gone: not compiled: file not found', 'file not found')
  })

  it('shares a pass between calls made while it runs', async () => {
    api.listEffects.mockResolvedValue([entry('Wave', 'stale', 'b')])

    const [first, second] = await Promise.all([refreshLibrary(), refreshLibrary()])

    expect(first).toBe(second)
    expect(api.listEffects).toHaveBeenCalledTimes(1)
    expect(api.cacheEffect).toHaveBeenCalledTimes(1)

    await refreshLibrary()
    expect(api.listEffects).toHaveBeenCalledTimes(2)
  })

  it('lets the next call try again after a failed listing', async () => {
    api.listEffects.mockRejectedValueOnce('folder unreadable').mockResolvedValue([])

    await expect(refreshLibrary()).rejects.toBe('folder unreadable')
    await expect(refreshLibrary()).resolves.toEqual([])
  })
})
