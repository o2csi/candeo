import { defineEffect } from '@candeo/effects-api'

// Two capture groups with the same name: an early error in the regular
// expression, invisible to a parser that does not read inside it.
const TWICE = /(?<digit>\d)(?<digit>\d)/

export default defineEffect({
  render({ frame }) {
    frame.fill({ r: TWICE.test('12') ? 200 : 10, g: 0, b: 0 })
  },
})
