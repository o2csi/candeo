import { describe, expect, it } from 'vitest'

import { transpile } from './effect'

/** The effects shipped with the application, as authors write them, by path. */
const SHIPPED = import.meta.glob<string>('../../../../packages/effects/*.ts', {
  query: '?raw',
  import: 'default',
  eager: true,
})

describe('transpile', () => {
  it('strips types and keeps the API import a module import', async () => {
    const js = await transpile(
      [
        "import { defineEffect, rgb } from '@candeo/effects-api'",
        'const level: number = 128',
        'export default defineEffect({',
        '  render({ layout, frame }) {',
        '    for (const key of layout.keys) frame.set(key, rgb(level, 0, 0))',
        '  },',
        '})',
      ].join('\n'),
    )

    // rquickjs resolves the import to the host module: a `require` would not load.
    expect(js).toContain("from '@candeo/effects-api'")
    expect(js).not.toContain('require(')
    expect(js).toContain('export default defineEffect(')
    expect(js).not.toContain(': number')
    // Loading Monaco's TypeScript compiler alone takes seconds, and ran past the
    // 5-second default with four suites running at once.
  }, 30_000)

  it('makes a module that throws a syntax error the compiler would repair', async () => {
    const js = await transpile(
      [
        "import { defineEffect } from '@candeo/effects-api'",
        'export default defineEffect({',
        '  render({ layout, frame }) {',
        '    for (const key of layout.keys frame.set(key, { r: 0, g: 0, b: 0 })',
        '  },',
        '})',
      ].join('\n'),
    )

    // Not the repaired loop: that one would run, and be listed as ready.
    expect(js).not.toContain('defineEffect(')
    expect(() => new Function(js)()).toThrow(new SyntaxError("line 4, column 35: ')' expected."))
  }, 30_000)

  it('finds no syntax error in the effects shipped', async () => {
    expect(Object.keys(SHIPPED).length).toBeGreaterThan(0)
    for (const [path, source] of Object.entries(SHIPPED)) {
      expect(await transpile(source), path).not.toMatch(/^throw new SyntaxError/)
    }
  }, 30_000)
})
