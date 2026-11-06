import * as api from '@candeo/effects-api'

// The same unknown function through a namespace: the module links, and the
// first frame throws. Only a type check finds it before running.
export default api.defineEffect({
  render({ frame }) {
    frame.fill(api.sparkle(1))
  },
})
