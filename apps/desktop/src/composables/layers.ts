/**
 * What the *Effects* view decides about layers, apart from drawing them
 * (`docs/design/layers-ui.md`): which row is selected and what the card says
 * of it, what the button that sends is called, what a stack preview runs,
 * how a mask, a blend and an opacity read, how the stack reorders, and what
 * the save dialog lists.
 *
 * Pure, so every decision is tested without a device or a window. The records
 * and the commands are `api/layers.ts`'s.
 */

import type { Bindings, Dimming, EffectParams } from '../api/candeo'
import {
  BLEND_DEFAULT,
  MAX_EFFECTS,
  OPACITY_DEFAULT,
  type Blend,
  type LayerEngineStatus,
  type LayerRecord,
  type Look,
  type Mask,
  type NewLayer,
  type SavedLayerRecord,
  type SavedMaskRecord,
  type StackRecord,
} from '../api/layers'
import { regionOf, type Target } from '../api/target'
import { t, type MessageKey } from '../i18n'
import { dimmingMode, dimmingSignal } from './dimming'
import { moved } from './rules'

// ---------------------------------------------------------------- selection

/**
 * The row of a target's stack the detail panel frames, and the gallery
 * previews into: the base, a layer by its id, or the layer about to be added.
 */
export type Selection = { kind: 'base' } | { kind: 'new' } | { kind: 'layer'; id: string }

export const BASE: Selection = { kind: 'base' }
export const NEW: Selection = { kind: 'new' }

export function layerRow(id: string): Selection {
  return { kind: 'layer', id }
}

export function sameSelection(a: Selection, b: Selection): boolean {
  return a.kind === b.kind && (a.kind !== 'layer' || (b.kind === 'layer' && a.id === b.id))
}

/**
 * The selection as it stands against the stack: a layer gone from it — removed
 * here, or from the file — gives way to the base rather than naming nothing.
 */
export function standing(selection: Selection, stack: readonly LayerRecord[]): Selection {
  if (selection.kind !== 'layer') return selection
  return stack.some((l) => l.id === selection.id) ? selection : BASE
}

/** The layer selected, `null` for the base and the new layer. */
export function selectedLayer(selection: Selection, stack: readonly LayerRecord[]): LayerRecord | null {
  return selection.kind === 'layer' ? (stack.find((l) => l.id === selection.id) ?? null) : null
}

/** A layer's place, 1 for the one just over the base: *Layer 2 of 3*. */
export function positionOf(stack: readonly LayerRecord[], id: string): number {
  return stack.findIndex((l) => l.id === id) + 1
}

// ---------------------------------------------------------------- look

/** The blend a layer draws with, its default when the record names none. */
export function blendOf(look: Look): Blend {
  return look.blend ?? BLEND_DEFAULT
}

/** The opacity a layer draws with, 0 to 100. */
export function opacityOf(look: Look): number {
  return look.opacity ?? OPACITY_DEFAULT
}

/**
 * A look as the file keeps it: no field at its default (`composition.md`
 * §3.11), the opacity a whole number from 0 to 100.
 */
export function cleanLook(look: Look): Look {
  const out: Look = {}
  const blend = blendOf(look)
  if (blend !== BLEND_DEFAULT) out.blend = blend
  const opacity = Math.min(100, Math.max(0, Math.round(opacityOf(look))))
  if (opacity !== OPACITY_DEFAULT) out.opacity = opacity
  if (look.follows) out.follows = look.follows
  return out
}

/** The look part of a layer, nothing else. */
export function lookOf(layer: Look): Look {
  return cleanLook({ blend: layer.blend, opacity: layer.opacity, follows: layer.follows })
}

export const BLENDS: readonly Blend[] = ['cover', 'add', 'screen', 'multiply']

export function blendName(blend: Blend): string {
  return t(`layers.blends.${blend}`)
}

export function blendSentence(blend: Blend): string {
  return t(`layers.blendSentences.${blend}`)
}

