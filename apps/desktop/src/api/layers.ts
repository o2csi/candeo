/**
 * Layers, saved layers and saved masks: the commands and the records
 * (`docs/design/composition.md` §3.11, `docs/design/layers-ui.md` §5.5).
 *
 * Apart from `candeo.ts` for size only: the same rule holds, no component calls
 * `invoke` but through here. Mirrors of `src-tauri/src/layers.rs`.
 *
 * A layer's position counts from the bottom of its stack, the base excluded:
 * `0` is the layer just over the base. The window lists the top first.
 */

import { invoke } from '@tauri-apps/api/core'

import type { Bindings, Dimming, EffectParams } from './candeo'
import { deviceOf, regionOf, type Target } from './target'
import type { DeviceRef } from './types'

// ---------------------------------------------------------------- records

/** How a layer's colour combines with what is under it. Absent from a record: `screen`. */
export type Blend = 'cover' | 'add' | 'screen' | 'multiply'

/** The blend a layer gets when its record names none (decision 5). */
export const BLEND_DEFAULT: Blend = 'screen'

/** The opacity a layer gets when its record names none. */
export const OPACITY_DEFAULT = 100

/** Eight effects at most on a target, the base included (decision 7). */
export const MAX_EFFECTS = 8

/**
 * The lights of its target a layer may touch: the union of what it names, the
 * whole target when it names nothing. Mirror of `Mask`, in
 * `src-tauri/src/runtime/composition.rs`, plus `saved`.
 *
 * `saved` names a saved mask by its id, and nothing else beside it
 * (`layers-ui.md` §5.5). The window never builds `keys` or `lights` itself: it
 * works in frame positions and asks {@link maskOfPositions} for the mask.
 */
export interface Mask {
  /** A key group's id: `wasd`. */
  group?: string
  /** Keys by scancode, as definitions write them: `"01"`, `"e01d"`. */
  keys?: string[]
  /** Lights by address: zones, and keys without a scancode. */
  lights?: number[]
  /** A region of the device, by its id. */
  region?: string
  /** A saved mask, by its id. */
  saved?: string
}

/** A layer's blend, opacity and follow: what changes live, without restarting it. */
export interface Look {
  /** Absent: {@link BLEND_DEFAULT}. */
  blend?: Blend
  /** 0 to 100. Absent: {@link OPACITY_DEFAULT}. */
  opacity?: number
  /** What its opacity follows, from its floor to the opacity set. Absent: nothing. */
  follows?: Dimming
}

/** One layer, as `settings.json` keeps it. Fields at their default are absent. */
export interface LayerRecord extends Look {
  /** Short, unique within its stack; what commands name the layer by. */
  id: string
  /** A library effect's key: `shipped:Equalizer`. */
  effect: string
  /** What departs from the declared values, as `effectParams` keeps them. */
  values?: EffectParams
  bindings?: Bindings
  /** Absent: the whole target. */
  mask?: Mask
}

/** A layer before it has an id: what {@link addLayer} takes. */
export type NewLayer = Omit<LayerRecord, 'id'>

/** A target's layers, bottom to top, the base excluded. Absent when it has none. */
export interface StackRecord {
  vid: number
  pid: number
  /** The region of a split device it runs on; absent for a whole device. */
  region?: string
  stack: LayerRecord[]
}

/**
 * A named layer, put back as a copy (`layers-ui.md` §5.1). `values` holds every
 * value the effect declared when it was saved, as a preset's do.
 */
export interface SavedLayerRecord extends NewLayer {
  /** Unique ignoring case, 1 to 64 characters. */
  name: string
}

/**
 * Named keys or lights, linked to the layers drawing through it. Its mask's
 * fields sit beside its id and name; `vid` and `pid` come with `lights` or
 * `region`, which mean something on one definition only.
 */
export interface SavedMaskRecord extends Omit<Mask, 'saved'> {
  id: string
  /** Unique ignoring case, 1 to 64 characters. */
  name: string
  vid?: number
  pid?: number
}

// ---------------------------------------------------------------- status

/**
 * One layer, as its loop runs it. Mirror of `LayerEngineStatus`, in
 * `src-tauri/src/runtime/mod.rs`. A layer whose mask lights nothing on this
 * device runs nothing and has no entry: the window says *nothing to light here*
 * from {@link resolveMask}.
 */
