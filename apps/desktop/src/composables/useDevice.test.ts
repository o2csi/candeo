import { beforeEach, describe, expect, it, vi } from 'vitest'

import type { DeviceInfo, LayoutInfo } from '../api/types'

const api = vi.hoisted(() => ({
  listDevices: vi.fn(),
  getLayout: vi.fn(),
  adoptDevice: vi.fn(),
}))
vi.mock('../api/candeo', () => api)

function device(pid: number, part: Partial<DeviceInfo> = {}): DeviceInfo {
  return {
    name: `Keyboard ${pid}`,
    vid: 1,
    pid,
    present: false,
    state: 'detected',
    open: false,
    error: null,
    surveyedFirmware: 'v1.5',
    firmware: null,
    warnings: [],
    origin: 'builtIn',
    file: null,
    definitions: [],
    unloadedChoice: null,
    unverified: false,
    lights: 'keys',
    lightCount: 1,
    hardwareBrightness: true,
    capabilities: ['matrix', 'geometry'],
    regions: [],
    split: false,
    ...part,
  }
}

const layoutOf = (pid: number): LayoutInfo => ({
  name: `layout ${pid}`,
  rows: 1,
  cols: 1,
  frameLen: 1,
  keys: [],
  firmwareEffects: [],
  lights: 'keys',
  capabilities: ['matrix', 'geometry'],
  outline: [],
  hardwareBrightness: true,
  regions: [],
})

/** Module state: each test starts from a fresh one. */
async function fresh(list: DeviceInfo[]) {
  api.listDevices.mockResolvedValue(list)
  const { useDevice } = await import('./useDevice')
  const d = useDevice()
  await d.refresh()
  return d
}

beforeEach(() => {
  vi.resetModules()
  vi.resetAllMocks()
  api.getLayout.mockImplementation(async (d: { pid: number }) => layoutOf(d.pid))
})

describe('current', () => {
  it('is the open device, then the first plugged in, then the first known', async () => {
    const open = await fresh([device(1), device(2, { present: true }), device(3, { open: true, present: true })])
    expect(open.current.value).toEqual({ vid: 1, pid: 3 })

    const plugged = await fresh([device(1), device(2, { present: true })])
    expect(plugged.current.value).toEqual({ vid: 1, pid: 2 })

    const known = await fresh([device(1), device(2)])
    expect(known.current.value).toEqual({ vid: 1, pid: 1 })

    const none = await fresh([])
    expect(none.current.value).toBeNull()
  })

  it('is the selected device while the list still has it', async () => {
    const d = await fresh([device(1, { open: true }), device(2)])

    d.select({ vid: 1, pid: 2 })
    expect(d.current.value).toEqual({ vid: 1, pid: 2 })

    api.listDevices.mockResolvedValue([device(1, { open: true })])
    await d.refresh()
    expect(d.current.value).toEqual({ vid: 1, pid: 1 })
  })
})

describe('refresh', () => {
  it('reads the layout of the selected device when it is open, else of the first open', async () => {
    const d = await fresh([device(1, { open: true }), device(2, { open: true })])
    expect(d.layout.value?.name).toBe('layout 1')

    d.select({ vid: 1, pid: 2 })
    await d.refresh()
    expect(d.layout.value?.name).toBe('layout 2')
  })

  it('has no layout when nothing is open', async () => {
    const d = await fresh([device(1, { open: true })])

    api.listDevices.mockResolvedValue([device(1, { present: true })])
    await d.refresh()

    expect(d.layout.value).toBeNull()
  })

  it('keeps the last list when reading fails, and says why', async () => {
    const d = await fresh([device(1)])

    api.listDevices.mockRejectedValueOnce('HID enumeration failed')
    await d.refresh()

    expect(d.devices.value).toHaveLength(1)
    expect(d.error.value).toBe('HID enumeration failed')
    expect(d.busy.value).toBe(false)
  })

  it('reads the list again after adopting, unplugged devices included', async () => {
    const d = await fresh([device(1)])
    api.adoptDevice.mockResolvedValue(null)
    api.listDevices.mockResolvedValue([device(1, { state: 'adopted' })])

    await expect(d.adopt(device(1))).resolves.toBeNull()

    expect(d.error.value).toBeNull()
    expect(d.devices.value[0].state).toBe('adopted')
  })
})

describe('target', () => {
  const regions = [
    { id: 'keyboard', name: 'Keyboard', positions: [4, 5], lights: 'zones' as const, capabilities: ['geometry'] },
    { id: 'lid', name: 'Lid', positions: [0, 1], lights: 'zones' as const, capabilities: ['geometry'] },
  ]

  it('is the device itself while it is whole', async () => {
    const d = await fresh([device(1, { open: true, regions })])

    expect(d.target.value).toEqual({ vid: 1, pid: 1 })
  })

  it('is the region chosen on a split device, its first until one is', async () => {
    const d = await fresh([device(1, { open: true, regions, split: true }), device(2)])
    expect(d.target.value).toEqual({ vid: 1, pid: 1, region: 'keyboard' })

    d.selectRegion({ vid: 1, pid: 1 }, 'lid')
    expect(d.target.value).toEqual({ vid: 1, pid: 1, region: 'lid' })

    // Each device keeps its own: going back finds the region left there.
    d.select({ vid: 1, pid: 2 })
    expect(d.target.value).toEqual({ vid: 1, pid: 2 })
    d.select({ vid: 1, pid: 1 })
    expect(d.target.value).toEqual({ vid: 1, pid: 1, region: 'lid' })
  })

  it('gives way when the definition no longer declares the region', async () => {
    const d = await fresh([device(1, { open: true, regions, split: true })])
    d.selectRegion({ vid: 1, pid: 1 }, 'lid')

    api.listDevices.mockResolvedValue([device(1, { open: true, regions: [regions[0]], split: true })])
    await d.refresh()

    expect(d.target.value).toEqual({ vid: 1, pid: 1, region: 'keyboard' })
  })
})
