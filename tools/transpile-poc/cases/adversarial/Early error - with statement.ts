import { defineEffect } from '@candeo/effects-api'

// A module is strict code: `with` is refused before the module runs.
const colour = { r: 1, g: 2, b: 3 }

export default defineEffect({
  render({ frame }) {
    with (colour) {
      frame.fill({ r, g, b })
    }
  },
})
