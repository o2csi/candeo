import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick, ref } from 'vue'

import type { DeviceRef } from '../api/types'
import { stubWindow } from '../test/window'
import { withSetup } from '../test/withSetup'

const api = vi.hoisted(() => ({
  startPreview: vi.fn(),
  stopPreview: vi.fn(),
  subscribeFrames: vi.fn(),
  subscribePreviewFrames: vi.fn(),
}))
vi.mock('../api/candeo', () => api)

import { useSimulatorFeed } from './simulatorFeed'

const keyboard = { vid: 1, pid: 2 }

function feed() {
  const device = ref<DeviceRef | null>(keyboard)
  const showsDevice = ref(false)
  const previewed = ref<string | null>(null)
  const onError = vi.fn()
  const mounted = withSetup(() =>
    useSimulatorFeed({
      layout: () => null,
      device: () => device.value,
      showsDevice: () => showsDevice.value,
      previewed: () => previewed.value,
      params: () => ({ speed: 1 }),
      onError,
    }),
  )
  return { device, showsDevice, previewed, onError, ...mounted }
}

beforeEach(() => {
  vi.useFakeTimers()
  stubWindow()
  vi.resetAllMocks()
  api.startPreview.mockResolvedValue(undefined)
  api.stopPreview.mockResolvedValue(undefined)
  api.subscribeFrames.mockResolvedValue(() => {})
  api.subscribePreviewFrames.mockResolvedValue(() => {})
})

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

describe('useSimulatorFeed', () => {
  it('starts the preview of the last effect selected, once selection settles', async () => {
    const f = feed()

    for (const id of ['Rain', 'Wave', 'Bubbles']) {
      f.previewed.value = id
      await nextTick()
      await vi.advanceTimersByTimeAsync(100)
    }
    expect(api.startPreview).not.toHaveBeenCalled()

    await vi.advanceTimersByTimeAsync(80)
    expect(api.startPreview.mock.calls).toEqual([[keyboard, 'Bubbles', { speed: 1 }]])
    // Every start builds a new channel: the simulator subscribes again after it.
    expect(api.subscribePreviewFrames).toHaveBeenCalledTimes(1)
  })

  it('borrows the default layout when there is no device', async () => {
    const f = feed()

    f.device.value = null
    f.previewed.value = 'Rain'
    await nextTick()
    await vi.advanceTimersByTimeAsync(180)

    expect(api.startPreview).toHaveBeenCalledWith(null, 'Rain', { speed: 1 })
  })

  it('shows the device frames, and stops the preview at once, when the effect runs there', async () => {
    const f = feed()
    f.previewed.value = 'Rain'
    await nextTick()
    await vi.advanceTimersByTimeAsync(100)

    f.showsDevice.value = true
    await nextTick()
    await vi.advanceTimersByTimeAsync(180)

    expect(api.subscribeFrames).toHaveBeenCalledWith(keyboard, expect.any(Function))
    expect(api.stopPreview).toHaveBeenCalled()
    expect(api.startPreview).not.toHaveBeenCalled()
  })

  it('starts the preview again for new code under the same name', async () => {
    const f = feed()
    f.previewed.value = 'Rain'
    await nextTick()
    await vi.advanceTimersByTimeAsync(180)

    f.result.restartPreview()
    await nextTick()
    await vi.advanceTimersByTimeAsync(180)

    expect(api.startPreview).toHaveBeenCalledTimes(2)
  })

  it('reports a preview that does not start', async () => {
    const f = feed()
    api.startPreview.mockRejectedValueOnce('effect not ready')

    f.previewed.value = 'Rain'
    await nextTick()
    await vi.advanceTimersByTimeAsync(180)

    expect(f.onError).toHaveBeenCalledWith('effect not ready')
    expect(api.subscribePreviewFrames).not.toHaveBeenCalled()
  })

  it('stops the preview with the screen, including one still waiting to start', async () => {
    const f = feed()
    f.previewed.value = 'Rain'
    await nextTick()
    api.stopPreview.mockClear()

    f.unmount()
    await vi.advanceTimersByTimeAsync(180)

    expect(api.stopPreview).toHaveBeenCalledTimes(1)
    expect(api.startPreview).not.toHaveBeenCalled()
  })
})