// ---------------------------------------------------------------- masks

/** What a mask is, as the picker groups it. */
export type MaskKind = 'whole' | 'group' | 'region' | 'saved' | 'chosen'

/**
 * A mask as the file keeps it: empty lists dropped, `undefined` for the whole
 * target, a saved mask's reference alone (`layers-ui.md` §5.5).
 */
export function cleanMask(mask: Mask | undefined): Mask | undefined {
  if (!mask) return undefined
  if (mask.saved !== undefined) return { saved: mask.saved }
  const out: Mask = {}
  if (mask.group !== undefined) out.group = mask.group
  if (mask.region !== undefined) out.region = mask.region
  if (mask.keys?.length) out.keys = [...mask.keys]
  if (mask.lights?.length) out.lights = [...mask.lights]
  return Object.keys(out).length > 0 ? out : undefined
}

/**
 * The picker's kind for a mask. A union the picker never builds — a group and
 * keys — reads as chosen keys: it is a set of lights someone chose.
 */
export function maskKind(mask: Mask | undefined): MaskKind {
  const m = cleanMask(mask)
  if (!m) return 'whole'
  if (m.saved !== undefined) return 'saved'
  const fields = Object.keys(m)
  if (fields.length === 1 && m.group !== undefined) return 'group'
  if (fields.length === 1 && m.region !== undefined) return 'region'
  return 'chosen'
}

/** The `select`'s value for a mask. */
export function maskValue(mask: Mask | undefined): string {
  const m = cleanMask(mask)
  switch (maskKind(m)) {
    case 'whole':
      return 'whole'
    case 'group':
      return `group:${m!.group}`
    case 'region':
      return `region:${m!.region}`
    case 'saved':
      return `saved:${m!.saved}`
    default:
      return 'chosen'
  }
}

/** What choosing an option of the `select` asks for. */
export type MaskPick =
  | { kind: 'mask'; mask: Mask | undefined }
  | { kind: 'choose' }
  | { kind: 'keep' }

export function maskOfValue(value: string): MaskPick {
  if (value === 'whole') return { kind: 'mask', mask: undefined }
  if (value === 'choose') return { kind: 'choose' }
  const at = value.indexOf(':')
  const field = value.slice(0, at)
  const id = value.slice(at + 1)
  if (at > 0 && id !== '') {
    if (field === 'group') return { kind: 'mask', mask: { group: id } }
    if (field === 'region') return { kind: 'mask', mask: { region: id } }
    if (field === 'saved') return { kind: 'mask', mask: { saved: id } }
  }
  // `chosen`: the keys the layer already has.
  return { kind: 'keep' }
}

/** What a mask's ⋯ menu asks for (§5.4). */
export type MaskAction = 'edit' | 'rename' | 'delete' | 'saveAs'

/** What names a mask's parts on one target. */
export interface MaskContext {
  /** What the target's lights are: keys are counted as keys, zones as zones. */
  lights: 'keys' | 'zones'
  /** The stack is a region's: *The whole region*. */
  region: boolean
  /** A region of the device, by id: its name in the interface's language. */
  regionName: (id: string) => string
  savedMasks: readonly SavedMaskRecord[]
}

/** The key groups the catalogs name, as `candeo_device::groups::KEY_GROUPS` lists them. */
const GROUP_IDS = [
  'escape',
  'wasd',
  'arrows',
  'functionRow',
  'numberRow',
  'numpad',
  'modifiers',
  'navigation',
  'space',
] as const
type Group = (typeof GROUP_IDS)[number]

/** A key group's name for people (`layers-ui.md` §7). An unknown group keeps its id. */
export function groupName(id: string): string {
  return (GROUP_IDS as readonly string[]).includes(id) ? t(`layers.groups.${id as Group}`) : id
}

