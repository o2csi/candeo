import { defineEffect } from '@candeo/effects-api'

// TC39 decorators, which TypeScript 5 compiles without `experimentalDecorators`
// and lowers for ES2020 with helpers it writes into the file.
function doubled(target: (this: Level) => number, _context: ClassMethodDecoratorContext) {
  return function (this: Level): number {
    return target.call(this) * 2
  }
}

function sealed(cls: typeof Level, _context: ClassDecoratorContext) {
  Object.seal(cls)
}

@sealed
class Level {
  @doubled
  value(): number {
    return 60
  }
}

export default defineEffect({
  render({ frame }) {
    frame.fill({ r: new Level().value(), g: 0, b: 0 })
  },
})
