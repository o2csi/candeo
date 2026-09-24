<script lang="ts">
import { ref } from 'vue'

/**
 * The state of the two collapsible columns, at module level.
 *
 * Going to the editor destroys this view: from `setup()`, the column just
 * collapsed would reopen on return, and the dead strip that was meant to go
 * would come back on every round trip. Same pattern as `useDevice`: screen
 * state survives navigation, it is not serialized for all that.
 *
 * Kept in an object, and taken up by name in `<script setup>`: only that
 * block's bindings are exposed to the template.
 */
const shut = { devices: ref(false), effects: ref(false) }
</script>

<script setup lang="ts">
/**
 * Studio: three columns, devices, effects, settings.
 *
 * The hierarchy is the one people think in: choose a device, then its effect,
 * then its settings. **Assignment is no longer a checkbox at the bottom of a
 * panel, it is the structure of the screen.**
 *
 * ## Two columns collapse, the third does not
 *
 * With a single controlled device, a whole column would be a permanent dead
 * strip, and it is the preview that needs the width. The third does not
 * collapse: it is the content, nothing would be left. The collapsing details
 * are in the style sheet, where the trap is.
 *
 * ## A single "active" effect
 *
 * The **selected** device's, and only that one. Marking every device's effects
 * active in a list that describes what *one* device does is not a
 * simplification, it is false information.
 *
 * ## Selecting starts the preview, Apply sends to the keyboard
 *
 * Settings used to be adjusted blind and the result discovered on the keyboard;
 * it is the other way round now (issue #63). **Two loops, and they do not touch
 * each other**: the device's writes to the LEDs, the preview's writes nowhere.
 * Browsing the gallery therefore cannot turn off the lighting in use, which
 * would have happened with a single loop per device, and would only have shown
 * once shipped.
 *
 * The screen says **which of the two** it shows, at every moment:
 * `engine_status` keeps them in two separate fields, and there is nothing to
 * filter here.
 *
 * The simulator sits in the right-hand panel: list on the left / render on the
 * right here, code on the left / render on the right in the editor. Same
 * grammar, and **a single drawing**: `KeyboardSimulator` is the same component
 * on both sides, there are not two drawings to keep in agreement.
 *
 * It is fed by a frame channel from the engine, the very frames that go to the
 * keyboard when the device is what is being watched (`docs/design/studio.md`
 * §3).
 */

import { computed, nextTick, onBeforeUnmount, onMounted, watch } from 'vue'
import { useRouter } from 'vue-router'
import type { UnlistenFn } from '@tauri-apps/api/event'
import type { ParamSpec, ParamValue } from '@candeo/effects-api'

import {
  deleteEffect,
  duplicateEffect,
  forgetEffectSettings,
  missingBuiltins,
  openEffectsDir,
  restoreBuiltin,
  engineStatus,
  getDefaultLayout,
  getLayout,
  getSignalsApi,
  listSignals,
  onSignalsChanged,
  resumeDevice,
  startEffect,
  stopEffect,
  type EffectEntry,
  type EffectState,
  type EngineReport,
  type HeldSignal,
} from '../api/candeo'
import { effectName as nameOfKey, isShippedKey } from '../api/effectKey'
import { message } from '../api/journal'
import type { DeviceRef, LayoutInfo, Rgb } from '../api/types'
import EffectParamsForm from '../components/EffectParamsForm.vue'
import { readsSignal } from '../composables/bindings'
import DeviceStatusDot from '../components/DeviceStatusDot.vue'
import EffectSwatch from '../components/EffectSwatch.vue'
import FailureNote from '../components/FailureNote.vue'
import KeyboardSimulator from '../components/KeyboardSimulator.vue'
import { deviceStatus, statusLabel } from '../composables/deviceStatus'
import { goesLive, interruptionLine, showsDeviceFrames } from '../composables/interruption'
import { deviceEffect } from '../composables/effectSelection'
import { useDevice } from '../composables/useDevice'
import { illustrates } from '../keyboard/illustration'
import {
  colourBytes,
  hardwareEffectsFor,
  hardwareParams,
  useEffects,
  type HardwareEffect,
} from '../composables/useEffects'
import { useSettings } from '../composables/useSettings'
import { refreshLibrary } from '../editor/library'
import { t } from '../i18n'
import { localized } from '../i18n/text'
import { useSimulatorFeed } from '../keyboard/simulatorFeed'

/** Engine polling period, in milliseconds. */
const STATUS_PERIOD = 1000

/**
 * Brightness range.
 *
 * A byte, because that is what the report carries (`0x0f`/`0x04`), not an
 * interface choice. Not to be confused with `BRIGHTNESS_DEFAULT`, which has the
 * same value today for an entirely different reason: the maximum is a protocol
 * constraint, the default is a decision.
 */
const BRIGHTNESS_MAX = 255

const shutDevices = shut.devices
const shutEffects = shut.effects

const router = useRouter()
const { devices, current, select, busy, refresh } = useDevice()
// `apply` does not throw: it stores its failure in `applyError`, which must
// therefore be shown, or a hardware mode refused by the device would say nothing.
const {
  appliedOn,
  apply,
  error: applyError,
  dismissError: dismissApplyError,
} = useEffects()
const {
  load: loadSettings,
  reload: reloadSettings,
  valuesFor,
  bindingsFor,
  keptFor,
  adjust,
  adjustPreview,
  bind,
  bindPreview,
  settle,
  forget,
  dropEffect,
  referencedEffects,
  lastAppliedOn,
  brightnessOf,
  setBrightness,
  flush: flushParams,
  error: paramsError,
  dismissError: dismissParamsError,
} = useSettings()

/** Errors coming up from Rust are already readable: they are shown as they are. */
// ---------------------------------------------------------------- devices

/**
 * The column lists the **controlled** devices, and nothing else.
 *
 * Adoption stays in the Devices view: choosing what to configure and choosing
 * what Candeo is allowed to control are two different gestures, and merging
 * them would turn a selection click into taking control.
 */
const controlled = computed(() => devices.value.filter((d) => d.state === 'adopted'))

/**
 * The device the column shows as chosen.
 *
 * The application's current choice if it is controlled, otherwise the first in
 * the list: `current` can point to a known but unadopted layout, which has no
 * row here.
 */
const selectedDevice = computed(() => {
  const list = controlled.value
  const c = current.value
  return list.find((d) => c !== null && d.vid === c.vid && d.pid === c.pid) ?? list[0] ?? null
})

/** A device's stable identity, to compare without depending on the object. */
const key = (d: DeviceRef | null) => (d ? `${d.vid}:${d.pid}` : null)

const deviceKey = computed(() => key(selectedDevice.value))

/**
 * What the column points to becomes the whole application's choice.
 *
 * That is what makes "open in the editor" work on the device being looked at:
 * the editor has no column, it takes up `current`.
 */
watch(
  deviceKey,
  () => {
    const d = selectedDevice.value
    if (d && key(current.value) !== key(d)) select({ vid: d.vid, pid: d.pid })
  },
  { immediate: true },
)

function choose(d: { vid: number; pid: number }) {
  select({ vid: d.vid, pid: d.pid })
}

/**
 * The warning waits for the search started at launch to end. Without this
 * `busy`, it would show for the length of the enumeration and then vanish:
 * announcing an absence not yet checked.
 */
const noDevice = computed(() => controlled.value.length === 0 && !busy.value)

// ---------------------------------------------------------------- library

type Nature = 'builtin' | 'user' | 'hardware'

/**
 * An effect, whatever its nature.
 *
 * The three lists have neither the same origin nor the same shape
 * (`list_effects` for the first two, a written catalogue for the hardware) but
 * the column shows them the same way. So they are brought to a single shape
 * here rather than keeping three sub-templates in the template.
 */
interface Choice {
  id: string
  name: string
  nature: Nature
  description: string
  /**
   * Color swatch **sampled by running the effect**, on the Rust side (issue #29).
   *
   * Empty for hardware, and that is the only honest answer: those effects are
   * run by the firmware, the application never sees their frames.
   * `EffectSwatch` then shows a muted patch: making up four plausible colors
   * would be describing an effect nobody looked at.
   */
  swatch: string[]
  params: Record<string, ParamSpec>
  /** Set for the only nature that does not go through the engine. */
  hardware: HardwareEffect | null
  /**
   * Whether the effect can run. Only a file can be anything but `ready`: one not
   * compiled yet, or one that does not load.
   */
  state: EffectState
  /** Why a `broken` effect does not load. */
  error: string | null
  /** A built-in whose file was edited outside the application. */
  modified: boolean
  /** Key presses are read while it runs: said on screen (`docs/design/key-input.md` §3). */
  readsKeys: boolean
  /** It is given the wall-clock time: said on screen, like every input read. */
  readsClock: boolean
  /**
   * It is given every signal held, `inputs: ['signals']`. A parameter bound to
   * a signal is not this, and not said here: the person chose it.
   */
  readsSignals: boolean
}

const library = ref<EffectEntry[]>([])
/** False until the library was read once: before that, every effect looks missing. */
const libraryRead = ref(false)
/** Already readable: Rust's messages are shown as they are. */
const listError = ref<string | null>(null)
/** What prevented applying or stopping. Already readable too. */
const problem = ref<string | null>(null)

