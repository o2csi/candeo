import { defineEffect } from '@candeo/effects-api'
// TypeScript's CommonJS import form, which an ES module cannot have.
import fs = require('fs')

export default defineEffect({
  description: typeof fs,
  render({ frame }) {
    frame.fill({ r: 0, g: 0, b: 0 })
  },
})
