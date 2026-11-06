import { describe, expect, it, vi } from 'vitest'

vi.mock('./candeo', () => ({ logFromWebview: vi.fn() }))

import { showIn } from '../i18n'
import { blockedByWindows, message } from './journal'

describe('blockedByWindows', () => {
  const refused = { code: 'controlledFolderAccess', params: {} }

  it('recognises a write Controlled folder access refused, and nothing else', () => {
    expect(blockedByWindows(refused)).toBe(true)
    expect(blockedByWindows({ code: 'unexpected', params: { detail: 'disk full' } })).toBe(false)
    expect(blockedByWindows(new Error('controlledFolderAccess'))).toBe(false)
    expect(blockedByWindows('controlledFolderAccess')).toBe(false)
    expect(blockedByWindows(null)).toBe(false)
  })

  it('says which program to allow, in the interface language', () => {
    showIn('fr')
    expect(message(refused)).toContain('autorisez le programme de Candeo, pas le dossier')
    showIn('en')
    expect(message(refused)).toContain("allow Candeo's program, not the folder")
  })
})
