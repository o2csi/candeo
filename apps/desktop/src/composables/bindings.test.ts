import { describe, expect, it } from 'vitest'
import type { ParamSpec } from '@candeo/effects-api'

import type { HeldSignal } from '../api/candeo'
import {
  bindingState,
  boundSignal,
  converts,
  declaredBindings,
  rebound,
  signalSource,
  withBinding,
} from './bindings'

const specs: Record<string, ParamSpec> = {
  speed: { kind: 'number', label: 'Speed', min: 0, max: 2, default: 1 },
  colour: { kind: 'color', label: 'Colour', default: { r: 255, g: 0, b: 0 } },
  mode: {
    kind: 'choice',
    label: 'Mode',
    options: ['calm', { value: 'wild', label: { en: 'Wild', fr: 'Sauvage' } }],
    default: 'calm',
  },
  reverse: { kind: 'boolean', label: 'Reverse', default: false },
}

function held(name: string, value: HeldSignal['value']): HeldSignal {
  return { name, value, received: 0, expires: null }
}

describe('sources', () => {
  it('names a signal the way Rust reads it back', () => {
    expect(signalSource('status')).toBe('signal:status')
    expect(boundSignal('signal:status')).toBe('status')
    expect(boundSignal(signalSource('ha.office:door-1'))).toBe('ha.office:door-1')
  })

  it('reads no signal from a source Rust would refuse', () => {
    expect(boundSignal('status')).toBeNull()
    expect(boundSignal('signal:')).toBeNull()
    expect(boundSignal('signal:two words')).toBeNull()
    expect(boundSignal(`signal:${'x'.repeat(65)}`)).toBeNull()
    expect(boundSignal('sound:level')).toBeNull()
  })
})

describe('declaredBindings', () => {
  it('keeps declared parameters bound to a signal, and nothing else', () => {
    expect(
      declaredBindings(specs, {
        colour: 'signal:status',
        speed: 'signal:volume',
        gone: 'signal:status',
        mode: 'nonsense',
      }),
    ).toEqual({ colour: 'signal:status', speed: 'signal:volume' })
  })

  it('has nothing for an effect without parameters', () => {
    expect(declaredBindings({}, { colour: 'signal:status' })).toEqual({})
  })
})

describe('rebound', () => {
  it('binds a parameter, or gives it its value back, without touching the others', () => {
    const before = { colour: 'signal:status' }

    expect(rebound(before, 'speed', 'signal:volume')).toEqual({
      colour: 'signal:status',
      speed: 'signal:volume',
    })
    expect(rebound(before, 'colour', 'signal:alert')).toEqual({ colour: 'signal:alert' })
    expect(rebound(before, 'colour', null)).toEqual({})
    expect(before).toEqual({ colour: 'signal:status' })
  })
})

describe('withBinding', () => {
  const show = { effect: 'shipped:Fixed gradient', params: { speed: 1.5 } }

  it('carries the binding with the rule, beside its values', () => {
    expect(withBinding(show, 'colour', 'signal:status')).toEqual({
      ...show,
      bindings: { colour: 'signal:status' },
    })
  })

  it('leaves no bindings behind once the last one goes', () => {
    const bound = withBinding(show, 'colour', 'signal:status')

    expect(withBinding(bound, 'colour', null)).toEqual(show)
    expect('bindings' in withBinding(bound, 'colour', null)).toBe(false)
  })
})

describe('converts', () => {
  it('takes a number, written or sent as one, whatever its range', () => {
    expect(converts(specs.speed, 0.4)).toBe(true)
    expect(converts(specs.speed, '1.5')).toBe(true)
    // Clamped by the bootstrap, not refused.
    expect(converts(specs.speed, 40)).toBe(true)
    expect(converts(specs.speed, 'fast')).toBe(false)
    expect(converts(specs.speed, ' ')).toBe(false)
    expect(converts(specs.speed, true)).toBe(false)
  })

  it('takes a colour written #rrggbb or #rgb, the # optional', () => {
    expect(converts(specs.colour, '#ff0000')).toBe(true)
    expect(converts(specs.colour, 'F00')).toBe(true)
    expect(converts(specs.colour, ' #00ff00 ')).toBe(true)
    expect(converts(specs.colour, 'red')).toBe(false)
    expect(converts(specs.colour, '#ff00')).toBe(false)
    expect(converts(specs.colour, 0xff0000)).toBe(false)
  })

  it('takes true or false, as a flag, a number or text', () => {
    for (const on of [true, false, 1, 0, 'true', 'false', '1', '0']) {
      expect(converts(specs.reverse, on)).toBe(true)
    }
    expect(converts(specs.reverse, 'yes')).toBe(false)
    expect(converts(specs.reverse, 2)).toBe(false)
  })

  it('takes one of the options, by value', () => {
    expect(converts(specs.mode, 'calm')).toBe(true)
    expect(converts(specs.mode, 'wild')).toBe(true)
    expect(converts(specs.mode, 'Wild')).toBe(false)
  })
})

describe('bindingState', () => {
  it('says the signal is not held, and the parameter keeps its value', () => {
    expect(bindingState(specs.colour, 'status', [held('volume', 0.4)])).toEqual({ kind: 'absent' })
  })

  it('says what the signal holds, and whether the parameter can take it', () => {
    const now = [held('status', '#ff0000'), held('volume', 'loud')]

    expect(bindingState(specs.colour, 'status', now)).toEqual({ kind: 'fits', value: '#ff0000' })
    expect(bindingState(specs.speed, 'volume', now)).toEqual({ kind: 'unfit', value: 'loud' })
    expect(bindingState(specs.reverse, 'on', [held('on', true)])).toEqual({
      kind: 'fits',
      value: 'true',
    })
  })
})
