import { afterEach, describe, expect, it } from 'vitest'

import type { EffectParamsRecord, Rule, RuleShow } from '../api/candeo'
import { showIn } from '../i18n'
import {
  expectedSignals,
  readerText,
  signalReaders,
  switchedOff,
  type KnownDevice,
  type KnownEffect,
  type ReadingSettings,
  type SignalReader,
} from './signalReaders'

afterEach(() => showIn('en'))

const KEYBOARD = { vid: 0x1532, pid: 0x0296 }
const LAPTOP = { vid: 0x187c, pid: 0x0550 }
const UNKNOWN = { vid: 0x1234, pid: 0x00ab }
const DEATHSTALKER = 'DeathStalker V2 Pro'
const M18 = 'Alienware m18'
const GRADIENT = 'shipped:Fixed gradient'

const devices: KnownDevice[] = [
  { ...KEYBOARD, name: DEATHSTALKER },
  { ...LAPTOP, name: M18 },
]

const library: KnownEffect[] = [
  {
    id: GRADIENT,
    name: 'Fixed gradient',
    params: {
      colour: {
        kind: 'color',
        label: { en: 'Colour', fr: 'Couleur' },
        default: { r: 0, g: 0, b: 0 },
      },
      speed: { kind: 'number', label: 'Speed', min: 0, max: 2, default: 1 },
    },
  },
  { id: 'shipped:Clock', name: 'Clock', params: {} },
]

function record(
  device: { vid: number; pid: number },
  effect: string,
  bindings?: Record<string, string>,
): EffectParamsRecord {
  return { ...device, effect, values: {}, ...(bindings ? { bindings } : {}) }
}

/** A rule waiting for `build` to equal `failed` on the keyboard, unless told otherwise. */
function signalRule(over: Partial<Rule> = {}): Rule {
  return {
    id: 'r1',
    name: 'CI failed',
    enabled: true,
    devices: [KEYBOARD],
    when: { kind: 'signal', name: 'build', equals: 'failed', hold: true },
    show: { effect: 'shipped:Clock', params: {} },
    for: { seconds: 10 },
    ...over,
  }
}

/** A rule showing Fixed gradient with its own bindings. */
function gradient(bindings: Record<string, string>): RuleShow {
  return { effect: GRADIENT, params: {}, bindings }
}

function settings(over: Partial<ReadingSettings> = {}): ReadingSettings {
  return { effectParams: [], rules: [], ...over }
}

function onDevice(device: string, setting: string): SignalReader {
  return { kind: 'setting', device, effect: 'Fixed gradient', setting }
}

