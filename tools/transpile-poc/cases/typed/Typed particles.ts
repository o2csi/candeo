// Particles kept from frame to frame in classes: the syntax ES2020 does not
// have and TypeScript lowers for it, and the TypeScript-only class features.
//
// Probes: class fields with and without initialisers, `#private` fields and
// methods, static fields and a static block, parameter properties, `readonly`,
// `abstract`, `override`, accessors, method overloads, an enum, a const enum,
// logical assignment, numeric separators, optional chaining, `#x in obj`.

import { defineEffect, hsv, mix, type Key, type Rgb } from '@candeo/effects-api'

enum Kind {
  Spark,
  Ember = 4,
  Smoke,
}

const enum Fade {
  Fast = 0.5,
  Slow = 1.5,
}

abstract class Body {
  static created = 0
  static readonly LIMIT: number = 1_000
  static {
    Body.created = 0
  }

  #age = 0
  hue: number
  declare tag: string

  constructor(
    protected readonly origin: number,
    public kind: Kind,
  ) {
    this.hue = (origin * 37) % 360
    Body.created++
  }

  abstract life(): number

  get age(): number {
    return this.#age
  }

  step(dt: number): boolean {
    this.#age += dt
    return this.#age < this.life()
  }

  static isBody(x: unknown): x is Body {
    return typeof x === 'object' && x !== null && #age in x
  }
}

class Spark extends Body {
  #fade: Fade = Fade.Fast
  colour?: Rgb

  override life(): number {
    return this.kind === Kind.Ember ? this.#fade * 2 : this.#fade
  }

  shade(t: number): Rgb
  shade(t: number, base: Rgb): Rgb
  shade(t: number, base?: Rgb): Rgb {
    this.colour ??= hsv(this.hue, 1, 1)
    return mix(base ?? this.colour, { r: 0, g: 0, b: 0 }, Math.min(1, t))
  }
}

const live: Spark[] = []
let last = 0

export default defineEffect({
  description: 'Sparks that fade, kept in classes',
  params: {
    rate: { kind: 'number', label: 'Rate', min: 1, max: 20, default: 6 },
  },
  render({ layout, time, frameIndex, frame, params }) {
    const dt = Math.max(0, time - last)
    last = time
    const keys: readonly Key[] = layout.keys
    if (keys.length === 0) return
    if (frameIndex % Math.max(1, Math.round(30 / params.rate)) === 0) {
      const at = (frameIndex * 7919) % keys.length
      live.push(new Spark(at, frameIndex % 3 === 0 ? Kind.Ember : Kind.Spark))
    }
    for (let i = live.length - 1; i >= 0; i--) {
      const spark = live[i]!
      if (!spark.step(dt) || !Body.isBody(spark)) live.splice(i, 1)
    }
    let lit = 0
    for (const spark of live) {
      const key = keys[(spark as unknown as { origin: number }).origin]
      if (key?.index === undefined) continue
      frame.set(key, spark.shade(spark.age / spark.life()))
      lit ||= 1
      lit &&= lit + 1
    }
    if (Body.created > Body.LIMIT) live.length = 0
  },
})
