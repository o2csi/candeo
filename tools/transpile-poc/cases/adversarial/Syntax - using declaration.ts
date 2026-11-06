import { defineEffect } from '@candeo/effects-api'

// Explicit resource management (TypeScript 5.2+): lowered by TypeScript.
let disposed = 0

export default defineEffect({
  render({ frame }) {
    {
      using lease = { [Symbol.dispose]: () => disposed++ }
      void lease
    }
    frame.fill({ r: disposed % 256, g: 0, b: 0 })
  },
})