function fromEntry(e: EffectEntry): Choice {
  return {
    id: e.id,
    name: e.name,
    nature: e.kind,
    description: localized(e.description) || t('effects.noDescription'),
    swatch: e.swatch,
    params: e.params ?? {},
    hardware: null,
    state: e.state,
    error: e.error ?? null,
    modified: e.modified,
    readsKeys: e.readsKeys ?? false,
    readsClock: e.readsClock ?? false,
    readsSignals: e.readsSignals ?? false,
  }
}

function fromHardware(e: HardwareEffect): Choice {
  return {
    id: e.id,
    name: e.name,
    nature: 'hardware',
    description: e.summary,
    swatch: [],
    // Its colours, when it paints with any: the settings column then draws the
    // same form as for any other effect, and what is chosen is kept the same way.
    params: hardwareParams(e),
    hardware: e,
    state: 'ready',
    error: null,
    modified: false,
    readsKeys: false,
    readsClock: false,
    readsSignals: false,
  }
}

/**
 * An effect reading key presses, on a surface nobody types on.
 *
 * It would run without ever seeing one — and so stay dark, with nothing saying
 * why. A device declares what its lights are; while it has said nothing,
 * nothing is left out.
 */
function offered(e: Choice): boolean {
  return !e.readsKeys || (board.value?.lights ?? 'keys') === 'keys'
}

const choices = computed<Choice[]>(() => [
  ...library.value.filter((e) => e.kind === 'builtin').map(fromEntry).filter(offered),
  ...library.value.filter((e) => e.kind === 'user').map(fromEntry).filter(offered),
  // Only what this device's firmware runs: offering a mode it does not know
  // would be offering an effect that never starts.
  ...hardwareEffectsFor(board.value).map(fromHardware),
])

/**
 * Grouped by nature. The cost of each one is spelled out in the right-hand
 * panel, not merely suggested by the order.
 *
 * Hardware comes first: it costs no processor time and survives everything,
 * which often makes it the right choice (`docs/design/studio.md` §1).
 *
 * The order is for display only. When the selected device has no effect to
 * select (`followDevice`), the fallback still takes the first entry of
 * `choices`, a built-in: a hardware effect would open the screen on a simulator
 * with nothing to animate.
 */
const GROUPS: readonly Nature[] = ['hardware', 'builtin', 'user']

const grouped = computed(() =>
  GROUPS.map((nature) => ({
    nature,
    title: t(`effects.groups.${nature}`),
    items: choices.value.filter((c) => c.nature === nature),
  })),
)

/**
 * Folded sections, remembered per viewer in the webview storage.
 *
 * A viewer who never uses hardware effects folds them once; asking again at
 * every launch would make folding pointless. Storage may be refused (hardened
 * webview, read-only profile): sections then open expanded and still fold for
 * the session.
 */
const SECTION_STORAGE_PREFIX = 'candeo:effects-section:'

function readSectionFolded(nature: Nature): boolean {
  try {
    return localStorage.getItem(SECTION_STORAGE_PREFIX + nature) === 'folded'
  } catch {
    return false
  }
}

function writeSectionFolded(nature: Nature, folded: boolean): void {
  try {
    // Removed rather than stored as "expanded": expanded is the default, and a
    // missing key must keep meaning it.
    if (folded) localStorage.setItem(SECTION_STORAGE_PREFIX + nature, 'folded')
    else localStorage.removeItem(SECTION_STORAGE_PREFIX + nature)
  } catch {
    // See `SECTION_STORAGE_PREFIX`: the fold simply does not outlive the session.
  }
}

const foldedSections = ref<Record<Nature, boolean>>({
  hardware: readSectionFolded('hardware'),
  builtin: readSectionFolded('builtin'),
  user: readSectionFolded('user'),
})

/**
 * Folding only hides entries: the selection is left alone, so the settings
 * panel keeps showing the effect even when its section is folded.
 */
function toggleSection(nature: Nature): void {
  const folded = !foldedSections.value[nature]
  foldedSections.value[nature] = folded
  writeSectionFolded(nature, folded)
}

/** The real cost, spelled out: it is what tells the three natures apart. */
function cost(nature: Nature): string {
  return t(nature === 'hardware' ? 'effects.costs.hardware' : 'effects.costs.host')
}

const chosenEffect = ref<string | null>(null)

/**
 * False until the library and the engine status were read once. Selecting
 * before would show the first effect, and start its preview, for the time it
 * takes to learn which one the device runs.
 */
const loaded = ref(false)

const selectedEffect = computed<Choice | null>(() =>
  loaded.value
    ? (choices.value.find((c) => c.id === chosenEffect.value) ?? choices.value[0] ?? null)
    : null,
)

/** The effects column's body, to bring the selected entry into view. */
const effectsList = ref<HTMLElement | null>(null)

/**
 * Selects the selected device's effect: the one running on it, else the one it
 * remembers (#118).
 *
 * Only when the screen opens and when another device is chosen, never on the
 * engine refresh: an effect clicked since stays selected.
 */
async function followDevice(): Promise<void> {
  const d = selectedDevice.value
  chosenEffect.value = deviceEffect(
    runningOn(d),
    lastAppliedOn(d),
    choices.value.map((c) => c.id),
  )
  const c = selectedEffect.value
  if (!c) return
  // Unfolded for this visit only: the fold the viewer saved stays theirs.
  foldedSections.value[c.nature] = false
  await nextTick()
  effectsList.value
    ?.querySelector(`[data-effect="${CSS.escape(c.id)}"]`)
    ?.scrollIntoView({ block: 'nearest' })
}

watch(deviceKey, () => {
  if (loaded.value) void followDevice()
})

// ---------------------------------------------------------------- engine

/**
 * The engine's state: what runs on the devices, and what is being watched.
 *
 * Everything is read back at once: it is a single round trip per second, and
 * the devices column needs every row to say what each one runs.
 *
 * **The two fields never mix.** `devices` describes the hardware; `preview`
 * what the simulator shows when the selected effect is not the one running.
 * Everything that speaks of "active" on this screen reads the first.
 */
const report = ref<EngineReport>({ devices: [], preview: null })

function statusOf(d: { vid: number; pid: number } | null) {
  if (!d) return null
  return report.value.devices.find((s) => s.device.vid === d.vid && s.device.pid === d.pid) ?? null
}

/**
 * The effect a device runs, **on its LEDs**.
 *
 * The host loop wins over the hardware mode: as long as it pushes frames, it is
 * what shows on the LEDs, whatever mode was set before. The preview does not
 * enter this computation, and cannot: it is not in `devices`.
 */
function runningOn(d: { vid: number; pid: number } | null): string | null {
  const s = statusOf(d)
  // A rule interrupting the device runs its own effect, but that effect is not
  // applied: the one the device goes back to still is (#106).
  if (d && s?.interruption) return lastAppliedOn({ vid: d.vid, pid: d.pid })
  if (s?.running === true && s.effectId !== null) return s.effectId
  return appliedOn(d ? { vid: d.vid, pid: d.pid } : null)
}

function effectName(id: string | null): string | null {
  if (id === null) return null
  return choices.value.find((c) => c.id === id)?.name ?? nameOfKey(id)
}

/**
 * What a device does, in one line, for the left-hand column.
 *
 * An idle device that **remembers** its last effect says so: that is where it
 * shows that the configuration is saved, at the very place where it describes
 * something.
 */
function deviceLine(d: { vid: number; pid: number }): string {
  const interruption = statusOf(d)?.interruption
  if (interruption) {
    const rule = interruption.name || (effectName(interruption.effect) ?? interruption.effect)
    const applied = effectName(lastAppliedOn({ vid: d.vid, pid: d.pid }))
    const line = interruptionLine(interruption, rule, applied, Date.now())
    return t(line.key, line.params)
  }
  const running = effectName(runningOn(d))
  if (running !== null) return running
  const remembered = effectName(lastAppliedOn({ vid: d.vid, pid: d.pid }))
  return remembered !== null
    ? t('effects.deviceStopped', { name: remembered })
    : t('effects.deviceIdle')
}

/** The only effect marked **applied**: the selected device's. */
const activeId = computed(() => runningOn(selectedDevice.value))

const status = computed(() => statusOf(selectedDevice.value))

/** Whether a change to this effect's settings reaches the selected device live (#218). */
function live(effect: string, applied: string | null = activeId.value): boolean {
  return goesLive(effect, applied, Boolean(status.value?.interruption))
}
const runningHere = computed(() => status.value?.running === true)

/** The preview in progress, when it does show the selected effect. */
const preview = computed(() => {
  const p = report.value.preview
  if (!p || !p.running) return null
  return p.effectId === selectedEffect.value?.id ? p : null
})

/**
 * A preview's failure, **when it concerns the effect being watched**.
 *
 * The filter on the identifier is not a stylistic precaution: the engine state
 * is read back every second, and a broken effect's preview outlives the next
 * selection for the time the new one takes to start. Without it, one effect
 * would be blamed for another's failure: the fastest way to send someone
 * looking in the wrong place.
 */
const previewError = computed<string | null>(() => {
  const p = report.value.preview
  if (!p || p.running || p.effectId !== selectedEffect.value?.id) return null
  return p.error
})

