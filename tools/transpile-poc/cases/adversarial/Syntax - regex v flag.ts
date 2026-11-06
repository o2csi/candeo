import { defineEffect } from '@candeo/effects-api'

// An ES2024 regular expression flag, past the ES2020 target.
const LOWER = /^[\p{L}--\p{Lu}]+$/v

export default defineEffect({
  render({ frame }) {
    frame.fill({ r: LOWER.test('abc') ? 200 : 10, g: 0, b: 0 })
  },
})