export interface LayerEngineStatus {
  layer: string
  running: boolean
  effectId: string | null
  /** Its effect's error, already readable; also set after thirty failed frames. */
  error: string | null
}

// ---------------------------------------------------------------- masks

/** One mask the picker offers, with the frame positions it lights on the target. */
export interface MaskChoice {
  id: string
  positions: number[]
}

/**
 * What the mask picker offers on a target, each with what it lights there,
 * those lighting nothing left out (`layers-ui.md` §4.3, §5.4).
 */
export interface MaskChoices {
  /** Key groups, in `KEY_GROUPS` order. */
  groups: MaskChoice[]
  /** The definition's regions, on a whole device that declares them; empty otherwise. */
  regions: MaskChoice[]
  /** Saved masks, in their order. */
  savedMasks: MaskChoice[]
}

export function maskChoices(target: Target): Promise<MaskChoices> {
  return invoke('mask_choices', { device: deviceOf(target), region: regionOf(target) })
}

/**
 * The frame positions `mask` lights on `target`, ascending; empty when it lights
 * nothing there, or names a group, region or saved mask nobody knows.
 */
export function resolveMask(target: Target, mask: Mask): Promise<number[]> {
  return invoke('resolve_mask', { device: deviceOf(target), region: regionOf(target), mask })
}

/**
 * The mask lighting exactly `positions` on `target`: keys by scancode where
 * they have one, lights by address otherwise. What *Choose keys* ends with.
 */
export function maskOfPositions(target: Target, positions: number[]): Promise<Mask> {
  return invoke('mask_of_positions', { device: deviceOf(target), region: regionOf(target), positions })
}

// ---------------------------------------------------------------- layers

/**
 * Adds a layer on top of `target`'s stack — or at `at`, counted from the bottom,
 * to put back a removed one — runs it where the target runs, and remembers it.
 * Answers with its id. Refused, as a `Failure`, past eight effects, with a mask
 * lighting nothing, or with an effect the mask's lights cannot run.
 */
export function addLayer(target: Target, layer: NewLayer | LayerRecord, at?: number): Promise<string> {
  return invoke('add_layer', { device: deviceOf(target), region: regionOf(target), layer, at: at ?? null })
}

/**
 * Replaces the layer with `layer.id`, where it stands: another effect, another
 * mask, a saved layer put in its place. Runs it and remembers it.
 */
export function replaceLayer(target: Target, layer: LayerRecord): Promise<void> {
  return invoke('replace_layer', { device: deviceOf(target), region: regionOf(target), layer })
}

/** Takes a layer out of its stack, live and in the file. One already gone is no error. */
export function removeLayer(target: Target, id: string): Promise<void> {
  return invoke('remove_layer', { device: deviceOf(target), region: regionOf(target), id })
}

/** Puts the stack in this order, bottom to top; `ids` names each layer once. */
export function orderLayers(target: Target, ids: string[]): Promise<void> {
  return invoke('order_layers', { device: deviceOf(target), region: regionOf(target), ids })
}

/**
 * A layer's blend, opacity and follow, **live** only: a slider being dragged.
 * {@link rememberLayerLook} writes it, as `setEffectParams` and
 * `rememberEffectParams` share the work.
 */
export function setLayerLook(target: Target, id: string, look: Look): Promise<void> {
  return invoke('set_layer_look', { device: deviceOf(target), region: regionOf(target), id, look })
}

export function rememberLayerLook(target: Target, id: string, look: Look): Promise<void> {
  return invoke('remember_layer_look', { device: deviceOf(target), region: regionOf(target), id, look })
}

/** A layer's settings, live only. */
export function setLayerParams(target: Target, id: string, params: EffectParams): Promise<void> {
  return invoke('set_layer_params', { device: deviceOf(target), region: regionOf(target), id, params })
}

/** Remembers a layer's settings, keeping what departs from the declared values. */
export function rememberLayerParams(target: Target, id: string, params: EffectParams): Promise<void> {
  return invoke('remember_layer_params', { device: deviceOf(target), region: regionOf(target), id, params })
}

