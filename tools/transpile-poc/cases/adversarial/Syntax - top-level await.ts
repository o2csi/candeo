import { defineEffect } from '@candeo/effects-api'

// Only an ES module may await at the top level: proof the output stays one.
const level = await Promise.resolve(90)

export default defineEffect({
  render({ frame }) {
    frame.fill({ r: level, g: 0, b: 0 })
  },
})
