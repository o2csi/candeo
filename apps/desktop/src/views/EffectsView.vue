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
 *
 * ## A split device: a header over its regions
 *
 * Each region runs its own effect (`docs/design/regions.md` §3). The device
 * becomes a header nobody selects, its regions the cards that are: the effects
 * offered, *Applied*, *Apply*, *Stop*, the settings and what the brightness
 * follows are the selected region's. The brightness slider, the state, the
 * interruption and *Resume* stay the device's, and the drawing stays the whole
 * device, the region in full colour.
 *
 * ## Layers: a stack under the target, a card per selected row
 *
 * The selected device's card, or the selected region's, lists its layers
 * (`docs/design/layers-ui.md` §2): *Add a layer*, the layers top first, the
 * base. The selected row is what the gallery previews into — the rest of the
 * stack around it — and what the sending button sends to, labelled by its
 * outcome: *Apply*, *Apply to this layer*, *Add the layer*. Selecting writes
 * nothing, as before. The detail panel frames that row as a card (§3): the
 * base's is the panel as it was, a layer's adds *Mask and blend* and *Save the
 * layer…*. A layer's look and settings change live on the device and are
 * saved at once, as an applied effect's are (#218).
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
  type Bindings,
  type Dimming,
  type EffectEntry,
  type EffectParams,
  type EffectState,
  type EngineReport,
  type HeldSignal,
  type PresetRecord,
} from '../api/candeo'
import { effectName as nameOfKey, isShippedKey } from '../api/effectKey'
import { message } from '../api/journal'
import {
  addLayer,
  deleteMask,
  deleteSavedLayer,
  maskChoices,
  maskOfPositions,
  orderLayers,
  rememberLayerLook,
  rememberLayerParams,
  removeLayer,
  renameMask,
  renameSavedLayer,
  replaceLayer,
  resolveMask,
  restoreSavedLayer,
  saveLayer,
  saveMask,
  setLayerBindings,
  setLayerLook,
  setLayerParams,
  setPreviewLayerBindings,
  setPreviewLayerParams,
  setPreviewLook,
  undeleteMask,
  updateMask,
  type Blend,
  type DeletedMask,
  type LayerRecord,
  type Look,
  type Mask,
  type MaskChoices,
  type PreviewBase,
  type SavedLayerRecord,
} from '../api/layers'
import { deviceOf, regionOf, targetFor, targetKey, type Target } from '../api/target'
import type { DeviceInfo, DeviceRef, LayoutInfo, RegionInfo, Rgb } from '../api/types'
import BrightnessFollow from '../components/BrightnessFollow.vue'
import DetailCard from '../components/DetailCard.vue'
import EffectParamsForm from '../components/EffectParamsForm.vue'
import EffectPresets from '../components/EffectPresets.vue'
import LayerLook from '../components/LayerLook.vue'
import LayerStack from '../components/LayerStack.vue'
import SaveDialog from '../components/SaveDialog.vue'
import SavedLayers from '../components/SavedLayers.vue'
import { declaredBindings, readsSignal, readsSound, readsSystem, rebound } from '../composables/bindings'
import DeviceStatusDot from '../components/DeviceStatusDot.vue'
import EffectSwatch from '../components/EffectSwatch.vue'
import FailureNote from '../components/FailureNote.vue'
import KeyboardSimulator from '../components/KeyboardSimulator.vue'
import RegionGlyph from '../components/RegionGlyph.vue'
import {
  applyLabel,
  atLimit,
  BASE,
  cardKicker,
  cleanLook,
  cleanMask,
  countText,
  droppedOrder,
  freshId,
  inOrder,
  layerLine,
  layerRecord,
  layerRow,
  layerSaveItems,
  layerToSave,
  lookOf,
  maskKey,
  maskName,
  maskSaveItems,
  movedOrder,
  nameTaken,
  newLayer,
  previewKey,
  previewLayers,
  previewState,
  previews,
  proposedName,
  rowState,
  savedLine,
  savedLook,
  savedMaskName,
  selectedLayer,
  standing,
  stackItems,
  usageOf,
  withLayers,
  withLook,
  withMask,
  withSettings,
  type EditedLayer,
  type MaskAction,
  type MaskContext,
  type MaskPick,
  type SavedItem,
  type SavedLayerView,
  type Selection,
  type StackRowView,
} from '../composables/layers'
import { deviceStatus, statusLabel } from '../composables/deviceStatus'
import { brightnessSlider } from '../composables/dimming'
import { goesLive, interruptionLine, showsDeviceFrames } from '../composables/interruption'
import { deviceEffect } from '../composables/effectSelection'
import { presetSettings } from '../composables/presets'
import { useDevice } from '../composables/useDevice'
import { useLayouts } from '../composables/layouts'
import { isSplit, regionStatus } from '../composables/regions'
import { applies, wants, type Offers } from '../composables/applies'
import { valveGames } from '../composables/games'
import { isLook } from '../keyboard/illustration'
import {
  colourBytes,
  hardwareEffectsFor,
  hardwareParams,
  useEffects,
  type HardwareEffect,
} from '../composables/useEffects'
import { apart, merge, sendLive, useSettings } from '../composables/useSettings'
import { refreshLibrary } from '../editor/library'
import { t } from '../i18n'
import { localized } from '../i18n/text'
import { painted, toggled } from '../keyboard/keySelection'
import { useSimulatorFeed, type PreviewedStack } from '../keyboard/simulatorFeed'

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
const { devices, current, select, selectRegion, regionOn, busy, refresh } = useDevice()
const { layoutOf, ensure: ensureLayouts } = useLayouts()
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
  replace,
  presetsOf,
  savePreset,
  updatePreset,
  renamePreset,
  deletePreset,
  dropEffect,
  referencedEffects,
  lastAppliedOn,
  brightnessOf,
  setBrightness,
  dimmingOf,
  setDimming,
  stacks,
  stackOf,
  installStack,
  savedLayers,
  installSavedLayers,
  savedMasks,
  installSavedMasks,
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

// ---------------------------------------------------------------- regions

/** The selected device runs one effect per region. */
const split = computed(() => isSplit(selectedDevice.value))

/**
 * The region selected on the selected device, `null` while it is whole: the
 * amber frame is on it, and everything the device's selection drove is its.
 */
const selectedRegion = computed<RegionInfo | null>(() =>
  selectedDevice.value ? regionOn(selectedDevice.value) : null,
)

/** What the effects, *Apply*, *Stop*, the settings and the follow act on. */
const target = computed<Target | null>(() =>
  selectedDevice.value ? targetFor(selectedDevice.value, selectedRegion.value?.id ?? null) : null,
)

/** Changes with the device and with the region: what reselects the effect. */
const targetId = computed(() => (target.value ? targetKey(target.value) : null))

function chooseRegion(d: DeviceRef, r: RegionInfo): void {
  selectRegion(d, r.id)
}

function regionSelected(d: DeviceInfo, r: RegionInfo): boolean {
  return deviceKey.value === key(d) && selectedRegion.value?.id === r.id
}

/** *Alienware 17 R5 · Keyboard*: where a setting is kept, said where it is made. */
const targetName = computed<string | null>(() => {
  const d = selectedDevice.value
  const r = selectedRegion.value
  if (!d) return null
  return r ? t('effects.regionOfDevice', { device: d.name, region: localized(r.name) }) : d.name
})

/** *Region Keyboard · 5 zones of 13*: the line under the drawing (§3). */
const regionShare = computed<string | null>(() => {
  const d = selectedDevice.value
  const r = selectedRegion.value
  if (!d || !r) return null
  const n = r.positions.length
  const params = { region: localized(r.name), n, total: d.lightCount }
  return t(`effects.regionShare.${r.lights}`, params, n)
})

/**
 * The column draws each region's pictogram from its device's layout, open or
 * not (`docs/design/regions.md` §11 c).
 */
watch(
  () => controlled.value.filter((d) => isSplit(d)),
  (list) => void ensureLayouts(list),
  { immediate: true },
)

/**
 * The warning waits for the search started at launch to end. Without this
 * `busy`, it would show for the length of the enumeration and then vanish:
 * announcing an absence not yet checked.
 */
const noDevice = computed(() => controlled.value.length === 0 && !busy.value)

// ---------------------------------------------------------------- layers

/** The selected target's layers, bottom to top. */
const stack = computed<LayerRecord[]>(() => stackOf(target.value))

/**
 * The row each target has selected, by target key: the base until someone
 * picks another. Kept per target as a split device keeps its region, and
 * written nowhere: selecting is a preview (`layers-ui.md` §9, P0 2).
 */
const selections = ref<Record<string, Selection>>({})

/** The selected row, the base when the layer it named is gone. */
const selection = computed<Selection>(() => {
  const k = targetId.value
  return standing((k !== null ? selections.value[k] : undefined) ?? BASE, stack.value)
})

/** The layer selected, `null` for the base and the new layer. */
const layer = computed(() => selectedLayer(selection.value, stack.value))

/** A saved layer picked in *Your layers*, previewed whole in the selected row's place (§5.3). */
const savedPicked = ref<string | null>(null)
const savedPick = computed<SavedLayerRecord | null>(() =>
  savedPicked.value === null
    ? null
    : (savedLayers.value.find((s) => s.name === savedPicked.value) ?? null),
)

/** *Only this layer*: the layer alone over black, in the preview only. */
const solo = ref(false)

/** The new layer's mask and look, kept while the gallery is browsed (§4.1). */
const pending = ref<{ mask?: Mask; look: Look }>({ look: {} })

/** Said to screen readers: a layer moved, removed, added (§4.5). */
const announcement = ref('')

function announce(text: string): void {
  // Emptied first: the same sentence twice is still news.
  announcement.value = ''
  void nextTick(() => (announcement.value = text))
}

// ---------------------------------------------------------------- library

type Nature = 'builtin' | 'user' | 'hardware'
/** A group of the column: a nature, or the effects made for a game. */
type Group = Nature | 'game'

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
  /** It is given the sound playing: captured while it runs, said on screen. */
  readsAudio: boolean
  /** It is given what the computer is doing: sampled while it runs, said on screen. */
  readsSystem: boolean
  /** The game it is for: its group is the games', shown only while they are on. */
  game: string | null
  /** The lights it makes sense on, and what it needs: see `applies`. */
  kinds: readonly string[]
  requires: readonly string[]
}

const library = ref<EffectEntry[]>([])
/** False until the library was read once: before that, every effect looks missing. */
const libraryRead = ref(false)
/** What reading or opening the library threw: the note says it. */
const listError = ref<unknown>(null)
/** What prevented applying, stopping or changing an effect, as it was thrown. */
const problem = ref<unknown>(null)

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
    readsClock: e.readsClock ?? false,
    readsSignals: e.readsSignals ?? false,
    readsAudio: e.readsAudio ?? false,
    readsSystem: e.readsSystem ?? false,
    game: e.game ?? null,
    ...wants(e),
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
    readsAudio: false,
    readsSystem: false,
    game: null,
    // The device's own: it goes where its firmware runs it.
    kinds: ['all'],
    requires: [],
  }
}