/** `{n} keys`, or `{n} zones` on a zones device. */
export function countText(n: number, lights: 'keys' | 'zones'): string {
  return lights === 'zones' ? t('layers.select.countZones', { n }, n) : t('layers.select.countKeys', { n }, n)
}

/** The name of a saved mask by id; `null` for one nobody saved. */
export function savedMaskName(id: string, saved: readonly SavedMaskRecord[]): string | null {
  return saved.find((m) => m.id === id)?.name ?? null
}

function wholeName(ctx: MaskContext): string {
  return ctx.region ? t('layers.mask.wholeRegion') : t('layers.mask.whole')
}

/** How many lights a mask's own lists name: its count before it is resolved. */
function listed(mask: Mask | undefined): number {
  return (mask?.keys?.length ?? 0) + (mask?.lights?.length ?? 0)
}

/**
 * A mask's short name, for a row's line: *Number row*, *Game keys*, *5 keys*.
 * `count` is what it lights on the target, when known.
 */
export function maskName(mask: Mask | undefined, ctx: MaskContext, count: number | null): string {
  const m = cleanMask(mask)
  switch (maskKind(m)) {
    case 'whole':
      return wholeName(ctx)
    case 'group':
      return groupName(m!.group!)
    case 'region':
      return ctx.regionName(m!.region!)
    case 'saved':
      return savedMaskName(m!.saved!, ctx.savedMasks) ?? t('layers.mask.unknown')
    default:
      return countText(count ?? listed(m), ctx.lights)
  }
}

/**
 * A mask with its count, as the picker and the save dialog say it: *Number
 * row · 13 keys*, *Chosen keys · 5 keys*, *The whole device*.
 */
export function maskLabel(mask: Mask | undefined, ctx: MaskContext, count: number | null): string {
  const m = cleanMask(mask)
  const kind = maskKind(m)
  if (kind === 'whole') return wholeName(ctx)
  const n = count ?? listed(m)
  if (kind === 'chosen') {
    return ctx.lights === 'zones'
      ? t('layers.mask.chosenZones', { n }, n)
      : t('layers.mask.chosenKeys', { n }, n)
  }
  return countedName(maskName(m, ctx, n), n, ctx.lights)
}

/** `{name} · {n} keys`. */
export function countedName(name: string, n: number, lights: 'keys' | 'zones'): string {
  return lights === 'zones'
    ? t('layers.mask.zones', { name, n }, n)
    : t('layers.mask.keys', { name, n }, n)
}

/** A key into a cache of resolved masks: one target, one mask. */
export function maskKey(target: Target, mask: Mask | undefined): string {
  const region = regionOf(target) ?? ''
  return `${target.vid}:${target.pid}@${region}|${JSON.stringify(cleanMask(mask) ?? {})}`
}

/**
 * How many layers and saved layers name a saved mask, on every device: what
 * *Used by* says, and what *Edit keys…* reaches.
 */
export function usageOf(
  id: string,
  stacks: readonly StackRecord[],
  savedLayers: readonly SavedLayerRecord[],
): number {
  const names = (l: { mask?: Mask }) => l.mask?.saved === id
  return (
    stacks.reduce((n, s) => n + s.stack.filter(names).length, 0) + savedLayers.filter(names).length
  )
}

// ---------------------------------------------------------------- rows

/** What a layer row says of its state, colour as a second channel (§6). */
export type RowState = 'default' | 'failed' | 'nothing'

/**
 * A row's state: *Stopped on an error* when its effect failed, *Nothing to
 * light here* when its mask resolves to no light on the target. `positions`
 * is `null` while the mask is being resolved: no state is guessed meanwhile.
 */
export function rowState(
  status: LayerEngineStatus | undefined,
  positions: readonly number[] | null,
): RowState {
  if (status?.error) return 'failed'
  if (positions !== null && positions.length === 0) return 'nothing'
  return 'default'
}

