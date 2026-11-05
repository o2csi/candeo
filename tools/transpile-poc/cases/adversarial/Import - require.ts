import { defineEffect } from '@candeo/effects-api'

// CommonJS in an ES module: `require` does not exist in the engine.
const fs = require('fs')

export default defineEffect({
  description: typeof fs,
  render({ frame }) {
    frame.fill({ r: 0, g: 0, b: 0 })
  },
})