async function refreshStatus(): Promise<void> {
  try {
    report.value = await engineStatus()
  } catch (e) {
    problem.value = message(e)
  }
}

// ---------------------------------------------------------------- preview

/**
 * Fallback layout, asked of Rust: something has to be drawn before a device is
 * open.
 */
const fallback = ref<LayoutInfo | null>(null)
/** The selected device's layout, when it is really open. */
const opened = ref<LayoutInfo | null>(null)
/**
 * The whole layout, not the simulator's view of it: this screen also reads what
 * the firmware runs, to offer those effects and no others. A `LayoutInfo` is a
 * `LayoutView` wherever the drawing is what matters.
 */
const board = computed<LayoutInfo | null>(() => opened.value ?? fallback.value)

/**
 * The drawing follows the selected device.
 *
 * A single layout is known today, but it comes from the device and not from a
 * constant: `get_layout` for the one that is open, the default layout
 * otherwise. The day a second model arrives, this view has nothing to learn.
 */
watch(
  [deviceKey, () => selectedDevice.value?.open === true],
  async ([, isOpen]) => {
    const d = selectedDevice.value
    if (!d || !isOpen) {
      opened.value = null
      return
    }
    // A layout that cannot be obtained is not a failure: the fallback draws.
    opened.value = await getLayout({ vid: d.vid, pid: d.pid }).catch(() => null)
  },
  { immediate: true },
)

/**
 * Is the selected effect already the one running **on the device**?
 *
 * It is the question that decides everything that follows: in that case the
 * simulator shows the keyboard's real frames, and there is no reason to keep a
 * second QuickJS context alive to display the same thing.
 */
const applied = computed(
  () => selectedEffect.value !== null && activeId.value === selectedEffect.value.id,
)

/**
 * What failed **writing to the device**, once what has been read is closed.
 *
 * Closing fixes nothing here: the failure goes on, and the loop will raise it
 * again at the next frame. So the message stays hidden **while it does not
 * change** — a device that starts failing differently has something new to say,
 * and the rest was the same sentence repeated to someone who has read it.
 */
const hushed = ref<string | null>(null)
const deviceTrouble = computed(() => {
  const trouble = status.value?.deviceError
  const said = trouble ? message(trouble) : null
  return said === hushed.value ? null : said
})

/** True when the simulator must show the device's stream: not a rule's, under the applied effect's name. */
const showsDevice = computed(() =>
  showsDeviceFrames(runningHere.value, applied.value, Boolean(status.value?.interruption)),
)

const { frame, restartPreview } = useSimulatorFeed({
  layout: () => board.value,
  device: () => selectedDevice.value,
  showsDevice: () => showsDevice.value,
  // A hardware effect is run by the firmware and the app never sees its frames:
  // previewing it would mean inventing them.
  previewed: () => {
    const c = selectedEffect.value
    // Nor an effect that cannot run: its JavaScript is not there, or does not load.
    return c && !c.hardware && c.state === 'ready' ? c.id : null
  },
  // What it *is* drawn as, which is a different promise: a legend of what the
  // effect was seen doing, not the device's frames. The note below says so.
  illustrated: () => {
    const c = selectedEffect.value
    return c?.hardware && illustrates(c.id) ? c.id : null
  },
  // The drawing paints with what the effect was given, so that it says what the
  // keyboard will show rather than a colour of its own.
  illustratedColours: () => {
    const bytes = colourBytes(paramValues.value)
    return bytes.length >= 3 ? [[bytes[0], bytes[1], bytes[2]] as Rgb] : []
  },
  params: () => paramValues.value,
  bindings: () => paramBindings.value,
  onError: (e) => {
    problem.value = message(e)
  },
})

/**
 * What the simulator shows, spelled out rather than guessed.
 *
 * **This is where the refusal to lie plays out.** A preview that looks like an
 * applied effect costs a debugging session: it is the fourth time this pattern
 * has come up in this project. So the sentence says both things at once: what
 * is being watched, and what the keyboard is doing meanwhile.
 */
const previewNote = computed(() => {
  const c = selectedEffect.value
  if (showsDevice.value) return t('effects.preview.device')
  if (c?.hardware) {
    // Two different promises, and the sentence must not confuse them: a drawing
    // of what the effect does, or nothing at all.
    return illustrates(c.id) ? t('effects.preview.illustration') : t('effects.preview.hardware')
  }

  // Under an interruption, what the keyboard shows is the rule's effect.
  const interruption = status.value?.interruption
  const running = interruption
    ? interruption.name || (effectName(interruption.effect) ?? interruption.effect)
    : effectName(activeId.value)
  const elsewhere = (runningHere.value || Boolean(interruption)) && running !== null
  if (preview.value) {
    // Without a controlled device, do not promise Apply: the button is
    // disabled, and announcing it would send people looking for why it does not
    // respond.
    if (!selectedDevice.value) return t('effects.preview.noDevice')
    return elsewhere
      ? t('effects.preview.elsewhere', { name: running })
      : t('effects.preview.apply')
  }
  if (previewError.value !== null) return t('effects.preview.stopped')
  return t('effects.preview.starting')
})

// ---------------------------------------------------------------- actions

const working = ref(false)

/**
 * Apply **promotes the preview to a device effect**: it is the gesture that
 * sends to the keyboard, and the only one.
 *
 * Two paths, because the two natures do not go through the same place: a
 * hardware effect is a mode set on the firmware, a host effect is a loop that
 * gets started. Setting a hardware mode stops the loop first: otherwise it
 * would keep writing over it, and the mode would stay invisible.
 *
 * Nothing stops the preview here: `showsDevice` turns true as soon as the state
 * is read back, the watcher above puts it away by itself, and the simulator
 * switches to the real frames. Doing it by hand would make two paths to keep
 * in agreement.
 */
async function applyEffect(): Promise<void> {
  const c = selectedEffect.value
  const d = selectedDevice.value
  if (!c || !d) return

  const device = { vid: d.vid, pid: d.pid }
  problem.value = null
  working.value = true
  try {
    if (c.hardware) {
      if (statusOf(d)?.running === true) await stopEffect(device)
      await apply(device, c.hardware, colourBytes(paramValues.value))
    } else {
      // The settings remembered for **this pair**, and not the declared values:
      // an effect adjusted then left must start again as it was left, or every
      // slider would have to be moved again after every Apply.
      //
      // Nothing to subscribe here: the channel lives in the loop's state, and it
      // is the watcher above that opens it as soon as the engine says "running".
      // One more subscription, set up here, would make two for a single stream.
      //
      // Its bindings too: Rust starts an effect with those it is given, and
      // without them the parameters bound here would stop reading their signal.
      await startEffect(device, c.id, paramValues.value, paramBindings.value)
    }
  } catch (e) {
    problem.value = message(e)
  } finally {
    working.value = false
    await refreshStatus()
    // Rust has just remembered the applied effect, or not: reading back is the
    // only honest way to know. Guessing it here would make the window a second
    // source of truth, which would diverge at the first write failure.
    await reloadSettings()
  }
}

/**
 * Stops the **device**'s loop. The last frame stays displayed as it stays on the
 * keyboard: stopping an effect does not turn the LEDs off.
 *
 * The preview is not affected, and that is precisely the point: stopping what
 * runs on the keyboard must not close what is being watched.
 */
async function halt(): Promise<void> {
  const d = selectedDevice.value
  if (!d) return

  problem.value = null
  working.value = true
  try {
    await stopEffect({ vid: d.vid, pid: d.pid })
  } catch (e) {
    problem.value = message(e)
  } finally {
    working.value = false
    await refreshStatus()
    // The stop **forgot** the applied effect in the file: without this read
    // back, the column would still announce "remembered: …" for a device that
    // no longer remembers anything.
    await reloadSettings()
  }
}

/**
 * Ends the rule interrupting the selected device, and gives the device its
 * effect back now. The rule comes back at its next occurrence.
 */
async function resume(): Promise<void> {
  const d = selectedDevice.value
  if (!d) return

  problem.value = null
  working.value = true
  try {
    await resumeDevice({ vid: d.vid, pid: d.pid })
  } catch (e) {
    problem.value = message(e)
  } finally {
    working.value = false
    await refreshStatus()
  }
}

// ------------------------------------------------------------- removal

/**
 * Only the user's effects are deleted from here. A built-in is not
 * (`docs/design/effects-library.md` §4), and a hardware effect lives in the
 * firmware.
 */
const removable = computed(() => selectedEffect.value?.nature === 'user')

/**
 * Shipped effects the folder no longer holds, offered back. Read with the
 * library, since both change with the folder.
 */
const missingBuiltinNames = ref<string[]>([])

async function readLibrary(): Promise<void> {
  const [entries, missing] = await Promise.all([refreshLibrary(), missingBuiltins()])
  library.value = entries
  missingBuiltinNames.value = missing
}

/**
 * Writes shipped effects' files again: missing ones, or a modified one whose
 * original is wanted back. The previewed code may have changed with them.
 */
async function restore(names: readonly string[]): Promise<void> {
  problem.value = null
  working.value = true
  try {
    for (const name of names) await restoreBuiltin(name)
  } catch (e) {
    problem.value = message(e)
  }
  try {
    await readLibrary()
    restartPreview()
  } catch (e) {
    listError.value = message(e)
  } finally {
    working.value = false
  }
}