/**
 * An effect that would show nothing, or a degenerate row, on this device: one
 * reading key presses on a surface nobody types on, one drawn on rows of keys
 * on a few zones (`docs/design/device-sdk.md` §3). While no device is known,
 * nothing is left out.
 *
 * On a split device, checked against the selected region: a region drawn later
 * may offer less than its device (`docs/design/regions.md` §1).
 */
const offers = computed<Offers | null>(() => selectedRegion.value ?? board.value)

function offered(e: Choice): boolean {
  return offers.value === null || applies(e, offers.value)
}

/**
 * An effect made for a game shows only while games are on: someone who does not
 * play never sees them (`docs/design/game-state-integration.md` §5).
 */
function forGamesShown(e: Choice): boolean {
  return e.game === null || valveGames.value
}

const choices = computed<Choice[]>(() => [
  ...library.value.filter((e) => e.kind === 'builtin').map(fromEntry).filter(offered).filter(forGamesShown),
  ...library.value.filter((e) => e.kind === 'user').map(fromEntry).filter(offered).filter(forGamesShown),
  // Only what this device's firmware runs: offering a mode it does not know
  // would be offering an effect that never starts. None on a split device: a
  // firmware lights all of it and knows nothing of regions.
  ...(split.value ? [] : hardwareEffectsFor(board.value).map(fromHardware)),
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
const GROUPS: readonly Group[] = ['hardware', 'builtin', 'user', 'game']

/** Where an effect sits: with its game's, else with its nature's. */
function groupOf(c: Choice): Group {
  return c.game === null ? c.nature : 'game'
}

const grouped = computed(() =>
  GROUPS.map((nature) => ({
    nature,
    title: t(`effects.groups.${nature}`),
    items: choices.value.filter((c) => groupOf(c) === nature),
  })).filter(
    // *Off* keeps the hardware section on any whole device; a split one has none.
    (g) => (g.nature !== 'game' && g.nature !== 'hardware') || g.items.length > 0,
  )
    // A layer is composed from frames, which a firmware effect never sends
    // (`composition.md` §3.2): the base alone is offered one.
    .filter((g) => g.nature !== 'hardware' || selection.value.kind === 'base'),
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

/**
 * The games' group starts folded, the others open: effects for a game are
 * picked on its card, and the gallery is for the desk's lighting.
 */
function foldedByDefault(nature: Group): boolean {
  return nature === 'game'
}

function readSectionFolded(nature: Group): boolean {
  try {
    const kept = localStorage.getItem(SECTION_STORAGE_PREFIX + nature)
    return kept === null ? foldedByDefault(nature) : kept === 'folded'
  } catch {
    return foldedByDefault(nature)
  }
}

function writeSectionFolded(nature: Group, folded: boolean): void {
  try {
    // Removed when it is the default, so that a missing key keeps meaning it.
    if (folded === foldedByDefault(nature)) localStorage.removeItem(SECTION_STORAGE_PREFIX + nature)
    else localStorage.setItem(SECTION_STORAGE_PREFIX + nature, folded ? 'folded' : 'expanded')
  } catch {
    // See `SECTION_STORAGE_PREFIX`: the fold simply does not outlive the session.
  }
}

const foldedSections = ref<Record<Group, boolean>>({
  hardware: readSectionFolded('hardware'),
  builtin: readSectionFolded('builtin'),
  user: readSectionFolded('user'),
  game: readSectionFolded('game'),
})

/**
 * Folding only hides entries: the selection is left alone, so the settings
 * panel keeps showing the effect even when its section is folded.
 */
function toggleSection(nature: Group): void {
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

const selectedEffect = computed<Choice | null>(() => {
  if (!loaded.value) return null
  const found = choices.value.find((c) => c.id === chosenEffect.value)
  if (found) return found
  // The base falls back on the first effect; a new layer waits for one to be
  // chosen (*Choose an effect*), and a layer runs the effect it names.
  return selection.value.kind === 'base' ? (choices.value[0] ?? null) : null
})

/** The effects column's body, to bring the selected entry into view. */
const effectsList = ref<HTMLElement | null>(null)

/**
 * Selects the selected device's effect: the one running on it, else the one it
 * remembers (#118). On a split device, the selected region's.
 *
 * Only when the screen opens and when another device or region is chosen,
 * never on the engine refresh: an effect clicked since stays selected.
 */
async function followDevice(): Promise<void> {
  const d = target.value
  const row = selection.value
  // A layer's row shows the effect it runs; the new layer, none yet.
  chosenEffect.value =
    row.kind === 'new'
      ? null
      : row.kind === 'layer'
        ? (layer.value?.effect ?? null)
        : deviceEffect(
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

watch(targetId, () => {
  // What the previous target's row previewed is not this one's.
  leaveRow()
  pending.value = { look: {} }
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
const report = ref<EngineReport>({ devices: [], preview: null, sound: 'idle' })

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
function runningOn(d: Target | null): string | null {
  const s = statusOf(d)
  // A rule interrupting the device runs its own effect, but that effect is not
  // applied: the one the device goes back to still is (#106). On a split
  // device, each region goes back to its own (`docs/design/regions.md` §4).
  if (d && s?.interruption) return lastAppliedOn(d)
  const region = d ? regionOf(d) : null
  if (region !== null) {
    // A region runs no firmware effect: what its part of the loop runs, or nothing.
    const r = regionStatus(s, region)
    return r?.running === true && r.effectId !== null ? r.effectId : null
  }
  if (s?.running === true && s.effectId !== null) return s.effectId
  return appliedOn(d ? deviceOf(d) : null)
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
  // *Diagonal wave + 2 layers* (`layers-ui.md` §4.8).
  const layers = stackOf({ vid: d.vid, pid: d.pid }).length
  const running = effectName(runningOn(d))
  if (running !== null) return withLayers(running, layers)
  const remembered = effectName(lastAppliedOn({ vid: d.vid, pid: d.pid }))
  return remembered !== null
    ? t('effects.deviceStopped', { name: withLayers(remembered, layers) })
    : t('effects.deviceIdle')
}

/**
 * A split device's header line: the rule interrupting it, under its name and
 * not under each region (`docs/design/regions.md` §4). It gives back its
 * regions, which their own lines name: the sentence does not.
 */
function interruptionOf(d: DeviceRef): string | null {
  const interruption = statusOf(d)?.interruption
  if (!interruption) return null
  const rule = interruption.name || (effectName(interruption.effect) ?? interruption.effect)
  const line = interruptionLine(interruption, rule, null, Date.now())
  return t(line.key, line.params)
}

/**
 * Why a region's effect stopped on its own, when it failed: its lights are
 * black and the other regions go on (`docs/design/regions.md` §7).
 */
function regionFailure(d: DeviceRef, r: RegionInfo): string | null {
  const s = regionStatus(statusOf(d), r.id)
  return s !== null && !s.running ? s.error : null
}

/**
 * What a region does, in one line, as a device's line says it: in amber the
 * effect it runs, *no effect* for a region running nothing (§3).
 *
 * During an interruption the regions' effects are set aside, and each line
 * names the effect its region goes back to.
 */
function regionLine(d: DeviceRef, r: RegionInfo): string {
  const region = targetFor(d, r.id)
  const layers = stackOf(region).length
  const s = regionStatus(statusOf(d), r.id)
  if (s?.running === true && s.effectId !== null) {
    return withLayers(effectName(s.effectId) ?? s.effectId, layers)
  }
  if (regionFailure(d, r) !== null) return t('effects.entryBroken')
  const remembered = effectName(lastAppliedOn(region))
  if (remembered === null) return t('effects.deviceIdle')
  const line = withLayers(remembered, layers)
  return statusOf(d)?.interruption ? line : t('effects.deviceStopped', { name: line })
}

/** The only effect marked **applied**: the selected device's, or the selected region's. */
const activeId = computed(() => runningOn(target.value))

/**
 * The effect the gallery marks as the selected row's: the base's applied
 * effect, the layer's own, none for a new layer (`layers-ui.md` §4.2).
 */
const rowEffectId = computed<string | null>(() => {
  const row = selection.value
  if (row.kind === 'base') return activeId.value
  return row.kind === 'layer' ? (layer.value?.effect ?? null) : null
})

const status = computed(() => statusOf(selectedDevice.value))

/** The selected region's part of the device's loop, while the loop runs its regions. */
const regionState = computed(() =>
  selectedRegion.value ? regionStatus(status.value, selectedRegion.value.id) : null,
)

/** Whether a change to this effect's settings reaches the selected device live (#218). */
function live(effect: string, applied: string | null = activeId.value): boolean {
  return goesLive(effect, applied, Boolean(status.value?.interruption))
}
/** What runs here, for *Stop*: on a split device, the selected region's effect. */
const runningHere = computed(() =>
  selectedRegion.value ? regionState.value?.running === true : status.value?.running === true,
)

/** The applied effect's error, the region's on a split device: the others go on. */
const effectError = computed(() =>
  selectedRegion.value ? (regionState.value?.error ?? null) : (status.value?.error ?? null),
)

/**
 * The preview in progress, when it does show the selected effect — or, for a
 * stack, the base asked for with layers under way: its own report carries
 * them, a single effect's does not.
 */
const preview = computed(() => {
  const p = report.value.preview
  const wanted = stackPreview.value
  if (!p) return null
  if (wanted) {
    const runs = p.running || (p.layers ?? []).some((l) => l.running)
    const sameBase = (p.effectId ?? null) === (wanted.base?.effect ?? null)
    return runs && sameBase && (p.layers !== undefined || wanted.layers.length === 0) ? p : null
  }
  if (!p.running) return null
  return p.effectId === selectedEffect.value?.id ? p : null
})

/** Whether the preview runs the layer `id` now: what its live changes go to. */
function previewRuns(id: string | null): boolean {
  return id !== null && (preview.value?.layers ?? []).some((l) => l.layer === id)
}

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
  if (!p) return null
  const wanted = stackPreview.value
  if (wanted) {
    // The row being edited first: it is the one being looked at.
    const edited = (p.layers ?? []).find((l) => l.layer === editedId.value)
    if (edited?.error) return edited.error
    const base = wanted.base?.effect ?? null
    return base !== null && !p.running && p.effectId === base ? p.error : null
  }
  if (p.running || p.effectId !== selectedEffect.value?.id) return null
  return p.error
})

async function refreshStatus(): Promise<void> {
  try {
    report.value = await engineStatus()
  } catch (e) {
    problem.value = e
  }
}

// ---------------------------------------------------------------- preview

/**
 * Fallback layout, asked of Rust: something has to be drawn before a device is
 * open.
 */
const fallback = ref<LayoutInfo | null>(null)
/**
 * The selected device's layout, when it is really open, or when it is split:
 * a region's preview has the device's frame length, and is drawn over the
 * device's own lights, open or not (`docs/design/regions.md` §3).
 */
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
  [deviceKey, () => selectedDevice.value?.open === true, split],
  async ([wanted, isOpen, isSplit]) => {
    const d = selectedDevice.value
    if (!d || (!isOpen && !isSplit)) {
      opened.value = null
      return
    }
    // A layout that cannot be obtained is not a failure: the fallback draws.
    const layout = await getLayout({ vid: d.vid, pid: d.pid }).catch(() => null)
    // Another device chosen meanwhile: this answer is not for it.
    if (deviceKey.value === wanted) opened.value = layout
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

// ---------------------------------------------------------------- selected row

/** The parameters declared by the effect being watched. */
const specs = computed<Record<string, ParamSpec>>(() => selectedEffect.value?.params ?? {})

/**
 * Whether the row previews what the device does not run: another effect, a
 * saved layer, a new layer with its effect (`layers-ui.md` §3). What the state
 * line, *Cancel preview* and the sending button follow.
 */
const previewing = computed(() =>
  previews(
    selection.value,
    selectedEffect.value?.id ?? null,
    layer.value,
    savedPicked.value,
    applied.value,
  ),
)

/** The selected layer shows the effect it runs: its own settings are shown, and changed live. */
const ownLayer = computed(
  () =>
    layer.value !== null &&
    savedPick.value === null &&
    selectedEffect.value?.id === layer.value.effect,
)

/**
 * Settings changed on an effect previewed in a layer's row, before it is sent:
 * that row's and that effect's only, a layer's settings being its own
 * (`composition.md` §3.1). Dropped with the row or the effect.
 */
const draft = ref<{ key: string; values: EffectParams; bindings: Bindings } | null>(null)
const draftKey = computed(
  () =>
    `${targetId.value}|${JSON.stringify(selection.value)}|${selectedEffect.value?.id ?? ''}|${savedPicked.value ?? ''}`,
)

/** A layer row's settings: the layer's own, the draft, else the saved layer's or the effect's. */
const layerSettings = computed<{ values: EffectParams; bindings: Bindings }>(() => {
  const s = specs.value
  const l = layer.value
  if (ownLayer.value && l) {
    return { values: merge(s, l.values ?? {}), bindings: declaredBindings(s, l.bindings ?? {}) }
  }
  if (draft.value?.key === draftKey.value) {
    return { values: draft.value.values, bindings: draft.value.bindings }
  }
  const saved = savedPick.value
  return {
    values: merge(s, saved?.values ?? {}),
    bindings: declaredBindings(s, saved?.bindings ?? {}),
  }
})

/**
 * The values this effect runs on, or would run on, on this device: its
 * manifest, overridden by what was remembered for this pair. On a split
 * device, for the selected region: each keeps its own per effect. In a layer's
 * row, the layer's own (`layerSettings`).
 */
const paramValues = computed(() =>
  selection.value.kind === 'base'
    ? valuesFor(target.value, selectedEffect.value?.id ?? '', specs.value)
    : layerSettings.value.values,
)

/**
 * Its parameters reading a signal on this device. None for a firmware effect:
 * it has no loop of ours to read one, and its colours reach the firmware only
 * when it is applied.
 */
const paramBindings = computed(() => {
  const c = selectedEffect.value
  if (!c || c.hardware) return {}
  return selection.value.kind === 'base'
    ? bindingsFor(target.value, c.id, specs.value)
    : layerSettings.value.bindings
})

/** The selected row's mask and look: a saved layer's while one is previewed, the layer's, the new one's. */
const rowLook = computed<{ mask?: Mask; look: Look } | null>(() => {
  const saved = savedPick.value
  if (saved && selection.value.kind !== 'base') {
    const look = savedLook(saved)
    return { mask: look.mask, look: lookOf(look) }
  }
  const l = layer.value
  if (l) return { mask: cleanMask(l.mask), look: lookOf(l) }
  return selection.value.kind === 'new' ? pending.value : null
})

/** The row as the preview runs it in the gallery's effect; `null` while it runs as it is. */
const edited = computed<EditedLayer | null>(() => {
  const c = selectedEffect.value
  const row = selection.value
  if (row.kind === 'base' || !c || c.hardware || c.state !== 'ready') return null
  if (row.kind === 'layer' && !previewing.value) return null
  const look = rowLook.value ?? { look: {} }
  return {
    effect: c.id,
    values: apart(specs.value, paramValues.value),
    bindings: paramBindings.value,
    mask: look.mask,
    look: look.look,
  }
})

/** The id the selected row has in the preview: its own, or the new layer's. */
const editedId = computed<string | null>(() => {
  const row = selection.value
  if (row.kind === 'layer') return row.id
  return row.kind === 'new' && edited.value ? freshId(stack.value) : null
})

/**
 * The base a stack preview runs under the layers: the gallery's choice on the
 * base's row, the applied base otherwise; none for *Only this layer*.
 */
const previewBase = computed<PreviewBase | null>(() => {
  const row = selection.value
  if (solo.value && row.kind !== 'base') return null
  const where = target.value
  if (row.kind === 'base') {
    const c = selectedEffect.value
    return c && !c.hardware && c.state === 'ready'
      ? { effect: c.id, params: paramValues.value, bindings: paramBindings.value }
      : null
  }
  const id = runningOn(where) ?? lastAppliedOn(where)
  const c = choices.value.find((x) => x.id === id)
  if (!c || c.hardware || c.state !== 'ready') return null
  return {
    effect: c.id,
    params: valuesFor(where, c.id, c.params),
    bindings: bindingsFor(where, c.id, c.params),
  }
})

/**
 * What the simulator previews when it does not show the device: the stack, as
 * soon as the target has layers or a layer's row is selected, with the
 * selected row in the gallery's effect (`composition.md` §3.9). `null` keeps
 * the single effect's preview of a target without layers, as before them.
 */
const stackPreview = computed<PreviewedStack | null>(() => {
  const row = selection.value
  if (row.kind === 'base' && (stack.value.length === 0 || selectedEffect.value?.hardware)) return null
  const layers = previewLayers(stack.value, row, edited.value, solo.value && row.kind !== 'base')
  const base = previewBase.value
  if (base === null && layers.length === 0) return null
  return { key: previewKey(target.value, base?.effect ?? null, layers), base, layers }
})

/**
 * True when the simulator must show the device's stream: not a rule's, under
 * the applied effect's name. A layer's row as it runs, and the new layer before
 * its effect, show the device's frames: the stack, composed.
 */
const showsDevice = computed(() => {
  const interrupted = Boolean(status.value?.interruption)
  if (selection.value.kind === 'base') {
    return showsDeviceFrames(runningHere.value, applied.value, interrupted)
  }
  return showsDeviceFrames(runningHere.value, !previewing.value && !solo.value, interrupted)
})

/**
 * What the device's loop runs, as the feed needs it to follow its frames: it
 * changes when the loop may have been started again, with a new channel.
 */
const deviceLoop = computed<string | null>(() => {
  const s = status.value
  if (s?.running !== true) return null
  const parts = s.regions
    ? s.regions.map((r) => `${r.region}=${r.effectId ?? ''}`).join(',')
    : (s.effectId ?? '')
  return `${parts}|${s.interruption?.rule ?? ''}`
})

const { frame, restartPreview } = useSimulatorFeed({
  layout: () => board.value,
  device: () => target.value,
  showsDevice: () => showsDevice.value,
  // A split device's region: its preview laid over what the device shows.
  focus: () => selectedRegion.value?.positions ?? null,
  deviceLoop: () => deviceLoop.value,
  // A hardware effect is run by the firmware and the app never sees its frames:
  // previewing it would mean inventing them.
  previewed: () => {
    if (stackPreview.value) return null
    const c = selectedEffect.value
    // Nor an effect that cannot run: its JavaScript is not there, or does not load.
    return c && !c.hardware && c.state === 'ready' ? c.id : null
  },
  // A stack wins over the single effect: the target's layers around the row
  // being edited.
  previewedStack: () => stackPreview.value,
  // What it *is* drawn as, which is a different promise: a legend of what the
  // effect was seen doing, not the device's frames. The note below says so.
  illustrated: () => {
    const looks = selectedEffect.value?.hardware?.looks
    return isLook(looks) ? looks : null
  },
  // The drawing paints with what the effect was given, so that it says what the
  // keyboard will show rather than a colour of its own.
  illustratedColours: () => {
    const bytes = colourBytes(paramValues.value)
    const colours: Rgb[] = []
    for (let i = 0; i + 3 <= bytes.length; i += 3) colours.push([bytes[i], bytes[i + 1], bytes[i + 2]])
    return colours
  },
  params: () => paramValues.value,
  bindings: () => paramBindings.value,
  onError: (e) => {
    problem.value = e
  },
})

/*
 * What the simulator shows is said in one state line over it — *Live on the
 * device*, *Preview · nothing is sent*, *Hardware effect · approximate preview*
 * (`layers-ui.md` §3, `stateLine`) — where a sentence used to say both what was
 * watched and what the keyboard did. A preview that looks like an applied
 * effect costs a debugging session: the line never leaves it unsaid.
 */

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
  const where = target.value
  if (!c || !d || !where) return

  const device = { vid: d.vid, pid: d.pid }
  problem.value = null
  working.value = true
  try {
    // Never on a split device: its list offers no firmware effect.
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
      //
      // On a split device, on the selected region: the others go on.
      await startEffect(where, c.id, paramValues.value, paramBindings.value)
    }
  } catch (e) {
    problem.value = e
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
 *
 * On a split device, the selected region's: its lights go black, the others go
 * on (`docs/design/regions.md` §7).
 */
async function halt(): Promise<void> {
  const where = target.value
  if (!where) return

  problem.value = null
  working.value = true
  try {
    await stopEffect(where)
  } catch (e) {
    problem.value = e
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
    problem.value = e
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
    problem.value = e
  }
  try {
    await readLibrary()
    restartPreview()
  } catch (e) {
    listError.value = e
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
    problem.value = e
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
    problem.value = e
  } finally {
    working.value = false
  }
}

async function openFolder(): Promise<void> {
  try {
    await openEffectsDir()
  } catch (e) {
    listError.value = e
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
    listError.value = e
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
    problem.value = e
  } finally {
    working.value = false
    // The loop may have stopped: the engine state will only say so once read back.
    await refreshStatus()
  }
}

// ---------------------------------------------------------------- settings

// `specs`, `paramValues` and `paramBindings` are declared with the selected
// row, before the simulator feed whose first start reads them.

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
  return !c.hardware && readsSignal(c.readsSignals, bindingsFor(target.value, c.id, c.params))
}

/** Whether an effect reads the sound on this device: declared, or a setting following it. */
function readsSoundHere(c: Choice): boolean {
  return !c.hardware && readsSound(c.readsAudio, bindingsFor(target.value, c.id, c.params))
}

/** Whether an effect reads the computer on this device: declared, or a setting following it. */
function readsSystemHere(c: Choice): boolean {
  return !c.hardware && readsSystem(c.readsSystem, bindingsFor(target.value, c.id, c.params))
}

/** An entry as it is read out: its name, applied or not, and whether it reads a signal. */
function entryLabel(c: Choice): string {
  const name = rowEffectId.value === c.id ? t('effects.appliedOnDevice', { name: c.name }) : c.name
  const marks = [
    c.readsKeys ? t('effects.readsKeys') : null,
    readsSoundHere(c) ? t('effects.readsAudio') : null,
    readsSystemHere(c) ? t('effects.readsSystem') : null,
    readsSignalHere(c) ? t('effects.entrySignal') : null,
  ]
  return [name, ...marks.filter((mark) => mark !== null)].join(' · ')
}

/** The selected effect reads the sound, and it cannot be captured (#107): said under its settings. */
const soundUnavailable = computed(
  () =>
    selectedEffect.value !== null &&
    readsSound(selectedEffect.value.readsAudio, paramBindings.value) &&
    report.value.sound === 'unavailable',
)

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
  const where = target.value
  // A layer's settings are its own, kept with it: the card says what it is.
  if (selection.value.kind !== 'base') return null
  if (!c || c.hardware || Object.keys(specs.value).length === 0) return null
  if (!where || targetName.value === null) return t('effects.savedNoDevice')
  // *Alienware 17 R5 · Keyboard* on a split device: the region keeps them.
  return keptFor(where, c.id)
    ? t('effects.savedKept', { device: targetName.value })
    : t('effects.savedDeclared', { device: targetName.value })
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
  const where = target.value
  const c = selectedEffect.value
  if (!where || !c) return
  if (selection.value.kind !== 'base') {
    setLayerSettings({ ...paramValues.value, [id]: value }, paramBindings.value, false)
    return
  }
  const complete = adjust(where, c.id, specs.value, id, value, live(c.id))
  if (preview.value) adjustPreview(complete)
}

/**
 * A layer row's settings: the applied layer's, live on the device and in the
 * preview, written at the end of the gesture (`commit`) as an effect's are; or
 * the draft of an effect previewed in the row, to the preview alone.
 */
function setLayerSettings(values: EffectParams, bindings: Bindings, commit: boolean): void {
  const where = target.value
  const l = layer.value
  if (!where) return
  if (ownLayer.value && l) {
    const kept = apart(specs.value, values)
    installStack(
      where,
      stack.value.map((x) => (x.id === l.id ? withSettings(x, kept, bindings) : x)),
    )
    sendLive(`layer:${targetKey(where)}#${l.id}`, (v) => setLayerParams(where, l.id, v), values)
    if (previewRuns(l.id)) {
      sendLive(`preview-layer:${l.id}`, (v) => setPreviewLayerParams(l.id, v), values)
    }
    if (commit) void rememberLayerParams(where, l.id, values).catch(fail)
    return
  }
  draft.value = { key: draftKey.value, values, bindings }
  const id = editedId.value
  if (previewRuns(id)) {
    sendLive(`preview-layer:${id}`, (v) => setPreviewLayerParams(id!, v), values)
  }
}

/** A layer row's bindings: live and kept at once for an applied layer, a binding changing rarely. */
function setLayerBindingsHere(bindings: Bindings): void {
  const where = target.value
  const l = layer.value
  if (!where) return
  if (ownLayer.value && l) {
    installStack(
      where,
      stack.value.map((x) => (x.id === l.id ? withSettings(x, apart(specs.value, paramValues.value), bindings) : x)),
    )
    void setLayerBindings(where, l.id, bindings).catch(fail)
    if (previewRuns(l.id)) void setPreviewLayerBindings(l.id, bindings).catch(fail)
    return
  }
  draft.value = { key: draftKey.value, values: paramValues.value, bindings }
  const id = editedId.value
  if (previewRuns(id)) void setPreviewLayerBindings(id!, bindings).catch(fail)
}

/** What a layer command refused, said where the others are. */
function fail(e: unknown): void {
  problem.value = e
}

/** The gesture is over (slider released, box checked): write now. */
function onParamCommit(): void {
  const where = target.value
  const c = selectedEffect.value
  if (!where || !c) return
  if (selection.value.kind !== 'base') {
    const l = layer.value
    if (ownLayer.value && l) void rememberLayerParams(where, l.id, paramValues.value).catch(fail)
    return
  }
  settle(where, c.id)

  // **One rule for every effect: a setting changed reaches the one running.** A
  // host effect has a loop, adjusted live above; a firmware effect has none and
  // holds the last colour it was given, so reaching it means sending the effect
  // again. At the end of the gesture, not at every shade a picker travels
  // through, and only on the device already showing it — a whole one: a split
  // device offers no firmware effect.
  const device = deviceOf(where)
  if (c.hardware && live(c.id, appliedOn(device))) {
    void apply(device, c.hardware, colourBytes(paramValues.value))
  }
}

/**
 * A setting reads a signal, or its value again: the same destinations as a
 * value, by the same rule — the keyboard only when this effect runs there.
 */
function onParamBind(id: string, source: string | null): void {
  const where = target.value
  const c = selectedEffect.value
  if (!where || !c || c.hardware) return
  if (selection.value.kind !== 'base') {
    setLayerBindingsHere(declaredBindings(specs.value, rebound(paramBindings.value, id, source)))
    return
  }
  const bindings = bind(where, c.id, specs.value, id, source, live(c.id))
  if (preview.value) bindPreview(bindings)
}

/** Back to what the effect declares: its values, and no setting reading a signal. */
function onParamReset(): void {
  const where = target.value
  const c = selectedEffect.value
  if (!where || !c) return
  if (selection.value.kind !== 'base') {
    setLayerSettings(merge(specs.value, {}), {}, true)
    setLayerBindingsHere({})
    return
  }
  const declared = forget(where, c.id, specs.value, live(c.id))
  if (preview.value) {
    adjustPreview(declared)
    bindPreview({})
  }
}

// ---------------------------------------------------------------- presets

/** The form, for a preset picked to clear what its rows were waiting for. */
const paramsForm = ref<InstanceType<typeof EffectParamsForm> | null>(null)

/**
 * Presets, for a library effect with settings (`docs/design/presets.md` §5): a
 * firmware effect's id is no library key, and an effect without parameters has
 * nothing to name.
 */
const offersPresets = computed(() => {
  const c = selectedEffect.value
  return c !== null && c.hardware === null && Object.keys(specs.value).length > 0
})

const presets = computed(() => (selectedEffect.value ? presetsOf(selectedEffect.value.id) : []))

/** The settings shown, as a preset keeps them and as the list marks them. */
const shownSettings = computed(() => ({ values: paramValues.value, bindings: paramBindings.value }))

/** A preset can be picked where the settings can be kept, and while the form is live. */
const pickable = computed(() => target.value !== null && frozen.value === null)

/**
 * Gives the settings a preset's values and bindings, as if every slider had
 * moved at once: on the selected device or region, on disk, in the preview, and
 * on the device only when this effect runs there (#218). Nothing starts.
 */
function onPresetPick(preset: PresetRecord): void {
  const where = target.value
  const c = selectedEffect.value
  if (!where || !c || c.hardware) return
  const wanted = presetSettings(specs.value, preset)
  paramsForm.value?.clearDrafts()
  // Copied into a layer, as a preset is (`composition.md` §3.1): renaming or
  // deleting it later leaves the layer as it is.
  if (selection.value.kind !== 'base') {
    setLayerSettings(wanted.values, wanted.bindings, true)
    setLayerBindingsHere(wanted.bindings)
    return
  }
  const taken = replace(where, c.id, specs.value, wanted.values, wanted.bindings, live(c.id))
  if (preview.value) {
    adjustPreview(taken.values)
    bindPreview(taken.bindings)
  }
}

/** The effect whose presets the commands below change: the one shown. */
function presetEffect(): string {
  const c = selectedEffect.value
  if (!c || c.hardware) throw new Error('presets belong to a library effect')
  return c.id
}

const onPresetSave = (name: string) =>
  savePreset(presetEffect(), name, paramValues.value, paramBindings.value)
const onPresetUpdate = (name: string) =>
  updatePreset(presetEffect(), name, paramValues.value, paramBindings.value)
const onPresetRename = (from: string, to: string) => renamePreset(presetEffect(), from, to)
const onPresetDelete = (name: string) => deletePreset(presetEffect(), name)

// ---------------------------------------------------------------- layers: rows

/** What names masks on the selected target: its lights, its regions, the saved masks. */
const maskCtx = computed<MaskContext>(() => ({
  lights: selectedRegion.value?.lights ?? board.value?.lights ?? 'keys',
  region: selectedRegion.value !== null,
  regionName: (id) => {
    const r =
      board.value?.regions.find((x) => x.id === id) ??
      selectedDevice.value?.regions.find((x) => x.id === id)
    return r ? localized(r.name) : id
  },
  savedMasks: savedMasks.value,
}))

/**
 * What each mask lights on the selected target, by `maskKey`: asked of Rust once
 * per mask, the window never resolving one itself (`api/layers.ts`).
 */
const resolved = ref<Record<string, number[]>>({})
const resolving = new Set<string>()

/** The positions a mask lights here; `null` for the whole target, and while unknown. */
function positionsOf(mask: Mask | undefined): number[] | null {
  const where = target.value
  if (!where || !cleanMask(mask)) return null
  return resolved.value[maskKey(where, mask)] ?? null
}

/** How many lights a mask lights here: the whole target's when it names nothing. */
function countOf(mask: Mask | undefined): number | null {
  if (!cleanMask(mask)) return selectedRegion.value?.positions.length ?? board.value?.keys.length ?? null
  return positionsOf(mask)?.length ?? null
}

function resolveMissing(): void {
  const where = target.value
  if (!where) return
  for (const mask of [...stack.value.map((l) => l.mask), rowLook.value?.mask]) {
    const clean = cleanMask(mask)
    if (!clean) continue
    const k = maskKey(where, clean)
    if (k in resolved.value || resolving.has(k)) continue
    resolving.add(k)
    resolveMask(where, clean)
      .then((positions) => {
        resolved.value = { ...resolved.value, [k]: positions }
      })
      // Unknown rather than empty: a row then says nothing instead of *Nothing to light here*.
      .catch(() => {})
      .finally(() => resolving.delete(k))
  }
}

/** A saved mask changed, was deleted or put back: what masks light is asked again. */
function forgetResolved(): void {
  resolved.value = {}
}

watch([targetId, stack, () => rowLook.value?.mask], resolveMissing, { immediate: true })

/** What the mask picker offers on the selected target; `null` until Rust answered. */
const choicesHere = ref<MaskChoices | null>(null)

async function readMaskChoices(): Promise<void> {
  const where = target.value
  const wanted = targetId.value
  // Asked only once a layer's row is selected: the base has no mask.
  if (!where || selection.value.kind === 'base') return
  try {
    const answer = await maskChoices(where)
    if (targetId.value === wanted) choicesHere.value = answer
  } catch {
    // The picker keeps the whole target and *Choose keys…*.
    if (targetId.value === wanted) choicesHere.value = null
  }
}

watch(targetId, () => {
  choicesHere.value = null
})
watch(
  [targetId, () => selection.value.kind !== 'base', savedMasks],
  () => void readMaskChoices(),
  { immediate: true },
)

/** The engine's state of each layer of the selected target. */
const layerStatuses = computed(
  () => (selectedRegion.value ? regionState.value?.layers : status.value?.layers) ?? [],
)

/** An effect's swatch, from the library: empty for a firmware effect or one gone. */
function swatchOf(id: string | null): string[] {
  return library.value.find((e) => e.id === id)?.swatch ?? []
}

/** A layer just removed, in its place, until the next change to its stack (§4.5). */
const removed = ref<{ key: string; record: LayerRecord; at: number } | null>(null)
const removedHere = computed(() => (removed.value?.key === targetId.value ? removed.value : null))

/** The stack's rows, top first, as `LayerStack` words them. */
const stackRows = computed<Array<StackRowView | { kind: 'removed' }>>(() => {
  const ctx = maskCtx.value
  const row = selection.value
  return stackItems(stack.value, removedHere.value?.at ?? null).map((item) => {
    if (item.kind === 'removed') return item
    const l = item.layer
    const editing = row.kind === 'layer' && row.id === l.id
    // The row says what the preview shows in its place, as the card does.
    const effect = editing && previewing.value && selectedEffect.value ? selectedEffect.value.id : l.effect
    const look = editing && savedPick.value ? savedLook(savedPick.value) : l
    const positions = positionsOf(look.mask)
    const engine = editing && previewing.value ? undefined : layerStatuses.value.find((s) => s.layer === l.id)
    return {
      kind: 'layer' as const,
      id: l.id,
      name: effectName(effect) ?? effect,
      line: layerLine(look, ctx, positions?.length ?? null),
      swatch: swatchOf(effect),
      state: rowState(engine, positions),
      position: item.position,
    }
  })
})

/** The base's row: what runs under the layers, or what the gallery previews there. */
const baseRow = computed(() => {
  const where = target.value
  const id =
    selection.value.kind === 'base' && selectedEffect.value
      ? selectedEffect.value.id
      : (runningOn(where) ?? lastAppliedOn(where))
  return {
    name: effectName(id) ?? t('effects.deviceIdle'),
    line: firmwareRuns.value ? t('layers.baseHardware') : t('layers.base'),
    swatch: swatchOf(id),
  }
})

/** The add row's words while it is selected: *New layer · Rain*, or *choose an effect*. */
const newRowLabel = computed(() => {
  const c = selectedEffect.value
  if (selection.value.kind !== 'new' || !c) return t('layers.newChoose')
  return t('layers.newOf', { effect: savedPick.value?.name ?? c.name })
})

/** A hardware effect is the base: the layers are set aside (`composition.md` §3.2). */
const waiting = computed(() => firmwareRuns.value && stack.value.length > 0)

// ---------------------------------------------------------------- layers: selecting

/** Drops what belonged to the row left: a saved layer previewed, *Only this layer*, a selection, a draft. */
function leaveRow(): void {
  savedPicked.value = null
  solo.value = false
  selecting.value = null
  draft.value = null
  dialog.value = null
  maskDeleted.value = null
}

/** Selects a row of the stack: a preview, never a write (§9, P0 2). */
function selectRow(next: Selection): void {
  const k = targetId.value
  if (k === null) return
  leaveRow()
  selections.value = { ...selections.value, [k]: next }
  void followDevice()
}

/** An effect picked in the gallery: previewed in the selected row's place. */
function pickEffect(id: string): void {
  chosenEffect.value = id
  savedPicked.value = null
}

/** A saved layer picked: previewed whole, effect, settings, mask and look (§5.3). */
function pickSaved(name: string): void {
  const saved = savedLayers.value.find((s) => s.name === name)
  if (!saved) return
  savedPicked.value = name
  chosenEffect.value = saved.effect
}

/** Back to what the row runs: its effect, its look. */
function cancelPreview(): void {
  savedPicked.value = null
  draft.value = null
  void followDevice()
}

/** *Cancel preview* goes back to something only where something runs or is set. */
const canCancelPreview = computed(
  () => previewing.value && (selection.value.kind !== 'base' || activeId.value !== null),
)

/** *Only this layer*, for a layer, or a new layer with its effect. */
const offersSolo = computed(
  () => selection.value.kind === 'layer' || (selection.value.kind === 'new' && edited.value !== null),
)

// ---------------------------------------------------------------- layers: the card

const cardKickerText = computed(() =>
  cardKicker(
    selection.value,
    stack.value,
    savedPick.value?.name ?? null,
    Boolean(selectedEffect.value?.hardware),
  ),
)

/** The card's title: the effect shown, else what the layer names, else *Choose an effect*. */
const cardTitle = computed(() => {
  if (selectedEffect.value) return selectedEffect.value.name
  const l = layer.value
  if (l) return effectName(l.effect) ?? l.effect
  return t('layers.card.choose')
})

/** *Save the layer…* saves what the device runs: absent while the row previews (§5.2). */
const canSaveLayer = computed(
  () => layer.value !== null && !previewing.value && savedPick.value === null,
)

/** The effect's origin, as *Effect* says it: *Built-in*, *Yours*, *Hardware*. */
function originText(c: Choice): string {
  const origin = t(`layers.effect.${c.nature}`)
  return c.modified ? t('layers.effect.modified', { origin }) : origin
}

/** The one state the preview's header says (§3). */
const stateLine = computed(() =>
  previewState(Boolean(selectedEffect.value?.hardware), showsDevice.value),
)

/** The selected layer's lights, outlined in dashes over the frame (§4.3). */
const outlined = computed<number[] | null>(() => {
  if (selecting.value || selection.value.kind === 'base') return null
  if (selection.value.kind === 'new' && !edited.value) return null
  return positionsOf(rowLook.value?.mask)
})

/** How many layers name the saved mask in force. */
const maskUsage = computed(() => {
  const id = rowLook.value?.mask?.saved
  return id ? usageOf(id, stacks.value, savedLayers.value) : 0
})

// ---------------------------------------------------------------- layers: look and mask

/**
 * A look changed: the new layer's, kept with it; an applied layer's, live on
 * the device and in the preview, written at the end of the gesture (#218).
 */
function changeLook(patch: Look, commit: boolean): void {
  if (savedPick.value) return
  const where = target.value
  if (selection.value.kind === 'new') {
    const look = cleanLook({ ...pending.value.look, ...patch })
    pending.value = { ...pending.value, look }
    const id = editedId.value
    if (!solo.value && previewRuns(id)) sendLive(`preview-look:${id}`, (v) => setPreviewLook(id!, v), look)
    return
  }
  const l = layer.value
  if (!where || !l) return
  const look = cleanLook({ ...lookOf(l), ...patch })
  removed.value = null
  installStack(
    where,
    stack.value.map((x) => (x.id === l.id ? withLook(x, look) : x)),
  )
  sendLive(`look:${targetKey(where)}#${l.id}`, (v) => setLayerLook(where, l.id, v), look)
  // *Only this layer* draws it covering at full opacity, whatever its look.
  if (!solo.value && previewRuns(l.id)) {
    sendLive(`preview-look:${l.id}`, (v) => setPreviewLook(l.id, v), look)
  }
  if (commit) void rememberLayerLook(where, l.id, look).catch(fail)
}

function onBlend(blend: Blend): void {
  changeLook({ blend }, true)
}

function onOpacity(opacity: number, commit: boolean): void {
  changeLook({ opacity }, commit)
}

function onFollows(follows: Dimming | null, commit: boolean): void {
  changeLook({ follows: follows ?? undefined }, commit)
}

/**
 * Runs a layer command after the window installed what it asked for: on
 * refusal, says why. Either way the file and the engine are read back, which
 * puts the stack as it really is.
 */
async function layerCommand(command: () => Promise<unknown>): Promise<boolean> {
  problem.value = null
  try {
    await command()
    return true
  } catch (e) {
    problem.value = e
    return false
  } finally {
    await reloadSettings()
    await refreshStatus()
  }
}

/** Another mask: the new layer keeps it; an applied layer runs through it at once. */
async function changeMask(mask: Mask | undefined): Promise<void> {
  if (selection.value.kind === 'new') {
    pending.value = { ...pending.value, mask: cleanMask(mask) }
    return
  }
  const where = target.value
  const l = layer.value
  if (!where || !l || savedPick.value) return
  const next = withMask(l, mask)
  removed.value = null
  installStack(
    where,
    stack.value.map((x) => (x.id === l.id ? next : x)),
  )
  await layerCommand(() => replaceLayer(where, next))
}

function onMaskPick(pick: MaskPick): void {
  maskDeleted.value = null
  if (pick.kind === 'choose') void startChoosing(null)
  else if (pick.kind === 'mask') void changeMask(pick.mask)
}

function onMaskAction(action: MaskAction): void {
  const saved = rowLook.value?.mask?.saved
  if (action === 'edit') void startChoosing(saved ?? null)
  else if (action === 'saveAs') dialog.value = { kind: 'mask' }
  else if (action === 'rename' && saved) dialog.value = { kind: 'renameMask', id: saved }
  else if (action === 'delete' && saved) void removeSavedMask(saved)
}

// ---------------------------------------------------------------- layers: choosing keys

/** The keys being chosen, and the saved mask they edit, if they do. `null` out of a selection. */
const selecting = ref<{ positions: number[]; editing: string | null } | null>(null)

/** Turns the preview into a selection, on the row's mask or a saved mask's keys (§4.3, §5.4). */
async function startChoosing(editing: string | null): Promise<void> {
  const where = target.value
  if (!where) return
  const mask = cleanMask(editing !== null ? { saved: editing } : rowLook.value?.mask)
  // The whole target starts from nothing chosen: choosing is narrowing it.
  const positions = mask ? await resolveMask(where, mask).catch(() => positionsOf(mask) ?? []) : []
  selecting.value = { positions, editing }
}

function onToggleKey(position: number): void {
  const s = selecting.value
  if (s) selecting.value = { ...s, positions: toggled(s.positions, position) }
}

function onPaintKey(position: number, on: boolean): void {
  const s = selecting.value
  if (s) selecting.value = { ...s, positions: painted(s.positions, position, on) }
}

function selectNone(): void {
  const s = selecting.value
  if (s) selecting.value = { ...s, positions: [] }
}

function cancelChoosing(): void {
  selecting.value = null
}

/**
 * *Done*: the keys become the row's mask, or the saved mask's new keys — every
 * layer naming it follows, on every device. Refused at zero keys.
 */
async function finishChoosing(): Promise<void> {
  const s = selecting.value
  const where = target.value
  if (!s || !where || s.positions.length === 0) return
  problem.value = null
  try {
    const mask = await maskOfPositions(where, s.positions)
    if (s.editing !== null) {
      // Keys without a scancode come back as lights, which mean something on
      // this definition only: its model goes with them.
      const n = await updateMask(s.editing, mask, deviceOf(where))
      forgetResolved()
      announce(t('layers.announce.maskUpdated', { n }, n))
      await reloadSettings()
      restartPreview()
    } else {
      await changeMask(mask)
    }
    selecting.value = null
  } catch (e) {
    problem.value = e
  }
}

/** The selection bar's words: *Click the keys to light:*, or *Edit "Game keys", used by 3 layers:*. */
const selectingHint = computed(() => {
  const s = selecting.value
  if (!s) return ''
  if (s.editing !== null) {
    const name = savedMaskName(s.editing, savedMasks.value) ?? ''
    const n = usageOf(s.editing, stacks.value, savedLayers.value)
    return t('layers.select.editing', { name, n }, n)
  }
  return maskCtx.value.lights === 'zones' ? t('layers.select.hintZones') : t('layers.select.hintKeys')
})

const selectingCount = computed(() =>
  selecting.value ? countText(selecting.value.positions.length, maskCtx.value.lights) : '',
)

// ---------------------------------------------------------------- layers: order

async function reorder(order: string[] | null, id: string): Promise<void> {
  const where = target.value
  if (!order || !where) return
  removed.value = null
  installStack(where, inOrder(stack.value, order))
  announce(t('layers.announce.moved', { n: order.indexOf(id) + 1, total: order.length }))
  await layerCommand(() => orderLayers(where, order))
}

function moveRow(id: string, delta: 1 | -1): void {
  void reorder(movedOrder(stack.value, id, delta), id)
}

function dropRow(id: string, onto: string): void {
  void reorder(droppedOrder(stack.value, id, onto), id)
}

/** *Remove*: at once, no question; *Undo* puts it back with its place, mask and settings (§4.5). */
async function removeRow(id: string): Promise<void> {
  const where = target.value
  const k = targetId.value
  const at = stack.value.findIndex((l) => l.id === id)
  if (!where || k === null || at < 0) return
  const record = stack.value[at]
  if (selection.value.kind === 'layer' && selection.value.id === id) selectRow(BASE)
  installStack(
    where,
    stack.value.filter((l) => l.id !== id),
  )
  removed.value = { key: k, record, at }
  announce(t('layers.announce.removed'))
  await layerCommand(() => removeLayer(where, id))
}

async function undoRemove(): Promise<void> {
  const r = removedHere.value
  const where = target.value
  if (!r || !where) return
  removed.value = null
  problem.value = null
  try {
    const id = await addLayer(where, r.record, r.at)
    await reloadSettings()
    selectRow(layerRow(id))
    announce(t('layers.announce.restored'))
  } catch (e) {
    problem.value = e
    await reloadSettings()
  } finally {
    await refreshStatus()
  }
}

// ---------------------------------------------------------------- layers: sending

/** The sending button's words, by outcome (§3). */
const sendLabel = computed(() => t(applyLabel(selection.value, previewing.value)))

/** Whether the sending button sends something: a preview to put where the row is. */
const canSend = computed(() => {
  const c = selectedEffect.value
  if (!selectedDevice.value || working.value || !c || c.state !== 'ready' || !previewing.value) {
    return false
  }
  if (selection.value.kind === 'base') return true
  // A layer runs a library effect, through a mask that lights something here.
  return !c.hardware && countOf(rowLook.value?.mask) !== 0
})

/**
 * Sends the row's preview: *Apply* for the base, as before layers; *Add the
 * layer* on top; *Apply to this layer* in the layer's place, its mask and
 * look kept — or a saved layer's, copied (§5.1).
 */
async function send(): Promise<void> {
  const row = selection.value
  if (row.kind === 'base') return applyEffect()
  const where = target.value
  const c = selectedEffect.value
  const look = rowLook.value
  if (!where || !c || c.hardware || !look) return
  const parts: EditedLayer = {
    effect: c.id,
    values: apart(specs.value, paramValues.value),
    bindings: paramBindings.value,
    mask: look.mask,
    look: look.look,
  }
  problem.value = null
  working.value = true
  try {
    if (row.kind === 'new') {
      const id = await addLayer(where, newLayer(parts))
      pending.value = { look: {} }
      removed.value = null
      await reloadSettings()
      selectRow(layerRow(id))
      announce(t('layers.announce.added'))
    } else {
      const record = layerRecord(row.id, parts)
      await replaceLayer(where, record)
      installStack(
        where,
        stack.value.map((x) => (x.id === row.id ? record : x)),
      )
      removed.value = null
      savedPicked.value = null
      draft.value = null
      announce(t('layers.announce.replaced'))
    }
  } catch (e) {
    problem.value = e
  } finally {
    working.value = false
    await refreshStatus()
    await reloadSettings()
  }
}

// ---------------------------------------------------------------- layers: saved

/** *Your layers* shows for a layer's row and the new layer's: the base has no mask nor blend. */
const showSavedLayers = computed(
  () =>
    selection.value.kind !== 'base' &&
    (savedLayers.value.length > 0 || savedDeleted.value !== null),
)

const savedViews = computed<SavedLayerView[]>(() =>
  savedLayers.value.map((s) => ({
    name: s.name,
    line: savedLine(s, effectName(s.effect) ?? s.effect, maskCtx.value, null),
    swatch: swatchOf(s.effect),
    pressed: savedPicked.value === s.name,
  })),
)

/** A saved layer just deleted, at its place, until the next change to the list. */
const savedDeleted = ref<{ record: SavedLayerRecord; at: number } | null>(null)
const savedDeletedView = computed(() =>
  savedDeleted.value ? { at: savedDeleted.value.at, name: savedDeleted.value.record.name } : null,
)

async function deleteSaved(name: string): Promise<void> {
  const at = savedLayers.value.findIndex((s) => s.name === name)
  if (at < 0) return
  const record = savedLayers.value[at]
  problem.value = null
  try {
    installSavedLayers(await deleteSavedLayer(name))
    savedDeleted.value = { record, at }
    if (savedPicked.value === name) cancelPreview()
    announce(t('layers.announce.savedDeleted'))
  } catch (e) {
    problem.value = e
  }
}

async function undoDeleteSaved(): Promise<void> {
  const d = savedDeleted.value
  if (!d) return
  savedDeleted.value = null
  try {
    installSavedLayers(await restoreSavedLayer(d.record, d.at))
    announce(t('layers.announce.savedRestored'))
  } catch (e) {
    problem.value = e
  }
}

/** A saved mask just deleted, under the `select`, until the next change (§5.4). */
const maskDeleted = ref<{ deleted: DeletedMask; name: string; count: number } | null>(null)
const maskDeletedView = computed(() =>
  maskDeleted.value ? { name: maskDeleted.value.name, count: maskDeleted.value.count } : null,
)

/**
 * Deletes a saved mask: Rust writes its keys into every layer and saved layer
 * naming it. The new layer, which only the window knows, gets them here.
 */
async function removeSavedMask(id: string): Promise<void> {
  const name = savedMaskName(id, savedMasks.value) ?? ''
  problem.value = null
  try {
    const deleted = await deleteMask(id)
    maskDeleted.value = { deleted, name, count: deleted.layers.length + deleted.savedLayers.length }
    if (pending.value.mask?.saved === id) {
      const r = deleted.record
      pending.value = {
        ...pending.value,
        mask: cleanMask({ group: r.group, keys: r.keys, lights: r.lights, region: r.region }),
      }
    }
    forgetResolved()
    announce(t('layers.announce.maskDeleted'))
  } catch (e) {
    problem.value = e
  } finally {
    await reloadSettings()
    await refreshStatus()
  }
}

async function undoMaskDeletion(): Promise<void> {
  const d = maskDeleted.value
  if (!d) return
  maskDeleted.value = null
  problem.value = null
  try {
    await undeleteMask(d.deleted)
    forgetResolved()
    announce(t('layers.announce.maskRestored'))
  } catch (e) {
    problem.value = e
  } finally {
    await reloadSettings()
    await refreshStatus()
  }
}

/** Saves the row's chosen keys as a mask, and links the row to it (§5.4). */
async function saveAsMask(name: string, mask: Mask | undefined): Promise<void> {
  const where = target.value
  const clean = cleanMask(mask)
  if (!where || !clean) return
  // Lights and regions mean something on one definition only: its model goes with them.
  const model = clean.lights || clean.region ? deviceOf(where) : null
  const record = await saveMask(name, clean, model)
  installSavedMasks([...savedMasks.value, record])
  announce(t('layers.announce.maskSaved', { name }))
  await changeMask({ saved: record.id })
}

// ---------------------------------------------------------------- layers: dialog

/** The save dialog open, and for what. */
type Dialog =
  | { kind: 'layer' }
  | { kind: 'mask' }
  | { kind: 'renameMask'; id: string }
  | { kind: 'renameLayer'; name: string }
const dialog = ref<Dialog | null>(null)

interface DialogContent {
  title: string
  name: string
  items: SavedItem[]
  note?: string
  confirm: string
  taken: (name: string) => boolean
  submit: (name: string) => Promise<void>
}

const dialogContent = computed<DialogContent | null>(() => {
  const d = dialog.value
  if (!d) return null
  const ctx = maskCtx.value
  const layerNames = savedLayers.value.map((s) => s.name)
  const maskNames = savedMasks.value.map((m) => m.name)
  if (d.kind === 'layer') {
    const l = layer.value
    const c = selectedEffect.value
    if (!l || !c) return null
    const count = countOf(l.mask)
    return {
      title: t('layers.save.layerTitle'),
      name: proposedName(c.name, maskName(l.mask, ctx, count)),
      items: layerSaveItems(c.name, l, ctx, count),
      note: t('layers.save.layerNote'),
      confirm: t('layers.save.save'),
      taken: (name) => nameTaken(name, layerNames),
      // What the device runs, every declared value with it (§5.5).
      submit: async (name) => {
        installSavedLayers(await saveLayer(name, layerToSave(l, paramValues.value)))
        savedDeleted.value = null
        announce(t('layers.announce.layerSaved', { name }))
      },
    }
  }
  if (d.kind === 'mask') {
    const mask = rowLook.value?.mask
    return {
      title: t('layers.save.maskTitle'),
      name: '',
      items: maskSaveItems(countOf(mask) ?? 0, ctx.lights),
      note: t('layers.save.maskNote'),
      confirm: t('layers.save.save'),
      taken: (name) => nameTaken(name, maskNames),
      submit: (name) => saveAsMask(name, mask),
    }
  }
  if (d.kind === 'renameMask') {
    const current = savedMaskName(d.id, savedMasks.value) ?? ''
    const n = usageOf(d.id, stacks.value, savedLayers.value)
    return {
      title: t('layers.save.renameMaskTitle', { name: current }),
      name: current,
      items: [],
      note: t('layers.save.renameMaskNote', { n }, n),
      confirm: t('layers.save.rename'),
      taken: (name) => nameTaken(name, maskNames, current),
      submit: async (name) => {
        installSavedMasks(await renameMask(d.id, name))
        announce(t('layers.announce.renamed', { name }))
      },
    }
  }
  return {
    title: t('layers.save.renameLayerTitle', { name: d.name }),
    name: d.name,
    items: [],
    confirm: t('layers.save.rename'),
    taken: (name) => nameTaken(name, layerNames, d.name),
    submit: async (name) => {
      installSavedLayers(await renameSavedLayer(d.name, name))
      if (savedPicked.value === d.name) savedPicked.value = name
      savedDeleted.value = null
      announce(t('layers.announce.renamed', { name }))
    },
  }
})

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

/** The selected card's slider: none without a brightness command (#334). */
const slider = computed(() =>
  selectedDevice.value ? brightnessSlider(selectedDevice.value) : 'none',
)

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

/**
 * What the selected device's brightness follows (§2.2.2): on a split device,
 * the selected region's, which scales its lights alone
 * (`docs/design/regions.md` §4).
 */
const dimming = computed(() => dimmingOf(target.value))

/**
 * A hardware effect runs there: no frame of ours, so nothing to dim. On a split
 * device only a rule can bring one, over the whole device.
 */
const firmwareRuns = computed(() => {
  if (split.value) return status.value?.interruption?.effect.startsWith('hardware:') === true
  const id = runningOn(selectedDevice.value)
  return id !== null && choices.value.some((c) => c.id === id && c.hardware !== null)
})

function onDimming(next: Dimming | null, commit: boolean): void {
  const where = target.value
  if (!where) return
  setDimming(where, next, commit)
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
    listError.value = e
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
        <template v-for="d in controlled" :key="`${d.vid}:${d.pid}`">
          <!--
            A split device: a header nobody selects, over one card per region in
            the definition's order (`docs/design/regions.md` §3). The amber frame
            goes to the region, the slider stays in the header, what the
            brightness follows goes to the selected region's card.
          -->
          <div v-if="isSplit(d)" class="card split-dev">
            <div class="dev-head" :title="`${d.name} · ${statusLabel(deviceStatus(d))}`">
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
                <DeviceStatusDot class="state-badge" :device="d" />
              </span>
              <span class="entry-text">
                <span class="dev-name">{{ d.name }}</span>
                <span class="dev-sub">{{
                  t('effects.regionCount', { n: d.regions.length }, d.regions.length)
                }}</span>
                <!-- The rule interrupts the whole device: said here, not under each region. -->
                <span v-if="interruptionOf(d)" class="dev-fx" :title="interruptionOf(d) ?? ''">{{
                  interruptionOf(d)
                }}</span>
              </span>
            </div>

            <!-- The device's own, under the same condition as a whole device's card (#334). -->
            <div v-if="deviceKey === key(d) && slider !== 'none'" class="brightness">
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
                :disabled="slider === 'disabled'"
                @input="onBrightness($event, false)"
                @change="onBrightness($event, true)"
              />
            </div>

            <div class="regions" role="group" :aria-label="d.name">
              <div
                v-for="r in d.regions"
                :key="r.id"
                class="region"
                :class="{ selected: regionSelected(d, r) }"
              >
                <button
                  class="entry"
                  type="button"
                  :aria-pressed="regionSelected(d, r)"
                  :aria-label="`${localized(r.name)} · ${regionLine(d, r)}`"
                  :title="`${localized(r.name)} · ${regionLine(d, r)}`"
                  @click="chooseRegion(d, r)"
                >
                  <RegionGlyph :layout="layoutOf(d)" :positions="r.positions" />
                  <span class="entry-text">
                    <span class="reg-name">{{ localized(r.name) }}</span>
                    <!-- A failed region says why on hover: its lights are black, the others go on. -->
                    <span
                      class="dev-fx"
                      :class="{ failed: regionFailure(d, r) !== null }"
                      :title="regionFailure(d, r) ?? regionLine(d, r)"
                      >{{ regionLine(d, r) }}</span
                    >
                  </span>
                </button>
                <!-- What the region's brightness follows: its lights, its own source. -->
                <div v-if="regionSelected(d, r)" class="brightness">
                  <div v-if="slider === 'none'" class="brightness-head">
                    <span class="brightness-label">{{ t('effects.brightness') }}</span>
                  </div>
                  <BrightnessFollow
                    :id="`follow-${key(d)}-${r.id}`"
                    :dimming="dimming"
                    :firmware="firmwareRuns"
                    :signals="held"
                    @change="onDimming"
                  />
                </div>
                <!-- Each region carries its own stack when selected; the header none (§4.7). -->
                <LayerStack
                  v-if="regionSelected(d, r) && loaded"
                  :target="`${d.name} · ${localized(r.name)}`"
                  :rows="stackRows"
                  :base="baseRow"
                  :selection="selection"
                  :new-label="newRowLabel"
                  :full="atLimit(stack)"
                  :waiting="waiting"
                  @select="selectRow"
                  @move="moveRow"
                  @drop="dropRow"
                  @remove="removeRow"
                  @undo="undoRemove"
                />
              </div>
            </div>
          </div>

          <!--
            The card is a plain container, not the button: the brightness slider
            lives in it, and an interactive control nested in a button is invalid
            markup that would also re-select the device on every drag.
          -->
          <div v-else class="card" :class="{ selected: deviceKey === key(d) }">
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

              A device with no brightness command keeps the heading and what its
              brightness follows, without a slider that would write nothing.
            -->
            <div v-if="deviceKey === key(d)" class="brightness">
              <label v-if="slider !== 'none'" class="brightness-head" :for="`brightness-${key(d)}`">
                <span class="brightness-label">{{ t('effects.brightness') }}</span>
                <span class="brightness-value">{{
                  t('effects.percent', { n: brightnessPercent })
                }}</span>
              </label>
              <div v-else class="brightness-head">
                <span class="brightness-label">{{ t('effects.brightness') }}</span>
              </div>
              <input
                v-if="slider !== 'none'"
                :id="`brightness-${key(d)}`"
                type="range"
                min="0"
                :max="BRIGHTNESS_MAX"
                step="1"
                :value="brightness"
                :disabled="slider === 'disabled'"
                @input="onBrightness($event, false)"
                @change="onBrightness($event, true)"
              />
              <BrightnessFollow
                :id="`brightness-${key(d)}`"
                :dimming="dimming"
                :firmware="firmwareRuns"
                :signals="held"
                @change="onDimming"
              />
            </div>
            <!--
              The stack, under its target as regions are (`layers-ui.md` §2):
              only the selected card carries one, as only it carries the
              brightness.
            -->
            <LayerStack
              v-if="deviceKey === key(d) && loaded"
              :target="d.name"
              :rows="stackRows"
              :base="baseRow"
              :selection="selection"
              :new-label="newRowLabel"
              :full="atLimit(stack)"
              :waiting="waiting"
              @select="selectRow"
              @move="moveRow"
              @drop="dropRow"
              @remove="removeRow"
              @undo="undoRemove"
            />
          </div>
        </template>

        <p v-if="!controlled.length" class="none">
          {{ t('effects.noDevice') }}
          <RouterLink to="/devices" class="link">{{ t('effects.chooseDevice') }}</RouterLink>
        </p>
      </div>
      <!-- Outside the body, which a collapsed column hides: a layer moved is still said. -->
      <p class="sr-only" aria-live="polite">{{ announcement }}</p>
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
        <!-- Above the effects, for a layer's row and the new layer's (§5.3). -->
        <SavedLayers
          v-if="showSavedLayers"
          :items="savedViews"
          :deleted="savedDeletedView"
          @pick="pickSaved"
          @rename="(name) => (dialog = { kind: 'renameLayer', name })"
          @delete="deleteSaved"
          @undo="undoDeleteSaved"
        />
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
              :aria-pressed="savedPicked === null && selectedEffect?.id === c.id"
              :aria-label="entryLabel(c)"
              :title="c.name"
              @click="pickEffect(c.id)"
            >
              <EffectSwatch class="mark" :colors="c.swatch" />
              <span class="fx-name">{{ c.name }}</span>
              <span v-if="rowEffectId !== c.id && c.state === 'broken'" class="fx-state broken">
                {{ t('effects.entryBroken') }}
              </span>
              <span v-else-if="rowEffectId !== c.id && c.state === 'stale'" class="fx-state stale">
                {{ t('effects.entryStale') }}
              </span>
              <!--
                Marks rather than words, so that several fit side by side in a
                narrow column (#224): the button's label reads them out, their
                titles name them on hover. Applied comes last, so its check
                stays in one column down the list.

                A key pressed down for an effect that reads the keys pressed:
                what it reads is said on screen (`docs/design/key-input.md`
                §3). Not strokes around a key, which read as light here. Waves,
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
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <title>{{ t('effects.readsKeys') }}</title>
                <rect x="3" y="8.5" width="10" height="6" rx="1.8" />
                <path d="M8 1.5v4.5M5.8 3.9L8 6.1l2.2-2.2" />
              </svg>
              <svg
                v-if="readsSoundHere(c)"
                class="fx-sound"
                viewBox="0 0 16 16"
                width="14"
                height="14"
                aria-hidden="true"
                fill="none"
                stroke="currentColor"
                stroke-width="1.4"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <title>{{ t('effects.readsAudio') }}</title>
                <path d="M2.5 6h2.5l3.5-3v10l-3.5-3H2.5z" />
                <path d="M11 5.5a3.5 3.5 0 0 1 0 5" />
              </svg>
              <!-- A processor, pins on four sides: what the computer is doing. -->
              <svg
                v-if="readsSystemHere(c)"
                class="fx-system"
                viewBox="0 0 16 16"
                width="14"
                height="14"
                aria-hidden="true"
                fill="none"
                stroke="currentColor"
                stroke-width="1.4"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <title>{{ t('effects.readsSystem') }}</title>
                <rect x="4.5" y="4.5" width="7" height="7" rx="1.2" />
                <path
                  d="M6.5 2v2.5M9.5 2v2.5M6.5 11.5V14M9.5 11.5V14M2 6.5h2.5M2 9.5h2.5M11.5 6.5H14M11.5 9.5H14"
                />
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
                v-if="rowEffectId === c.id"
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
      <FailureNote
        v-if="listError"
        class="failure"
        :failure="listError"
        @close="listError = null"
      />
      <FailureNote v-if="problem" class="failure" :failure="problem" @close="problem = null" />
      <FailureNote v-if="applyError" class="failure" @close="dismissApplyError">
        {{ applyError }}
      </FailureNote>
      <FailureNote v-if="paramsError" class="failure" @close="dismissParamsError">
        {{ paramsError }}
      </FailureNote>
      <!-- The effect that raised keeps raising: nothing here can close this one. -->
      <FailureNote v-if="effectError" class="failure" :closable="false">
        {{ t('effects.effectError', { error: effectError }) }}
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

      <!--
        The preview, with one state in words over it (`layers-ui.md` §3); during
        a key selection, the selection's bar instead. The layout comes from Rust:
        it is null for the length of a round trip, and no empty keyboard is
        drawn in the meantime.
      -->
      <div v-if="board" class="preview">
        <div
          v-if="selecting"
          class="bar choosing"
          role="toolbar"
          :aria-label="t('layers.select.bar')"
          @keydown.esc.prevent="cancelChoosing"
        >
          <span class="hint">{{ selectingHint }}</span>
          <span class="count" aria-live="polite">{{ selectingCount }}</span>
          <span class="spacer" />
          <button type="button" class="ghost small" @click="selectNone">
            {{ t('layers.select.none') }}
          </button>
          <button type="button" class="ghost small" @click="cancelChoosing">
            {{ t('layers.select.cancel') }}
          </button>
          <button
            type="button"
            class="solid small"
            :disabled="selecting.positions.length === 0"
            @click="finishChoosing"
          >
            {{ selecting.editing !== null ? t('layers.select.update') : t('layers.select.done') }}
          </button>
        </div>
        <div v-else class="bar">
          <span class="state" :class="stateLine" role="status">
            <span class="dot" aria-hidden="true" />
            {{ t(`layers.state.${stateLine}`) }}
          </span>
          <span class="spacer" />
          <label v-if="offersSolo" class="solo">
            <input v-model="solo" type="checkbox" />
            {{ t('layers.solo') }}
          </label>
        </div>
        <!--
          On a split device, still the whole device: the selected region in
          full colour, the others faded with their real frames, and what the
          region's effect measures as its device drawn over it.
        -->
        <KeyboardSimulator
          class="sim"
          :layout="board"
          :frame="frame"
          :focus="selectedRegion?.positions"
          :focus-name="selectedRegion ? localized(selectedRegion.name) : undefined"
          :selecting="selecting?.positions ?? null"
          :outlined="outlined"
          @toggle="onToggleKey"
          @paint="onPaintKey"
          @done="finishChoosing"
          @cancel="cancelChoosing"
        />
        <p v-if="regionShare" class="cost">{{ regionShare }}</p>
        <p v-if="!opened" class="cost">{{ t('effects.preview.defaultLayout') }}</p>
      </div>

      <!--
        The card of the selected row (§3). Inert while keys are chosen: the
        selection ends by its own bar, *Done* or *Cancel*.
      -->
      <DetailCard
        v-if="selectedEffect || selection.kind !== 'base'"
        :inert="selecting !== null || undefined"
        :kicker="cardKickerText"
        :title="cardTitle"
        :swatch="selectedEffect ? selectedEffect.swatch : null"
        :hint="selection.kind === 'new' && !selectedEffect ? t('layers.card.chooseHint') : undefined"
        :saveable="canSaveLayer"
        @save="dialog = { kind: 'layer' }"
      >
        <!-- What concerns the base alone: the device's loop, and a rule interrupting it. -->
        <template v-if="selection.kind === 'base'" #actions>
          <!--
            Stops the **device**'s loop, not the selected effect's: it is the one
            that writes, whatever effect is being watched.
          -->
          <button v-if="runningHere" type="button" class="ghost" :disabled="working" @click="halt">
            {{ t('effects.stop') }}
          </button>
          <!-- A rule interrupts the device: the gesture that ends it now. -->
          <button
            v-if="status?.interruption"
            type="button"
            class="ghost"
            :disabled="working"
            @click="resume"
          >
            {{ t('effects.resume') }}
          </button>
        </template>

        <LayerLook
          v-if="rowLook && (selectedEffect || selection.kind === 'layer')"
          :mask="rowLook.mask"
          :look="rowLook.look"
          :read-only="savedPick !== null"
          :choices="choicesHere"
          :ctx="maskCtx"
          :count="countOf(rowLook.mask)"
          :usage="maskUsage"
          :deleted="maskDeletedView"
          :signals="held"
          @mask="onMaskPick"
          @mask-action="onMaskAction"
          @undo-mask="undoMaskDeletion"
          @blend="onBlend"
          @opacity="onOpacity"
          @follows="onFollows"
        />

        <section v-if="selectedEffect" class="section effect" aria-labelledby="effect-title">
          <div class="effect-head">
            <h2 id="effect-title" class="section-title">{{ t('layers.effect.title') }}</h2>
            <!-- The cost, which tells the natures apart, on hover of the origin. -->
            <span class="badge" :class="selectedEffect.nature" :title="cost(selectedEffect.nature)">
              {{ originText(selectedEffect) }}
            </span>
            <span v-if="selectedEffect.readsKeys" class="badge keys">{{ t('effects.readsKeys') }}</span>
            <span v-if="selectedEffect.readsClock" class="badge keys">{{
              t('effects.readsClock')
            }}</span>
            <span v-if="selectedEffect.readsSignals" class="badge keys">{{
              t('effects.readsSignals')
            }}</span>
            <span v-if="selectedEffect.readsAudio" class="badge keys">{{
              t('effects.readsAudio')
            }}</span>
            <span v-if="selectedEffect.readsSystem" class="badge keys">{{
              t('effects.readsSystem')
            }}</span>
            <span class="spacer" />
            <!--
              In *Effect*, not the action bar: they concern the effect alone
              (§3). A hardware effect has no code: none to view.
            -->
            <button
              v-if="selectedEffect.hardware === null"
              type="button"
              class="text-button"
              @click="router.push({ name: 'editor', params: { id: selectedEffect.id } })"
            >
              {{ selectedEffect.nature === 'builtin' ? t('effects.viewCode') : t('effects.edit') }}
            </button>
            <button
              v-if="selectedEffect.hardware === null"
              type="button"
              class="text-button"
              :disabled="working"
              @click="duplicateSelected"
            >
              {{ t('effects.duplicate') }}
            </button>
            <button
              v-if="selectedEffect.modified"
              type="button"
              class="text-button"
              :disabled="working"
              @click="pendingRestore = selectedEffect.id"
            >
              {{ t('effects.restoreOriginal') }}
            </button>
            <!--
              Offered for the user's effects only. It stays in place and enabled
              while the question is asked: hiding or disabling it would take
              keyboard focus away at the very moment it must reach the answer,
              which follows in the document.
            -->
            <button
              v-if="removable"
              type="button"
              class="text-button danger"
              :disabled="working"
              @click="pendingRemoval = selectedEffect.id"
            >
              {{ t('effects.delete') }}
            </button>
          </div>

          <p class="desc">{{ selectedEffect.description }}</p>
          <!-- A load error its author reads, and copies into the editor: it stays
               as long as the file does not compile. -->
          <FailureNote v-if="selectedEffect.state === 'broken'" class="failure" :closable="false">
            {{ selectedEffect.error }}
          </FailureNote>

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

          <!--
            Named settings for this effect, above the settings they fill: a
            preset is picked, then tuned. *Save as preset…* sits here, in what a
            preset keeps.
          -->
          <EffectPresets
            v-if="offersPresets"
            :presets="presets"
            :specs="specs"
            :shown="shownSettings"
            :pickable="pickable"
            :save="onPresetSave"
            :update="onPresetUpdate"
            :rename="onPresetRename"
            :remove="onPresetDelete"
            @pick="onPresetPick"
          />

          <!--
            The settings, generated from the manifest. The form knows no effect in
            particular: it knows the four kinds of `ParamSpec`, and nothing else.
          -->
          <EffectParamsForm
            ref="paramsForm"
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
          <p v-if="soundUnavailable" class="warn">{{ t('effects.soundUnavailable') }}</p>

          <!-- What is remembered, said where it is made. See `savedNote`. -->
          <p v-if="savedNote" class="cost">{{ savedNote }}</p>
        </section>
      </DetailCard>

      <!--
        The button that sends, labelled by its outcome, and *Cancel preview*
        while there is one: nothing else (§3, P0 4).
      -->
      <footer v-if="selectedEffect || selection.kind !== 'base'" class="actions" :inert="selecting !== null || undefined">
        <button type="button" class="solid" :disabled="!canSend" @click="send">
          {{ sendLabel }}
        </button>
        <button v-if="canCancelPreview" type="button" class="ghost" @click="cancelPreview">
          {{ t('layers.apply.cancelPreview') }}
        </button>
      </footer>

      <SaveDialog
        v-if="dialogContent"
        open
        :title="dialogContent.title"
        :name="dialogContent.name"
        :items="dialogContent.items"
        :note="dialogContent.note"
        :confirm="dialogContent.confirm"
        :taken="dialogContent.taken"
        :submit="dialogContent.submit"
        @close="dialog = null"
      />
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
  /* 264 px: a layer's row holds a swatch, a name, a line and its tools (`layers-ui.md` §2). */
  --col-devices: 264px;
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
 * A split device: framed neutral, as a group. Only a region is selected, and
 * only it carries the amber frame.
 */
.card.split-dev {
  background: color-mix(in srgb, var(--raised-2) 45%, transparent);
  border-color: var(--line);
}

/* The header: the device's pictogram, state, name and count, not a button. */
.dev-head {
  display: flex;
  gap: var(--gap-2);
  align-items: flex-start;
  padding: 6px var(--gap-2) 2px calc(1px + var(--gap-2));
}

.dev-sub {
  display: block;
  color: var(--text-faint);
  font-size: 11px;
}

.regions {
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 2px 4px 4px 14px;
}

.region {
  border: 1px solid transparent;
  border-radius: var(--r-md);
}

.region .entry {
  padding-block: 4px;
}

.region.selected {
  background: var(--raised-2);
  border-color: var(--accent);
}

.region.selected .rglyph {
  color: var(--accent);
}

.reg-name {
  display: block;
  font-size: 13px;
  font-weight: 500;
  overflow-wrap: anywhere;
}

/* Said in words too: *error*, and why on hover. */
.dev-fx.failed {
  color: var(--bad);
}

/*
 * Against the region card's own edge, not under its name: indented once already,
 * the card has no room left for the follow switch's four choices.
 */
.region .brightness {
  padding-left: var(--gap-2);
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
.fx-sound,
.fx-system,
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

  /* A split device keeps its pictogram with its state, and below it each
     region's pictogram alone, as an effect keeps its swatch. */
  .col.shut .card > .dev-head {
    display: flex;
    justify-content: center;
    padding-inline: 0;
  }

  .col.shut .dev-head > * {
    display: none;
  }

  .col.shut .dev-head > .glyph {
    display: block;
  }

  .col.shut .card > .regions {
    display: flex;
    padding-inline: 0;
  }

  .col.shut .region > * {
    display: none;
  }

  .col.shut .region > .entry {
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

  .col.shut .entry > .glyph,
  .col.shut .entry > .rglyph {
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

/* Over the drawing: its one state, or the selection's bar. */
.bar {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-2);
  align-items: center;
  min-height: 32px;
  max-width: 760px;
}

.bar.choosing {
  padding: 4px var(--gap-3);
  border: 1px solid var(--accent);
  border-radius: var(--r-md);
  background: var(--accent-soft);
  font-size: 13px;
}

.count {
  font-weight: 600;
}

/* In words, the colour a second channel: live, preview, hardware. */
.state {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  font-size: 12px;
  font-weight: 500;
}

.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: currentColor;
}

.state.live {
  color: var(--ok);
}

.state.preview {
  color: var(--accent);
}

.state.hardware {
  color: var(--text-muted);
}

.solo {
  display: inline-flex;
  gap: var(--gap-2);
  align-items: center;
  min-height: 24px;
  font-size: 13px;
}

.solo input {
  accent-color: var(--accent);
}

/* The card's *Effect* section: its title, the origin, and what concerns the effect alone. */
.effect {
  display: flex;
  flex-direction: column;
  gap: var(--gap-3);
}

.effect-head {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-2);
  align-items: center;
}

.section-title {
  color: var(--text-faint);
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

/* Actions in the accent's words, no frame: they sit in a heading's line. */
.text-button {
  min-height: 28px;
  padding: 4px 6px;
  border-radius: var(--r-sm);
  color: var(--accent);
  font-size: 13px;
}

.text-button:hover:not(:disabled) {
  background: var(--raised-2);
}

.text-button.danger {
  color: var(--bad);
}

.text-button:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.solid.small,
.ghost.small {
  min-height: 28px;
  padding: 3px var(--gap-3);
  font-size: 13px;
}

/* The drawing takes no more than its share: the column also holds the settings
   and the actions, and a keyboard pushing the rest off screen would mean
   scrolling to find a button. Bounded in **width** and not in height: the SVG
   keeps its proportions, a maximum height would crop it. */
.sim {
  max-width: 760px;
}

/* Kept in view at the bottom of the panel: the card above it runs long. */
.actions {
  position: sticky;
  bottom: calc(-1 * var(--gap-4));
  z-index: 1;
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-2);
  align-items: center;
  margin-top: auto;
  padding: var(--gap-3) 0;
  border-top: 1px solid var(--line);
  background: var(--ground);
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