/** A row's line: `{mask} · {blend} {opacity}`. */
export function layerLine(layer: Look & { mask?: Mask }, ctx: MaskContext, count: number | null): string {
  return t('layers.line', {
    mask: maskName(layer.mask, ctx, count),
    blend: blendName(blendOf(layer)),
    opacity: t('effects.percent', { n: opacityOf(layer) }),
  })
}

/** A layer's row, as the view words it for `LayerStack`. */
export interface StackRowView {
  kind: 'layer'
  id: string
  name: string
  line: string
  swatch: string[]
  state: RowState
  /** 1 for the layer just over the base. */
  position: number
}

/** A saved layer in the gallery, as the view words it for `SavedLayers`. */
export interface SavedLayerView {
  name: string
  line: string
  swatch: string[]
  pressed: boolean
}

/** One action of a ⋯ menu. */
export interface MenuItem {
  id: string
  label: string
  /** Deleting something: said in the danger colour, after its word. */
  danger?: boolean
}

/** What the stack lists between *Add a layer* and the base, top first. */
export type StackItem =
  | { kind: 'layer'; layer: LayerRecord; position: number }
  | { kind: 'removed' }

/**
 * The stack's rows, top first, with *Layer removed · Undo* where the removed
 * layer stood (`at`, counted from the bottom) until the next change.
 */
export function stackItems(stack: readonly LayerRecord[], removedAt: number | null): StackItem[] {
  const items: StackItem[] = stack.map((layer, i) => ({ kind: 'layer', layer, position: i + 1 }))
  if (removedAt !== null) items.splice(Math.min(Math.max(0, removedAt), items.length), 0, { kind: 'removed' })
  return items.reverse()
}

/** True once the stack holds eight effects, the base included: *Add a layer* becomes a sentence. */
export function atLimit(stack: readonly unknown[]): boolean {
  return stack.length >= MAX_EFFECTS - 1
}

/**
 * The stack's order, bottom to top, with layer `id` moved one place up (`+1`,
 * towards the top) or down; `null` when it is already at that end.
 */
export function movedOrder(stack: readonly LayerRecord[], id: string, delta: 1 | -1): string[] | null {
  const from = stack.findIndex((l) => l.id === id)
  const to = from + delta
  if (from < 0 || to < 0 || to >= stack.length) return null
  return moved(
    stack.map((l) => l.id),
    from,
    to,
  )
}

/** An effect and its settings: what the base runs, or what a layer gives it. */
export interface EffectSettings {
  effect: string
  values: EffectParams
  bindings: Bindings
}

/**
 * A layer and the base trading effects (`layers-ui.md` §4.5): the layer keeps
 * its id, place, mask and look and takes the base's effect and settings; the
 * base takes the layer's. The base has no mask or look to give: it covers the
 * whole target.
 */
export function swappedWithBase(
  layer: LayerRecord,
  base: EffectSettings,
): { layer: LayerRecord; base: EffectSettings } {
  return {
    layer: { ...layer, effect: base.effect, values: base.values, bindings: base.bindings },
    base: { effect: layer.effect, values: layer.values ?? {}, bindings: layer.bindings ?? {} },
  }
}

/** The order once layer `id` is dropped on the row of layer `onto`. */
export function droppedOrder(stack: readonly LayerRecord[], id: string, onto: string): string[] | null {
  const from = stack.findIndex((l) => l.id === id)
  const to = stack.findIndex((l) => l.id === onto)
  if (from < 0 || to < 0 || from === to) return null
  return moved(
    stack.map((l) => l.id),
    from,
    to,
  )
}

/** A stack in the order `ids` gives, bottom to top; layers it does not name keep their place after. */
export function inOrder(stack: readonly LayerRecord[], ids: readonly string[]): LayerRecord[] {
  const named = ids.map((id) => stack.find((l) => l.id === id)).filter((l): l is LayerRecord => !!l)
  return [...named, ...stack.filter((l) => !ids.includes(l.id))]
}