/** The built-in whose original waits for confirmation, by id, like a removal. */
const pendingRestore = ref<string | null>(null)

async function restoreOriginal(): Promise<void> {
  const id = pendingRestore.value
  if (id === null) return
  await restore([nameOfKey(id)])
  if (problem.value === null) pendingRestore.value = null
}

/**
 * Effects the settings still refer to that the folder no longer holds: renamed
 * or removed outside the application.
 *
 * Said, not purged: putting the file back under its name restores everything,
 * and forgetting is a gesture of its own.
 */
const missingEffects = computed(() => {
  if (!libraryRead.value) return []
  const present = new Set(library.value.map((e) => e.id))
  return (
    [...referencedEffects.value]
      // A firmware effect is never in the folder: it is run by the device, and
      // the settings kept for it — a colour — refer to no file to put back.
      .filter((name) => !name.startsWith('hardware:') && !present.has(name))
      .sort()
  )
})

async function forgetMissing(name: string): Promise<void> {
  problem.value = null
  try {
    await forgetEffectSettings(name)
    dropEffect(name)
  } catch (e) {
    problem.value = message(e)
  }
}

/** Copies the selected effect, then selects the copy. */
async function duplicateSelected(): Promise<void> {
  const c = selectedEffect.value
  if (!c || c.hardware) return
  problem.value = null
  working.value = true
  try {
    const name = await duplicateEffect(c.id)
    await readLibrary()
    chosenEffect.value = name
  } catch (e) {
    problem.value = message(e)
  } finally {
    working.value = false
  }
}

async function openFolder(): Promise<void> {
  try {
    await openEffectsDir()
  } catch (e) {
    listError.value = message(e)
  }
}

/**
 * Reads the effects folder again, compiling what changed: effects saved there
 * from outside the application appear, edited ones are recompiled.
 */
async function refreshEffects(): Promise<void> {
  listError.value = null
  working.value = true
  try {
    await readLibrary()
  } catch (e) {
    listError.value = message(e)
  } finally {
    working.value = false
  }
}

/**
 * The effect whose removal awaits confirmation, **by its identifier**.
 *
 * An identifier and not a boolean: the question does not freeze the screen,
 * one can click elsewhere while it is asked, and a flag would end up confirming
 * the removal of an effect other than the one that was pointed to.
 */
const pendingRemoval = ref<string | null>(null)

/**
 * Changing effect withdraws the question.
 *
 * A confirmation that outlived the selection would reopen by itself on return,
 * without being asked for again, and that is not a box anyone wants to see pop
 * up by surprise.
 */
watch(
  () => selectedEffect.value?.id,
  () => {
    pendingRemoval.value = null
    pendingRestore.value = null
  },
)

/**
 * Removes the effect pointed to **by the confirmation**, never the one the
 * selection shows at the time of the click.
 *
 * Rust does the rest in the right order: it refuses what cannot be removed,
 * stops the loops running this effect on any device, deletes the folder, then
 * forgets the settings remembered for it. None of this is spread out here: it
 * is the only way for the invariant to hold whoever the caller is.
 */
async function removeEffect(): Promise<void> {
  const id = pendingRemoval.value
  if (id === null) return

  problem.value = null
  working.value = true
  try {
    await deleteEffect(id)

    // The in-memory counterpart of what Rust has just done on disk: without
    // this forgetting, an effect saved again under the same name in the same
    // session would inherit the settings of its vanished namesake.
    dropEffect(id)

    pendingRemoval.value = null
    // The selection falls back on the first in the list: the effect it pointed
    // to no longer exists.
    if (chosenEffect.value === id) chosenEffect.value = null
    await readLibrary()
  } catch (e) {
    problem.value = message(e)
  } finally {
    working.value = false
    // The loop may have stopped: the engine state will only say so once read back.
    await refreshStatus()
  }
}

// ---------------------------------------------------------------- settings

/** The parameters declared by the effect being watched. */
const specs = computed<Record<string, ParamSpec>>(() => selectedEffect.value?.params ?? {})

/**
 * The values this effect runs on, or would run on, on this device: its
 * manifest, overridden by what was remembered for this pair.
 */
const paramValues = computed(() =>
  valuesFor(selectedDevice.value, selectedEffect.value?.id ?? '', specs.value),
)

/**
 * Its parameters reading a signal on this device. None for a firmware effect:
 * it has no loop of ours to read one, and its colours reach the firmware only
 * when it is applied.
 */
const paramBindings = computed(() => {
  const c = selectedEffect.value
  return c && !c.hardware ? bindingsFor(selectedDevice.value, c.id, specs.value) : {}
})

/** The signals held now: suggested to a setting that reads one, and what it reads. */
const held = ref<HeldSignal[]>([])

async function readSignals(): Promise<void> {
  // Only suggestions and a line beside a field: a failure keeps the last list.
  held.value = await listSignals().catch(() => held.value)
}

/**
 * Whether Candeo receives signals. Assumed until read, so that no effect is said
 * to read in vain before anyone knows. Turned on or off in Settings, another
 * tab: this one is mounted again when it comes back.
 */
const receiving = ref(true)

/** Whether an effect reads a signal on this device: marked in the list (#224). */
function readsSignalHere(c: Choice): boolean {
  return !c.hardware && readsSignal(c.readsSignals, bindingsFor(selectedDevice.value, c.id, c.params))
}

/** An entry as it is read out: its name, applied or not, and whether it reads a signal. */
function entryLabel(c: Choice): string {
  const name = activeId.value === c.id ? t('effects.appliedOnDevice', { name: c.name }) : c.name
  const marks = [
    c.readsKeys ? t('effects.readsKeys') : null,
    readsSignalHere(c) ? t('effects.entrySignal') : null,
  ]
  return [name, ...marks.filter((mark) => mark !== null)].join(' · ')
}

/** The selected effect reads a signal that cannot arrive: said once, under its settings. */
const signalsOff = computed(
  () =>
    !receiving.value &&
    selectedEffect.value !== null &&
    readsSignal(selectedEffect.value.readsSignals, paramBindings.value),
)

/**
 * Why the controls are inert, or `null` if they are live.
 *
 * **They are live almost always, now.** They used to be live only for the
 * applied effect, which forced adjusting blind and then discovering the result
 * on the keyboard; the preview loop does away with that bargain (issue #63):
 * one adjusts while seeing, and without sending anything anywhere.
 *
 * What remains is the case where there is no loop to adjust: a hardware effect,
 * whose firmware exposes nothing, and the brief moment when the preview has not
 * started yet.
 */
const frozen = computed<string | null>(() => {
  if (preview.value || showsDevice.value) return null
  // **Not while the preview is starting.** The controls stay live: what is
  // adjusted is remembered, and the preview will start with those values: it
  // is `paramValues` that is passed to it. Freezing them for a round trip would
  // make the form flicker at every selection change, for nothing.
  //
  // A hardware effect has nothing to adjust either, but it declares no
  // parameter: `noParams` speaks for it, and saying it again here would have
  // the same sentence read twice.
  return previewError.value === null ? null : t('effects.frozen')
})

/** An effect without parameters says so, and says it differently depending on its nature. */
const noParams = computed(() =>
  selectedEffect.value?.hardware ? t('effects.noParamsHardware') : t('effects.noParams'),
)

/**
 * What the configuration remembers, said where it is made.
 *
 * It is the real defect issue #64 points out: settings have been kept since
 * issue #28, and **nothing on screen let anyone guess it**. One adjusts, closes,
 * and has no reason to believe it held.
 */
const savedNote = computed<string | null>(() => {
  const c = selectedEffect.value
  const d = selectedDevice.value
  if (!c || c.hardware || Object.keys(specs.value).length === 0) return null
  if (!d) return t('effects.savedNoDevice')
  return keptFor({ vid: d.vid, pid: d.pid }, c.id)
    ? t('effects.savedKept', { device: d.name })
    : t('effects.savedDeclared', { device: d.name })
})

/**
 * A setting goes to **the loop being watched**, and to disk.
 *
 * ⚠️ **To the keyboard only if that very effect is the one running on it.** A
 * first version always pushed to both loops, on the idea that the applied
 * effect was being adjusted while another was previewed. That is wrong:
 * `selectedEffect` is the one being **watched**, not the one that is applied.
 * Adjusting the color of a previewed effect therefore changed the lighting in
 * use, and could stop the applied effect: one effect's values arrived in
 * another's loop. The same holds while a rule interrupts the device: the
 * applied effect is marked applied, but the loop runs the rule's (#218).
 *
 * The disk always remembers: the setting belongs to the device/effect pair and
 * will hold at the next launch of this effect.
 */
function onParamChange(id: string, value: ParamValue): void {
  const d = selectedDevice.value
  const c = selectedEffect.value
  if (!d || !c) return
  const complete = adjust(
    { vid: d.vid, pid: d.pid },
    c.id,
    specs.value,
    id,
    value,
    live(c.id),
  )
  if (preview.value) adjustPreview(complete)
}

