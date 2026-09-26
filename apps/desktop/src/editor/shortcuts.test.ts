import { describe, expect, it } from 'vitest'

import { commentKey, keysOf, SHORTCUTS, taught } from './shortcuts'

const comment = SHORTCUTS.find((s) => s.action === 'comment')!
const find = SHORTCUTS.find((s) => s.action === 'find')!

/** A few keys of each layout, as `getLayoutMap` gives them. */
const US = new Map([['KeyQ', 'q'], ['KeyY', 'y'], ['Period', '.'], ['Slash', '/'], ['Backquote', '`']])
const AZERTY = new Map([['KeyQ', 'a'], ['KeyY', 'y'], ['Period', ':'], ['Slash', '!'], ['Backquote', '²']])
const QWERTZ = new Map([['KeyQ', 'q'], ['KeyY', 'z'], ['Period', '.'], ['Slash', '-'], ['Backquote', '^']])

describe('the shortcuts as this keyboard prints them', () => {
  it('names the comment key after the layout', () => {
    expect(commentKey(US)).toBe('/')
    expect(commentKey(AZERTY)).toBe(':')
    expect(commentKey(QWERTZ)).toBe('#')
    expect(commentKey(null)).toBe('/')
  })

  it('prefers what the comment key was seen typing', () => {
    expect(commentKey(null, ':')).toBe(':')
    expect(commentKey(US, ':')).toBe(':')
    const press = { keyCode: 191, key: ':', shiftKey: false, altKey: false }
    expect(taught(press)).toBe(':')
    expect(taught({ ...press, shiftKey: true, key: '/' })).toBeNull()
    expect(taught({ ...press, keyCode: 190 })).toBeNull()
  })

  it('leaves the letters as they are', () => {
    expect(keysOf(find, AZERTY)).toBe('Ctrl+F')
    expect(keysOf(comment, AZERTY)).toBe('Ctrl+:')
  })
})
