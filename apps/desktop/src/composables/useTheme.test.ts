import { beforeEach, describe, expect, it, vi } from 'vitest'

const api = vi.hoisted(() => ({
  getSettings: vi.fn(),
  setTheme: vi.fn(),
}))
vi.mock('../api/candeo', () => api)
vi.mock('../api/journal', () => ({ warn: vi.fn(), message: (e: unknown) => String(e) }))

/** The document root, as the composable sees it: the tests run without a DOM. */
const root = { dataset: {} as Record<string, string> }

beforeEach(() => {
  vi.resetModules()
  vi.clearAllMocks()
  vi.unstubAllGlobals()
  root.dataset = {}
})

async function fresh() {
  const { useTheme } = await import('./useTheme')
  // After the import: Vue probes `document` as it loads, and this stub is only the root.
  vi.stubGlobal('document', { documentElement: root })
  return useTheme()
}

describe('useTheme', () => {
  it('shows the saved theme', async () => {
    api.getSettings.mockResolvedValue({ preferences: { theme: 'light' } })
    const t = await fresh()
    await t.load()
    expect(t.theme.value).toBe('light')
    expect(root.dataset.theme).toBe('light')
  })

  it('leaves the system to decide when nothing is saved or settings are unreadable', async () => {
    root.dataset.theme = 'dark'
    api.getSettings.mockResolvedValue({ preferences: {} })
    const t = await fresh()
    await t.load()
    expect(t.theme.value).toBe('system')
    expect(root.dataset.theme).toBeUndefined()

    api.getSettings.mockRejectedValue(new Error('unreadable'))
    await t.load()
    expect(t.theme.value).toBe('system')
  })

  it('shows a chosen theme at once and saves it', async () => {
    api.setTheme.mockResolvedValue(undefined)
    const t = await fresh()
    await t.choose('dark')
    expect(root.dataset.theme).toBe('dark')
    expect(api.setTheme).toHaveBeenCalledWith('dark')

    await t.choose('system')
    expect(root.dataset.theme).toBeUndefined()
  })

  it('keeps a chosen theme for the session when saving fails', async () => {
    api.setTheme.mockRejectedValue(new Error('read-only'))
    const t = await fresh()
    await t.choose('light')
    expect(t.theme.value).toBe('light')
    expect(root.dataset.theme).toBe('light')
  })
})