/** The gesture is over (slider released, box checked): write now. */
function onParamCommit(): void {
  const d = selectedDevice.value
  const c = selectedEffect.value
  if (!d || !c) return
  const device = { vid: d.vid, pid: d.pid }
  settle(device, c.id)

  // **One rule for every effect: a setting changed reaches the one running.** A
  // host effect has a loop, adjusted live above; a firmware effect has none and
  // holds the last colour it was given, so reaching it means sending the effect
  // again. At the end of the gesture, not at every shade a picker travels
  // through, and only on the device already showing it.
  if (c.hardware && live(c.id, appliedOn(device))) {
    void apply(device, c.hardware, colourBytes(paramValues.value))
  }
}

/**
 * A setting reads a signal, or its value again: the same destinations as a
 * value, by the same rule — the keyboard only when this effect runs there.
 */
function onParamBind(id: string, source: string | null): void {
  const d = selectedDevice.value
  const c = selectedEffect.value
  if (!d || !c || c.hardware) return
  const bindings = bind(
    { vid: d.vid, pid: d.pid },
    c.id,
    specs.value,
    id,
    source,
    live(c.id),
  )
  if (preview.value) bindPreview(bindings)
}

/** Back to what the effect declares: its values, and no setting reading a signal. */
function onParamReset(): void {
  const d = selectedDevice.value
  const c = selectedEffect.value
  if (!d || !c) return
  const declared = forget({ vid: d.vid, pid: d.pid }, c.id, specs.value, live(c.id))
  if (preview.value) {
    adjustPreview(declared)
    bindPreview({})
  }
}

// ---------------------------------------------------------------- brightness

/**
 * The selected device's level, from 0 to 255.
 *
 * In the devices column, and not in an effect's settings: it is a property of
 * the device, a separate protocol command (`0x0f`/`0x04`) that has nothing to
 * do with the running effect. The command existed from day one and was shown
 * nowhere: it was not a display bug, it was an interface that had never been
 * written (issue #64).
 */
const brightness = computed(() => brightnessOf(selectedDevice.value))

/** As a percentage, because 0-255 means nothing to someone adjusting their light. */
const brightnessPercent = computed(() =>
  Math.round((brightness.value / BRIGHTNESS_MAX) * 100),
)

/**
 * `commit` separates the drag from its end: during it, the keyboard is written;
 * only at the end, the disk. Same split as for effect settings, and for the
 * same reason: `settings.json` is written through a temporary file then a
 * rename, it is a complete disk operation.
 */
function onBrightness(event: Event, commit: boolean): void {
  const d = selectedDevice.value
  if (!d) return
  const level = Number((event.target as HTMLInputElement).value)
  setBrightness({ vid: d.vid, pid: d.pid }, level, commit)
}

// ---------------------------------------------------------------- lifecycle

let statusTimer = 0
/** False once destroyed: opening chains round trips to Rust. */
let alive = true
let unlistenSignals: UnlistenFn | null = null

onMounted(async () => {
  void getDefaultLayout().then((l) => {
    fallback.value = l
  })

  // Pushed rather than polled, expiry included: a bound setting says what its
  // signal holds without a second timer beside the engine's.
  void readSignals()
  void getSignalsApi()
    .then((api) => {
      receiving.value = api.enabled
    })
    .catch(() => null)
  void onSignalsChanged(() => void readSignals())
    .then((stop) => {
      if (alive) unlistenSignals = stop
      else stop()
    })
    .catch(() => null)

  // Before anything else: Apply and the preview start from the remembered
  // values, and reading them afterwards would leave a window where the effect
  // would start on its defaults.
  await loadSettings()

  await refresh()

  // A single alert: the library is read in one go, it fails in one go. The
  // hardware effects are written here: the column is never empty.
  try {
    await readLibrary()
    libraryRead.value = true
  } catch (e) {
    listError.value = message(e)
  }

  await refreshStatus()

  loaded.value = true
  void followDevice()

  // The screen may have been left in the meantime: setting up the periodic
  // polling now would leave it running for nobody.
  if (alive) statusTimer = window.setInterval(() => void refreshStatus(), STATUS_PERIOD)
})

onBeforeUnmount(() => {
  alive = false
  window.clearInterval(statusTimer)
  unlistenSignals?.()
  // A slider's last movement must not depend on someone having stayed on the
  // screen for the write's idle delay.
  flushParams()
})
</script>

