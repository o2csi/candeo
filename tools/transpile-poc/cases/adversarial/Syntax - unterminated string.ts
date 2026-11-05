import { defineEffect } from '@candeo/effects-api'

const NAME = 'unterminated

export default defineEffect({
  description: NAME,
  render({ frame }) {
    frame.fill({ r: 10, g: 20, b: 30 })
  },
})
