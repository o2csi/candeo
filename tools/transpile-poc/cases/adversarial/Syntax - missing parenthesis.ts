import { defineEffect } from '@candeo/effects-api'
export default defineEffect({
  render({ layout, frame }) {
    for (const key of layout.keys frame.set(key, { r: 0, g: 0, b: 0 })
  },
})