<template>
  <section class="studio" :class="{ 'shut-1': shutDevices, 'shut-2': shutEffects }">
    <!-- --------------------------------------------------------- devices -->
    <section class="col devices" :class="{ shut: shutDevices }" :aria-label="t('effects.columns.devices')">
      <!--
        A caption, not a heading: the screen's only `h1` is the name of the
        effect being configured, and it comes later in the document. Each column
        is already named for screen readers by its `aria-label`.
      -->
      <div class="col-head">
        <p class="col-title">{{ t('effects.columns.devices') }}</p>
        <button
          class="collapse"
          type="button"
          aria-controls="col-devices"
          :aria-expanded="!shutDevices"
          :aria-label="shutDevices ? t('effects.expandDevices') : t('effects.collapseDevices')"
          @click="shutDevices = !shutDevices"
        >
          {{ shutDevices ? '›' : '‹' }}
        </button>
      </div>

      <div id="col-devices" class="col-body">
        <!--
          The card is a plain container, not the button: the brightness slider
          lives in it, and an interactive control nested in a button is invalid
          markup that would also re-select the device on every drag.
        -->
        <div
          v-for="d in controlled"
          :key="`${d.vid}:${d.pid}`"
          class="card"
          :class="{ selected: deviceKey === key(d) }"
        >
          <button
            class="entry"
            type="button"
            :aria-pressed="deviceKey === key(d)"
            :aria-label="`${d.name} · ${statusLabel(deviceStatus(d))}`"
            :title="`${d.name} · ${statusLabel(deviceStatus(d))}`"
            @click="choose(d)"
          >
            <!--
              A type pictogram, not a manufacturer's logo: those are protected
              trademarks, they do not tell a keyboard from a mouse, and the
              product name already carries the information. The only known
              layout is a keyboard; the day Rust declares a type, it will come
              from there rather than be guessed from the name.
            -->
            <!--
              The state rides on the pictogram rather than in the text: it stays
              visible when the column is collapsed to icons, and it says which
              device dropped without a line of its own.
            -->
            <span class="glyph">
              <svg
                viewBox="0 0 24 16"
                width="18"
                height="12"
                aria-hidden="true"
                fill="none"
                stroke="currentColor"
                stroke-width="1.6"
              >
                <rect x="1" y="2" width="22" height="12" rx="2" />
                <path d="M6 11h12" stroke-linecap="round" />
                <path d="M5 6h1M9 6h1M13 6h1M17 6h1" stroke-linecap="round" />
              </svg>
              <DeviceStatusDot class="state-badge" :device="d" aria-hidden="true" />
            </span>

            <span class="entry-text">
              <!-- The **product** name, not a category: it is what tells two
                   keyboards of the same brand apart. It wraps rather than being
                   truncated. -->
              <span class="dev-name">{{ d.name }}</span>
              <!-- The column is narrow and cuts the line: the whole of it on hover. -->
              <span class="dev-fx" :title="deviceLine(d)">{{ deviceLine(d) }}</span>
            </span>
          </button>

          <!--
            Brightness is a device property, not an effect setting, so it sits
            in the device card. Only the selected card carries it: one slider
            per row would weigh down the column for a setting set once.

            Collapsed column: the card rules below restore the button alone, so
            the slider is hidden without having to be named.
          -->
          <div v-if="deviceKey === key(d)" class="brightness">
            <label class="brightness-head" :for="`brightness-${key(d)}`">
              <span class="brightness-label">{{ t('effects.brightness') }}</span>
              <span class="brightness-value">{{
                t('effects.percent', { n: brightnessPercent })
              }}</span>
            </label>
            <input
              :id="`brightness-${key(d)}`"
              type="range"
              min="0"
              :max="BRIGHTNESS_MAX"
              step="1"
              :value="brightness"
              :disabled="!d.open"
              @input="onBrightness($event, false)"
              @change="onBrightness($event, true)"
            />
          </div>
        </div>

        <p v-if="!controlled.length" class="none">
          {{ t('effects.noDevice') }}
          <RouterLink to="/devices" class="link">{{ t('effects.chooseDevice') }}</RouterLink>
        </p>
      </div>
    </section>

    <!-- --------------------------------------------------------- effects -->
    <section class="col effects" :class="{ shut: shutEffects }" :aria-label="t('effects.columns.effects')">
      <div class="col-head">
        <p class="col-title">{{ t('effects.columns.effects') }}</p>
        <button
          class="collapse"
          type="button"
          aria-controls="col-effects"
          :aria-expanded="!shutEffects"
          :aria-label="shutEffects ? t('effects.expandEffects') : t('effects.collapseEffects')"
          @click="shutEffects = !shutEffects"
        >
          {{ shutEffects ? '›' : '‹' }}
        </button>
      </div>

      <div id="col-effects" ref="effectsList" class="col-body">
        <template v-for="g in grouped" :key="g.nature">
          <!--
            The title attribute names the button once the column is collapsed
            and only the chevron is left.
          -->
          <button
            :id="`fx-section-head-${g.nature}`"
            class="group"
            type="button"
            :aria-expanded="!foldedSections[g.nature]"
            :aria-controls="`fx-section-${g.nature}`"
            :title="g.title"
            @click="toggleSection(g.nature)"
          >
            <svg
              class="chevron"
              viewBox="0 0 10 10"
              width="10"
              height="10"
              aria-hidden="true"
              fill="none"
              stroke="currentColor"
              stroke-width="1.6"
            >
              <path d="M3.5 2l3 3-3 3" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
            <span class="group-title">{{ g.title }}</span>
          </button>
          <!--
            `v-show` rather than `v-if`: the element named by `aria-controls`
            must exist while folded, and `display: none` already takes its
            entries out of the tab order.
          -->
          <div
            v-show="!foldedSections[g.nature]"
            :id="`fx-section-${g.nature}`"
            class="group-items"
            role="group"
            :aria-labelledby="`fx-section-head-${g.nature}`"
          >
            <!--
              "applied", and not "active". The earlier word stood for both
              states at once, yet they have nothing in common: one says what
              the keyboard does, the other what is being watched. The selection
              already reads from `aria-pressed` and from the border.
            -->
            <button
              v-for="c in g.items"
              :key="c.id"
              class="entry"
              type="button"
              :data-effect="c.id"
              :aria-pressed="selectedEffect?.id === c.id"
              :aria-label="entryLabel(c)"
              :title="c.name"
              @click="chosenEffect = c.id"
            >
              <EffectSwatch class="mark" :colors="c.swatch" />
              <span class="fx-name">{{ c.name }}</span>
              <span v-if="activeId !== c.id && c.state === 'broken'" class="fx-state broken">
                {{ t('effects.entryBroken') }}
              </span>
              <span v-else-if="activeId !== c.id && c.state === 'stale'" class="fx-state stale">
                {{ t('effects.entryStale') }}
              </span>
              <!--
                Marks rather than words, so that several fit side by side in a
                narrow column (#224): the button's label reads them out, their
                titles name them on hover. Applied comes last, so its check
                stays in one column down the list.

                A keycap for an effect that reads the keys pressed: what it
                reads is said on screen (`docs/design/key-input.md` §3). Waves,
                not the Wi-Fi fan, for a signal: it can come from this computer
                alone.
              -->
              <svg
                v-if="c.readsKeys"
                class="fx-keys"
                viewBox="0 0 16 16"
                width="14"
                height="14"
                aria-hidden="true"
                fill="none"
                stroke="currentColor"
                stroke-width="1.4"
                stroke-linejoin="round"
              >
                <title>{{ t('effects.readsKeys') }}</title>
                <rect x="2" y="2.5" width="12" height="11" rx="2.5" />
                <rect x="4.5" y="4.5" width="7" height="5" rx="1.2" />
              </svg>
              <svg
                v-if="readsSignalHere(c)"
                class="fx-signal"
                viewBox="0 0 16 16"
                width="14"
                height="14"
                aria-hidden="true"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
              >
                <title>{{ t('effects.entrySignal') }}</title>
                <circle cx="8" cy="8" r="1.4" fill="currentColor" stroke="none" />
                <path d="M5.2 5.2a4 4 0 0 0 0 5.6M10.8 5.2a4 4 0 0 1 0 5.6" />
                <path d="M3 3a7 7 0 0 0 0 10M13 3a7 7 0 0 1 0 10" />
              </svg>
              <svg
                v-if="activeId === c.id"
                class="fx-applied"
                viewBox="0 0 16 16"
                width="14"
                height="14"
                aria-hidden="true"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <title>{{ t('effects.entryApplied') }}</title>
                <path d="M3.5 8.5l3 3 6-7" />
              </svg>
            </button>
          </div>
        </template>

        <button class="new" type="button" @click="router.push({ name: 'editor' })">
          <span class="plus" aria-hidden="true">＋</span>
          <span>{{ t('effects.new') }}</span>
        </button>
        <button
          class="new"
          type="button"
          :disabled="working"
          :title="t('effects.refreshTitle')"
          @click="refreshEffects"
        >
          <span class="plus" aria-hidden="true">↻</span>
          <span>{{ t('effects.refresh') }}</span>
        </button>
        <button class="new" type="button" :title="t('effects.openFolderTitle')" @click="openFolder">
          <span class="plus" aria-hidden="true">↗</span>
          <span>{{ t('effects.openFolder') }}</span>
        </button>
        <!--
          With the column's other actions rather than in the Built-in heading,
          which is the folding button: this one stays reachable folded, and in
          the collapsed column.
        -->
        <button
          v-if="missingBuiltinNames.length"
          class="new"
          type="button"
          :disabled="working"
          :title="t('effects.restoreTitle', { names: missingBuiltinNames.join(', ') })"
          @click="restore(missingBuiltinNames)"
        >
          <span class="plus" aria-hidden="true">↺</span>
          <span>{{ t('effects.restoreBuiltins', { n: missingBuiltinNames.length }) }}</span>
        </button>
      </div>
    </section>

    <!-- --------------------------------------------------------- settings -->
    <section class="col detail" :aria-label="t('effects.columns.settings')">
      <FailureNote v-if="listError" class="failure" @close="listError = null">
        {{ listError }}
      </FailureNote>
      <FailureNote v-if="problem" class="failure" @close="problem = null">{{ problem }}</FailureNote>
      <FailureNote v-if="applyError" class="failure" @close="dismissApplyError">
        {{ applyError }}
      </FailureNote>
      <FailureNote v-if="paramsError" class="failure" @close="dismissParamsError">
        {{ paramsError }}
      </FailureNote>
      <!-- The effect that raised keeps raising: nothing here can close this one. -->
      <FailureNote v-if="status?.error" class="failure" :closable="false">
        {{ t('effects.effectError', { error: status.error }) }}
      </FailureNote>
      <FailureNote v-if="deviceTrouble" class="notice warn" @close="hushed = deviceTrouble">
        {{ deviceTrouble }}
      </FailureNote>
      <!--
        The preview's error is distinct from the applied effect's, and says so:
        an effect being watched can throw while another lights the keyboard
        flawlessly. Confusing them would send people looking in the wrong place.
      -->
      <FailureNote v-if="previewError" class="notice warn" :closable="false">
        {{ t('effects.previewError', { error: previewError }) }}
      </FailureNote>

      <!--
        The library can be browsed without a device: one must be able to see
        what the application offers before allowing anything. The preview runs
        anyway, on the default layout, without writing anything anywhere. But
        the screen says what is missing and where to go.
      -->
      <p v-for="key in missingEffects" :key="key" class="notice warn" role="status">
        {{ t('effects.missing', { name: nameOfKey(key) }) }}
        <button
          v-if="isShippedKey(key) && missingBuiltinNames.includes(nameOfKey(key))"
          class="link"
          type="button"
          :disabled="working"
          @click="restore([nameOfKey(key)])"
        >
          {{ t('effects.restore') }}
        </button>
        <button class="link" type="button" @click="forgetMissing(key)">
          {{ t('effects.forgetSettings') }}
        </button>
      </p>

      <p v-if="noDevice" class="notice" role="status">
        {{ t('effects.noDeviceNotice') }}
        <RouterLink to="/devices" class="link">{{ t('effects.chooseDevice') }}</RouterLink>
      </p>

      <template v-if="selectedEffect">
        <header class="fx-head">
          <h1>{{ selectedEffect.name }}</h1>
          <span class="badge" :class="selectedEffect.nature">
            {{
              selectedEffect.modified
                ? t('effects.modified', { nature: t(`effects.natures.${selectedEffect.nature}`) })
                : t(`effects.natures.${selectedEffect.nature}`)
            }}
          </span>
          <span v-if="selectedEffect.readsKeys" class="badge keys">{{ t('effects.readsKeys') }}</span>
          <span v-if="selectedEffect.readsClock" class="badge keys">{{
            t('effects.readsClock')
          }}</span>
          <span v-if="selectedEffect.readsSignals" class="badge keys">{{
            t('effects.readsSignals')
          }}</span>
        </header>

        <p class="desc">{{ selectedEffect.description }}</p>
        <!-- A load error its author reads, and copies into the editor: it stays
             as long as the file does not compile. -->
        <FailureNote v-if="selectedEffect.state === 'broken'" class="failure" :closable="false">
          {{ selectedEffect.error }}
        </FailureNote>
        <p class="cost">{{ cost(selectedEffect.nature) }}</p>

        <div class="preview">
          <p class="cost">{{ previewNote }}</p>
          <!--
            The layout comes from Rust: it is null for the length of a round
            trip. No empty keyboard is drawn in the meantime.
          -->
          <KeyboardSimulator v-if="board" class="sim" :layout="board" :frame="frame" />
          <p v-if="board && !opened" class="cost">{{ t('effects.preview.defaultLayout') }}</p>
        </div>

        <!--
          The settings, generated from the manifest. The form knows no effect in
          particular: it knows the four kinds of `ParamSpec`, and nothing else.
        -->
        <EffectParamsForm
          :specs="specs"
          :values="paramValues"
          :bindings="paramBindings"
          :bindable="selectedEffect.hardware === null"
          :signals="held"
          :receiving="receiving"
          :frozen="frozen"
          :empty="noParams"
          @change="onParamChange"
          @commit="onParamCommit"
          @bind="onParamBind"
          @reset="onParamReset"
        />

        <p v-if="signalsOff" class="warn">{{ t('effects.signalsOff') }}</p>

        <!-- What is remembered, said where it is made. See `savedNote`. -->
        <p v-if="savedNote" class="cost">{{ savedNote }}</p>

        <footer class="actions">
          <button
            class="solid"
            :disabled="!selectedDevice || working || applied || selectedEffect.state !== 'ready'"
            @click="applyEffect"
          >
            {{ applied ? t('effects.applied') : t('effects.apply') }}
          </button>

          <!--
            Stops the **device**'s loop, not the selected effect's: it is the one
            that writes, whatever effect is being watched.
          -->
          <button v-if="runningHere" class="ghost" :disabled="working" @click="halt">
            {{ t('effects.stop') }}
          </button>

          <!-- A rule interrupts the device: the gesture that ends it now. -->
          <button v-if="status?.interruption" class="ghost" :disabled="working" @click="resume">
            {{ t('effects.resume') }}
          </button>

          <!-- A hardware effect has no code: saying so is better than letting
               people click into the void. -->
          <button
            class="ghost"
            :disabled="selectedEffect.hardware !== null"
            @click="router.push({ name: 'editor', params: { id: selectedEffect.id } })"
          >
            {{ selectedEffect.nature === 'builtin' ? t('effects.viewCode') : t('effects.edit') }}
          </button>

          <button
            v-if="selectedEffect.hardware === null"
            class="ghost"
            :disabled="working"
            @click="duplicateSelected"
          >
            {{ t('effects.duplicate') }}
          </button>

          <button
            v-if="selectedEffect.modified"
            class="ghost"
            :disabled="working"
            @click="pendingRestore = selectedEffect.id"
          >
            {{ t('effects.restoreOriginal') }}
          </button>

          <!--
            Offered for the user's effects only.

            It stays in place and enabled while the question is asked: hiding or
            disabling it would take keyboard focus away at the very moment it
            must reach the answer, which follows in the document.
          -->
          <button
            v-if="removable"
            class="ghost danger"
            :disabled="working"
            @click="pendingRemoval = selectedEffect.id"
          >
            {{ t('effects.delete') }}
          </button>

          <span class="spacer" />

          <p class="cost">
            {{ selectedEffect.hardware ? t('effects.footerHardware') : t('effects.footerPreview') }}
          </p>
        </footer>

        <!--
          The question is asked in the column, not in a modal box: it stays next
          to what it describes, and does not prevent looking elsewhere while
          thinking it over.

          **After** the button that triggers it, and that is what makes the
          whole keyboard path: the answer is the next tab stop. The title
          carries `role="alert"`: the box appears without anything moving on
          screen, so it has to be announced.
        -->
        <div
          v-if="pendingRemoval"
          class="confirm"
          role="group"
          aria-labelledby="confirm-remove"
        >
          <p id="confirm-remove" class="confirm-title" role="alert">
            {{ t('effects.deleteTitle', { name: selectedEffect.name }) }}
          </p>
          <!--
            What goes, spelled out. The source is the only irreplaceable item on
            the list: settings can be redone, the loop restarted, hand-written
            code cannot be reinstalled.
          -->
          <p class="cost">{{ t('effects.deleteDetail') }}</p>
          <div class="confirm-actions">
            <button class="solid danger" :disabled="working" @click="removeEffect">
              {{ t('effects.deleteConfirm') }}
            </button>
            <button class="ghost" :disabled="working" @click="pendingRemoval = null">
              {{ t('effects.cancel') }}
            </button>
          </div>
        </div>

        <!-- Same placement and keyboard path as the removal question above. -->
        <div
          v-if="pendingRestore"
          class="confirm"
          role="group"
          aria-labelledby="confirm-restore"
        >
          <p id="confirm-restore" class="confirm-title" role="alert">
            {{ t('effects.restoreOriginalTitle', { name: selectedEffect.name }) }}
          </p>
          <p class="cost">{{ t('effects.restoreOriginalDetail') }}</p>
          <div class="confirm-actions">
            <button class="solid danger" :disabled="working" @click="restoreOriginal">
              {{ t('effects.restoreOriginal') }}
            </button>
            <button class="ghost" :disabled="working" @click="pendingRestore = null">
              {{ t('effects.cancel') }}
            </button>
          </div>
        </div>
      </template>
    </section>
  </section>
</template>

<style scoped>
/*
 * The two collapsible widths are **variables**, not competing rules: `.shut-1`
 * and `.shut-2` each write their own, and the breakpoint redefines
 * `grid-template-columns` once and for all. None of the three has to win over
 * the others: there is nothing to settle.
 */
.studio {
  --col-devices: 212px;
  --col-effects: 244px;

  display: grid;
  grid-template-columns: var(--col-devices) var(--col-effects) minmax(0, 1fr);
  height: 100%;
  min-height: 0;
}

.studio.shut-1 {
  --col-devices: 40px;
}

.studio.shut-2 {
  --col-effects: 40px;
}

.col {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  background: var(--raised);
  border-right: 1px solid var(--line);
}

.col-head {
  display: flex;
  gap: var(--gap-2);
  align-items: center;
  padding: var(--gap-2) var(--gap-2) var(--gap-2) var(--gap-3);
  border-bottom: 1px solid var(--line);
}

.col-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  color: var(--text-faint);
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  white-space: nowrap;
}

