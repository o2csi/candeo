import { defineEffect } from '@candeo/effects-api'

// Valid syntax, refused by any JavaScript engine before running: an early
// error, which TypeScript reports from its checker, not its parser.
let level = 64
let level = 128

export default defineEffect({
  render({ frame }) {
    frame.fill({ r: level, g: 0, b: 0 })
  },
})
