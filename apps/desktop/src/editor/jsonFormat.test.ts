import { describe, expect, it } from 'vitest'

import { formatJson, WIDTH } from './jsonFormat'

/** The built-in definitions, as the repository holds them, by path. */
const BUILT_IN = import.meta.glob<string>('../../../../crates/candeo-device/devices/*.json', {
  query: '?raw',
  import: 'default',
  eager: true,
})
const builtIn = (file: string) =>
  BUILT_IN[`../../../../crates/candeo-device/devices/${file}`].replace(/\r\n/g, '\n')

describe('formatJson', () => {
  it('gives the Alienware keyboard definition back unchanged', () => {
    const text = builtIn('alienware-m18-r1.json')
    expect(formatJson(text)).toBe(text)
  })

  it('keeps each light of the Razer on one line, and changes nothing twice', () => {
    const once = formatJson(builtIn('razer-deathstalker-v2-pro.json'))
    expect(once).not.toBeNull()
    expect(once!.split('\n').length).toBeLessThan(200)
    expect(formatJson(once!)).toBe(once)
  })

  it('opens what does not fit, and only that', () => {
    const long = 'x'.repeat(WIDTH)
    expect(formatJson(`{"a":[1,2],"b":{"c":"${long}"}}`)).toBe(
      `{\n  "a": [1, 2],\n  "b": {\n    "c": "${long}"\n  }\n}\n`,
    )
    expect(formatJson('{"a":{},"b":[]}')).toBe('{ "a": {}, "b": [] }\n')
  })

  it('leaves alone what it cannot print back as written', () => {
    expect(formatJson('{"a": 1,}')).toBeNull()
    expect(formatJson('{"b": 1, "2": 0}')).toBeNull()
  })
})
