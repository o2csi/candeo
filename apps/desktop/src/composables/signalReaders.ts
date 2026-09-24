// What reads each signal: the settings bound to it on a device, the rules waiting
// for it, and the settings bound inside a rule (`docs/design/inputs-and-automations.md`
// §2.3). A sender names values and never a device, so Settings is where someone
// sees what a name drives, and which names something waits for in vain.
//
// Pure, so what Settings lists under a signal is tested without Rust or a DOM.
//
// Names come from what the window knows, the devices listed and the library, and
// fall back to what the file holds: a binding or a rule outlives the device
// unplugged or the effect deleted that it names, and it still names the signal.

import type { EffectEntry, HeldSignal, Settings } from '../api/candeo'
import type { DeviceInfo, DeviceRef } from '../api/types'
import { t } from '../i18n'
import { localized } from '../i18n/text'
import { boundSignal } from './bindings'
import { editable } from './rules'
import { named } from './useEffects'

/** A device the window lists: enough to name it. */
export type KnownDevice = Pick<DeviceInfo, 'vid' | 'pid' | 'name'>

/** A library effect: enough to name it and its settings. */
export type KnownEffect = Pick<EffectEntry, 'id' | 'name' | 'params'>

/** What the file says reads signals. */
export type ReadingSettings = Pick<Settings, 'effectParams' | 'rules'>

/** Something that reads a signal, named as Settings lists it. */
export type SignalReader =
  /** A setting of an effect, on a device, bound to the signal. */
  | { kind: 'setting'; device: string; effect: string; setting: string }
  /** A rule under way while the signal equals `equals`. */
  | { kind: 'rule'; rule: string; devices: string[]; equals: string; enabled: boolean }
  /** A setting of the effect a rule shows, bound to the signal. */
  | { kind: 'ruleSetting'; rule: string; effect: string; setting: string; enabled: boolean }

/** A name something reads, and what reads it. */
export interface ExpectedSignal {
  name: string
  readers: SignalReader[]
}

/**
 * What reads each signal, by name: the device settings in the file's order, then
 * the rules in theirs, which is their priority, each rule's trigger before its
 * own settings.
 *
 * A switched-off rule is listed, marked: it is often why a signal lights
 * nothing. A source Rust would refuse is not a signal, and a rule the
 * Automations tab cannot read is left out, as that tab leaves it unedited.
 */
export function signalReaders(
  settings: ReadingSettings,
  devices: readonly KnownDevice[],
  library: readonly KnownEffect[],
): Map<string, SignalReader[]> {
  const readers = new Map<string, SignalReader[]>()
  const add = (name: string, reader: SignalReader) => {
    const list = readers.get(name)
    if (list) list.push(reader)
    else readers.set(name, [reader])
  }

  const deviceName = (d: DeviceRef) =>
    devices.find((k) => k.vid === d.vid && k.pid === d.pid)?.name ?? usbId(d)
  const effectOf = (id: string) => library.find((e) => e.id === id)
  // As the Automations tab names a rule's effect: a firmware one by its words,
  // one the library no longer holds by its key.
  const effectName = (id: string) =>
    effectOf(id)?.name ?? (id.startsWith('hardware:') ? named(id).name : id)
  const settingName = (effect: string, param: string) =>
    localized(effectOf(effect)?.params?.[param]?.label) || param

  for (const record of settings.effectParams ?? []) {
    for (const [param, name] of boundNames(record.bindings)) {
      add(name, {
        kind: 'setting',
        device: deviceName(record),
        effect: effectName(record.effect),
        setting: settingName(record.effect, param),
      })
    }
  }

  for (const rule of settings.rules ?? []) {
    if (!editable(rule)) continue
    const enabled = rule.enabled === true
    // Unnamed, a rule goes by the effect it shows, as the device card calls it.
    const label =
      (typeof rule.name === 'string' ? rule.name.trim() : '') || effectName(rule.show.effect)
    if (rule.when.kind === 'signal') {
      add(rule.when.name, {
        kind: 'rule',
        rule: label,
        devices: rule.devices.filter(isRef).map(deviceName),
        equals: rule.when.equals,
        enabled,
      })
    }
    for (const [param, name] of boundNames(rule.show.bindings)) {
      add(name, {
        kind: 'ruleSetting',
        rule: label,
        effect: effectName(rule.show.effect),
        setting: settingName(rule.show.effect, param),
        enabled,
      })
    }
  }

  return readers
}

/**
 * The names something reads that no signal held now carries, in the order Rust
 * lists the held ones. A name bound before its sender ever ran is one of them.
 */
export function expectedSignals(
  readers: ReadonlyMap<string, SignalReader[]>,
  held: readonly Pick<HeldSignal, 'name'>[],
): ExpectedSignal[] {
  const now = new Set(held.map((s) => s.name))
  return [...readers]
    .filter(([name]) => !now.has(name))
    .map(([name, list]) => ({ name, readers: list }))
    .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
}

/** A reader in one line, in the interface's language. */
export function readerText(reader: SignalReader): string {
  switch (reader.kind) {
    case 'setting':
      return t('settings.signals.readerSetting', {
        device: reader.device,
        effect: reader.effect,
        setting: reader.setting,
      })
    case 'rule':
      // Device names are not translated: listing them needs no grammar.
      return reader.devices.length > 0
        ? t('settings.signals.readerRule', {
            rule: reader.rule,
            devices: reader.devices.join(', '),
            value: reader.equals,
          })
        : t('settings.signals.readerRuleNoDevice', { rule: reader.rule, value: reader.equals })
    case 'ruleSetting':
      return t('settings.signals.readerRuleSetting', {
        rule: reader.rule,
        effect: reader.effect,
        setting: reader.setting,
      })
  }
}

/** Whether a reader is a rule switched off, which reads nothing until it is on. */
export function switchedOff(reader: SignalReader): boolean {
  return reader.kind !== 'setting' && !reader.enabled
}

/**
 * The parameters of a bindings table that read a signal, with its name. The
 * table comes from the file: anything that is not a source Rust accepts is not
 * a signal read.
 */
function boundNames(bindings: unknown): [string, string][] {
  if (typeof bindings !== 'object' || bindings === null) return []
  const out: [string, string][] = []
  for (const [param, source] of Object.entries(bindings)) {
    const name = typeof source === 'string' ? boundSignal(source) : null
    if (name !== null) out.push([param, name])
  }
  return out
}

function isRef(d: unknown): d is DeviceRef {
  return (
    typeof d === 'object' &&
    d !== null &&
    typeof (d as DeviceRef).vid === 'number' &&
    typeof (d as DeviceRef).pid === 'number'
  )
}

/** A device the window does not list, as the Devices screen writes its ids. */
function usbId(d: DeviceRef): string {
  const hex = (n: number) => n.toString(16).padStart(4, '0')
  return `${hex(d.vid)}:${hex(d.pid)}`
}
