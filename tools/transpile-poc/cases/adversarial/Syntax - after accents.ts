import { defineEffect } from '@candeo/effects-api'

// The error sits after non-ASCII characters on its line: UTF-8 bytes and
// UTF-16 units no longer agree on the column.
const étiquette = { déjà: 'vu 😀' ) }

export default defineEffect({
  description: étiquette.déjà,
  render({ frame }) {
    frame.fill({ r: 10, g: 20, b: 30 })
  },
})
