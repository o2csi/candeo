import { beforeEach, describe, expect, it, vi } from 'vitest'

import { withSetup } from '../test/withSetup'
import type { LayoutView } from './layout'

const api = vi.hoisted(() => ({
  subscribeFrames: vi.fn(),
  subscribePreviewFrames: vi.fn(),
}))
vi.mock('../api/candeo', () => api)

import { useEngineFrames } from './engineFrames'

const layout: LayoutView = { name: 'test', rows: 1, cols: 2, frameLen: 2, keys: [] }
const keyboard = { vid: 1, pid: 2 }

/** The frame handlers and close functions handed out by the mocked channels. */
let onFrame: (bytes: Uint8Array) => void
let closes: ReturnType<typeof vi.fn>[]

function channel(handler: (bytes: Uint8Array) => void) {
  onFrame = handler
  const close = vi.fn()
  closes.push(close)
  return Promise.resolve(close)
}

beforeEach(() => {
  vi.resetAllMocks()
  closes = []
  api.subscribeFrames.mockImplementation((_device: unknown, handler: (b: Uint8Array) => void) =>
    channel(handler),
  )
  api.subscribePreviewFrames.mockImplementation(channel)
})

describe('useEngineFrames', () => {
  it('draws a black frame of the whole matrix until the engine sends one', () => {
    const { result } = withSetup(() => useEngineFrames(() => layout))
    expect(result.frame.value).toEqual([
      [0, 0, 0],
      [0, 0, 0],
    ])

    const empty = withSetup(() => useEngineFrames(() => null))
    expect(empty.result.frame.value).toEqual([])
  })

  it('reads the bytes as RGB triplets, and ignores an incomplete one', async () => {
    const { result } = withSetup(() => useEngineFrames(() => layout))

    await result.listen(keyboard)
    onFrame(Uint8Array.from([255, 0, 10, 1, 2, 3, 9]))

    expect(result.frame.value).toEqual([
      [255, 0, 10],
      [1, 2, 3],
    ])
  })

  it('closes the device channel when switching to the preview', async () => {
    const { result } = withSetup(() => useEngineFrames(() => layout))

    await result.listen(keyboard)
    await result.listenPreview()

    expect(closes[0]).toHaveBeenCalledTimes(1)
    expect(closes[1]).not.toHaveBeenCalled()
  })

  it('does not close first when subscribing again to the same device', async () => {
    const { result } = withSetup(() => useEngineFrames(() => layout))

    await result.listen(keyboard)
    await result.listen({ ...keyboard })

    // The engine replaces the channel: an unsubscribe landing second would
    // remove the new one.
    expect(closes[0]).not.toHaveBeenCalled()
    expect(api.subscribeFrames).toHaveBeenCalledTimes(2)
  })

  it('closes a channel that opens after the screen is gone', async () => {
    const { result, unmount } = withSetup(() => useEngineFrames(() => layout))

    const opening = result.listen(keyboard)
    unmount()
    await opening

    expect(closes[0]).toHaveBeenCalledTimes(1)
  })
})