/** The device card's line: *Diagonal wave + 2 layers*, the effect alone without layers. */
export function withLayers(effect: string, layers: number): string {
  return layers > 0 ? t('layers.withLayers', { effect, n: layers }, layers) : effect
}

/** A target's stack in the file, bottom to top; empty when it has none. */
export function stackIn(stacks: readonly StackRecord[], target: Target): LayerRecord[] {
  const region = regionOf(target)
  return (
    stacks.find(
      (s) => s.vid === target.vid && s.pid === target.pid && (s.region ?? null) === region,
    )?.stack ?? []
  )
}

/** The file's stacks with one target's replaced, the record dropped when it holds no layer. */
export function withStack(
  stacks: readonly StackRecord[],
  target: Target,
  stack: readonly LayerRecord[],
): StackRecord[] {
  const region = regionOf(target)
  const same = (s: StackRecord) =>
    s.vid === target.vid && s.pid === target.pid && (s.region ?? null) === region
  const others = stacks.filter((s) => !same(s))
  if (stack.length === 0) return others
  const record: StackRecord = { vid: target.vid, pid: target.pid, stack: [...stack] }
  if (region !== null) record.region = region
  return [...others, record]
}

// ---------------------------------------------------------------- card

/**
 * The card header's line over the effect's name (§3): *Base*, *Layer 2 of 3*,
 * *New layer*, and where a saved layer is previewed, *New layer · from "CI on
 * Escape"* or *Layer 2 of 3 · replaced by "CI on Escape"*.
 */
export function cardKicker(
  selection: Selection,
  stack: readonly LayerRecord[],
  savedPicked: string | null,
  hardwareBase: boolean,
): string {
  if (selection.kind === 'base') return hardwareBase ? t('layers.card.baseHardware') : t('layers.card.base')
  if (selection.kind === 'new') {
    return savedPicked === null ? t('layers.card.new') : t('layers.card.newFrom', { name: savedPicked })
  }
  const n = positionOf(stack, selection.id)
  const total = stack.length
  return savedPicked === null
    ? t('layers.card.layer', { n, total })
    : t('layers.card.replacedBy', { n, total, name: savedPicked })
}

/**
 * Whether the row previews something the device does not run: another effect
 * than its own, a saved layer, or a new layer with an effect. What turns the
 * state line to *Preview* and offers *Cancel preview*.
 *
 * `applied` is the base's: today's rule, the gallery's choice being the effect
 * that runs.
 */
export function previews(
  selection: Selection,
  chosen: string | null,
  layer: LayerRecord | null,
  savedPicked: string | null,
  applied: boolean,
): boolean {
  if (selection.kind === 'base') return !applied
  if (selection.kind === 'new') return chosen !== null
  if (savedPicked !== null) return true
  return layer !== null && chosen !== null && chosen !== layer.effect
}

/** The sending button, labelled by its outcome (§3, §8 2.4.6). */
export function applyLabel(selection: Selection, previewing: boolean): MessageKey {
  if (selection.kind === 'new') return 'layers.apply.add'
  if (!previewing) return 'effects.applied'
  return selection.kind === 'base' ? 'effects.apply' : 'layers.apply.toLayer'
}

/** The preview's one state line (§3). */
export type PreviewState = 'live' | 'preview' | 'hardware'

export function previewState(hardware: boolean, showsDevice: boolean): PreviewState {
  if (hardware) return 'hardware'
  return showsDevice ? 'live' : 'preview'
}

// ---------------------------------------------------------------- preview

/** The row being edited, as the preview runs it: the gallery's effect in the row's look. */
export interface EditedLayer {
  effect: string
  values: EffectParams
  bindings: Bindings
  mask?: Mask
  look: Look
}

/** An id for the new layer in a preview, unique in its stack. */
export function freshId(stack: readonly LayerRecord[]): string {
  const taken = new Set(stack.map((l) => l.id))
  let id = 'new'
  for (let i = 2; taken.has(id); i++) id = `new${i}`
  return id
}