.collapse {
  flex: none;
  width: 22px;
  height: 22px;
  border: 1px solid transparent;
  border-radius: var(--r-sm);
  color: var(--text-faint);
  font-size: 13px;
  line-height: 1;
}

.collapse:hover {
  color: var(--accent);
  background: var(--raised-2);
  border-color: var(--line);
}

/* Scroll container: nothing can overflow sideways out of a collapsed column,
   whatever mistake is made further down. */
.col-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: var(--gap-2) var(--gap-1) var(--gap-3);
}

.entry {
  display: flex;
  gap: var(--gap-2);
  align-items: center;
  width: 100%;
  padding: 6px var(--gap-2);
  border: 1px solid transparent;
  border-radius: var(--r-md);
  text-align: left;
}

.entry:hover {
  background: var(--raised-2);
}

/*
 * A device is framed by its card, not its button: the card also holds the
 * brightness slider, and framing the button alone would leave the slider
 * outside the selection it belongs to.
 */
.card {
  border: 1px solid transparent;
  border-radius: var(--r-md);
}

/* Backed by the "active" mark for effects, and by the position in the list for
   devices: the amber border carries nothing alone. */
.effects .entry[aria-pressed="true"],
.card.selected {
  background: var(--raised-2);
  border-color: var(--accent);
}

.glyph {
  position: relative;
  display: inline-flex;
  flex: none;
  color: var(--text-muted);
}

/* On the pictogram's lower right corner. Not `.badge`, which is the kind label
   of the effect panel. */
.state-badge {
  position: absolute;
  right: -4px;
  bottom: -3px;
}

.entry[aria-pressed="true"] .glyph {
  color: var(--accent);
}

.entry-text {
  display: block;
  min-width: 0;
}

.dev-name {
  display: block;
  font-weight: 500;

  /* A product name is long: it wraps rather than being truncated, since it is
     what tells two keyboards of the same brand apart. `anywhere` covers the
     case of an unbroken model number. */
  overflow-wrap: anywhere;
}

