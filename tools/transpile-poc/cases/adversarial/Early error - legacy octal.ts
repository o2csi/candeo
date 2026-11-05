import { defineEffect } from '@candeo/effects-api'

// A legacy octal literal: refused in strict code, so in every module.
const level = 0177

export default defineEffect({
  render({ frame }) {
    frame.fill({ r: level, g: 0, b: 0 })
  },
})