describe('signalReaders', () => {
  it('lists a setting bound on each of two devices under the name they share', () => {
    const readers = signalReaders(
      settings({
        effectParams: [
          record(KEYBOARD, GRADIENT, { colour: 'signal:status' }),
          record(LAPTOP, GRADIENT, { colour: 'signal:status' }),
        ],
      }),
      devices,
      library,
    )
    expect([...readers.keys()]).toEqual(['status'])
    expect(readers.get('status')).toEqual([
      onDevice(DEATHSTALKER, 'Colour'),
      onDevice(M18, 'Colour'),
    ])
  })

  it('lists settings bound to different names each under its own', () => {
    const readers = signalReaders(
      settings({
        effectParams: [
          record(KEYBOARD, GRADIENT, { colour: 'signal:m18.colour', speed: 'signal:volume' }),
          record(LAPTOP, GRADIENT, { colour: 'signal:m18.colour' }),
        ],
      }),
      devices,
      library,
    )
    expect(readers.get('volume')).toEqual([onDevice(DEATHSTALKER, 'Speed')])
    expect(readers.get('m18.colour')).toEqual([
      onDevice(DEATHSTALKER, 'Colour'),
      onDevice(M18, 'Colour'),
    ])
  })

  it('lists a rule waiting for a signal, with its devices and the value it waits for', () => {
    const readers = signalReaders(
      settings({ rules: [signalRule({ devices: [KEYBOARD, LAPTOP] })] }),
      devices,
      library,
    )
    expect(readers.get('build')).toEqual([
      {
        kind: 'rule',
        rule: 'CI failed',
        devices: [DEATHSTALKER, M18],
        equals: 'failed',
        enabled: true,
      },
    ])
  })

  it('names an unnamed rule by the effect it shows, as the device card does', () => {
    const shipped = signalReaders(
      settings({ rules: [signalRule({ name: '  ' })] }),
      devices,
      library,
    )
    expect(shipped.get('build')?.[0]).toMatchObject({ kind: 'rule', rule: 'Clock' })

    const firmware = signalReaders(
      settings({ rules: [signalRule({ name: '', show: { effect: 'hardware:off', params: {} } })] }),
      devices,
      library,
    )
    expect(firmware.get('build')?.[0]).toMatchObject({ kind: 'rule', rule: 'Off' })
  })

  it("lists a rule's own bindings after its trigger, whatever the trigger", () => {
    const show = gradient({ colour: 'signal:status' })
    const hourly = { id: 'r2', name: 'Hourly', when: { kind: 'cron', expr: '0 * * * *' } } as const
    const readers = signalReaders(
      settings({ rules: [signalRule({ show }), signalRule({ ...hourly, show })] }),
      devices,
      library,
    )
    expect(readers.get('build')).toHaveLength(1)
    const inRule = (rule: string): SignalReader => ({
      kind: 'ruleSetting',
      rule,
      effect: 'Fixed gradient',
      setting: 'Colour',
      enabled: true,
    })
    expect(readers.get('status')).toEqual([inRule('CI failed'), inRule('Hourly')])
  })

  it('lists device settings before rules, and rules in their order', () => {
    const readers = signalReaders(
      settings({
        rules: [signalRule({ name: 'First' }), signalRule({ id: 'r2', name: 'Second' })],
        effectParams: [record(KEYBOARD, GRADIENT, { colour: 'signal:build' })],
      }),
      devices,
      library,
    )
    expect(readers.get('build')?.map((r) => (r.kind === 'setting' ? r.device : r.rule))).toEqual([
      DEATHSTALKER,
      'First',
      'Second',
    ])
  })

  it('marks a switched-off rule rather than leaving it out', () => {
    const readers = signalReaders(
      settings({
        rules: [signalRule({ enabled: false, show: gradient({ speed: 'signal:volume' }) })],
      }),
      devices,
      library,
    )
    expect(readers.get('build')?.every(switchedOff)).toBe(true)
    expect(readers.get('volume')?.every(switchedOff)).toBe(true)
    expect(switchedOff(onDevice(DEATHSTALKER, 'Colour'))).toBe(false)
  })

  it('falls back to what the file holds for a device, an effect or a setting nobody knows', () => {
    const readers = signalReaders(
      settings({
        effectParams: [
          record(UNKNOWN, GRADIENT, { colour: 'signal:a' }),
          record(KEYBOARD, 'user:Gone', { hue: 'signal:b' }),
          record(KEYBOARD, GRADIENT, { removed: 'signal:c' }),
        ],
        rules: [
          signalRule({
            name: '',
            devices: [UNKNOWN],
            show: { effect: 'user:Gone', params: {}, bindings: { hue: 'signal:d' } },
          }),
        ],
      }),
      devices,
      library,
    )
    expect(readers.get('a')?.[0]).toMatchObject({ device: '1234:00ab', setting: 'Colour' })
    expect(readers.get('b')?.[0]).toMatchObject({ effect: 'user:Gone', setting: 'hue' })
    expect(readers.get('c')?.[0]).toMatchObject({ effect: 'Fixed gradient', setting: 'removed' })
    expect(readers.get('build')?.[0]).toMatchObject({ rule: 'user:Gone', devices: ['1234:00ab'] })
    expect(readers.get('d')?.[0]).toMatchObject({
      rule: 'user:Gone',
      effect: 'user:Gone',
      setting: 'hue',
    })
  })

  it('names everything by its source when neither the devices nor the library are known', () => {
    const readers = signalReaders(
      settings({ effectParams: [record(KEYBOARD, GRADIENT, { colour: 'signal:a' })] }),
      [],
      [],
    )
    expect(readers.get('a')).toEqual([
      { kind: 'setting', device: '1532:0296', effect: GRADIENT, setting: 'colour' },
    ])
  })

  it('reads no signal from a source Rust would refuse, nor from a rule it cannot show', () => {
    const notText = { x: 7 } as unknown as Record<string, string>
    const readers = signalReaders(
      settings({
        effectParams: [
          record(KEYBOARD, GRADIENT, { colour: 'status', speed: 'signal:' }),
          { ...record(KEYBOARD, 'shipped:Clock'), bindings: notText },
        ],
        rules: [
          { id: 'broken', when: { kind: 'signal', name: 'build' } } as unknown as Rule,
          signalRule({ devices: [null, KEYBOARD] as unknown as Rule['devices'] }),
        ],
      }),
      devices,
      library,
    )
    expect([...readers.keys()]).toEqual(['build'])
    expect(readers.get('build')).toHaveLength(1)
    expect(readers.get('build')?.[0]).toMatchObject({ devices: [DEATHSTALKER] })
  })

  it('follows the interface language for the settings an effect labels', () => {
    showIn('fr')
    const readers = signalReaders(
      settings({ effectParams: [record(KEYBOARD, GRADIENT, { colour: 'signal:a' })] }),
      devices,
      library,
    )
    expect(readers.get('a')?.[0]).toMatchObject({ setting: 'Couleur' })
  })
})