.dev-fx {
  display: block;
  overflow: hidden;
  color: var(--accent);
  font-size: 11px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/*
 * Aligned with the device name, not the card edge, so the slider reads as part
 * of that device. The left offset adds up the button's border, padding, glyph
 * width and gap.
 */
.brightness {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 0 var(--gap-2) 6px calc(1px + var(--gap-2) + 18px + var(--gap-2));
}

.brightness-head {
  display: flex;
  flex-wrap: wrap;
  gap: 0 var(--gap-2);
  align-items: baseline;
  color: var(--text-faint);
  font-size: 11px;
}

.brightness-label {
  flex: 1;
}

/* The figure spelled out: a slider without a value is not set back to the same
   place from one session to the next, and that is precisely what is remembered
   here. */
.brightness-value {
  color: var(--text-muted);
  font-variant-numeric: tabular-nums;
}

.brightness input {
  width: 100%;
  accent-color: var(--accent);
}

.brightness input:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.group {
  display: flex;
  gap: var(--gap-1);
  align-items: center;
  width: 100%;
  margin: var(--gap-3) 0 var(--gap-1);
  padding: 2px var(--gap-2);
  border-radius: var(--r-sm);
  color: var(--text-faint);
  font-size: 10px;
  letter-spacing: 0.08em;
  text-align: left;
  text-transform: uppercase;
}

.group:hover {
  color: var(--text-muted);
}

.group:first-child {
  margin-top: 0;
}

.chevron {
  flex: none;
  transition: transform 120ms ease;
}

/* Keyed on `aria-expanded` itself, so the chevron cannot disagree with what
   assistive technology announces. */
.group[aria-expanded="true"] .chevron {
  transform: rotate(90deg);
}

.fx-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.fx-state {
  flex: none;
  color: var(--accent);
  font-size: 10px;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.fx-state.broken {
  color: var(--bad);
}

.fx-state.stale {
  color: var(--text-faint);
}

.fx-keys,
.fx-signal {
  flex: none;
  color: var(--text-faint);
}

.fx-applied {
  flex: none;
  color: var(--accent);
}

/* The swatch is decorative: it already carries `aria-hidden`. Making it
   transparent to the pointer lets the button's tooltip through: once the column
   is collapsed, it is the only place where the effect's name can still be read. */
.mark {
  pointer-events: none;
}

.new {
  width: 100%;
  margin-top: var(--gap-3);
  padding: 8px;
  border: 1px dashed var(--line-strong);
  border-radius: var(--r-md);
  color: var(--text-muted);
  font-size: 12px;
}

.new:hover:not(:disabled) {
  color: var(--accent);
  border-color: var(--accent);
}

.new + .new {
  margin-top: var(--gap-2);
}

.plus {
  margin-right: var(--gap-1);
}

/*
 * ---------------------------------------------------------------- collapsing
 *
 * The same rule twice, in the same place: **hide every child, then explicitly
 * restore the only one that stays**.
 *
 * This is not a stylistic detail. Listing what is hidden ("hide the name, hide
 * the effect") already produced a specificity collision here: a rule added
 * elsewhere for the name won over the hiding, and the text came back to
 * overflow into 40 px. Written this way, the rule survives the addition of a
 * child or a class:
 *
 * 1. the universal selector covers what does not exist yet: a child added
 *    tomorrow is hidden without anyone having to think about it;
 * 2. the restoring is more specific than the hiding, and it is here, two lines
 *    away from it, not in another block;
 * 3. any rule that could compete with them is guarded by `:not(.shut)`: it
 *    **does not apply** in the collapsed state, instead of winning or losing a
 *    specificity contest;
 * 4. `.col-body` is a scroll container: even a faulty rule could not make the
 *    column overflow onto its neighbor.
 */
@media (width > 820px) {
  .col.shut .col-head {
    justify-content: center;
    padding-inline: var(--gap-1);
  }

  .col.shut .col-title {
    display: none;
  }

  .col.shut .col-body {
    padding-inline: var(--gap-1);
  }

  /* First level: the body shows only device cards, section headers with their
     entries, and the add button. Everything else — messages, hints, whatever
     gets added — disappears without having to be named. */
  .col.shut .col-body > * {
    display: none;
  }

  .col.shut .col-body > .card {
    display: block;
  }

  /* Inside a card only the selection button survives: the brightness slider
     has no room in 40 px and goes with the rest. */
  .col.shut .card > * {
    display: none;
  }

  .col.shut .card > .entry {
    display: flex;
  }

  .col.shut .col-body > .group {
    display: flex;
  }

  /* A folded section keeps its own `display: none`, set inline by `v-show`,
     which this rule cannot override. */
  .col.shut .col-body > .group-items {
    display: block;
  }

  .col.shut .col-body > .new {
    display: block;
  }

  /* Second level: inside an entry, a single child survives: the device icon or
     the color swatch. */
  .col.shut .entry > * {
    display: none;
  }

  .col.shut .entry > .glyph {
    display: block;
  }

  .col.shut .entry > .mark {
    display: flex;

    /* 34 px do not fit in the 32 usable px of a collapsed column: the swatch
       narrows rather than making the column scroll sideways. */
    width: 26px;
  }

  .col.shut .entry {
    justify-content: center;
    align-items: center;
    gap: 0;
    padding-inline: 0;
  }

  /* Same form for the add button: everything hidden, the sign restored. */
  .col.shut .new > * {
    display: none;
  }

  .col.shut .new > .plus {
    display: inline;
    margin-right: 0;
  }

  .col.shut .new {
    padding-inline: 0;
  }

  /*
   * A section header shrinks to a rule and its chevron, same form as above:
   * everything hidden, the chevron restored. It stays a button, because a
   * header reduced to a bare rule would leave a folded section impossible to
   * reopen without first expanding the column.
   */
  .col.shut .group > * {
    display: none;
  }

  .col.shut .group > .chevron {
    display: block;
  }

  .col.shut .group {
    justify-content: center;
    margin: var(--gap-2) 0 var(--gap-1);
    padding: 2px 0;
    border-top: 1px solid var(--line);
    border-radius: 0;
  }

  .col.shut .group:first-child {
    margin-top: 0;
    border-top: none;
  }

  /*
   * Guarded by `:not(.shut)`, and that is the clause that matters.
   *
   * A device's name wraps, so its entry aligns to the top. Without this guard,
   * the rule **would apply** in the collapsed state and would have to lose a
   * specificity contest against the hiding: exactly the trap this interface
   * already fell into.
   */
  .devices:not(.shut) .entry {
    align-items: flex-start;
  }

  .devices:not(.shut) .glyph {
    margin-top: 3px;
  }
}

/*
 * ------------------------------------------------------------------ settings
 */
.detail {
  overflow-y: auto;
  padding: var(--gap-4);
  gap: var(--gap-3);
  background: var(--ground);
  border-right: none;
}

.fx-head {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-2);
  align-items: baseline;
}

.fx-head h1 {
  min-width: 0;
  overflow-wrap: anywhere;
}

.badge {
  flex: none;
  padding: 2px var(--gap-2);
  border-radius: 99px;
  background: var(--raised-2);
  color: var(--text-muted);
  font-size: 11px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.badge.user {
  background: var(--accent-soft);
  color: var(--accent);
}

.badge.hardware {
  background: color-mix(in srgb, var(--ok) 14%, transparent);
  color: var(--ok);
}

/* Not a warning: a fact about what the effect reads, said where it is chosen. */
.badge.keys {
  border: 1px solid var(--line-strong);
  background: none;
}

.desc {
  max-width: 68ch;
  color: var(--text-muted);
  font-size: 13px;
}

/* The real cost, the state notes and the hints: same voice, the quietest. */
.cost {
  max-width: 68ch;
  color: var(--text-faint);
  font-size: 12px;
}

/* As Automations says it: the same state, the same look. */
.warn {
  margin: 0;
  padding: var(--gap-2) var(--gap-3);
  border: 1px solid var(--warn);
  border-radius: var(--r-md);
  background: color-mix(in srgb, var(--warn) 10%, transparent);
  font-size: 13px;
}

.preview {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
}

/* The drawing takes no more than its share: the column also holds the settings
   and the actions, and a keyboard pushing the rest off screen would mean
   scrolling to find a button. Bounded in **width** and not in height: the SVG
   keeps its proportions, a maximum height would crop it. */
.sim {
  max-width: 760px;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-2);
  align-items: center;
  margin-top: auto;
  padding-top: var(--gap-3);
  border-top: 1px solid var(--line);
}

.spacer {
  flex: 1;
}

.solid {
  padding: 6px var(--gap-3);
  background: var(--accent);
  border-radius: var(--r-md);
  color: var(--accent-ink);
  font-size: 13px;
  font-weight: 500;
}

.ghost {
  padding: 6px var(--gap-3);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  color: var(--text-muted);
  font-size: 13px;
}

.ghost:hover:not(:disabled) {
  color: var(--text);
  background: var(--raised-2);
}

.solid:disabled,
.ghost:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

/*
 * A gesture with no way back. Color does not say it alone: the label announces
 * the removal, and the confirmation lists what goes: a color-blind person reads
 * the same thing as everyone else.
 */
.danger {
  color: var(--bad);
  border-color: var(--bad);
}

.solid.danger {
  background: var(--bad);
  color: var(--accent-ink);
}

.ghost.danger:hover:not(:disabled) {
  color: var(--bad);
  background: color-mix(in srgb, var(--bad) 12%, transparent);
}

.confirm {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  padding: var(--gap-3);
  border: 1px solid var(--bad);
  border-radius: var(--r-md);
  background: color-mix(in srgb, var(--bad) 8%, var(--raised));
}

.confirm-title {
  font-weight: 600;

  /* An effect's name is free-form: it wraps rather than overflowing. */
  overflow-wrap: anywhere;
}

.confirm-actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-2);
}

.notice,
.failure {
  padding: var(--gap-3);
  border-radius: var(--r-md);
  font-size: 13px;
}

.notice {
  background: var(--raised);
  border: 1px solid var(--line);
  color: var(--text-muted);
}

/* A gap between what was asked and what is happening. Color does not carry it
   alone: the text says it too. */
.notice.warn {
  background: color-mix(in srgb, var(--warn) 12%, var(--raised));
  border-color: var(--warn);
  color: var(--text);
}

.failure {
  background: color-mix(in srgb, var(--bad) 10%, transparent);
  border: 1px solid var(--bad);
}

.none {
  color: var(--text-faint);
  font-size: 12px;
}

.link {
  color: var(--accent);
  text-decoration: none;
  white-space: nowrap;
}

.link:hover {
  text-decoration: underline;
}

/*
 * Narrow window: the three columns stack. Collapsing no longer makes sense
 * there (a full-width column reduced to a strip of icons would gain nothing),
 * so its rules are **entirely** in the wide breakpoint, and the button
 * disappears rather than toggling a state with no effect.
 */
@media (width <= 820px) {
  .studio {
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: auto auto minmax(0, 1fr);
    overflow-y: auto;
  }

  .col {
    border-right: none;
    border-bottom: 1px solid var(--line);
  }

  .detail {
    border-bottom: none;
  }

  .collapse {
    display: none;
  }

  /* The two lists give way to the content, without disappearing. */
  .devices .col-body,
  .effects .col-body {
    max-height: 24vh;
  }
}
</style>
