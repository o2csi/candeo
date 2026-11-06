import { defineEffect } from '@candeo/effects-api'

// `accessor`, from the decorators proposal: TypeScript 4.9+ lowers it.
class Level {
  accessor value = 60
}

export default defineEffect({
  render({ frame }) {
    frame.fill({ r: new Level().value, g: 0, b: 0 })
  },
})
