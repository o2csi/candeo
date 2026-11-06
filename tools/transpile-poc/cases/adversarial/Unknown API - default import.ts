import api from '@candeo/effects-api'

// The API has no default export.
export default api.defineEffect({
  render({ frame }) {
    frame.fill(api.BLACK)
  },
})