/** A layer's bindings, live and remembered at once, as a binding changes rarely. */
export function setLayerBindings(target: Target, id: string, bindings: Bindings): Promise<void> {
  return invoke('set_layer_bindings', { device: deviceOf(target), region: regionOf(target), id, bindings })
}

// ---------------------------------------------------------------- preview

/** The base a stack preview runs: an effect and its settings. */
export interface PreviewBase {
  effect: string
  params: EffectParams
  bindings: Bindings
}

/**
 * Previews a stack, as `startPreview` does one effect: no hardware output,
 * nothing written. `base` `null` draws the layers over black, which is what
 * *Only this layer* asks for with a single layer in `cover` at 100 %. Masks
 * naming a saved mask are resolved here.
 */
export function startPreviewStack(
  target: Target | null,
  base: PreviewBase | null,
  layers: LayerRecord[],
): Promise<void> {
  return invoke('start_preview_stack', {
    device: target ? deviceOf(target) : null,
    region: target ? regionOf(target) : null,
    base,
    layers,
  })
}

/** A previewed layer's settings, live. `setPreviewParams` stays the base's. */
export function setPreviewLayerParams(id: string, params: EffectParams): Promise<void> {
  return invoke('set_preview_layer_params', { id, params })
}

export function setPreviewLayerBindings(id: string, bindings: Bindings): Promise<void> {
  return invoke('set_preview_layer_bindings', { id, bindings })
}

/** A previewed layer's blend, opacity and follow, live. */
export function setPreviewLook(id: string, look: Look): Promise<void> {
  return invoke('set_preview_look', { id, look })
}

// ---------------------------------------------------------------- saved layers

/**
 * Saves a layer under `name`, last of the list. Answers with the saved layers.
 * A name taken ignoring case is refused, as are an empty one and one past 64
 * characters. The window sends every declared value.
 */
export function saveLayer(name: string, layer: NewLayer): Promise<SavedLayerRecord[]> {
  return invoke('save_layer', { name, layer })
}

export function renameSavedLayer(from: string, to: string): Promise<SavedLayerRecord[]> {
  return invoke('rename_saved_layer', { from, to })
}

/** Deletes a saved layer; no layer changes. One already gone is no error. */
export function deleteSavedLayer(name: string): Promise<SavedLayerRecord[]> {
  return invoke('delete_saved_layer', { name })
}

/** Puts a deleted saved layer back at `at` in the list: *Undo*. */
export function restoreSavedLayer(record: SavedLayerRecord, at: number): Promise<SavedLayerRecord[]> {
  return invoke('restore_saved_layer', { record, at })
}

// ---------------------------------------------------------------- saved masks

/**
 * Saves a mask under `name`, last of the list, and answers with its record.
 * `model` is required when the mask names lights or a region.
 */
export function saveMask(name: string, mask: Mask, model: DeviceRef | null): Promise<SavedMaskRecord> {
  return invoke('save_mask', { name, mask, model: model ? deviceOf(model) : null })
}

/**
 * Changes a saved mask's lights. Every layer naming it follows, on every
 * device, live; answers with how many layers and saved layers name it.
 * `model` is required when the mask names lights: keys without a scancode,
 * Fn among them, come back from {@link maskOfPositions} as lights.
 */
export function updateMask(id: string, mask: Mask, model: DeviceRef | null = null): Promise<number> {
  return invoke('update_mask', { id, mask, model: model ? deviceOf(model) : null })
}

export function renameMask(id: string, name: string): Promise<SavedMaskRecord[]> {
  return invoke('rename_mask', { id, name })
}

/** Where a deleted saved mask was named, so that *Undo* can link them again. */
export interface DeletedMask {
  record: SavedMaskRecord
  /** Its place in the list. */
  at: number
  layers: Array<{ vid: number; pid: number; region?: string; layer: string }>
  savedLayers: string[]
}

/**
 * Deletes a saved mask: each layer and saved layer naming it gets its lights
 * written in, and runs on unchanged.
 */
export function deleteMask(id: string): Promise<DeletedMask> {
  return invoke('delete_mask', { id })
}

/** Puts a deleted mask back, and links again what named it: *Undo*. */
export function undeleteMask(deleted: DeletedMask): Promise<void> {
  return invoke('undelete_mask', { deleted })
}
