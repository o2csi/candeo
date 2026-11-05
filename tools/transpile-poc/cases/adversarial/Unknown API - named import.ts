import { defineEffect, sparkle } from '@candeo/effects-api'

// A function the API does not have, imported by name: QuickJS refuses to link
// the module.
export default defineEffect({
  render({ frame }) {
    frame.fill(sparkle(1))
  },
})
