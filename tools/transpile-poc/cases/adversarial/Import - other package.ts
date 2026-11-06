import { defineEffect } from '@candeo/effects-api'
import _ from 'lodash'

export default defineEffect({
  render({ frame }) {
    frame.fill({ r: _.clamp(300, 0, 255), g: 0, b: 0 })
  },
})
