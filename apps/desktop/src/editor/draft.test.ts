import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

/** A `localStorage` over a map, or one that refuses everything. */
function storage(refuse = false) {
  const items = new Map<string, string>()
  const guard = () => {
    if (refuse) throw new DOMException('storage disabled', 'SecurityError')
  }
  return {
    items,
    getItem: (k: string) => (guard(), items.get(k) ?? null),
    setItem: (k: string, v: string) => (guard(), void items.set(k, v)),
    removeItem: (k: string) => (guard(), void items.delete(k)),
  }
}

async function drafts(store = storage()) {
  vi.stubGlobal('localStorage', store)
  return { store, ...(await import('./draft')) }
}

beforeEach(() => {
  vi.resetModules()
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('drafts', () => {
  it('keeps one draft per effect, and one for the effect being created', async () => {
    const d = await drafts()

    d.writeDraft('Rain', 'rain source')
    d.writeDraft(null, 'new source')

    expect(d.readDraft('Rain')).toBe('rain source')
    expect(d.readDraft(null)).toBe('new source')
    d.clearDraft('Rain')
    expect(d.readDraft('Rain')).toBeNull()
  })

  it('does not fail when the webview refuses storage', async () => {
    const d = await drafts(storage(true))

    expect(() => d.writeDraft('Rain', 'source')).not.toThrow()
    expect(d.readDraft('Rain')).toBeNull()
    expect(() => d.clearDraft('Rain')).not.toThrow()
  })

  it('moves a draft with a rename, unless the new name already has a newer one', async () => {
    const d = await drafts()

    d.writeDraft('Rain', 'old')
    d.moveDraft('Rain', 'Storm')
    expect(d.readDraft('Storm')).toBe('old')
    expect(d.readDraft('Rain')).toBeNull()

    d.writeDraft('Wave', 'moved')
    d.writeDraft('Tide', 'newer')
    d.moveDraft('Wave', 'Tide')
    expect(d.readDraft('Tide')).toBe('newer')
    expect(d.readDraft('Wave')).toBe('moved')
  })

  it('moves drafts of former ids once, and again after a failure', async () => {
    const d = await drafts()
    d.writeDraft('onde-radiale', 'draft')
    const renames = vi
      .fn()
      .mockRejectedValueOnce('not ready')
      .mockResolvedValue({ 'onde-radiale': 'Radial wave' })

    await d.migrateDrafts(renames)
    expect(d.readDraft('onde-radiale')).toBe('draft')

    await d.migrateDrafts(renames)
    await d.migrateDrafts(renames)
    expect(d.readDraft('Radial wave')).toBe('draft')
    expect(renames).toHaveBeenCalledTimes(2)
  })
})
