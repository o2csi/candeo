import { defineEffect } from '@candeo/effects-api'
import { palette } from './palette.ts'

export default defineEffect({
  render({ frame }) {
    frame.fill(palette[0])
  },
})
