import { describe, expect, it } from 'vitest'

import { transpile } from './effect'

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
  })
})
