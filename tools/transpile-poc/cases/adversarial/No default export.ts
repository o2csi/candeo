import { defineEffect } from '@candeo/effects-api'

export const effect = defineEffect({
  render({ frame }) {
    frame.fill({ r: 0, g: 0, b: 0 })
  },
})