/** A layer record from its parts, every field at its default left out. */
export function layerRecord(id: string, edited: EditedLayer): LayerRecord {
  return { id, ...newLayer(edited) }
}

/** A layer before it has an id, what *Add the layer* sends. */
export function newLayer(edited: EditedLayer): NewLayer {
  const record: NewLayer = { effect: edited.effect, ...cleanLook(edited.look) }
  if (Object.keys(edited.values).length > 0) record.values = edited.values
  if (Object.keys(edited.bindings).length > 0) record.bindings = edited.bindings
  const mask = cleanMask(edited.mask)
  if (mask) record.mask = mask
  return record
}

/** A layer with another look — blend, opacity and follow all replaced — the rest of it kept. */
export function withLook(layer: LayerRecord, look: Look): LayerRecord {
  const out: LayerRecord = { id: layer.id, effect: layer.effect, ...cleanLook(look) }
  if (layer.values) out.values = layer.values
  if (layer.bindings) out.bindings = layer.bindings
  const mask = cleanMask(layer.mask)
  if (mask) out.mask = mask
  return out
}

/** A layer drawing through another mask, the rest of it kept; `undefined` for the whole target. */
export function withMask(layer: LayerRecord, mask: Mask | undefined): LayerRecord {
  const out = withLook(layer, lookOf(layer))
  const clean = cleanMask(mask)
  if (clean) out.mask = clean
  else delete out.mask
  return out
}

/** A layer with other settings: what departs from the declared values, and its bindings. */
export function withSettings(layer: LayerRecord, values: EffectParams, bindings: Bindings): LayerRecord {
  const out = withMask(layer, layer.mask)
  if (Object.keys(values).length > 0) out.values = values
  else delete out.values
  if (Object.keys(bindings).length > 0) out.bindings = bindings
  else delete out.bindings
  return out
}

/**
 * The layers a stack preview runs (`composition.md` §3.9): the target's stack
 * with the selected row in the gallery's effect — replaced in its place, or
 * added on top for a new layer.
 *
 * *Only this layer* runs that layer alone, in `cover` at 100 %, the caller
 * passing no base: what it draws is the layer, not how it blends.
 */
export function previewLayers(
  stack: readonly LayerRecord[],
  selection: Selection,
  edited: EditedLayer | null,
  solo: boolean,
): LayerRecord[] {
  let layers: LayerRecord[] = [...stack]
  let shown: string | null = null
  if (selection.kind === 'layer') {
    shown = selection.id
    if (edited) layers = layers.map((l) => (l.id === selection.id ? layerRecord(l.id, edited) : l))
  } else if (selection.kind === 'new' && edited) {
    shown = freshId(stack)
    layers.push(layerRecord(shown, edited))
  }
  if (!solo || shown === null) return layers
  const found = layers.find((l) => l.id === shown)
  if (!found) return layers
  const alone: LayerRecord = { ...found, blend: 'cover' }
  delete alone.opacity
  delete alone.follows
  return [alone]
}

/**
 * What makes a stack preview start again: the effects, the masks and the
 * order. Settings and looks change live, without a new context.
 */
export function previewKey(
  target: Target | null,
  base: string | null,
  layers: readonly LayerRecord[],
): string {
  const where = target ? `${target.vid}:${target.pid}@${regionOf(target) ?? ''}` : '-'
  const parts = layers.map((l) => `${l.id}=${l.effect}${JSON.stringify(cleanMask(l.mask) ?? {})}`)
  return `${where}|${base ?? ''}|${parts.join(',')}`
}

// ---------------------------------------------------------------- saving

/** Longest name for a saved layer or mask, in characters: Rust refuses more. */
export const SAVED_NAME_MAX = 64

/** A name as Rust keeps it: trimmed. */
export function savedName(typed: string): string {
  return typed.trim()
}

