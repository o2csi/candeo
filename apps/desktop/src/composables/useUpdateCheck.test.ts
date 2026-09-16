import { beforeEach, describe, expect, it, vi } from 'vitest'

import { forgetUpdateCheck, useUpdateCheck } from './useUpdateCheck'

const getUpdateCheck = vi.fn()
const setCheckForUpdates = vi.fn()
const latestRelease = vi.fn()

vi.mock('../api/candeo', () => ({
  getUpdateCheck: (...args: unknown[]) => getUpdateCheck(...args),
  setCheckForUpdates: (...args: unknown[]) => setCheckForUpdates(...args),
}))

vi.mock('../api/version', async () => {
  const real = await vi.importActual<typeof import('../api/version')>('../api/version')
  return { ...real, latestRelease: (...args: unknown[]) => latestRelease(...args) }
})

const RELEASE = { version: '0.5.0', url: 'https://example.invalid/v0.5.0' }

beforeEach(() => {
  forgetUpdateCheck()
  vi.clearAllMocks()
  latestRelease.mockResolvedValue(RELEASE)
})

describe('useUpdateCheck', () => {
  it('asks nothing when the setting is off', async () => {
    getUpdateCheck.mockResolvedValue({ version: '0.4.0', available: true, enabled: false })
    const { start, found } = useUpdateCheck()

    await start()

    expect(latestRelease).not.toHaveBeenCalled()
    expect(found.value).toBeNull()
  })

  it('asks nothing in the Store version, whatever the setting says', async () => {
    getUpdateCheck.mockResolvedValue({ version: '0.4.0', available: false, enabled: true })
    const { start } = useUpdateCheck()

    await start()

    expect(latestRelease).not.toHaveBeenCalled()
  })

  it('asks once per window, not once per screen opened', async () => {
    getUpdateCheck.mockResolvedValue({ version: '0.4.0', available: true, enabled: true })
    const { start, found } = useUpdateCheck()

    await start()
    await start()

    expect(latestRelease).toHaveBeenCalledTimes(1)
    expect(found.value).toEqual({ state: 'newer', release: RELEASE })
  })

  it('says it could not check rather than up to date', async () => {
    getUpdateCheck.mockResolvedValue({ version: '0.4.0', available: true, enabled: true })
    latestRelease.mockRejectedValue(new Error('no network'))
    const { start, found } = useUpdateCheck()

    await start()

    expect(found.value).toEqual({ state: 'failed' })
  })

  it('the button asks even when the setting is off', async () => {
    getUpdateCheck.mockResolvedValue({ version: '0.4.0', available: true, enabled: false })
    const { start, checkNow, found } = useUpdateCheck()
    await start()

    await checkNow()

    expect(latestRelease).toHaveBeenCalledTimes(1)
    expect(found.value).toEqual({ state: 'newer', release: RELEASE })
  })

  it('turning the setting on writes it and asks', async () => {
    getUpdateCheck.mockResolvedValue({ version: '0.4.0', available: true, enabled: false })
    const { start, choose, status } = useUpdateCheck()
    await start()

    await choose(true)

    expect(setCheckForUpdates).toHaveBeenCalledWith(true)
    expect(status.value?.enabled).toBe(true)
    expect(latestRelease).toHaveBeenCalledTimes(1)
  })

  it('turning it off forgets what was found', async () => {
    getUpdateCheck.mockResolvedValue({ version: '0.4.0', available: true, enabled: true })
    const { start, choose, found } = useUpdateCheck()
    await start()
    expect(found.value).not.toBeNull()

    await choose(false)

    expect(setCheckForUpdates).toHaveBeenCalledWith(false)
    expect(found.value).toBeNull()
  })

  it('does not compare a version it cannot read', async () => {
    getUpdateCheck.mockResolvedValue({ version: '0.4.0', available: true, enabled: true })
    latestRelease.mockResolvedValue({ version: '0.5.0-rc.1', url: RELEASE.url })
    const { start, found } = useUpdateCheck()

    await start()

    expect(found.value).toEqual({
      state: 'unreadable',
      release: { version: '0.5.0-rc.1', url: RELEASE.url },
    })
  })
})
