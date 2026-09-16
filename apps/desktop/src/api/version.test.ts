import { describe, expect, it } from 'vitest'

import { isNewer, parseVersion, readRelease } from './version'

describe('parseVersion', () => {
  it('reads three numbers, with or without the tag’s v', () => {
    expect(parseVersion('0.4.0')).toEqual([0, 4, 0])
    expect(parseVersion('v1.2.3')).toEqual([1, 2, 3])
    expect(parseVersion(' 0.4.0 ')).toEqual([0, 4, 0])
  })

  it('refuses what it does not understand', () => {
    // A pre-release, a build, a name: unreadable, not "the same version".
    for (const text of ['0.5.0-rc.1', '0.4', '0.4.0.1', 'latest', '']) {
      expect(parseVersion(text), text).toBeNull()
    }
  })
})

describe('isNewer', () => {
  it('compares numbers, not text', () => {
    // The comparison this exists for: as text, "0.10.0" sorts before "0.9.0".
    expect(isNewer('0.9.0', '0.10.0')).toBe(true)
    expect(isNewer('0.10.0', '0.9.0')).toBe(false)
  })

  it('answers every position', () => {
    expect(isNewer('0.4.0', '1.0.0')).toBe(true)
    expect(isNewer('0.4.0', '0.4.1')).toBe(true)
    expect(isNewer('0.4.0', 'v0.4.0')).toBe(false)
    expect(isNewer('1.0.0', '0.9.9')).toBe(false)
  })

  it('says it cannot tell rather than "up to date"', () => {
    expect(isNewer('0.4.0', '0.5.0-rc.1')).toBeNull()
    expect(isNewer('nightly', '0.5.0')).toBeNull()
  })
})

describe('readRelease', () => {
  it('takes the version and the page, dropping the tag’s v', () => {
    expect(readRelease({ tag_name: 'v0.5.0', html_url: 'https://example.invalid/v0.5.0' })).toEqual({
      version: '0.5.0',
      url: 'https://example.invalid/v0.5.0',
    })
  })

  it('refuses an answer that carries no release', () => {
    for (const answer of [{}, { tag_name: 'v0.5.0' }, { message: 'Not Found' }, null]) {
      expect(() => readRelease(answer)).toThrow()
    }
  })
})
