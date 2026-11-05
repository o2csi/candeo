import { defineEffect } from '@candeo/effects-api'

// A parameter decorator: legacy decorators only, which `effect.ts` does not
// turn on.
function inject(_target: object, _key: string | undefined, _index: number) {}

class Level {
  constructor(@inject readonly value: number) {}
}

export default defineEffect({
  render({ frame }) {
    frame.fill({ r: new Level(60).value, g: 0, b: 0 })
  },
})
