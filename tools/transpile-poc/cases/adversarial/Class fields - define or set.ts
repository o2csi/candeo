import { defineEffect } from '@candeo/effects-api'

// A field over an inherited accessor (a type error TypeScript reports, TS2610):
// assigned, it calls the setter; defined, it hides it.
class Base {
  seen = 0
  set level(v: number) {
    this.seen = v * 2
  }
  get level(): number {
    return this.seen
  }
}

class Derived extends Base {
  level = 100
}

export default defineEffect({
  render({ frame }) {
    const d = new Derived()
    frame.fill({ r: d.seen, g: d.level, b: 0 })
  },
})
