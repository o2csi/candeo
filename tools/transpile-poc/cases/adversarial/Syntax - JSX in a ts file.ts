import { defineEffect } from '@candeo/effects-api'

// JSX is not TypeScript in a `.ts` file: `<div>` reads as a type assertion.
const view = <div>light</div>

export default defineEffect({
  description: String(view),
  render({ frame }) {
    frame.fill({ r: 0, g: 0, b: 0 })
  },
})
