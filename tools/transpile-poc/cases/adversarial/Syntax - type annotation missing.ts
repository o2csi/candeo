import { defineEffect } from '@candeo/effects-api'

// TypeScript-only syntax broken: a colon with no type after it.
const level: = 128

export default defineEffect({
  render({ frame }) {
    frame.fill({ r: level, g: 0, b: 0 })
  },
})