/** Whether `name` is taken among `names`, ignoring case; `except` is the name being renamed. */
export function nameTaken(name: string, names: readonly string[], except: string | null = null): boolean {
  const wanted = savedName(name).toLocaleLowerCase()
  const kept = except === null ? null : except.toLocaleLowerCase()
  return names.some((n) => {
    const lower = n.toLocaleLowerCase()
    return lower === wanted && lower !== kept
  })
}

/** The name the save dialog proposes: `{effect} · {mask}`, cut at the limit. */
export function proposedName(effect: string, mask: string): string {
  return `${effect} · ${mask}`.slice(0, SAVED_NAME_MAX)
}

/** What a follow is said as, after *follows*: *the sound*, *the signal "lux"*. */
export function followText(follows: Dimming | undefined): string | null {
  if (!follows) return null
  switch (dimmingMode(follows)) {
    case 'sound':
      return t('layers.save.sources.sound')
    case 'system':
      return t('layers.save.sources.system')
    case 'signal':
      return t('layers.save.sources.signal', { name: dimmingSignal(follows) ?? '' })
    default:
      return null
  }
}

/** The opacity as *What is saved* says it: *100 %*, or *100 %, follows the sound*. */
export function opacityText(look: Look): string {
  const opacity = opacityOf(look)
  const source = followText(look.follows)
  return source === null
    ? t('layers.save.opacityValue', { opacity })
    : t('layers.save.opacityFollows', { opacity, source })
}

/** One line of *What is saved*. */
export interface SavedItem {
  label: string
  value: string
  /** Under the value: *Linked: …* for a saved mask. */
  note?: string
}

/** *What is saved*, for a layer (§5.2): the effect, the mask, the blend, the opacity. */
export function layerSaveItems(
  effect: string,
  layer: Look & { mask?: Mask },
  ctx: MaskContext,
  count: number | null,
): SavedItem[] {
  const mask: SavedItem = { label: t('layers.save.mask'), value: maskLabel(layer.mask, ctx, count) }
  if (maskKind(layer.mask) === 'saved') mask.note = t('layers.save.linked')
  return [
    { label: t('layers.save.effect'), value: t('layers.save.effectValue', { effect }) },
    mask,
    { label: t('layers.save.blend'), value: blendName(blendOf(layer)) },
    { label: t('layers.save.opacity'), value: opacityText(layer) },
  ]
}

/** *What is saved*, for a mask (§5.4): its keys, or its zones. */
export function maskSaveItems(count: number, lights: 'keys' | 'zones'): SavedItem[] {
  return [
    {
      label: lights === 'zones' ? t('layers.save.zones') : t('layers.save.keys'),
      value: countText(count, lights),
    },
  ]
}

/**
 * A layer as a saved one keeps it (§5.5): every declared value, so that a
 * later default does not change it, and no id.
 */
export function layerToSave(layer: LayerRecord, values: EffectParams): NewLayer {
  const out: NewLayer = { effect: layer.effect, ...lookOf(layer), values: { ...values } }
  if (layer.bindings && Object.keys(layer.bindings).length > 0) out.bindings = { ...layer.bindings }
  const mask = cleanMask(layer.mask)
  if (mask) out.mask = mask
  return out
}

/** The parts of a saved layer the card shows read-only while it is previewed. */
export function savedLook(saved: SavedLayerRecord): Look & { mask?: Mask } {
  return { ...lookOf(saved), mask: cleanMask(saved.mask) }
}

/** A saved layer's gallery line: `{effect} · {mask} · {blend} {opacity}`. */
export function savedLine(saved: SavedLayerRecord, effect: string, ctx: MaskContext, count: number | null): string {
  return t('layers.saved.line', {
    effect,
    mask: maskName(saved.mask, ctx, count),
    blend: blendName(blendOf(saved)),
    opacity: t('effects.percent', { n: opacityOf(saved) }),
  })
}
