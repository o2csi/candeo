import { defineEffect } from '@candeo/effects-api'

// A dynamic import at the top level, awaited: no static import to refuse, and
// the module body fails while it runs.
const fs = await import('node:fs')

export default defineEffect({
  description: typeof fs,
  render({ frame }) {
    frame.fill({ r: 0, g: 0, b: 0 })
  },
})
