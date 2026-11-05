import { defineEffect } from '@candeo/effects-api'

// The engine's budget, not the transpiler's: the module body never ends.
while (true) {}

export default defineEffect({
  render({ frame }) {
    frame.fill({ r: 0, g: 0, b: 0 })
  },
})
