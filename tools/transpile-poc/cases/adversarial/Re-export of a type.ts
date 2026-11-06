import { defineEffect } from '@candeo/effects-api'

// A type re-exported without `type`: a file compiled alone cannot know `Rgb` is
// a type, so the re-export stays, and `api.js` has no `Rgb` to link.
export { Rgb } from '@candeo/effects-api'

export default defineEffect({
  render({ frame }) {
    frame.fill({ r: 0, g: 0, b: 0 })
  },
})