describe('expectedSignals', () => {
  const readers = signalReaders(
    settings({
      effectParams: [
        record(KEYBOARD, GRADIENT, { colour: 'signal:status', speed: 'signal:volume' }),
      ],
      rules: [signalRule()],
    }),
    devices,
    library,
  )

  it('keeps the names read that no signal held carries, with what reads them', () => {
    const expected = expectedSignals(readers, [{ name: 'status' }, { name: 'unread' }])
    expect(expected.map((e) => e.name)).toEqual(['build', 'volume'])
    expect(expected[0].readers).toEqual(readers.get('build'))
  })

  it('is empty once every name read is held', () => {
    const held = [{ name: 'build' }, { name: 'status' }, { name: 'volume' }]
    expect(expectedSignals(readers, held)).toEqual([])
  })

  it('lists every name read when nothing is held, sorted as Rust lists the held ones', () => {
    const many = signalReaders(
      settings({
        effectParams: [record(KEYBOARD, GRADIENT, { colour: 'signal:b', speed: 'signal:B' })],
        rules: [signalRule({ when: { kind: 'signal', name: 'a', equals: '1' } })],
      }),
      devices,
      library,
    )
    expect(expectedSignals(many, []).map((e) => e.name)).toEqual(['B', 'a', 'b'])
  })
})

describe('readerText', () => {
  const rule: SignalReader = {
    kind: 'rule',
    rule: 'CI failed',
    devices: [DEATHSTALKER, M18],
    equals: 'failed',
    enabled: true,
  }

  it('says each reader in one line', () => {
    expect(readerText(onDevice(DEATHSTALKER, 'Colour'))).toBe(
      'DeathStalker V2 Pro · Fixed gradient · Colour',
    )
    expect(readerText(rule)).toBe(
      'Rule “CI failed” on DeathStalker V2 Pro, Alienware m18, when it is “failed”',
    )
    expect(readerText({ ...rule, devices: [] })).toBe(
      'Rule “CI failed” on no device, when it is “failed”',
    )
    expect(
      readerText({
        kind: 'ruleSetting',
        rule: 'CI failed',
        effect: 'Fixed gradient',
        setting: 'Colour',
        enabled: false,
      }),
    ).toBe('Rule “CI failed” · Fixed gradient · Colour')
  })

  it('says it in French', () => {
    showIn('fr')
    expect(readerText({ ...rule, devices: [DEATHSTALKER] })).toBe(
      'Règle « CI failed » sur DeathStalker V2 Pro, quand il vaut « failed »',
    )
  })
})
