import { afterEach, describe, expect, it } from 'vitest'

import type { LayerRecord, SavedLayerRecord, StackRecord } from '../api/layers'
import { showIn } from '../i18n'
import {
  applyLabel,
  atLimit,
  BASE,
  blendOf,
  cardKicker,
  cleanLook,
  cleanMask,
  droppedOrder,
  freshId,
  inOrder,
  layerLine,
  layerRecord,
  layerRow,
  layerSaveItems,
  layerToSave,
  maskKind,
  maskLabel,
  maskName,
  maskOfValue,
  maskSaveItems,
  maskValue,
  movedOrder,
  nameTaken,
  NEW,
  opacityOf,
  opacityText,
  previewKey,
  previewLayers,
  previewState,
  previews,
  proposedName,
  rowState,
  savedLine,
  stackIn,
  stackItems,
  standing,
  swappedWithBase,
  usageOf,
  withLayers,
  withLook,
  withMask,
  withSettings,
  withStack,
  type MaskContext,
} from './layers'

const keyboard = { vid: 1, pid: 2 }
const lid = { vid: 1, pid: 3, region: 'lid' }

const eq: LayerRecord = { id: 'a', effect: 'shipped:Equalizer', mask: { group: 'numberRow' } }
const red: LayerRecord = { id: 'b', effect: 'shipped:Fixed colour', mask: { saved: 'k1' }, blend: 'cover' }
const stack = [eq, red]

const ctx: MaskContext = {
  lights: 'keys',
  region: false,
  regionName: (id) => (id === 'lid' ? 'Lid' : id),
  savedMasks: [{ id: 'k1', name: 'Game keys', keys: ['11', '1e', '1f', '20'] }],
}

afterEach(() => showIn('en'))

describe('selection', () => {
  it('falls back on the base when the layer selected is gone', () => {
    expect(standing(layerRow('b'), stack)).toEqual(layerRow('b'))
    expect(standing(layerRow('gone'), stack)).toBe(BASE)
    expect(standing(NEW, [])).toBe(NEW)
  })
})

describe('look', () => {
  it('reads screen at 100 % when the record names nothing', () => {
    expect(blendOf({})).toBe('screen')
    expect(opacityOf({})).toBe(100)
  })

  it('keeps no field at its default, as the file does', () => {
    expect(cleanLook({ blend: 'screen', opacity: 100 })).toEqual({})
    expect(cleanLook({ blend: 'cover', opacity: 80.4 })).toEqual({ blend: 'cover', opacity: 80 })
    expect(cleanLook({ opacity: 140 })).toEqual({})
    expect(cleanLook({ opacity: -3 })).toEqual({ opacity: 0 })
  })
})

describe('masks', () => {
  it('keeps a saved mask alone, and the whole target as nothing', () => {
    expect(cleanMask({ saved: 'k1', keys: ['01'] })).toEqual({ saved: 'k1' })
    expect(cleanMask({ keys: [], lights: [] })).toBeUndefined()
    expect(cleanMask(undefined)).toBeUndefined()
  })

  it('tells the picker what kind each mask is', () => {
    expect(maskKind(undefined)).toBe('whole')
    expect(maskKind({ group: 'wasd' })).toBe('group')
    expect(maskKind({ region: 'lid' })).toBe('region')
    expect(maskKind({ saved: 'k1' })).toBe('saved')
    expect(maskKind({ keys: ['01'] })).toBe('chosen')
    // A union the picker never builds reads as keys someone chose.
    expect(maskKind({ group: 'wasd', keys: ['01'] })).toBe('chosen')
  })

  it('goes to the select and back', () => {
    for (const mask of [{ group: 'wasd' }, { region: 'lid' }, { saved: 'k1' }]) {
      expect(maskOfValue(maskValue(mask))).toEqual({ kind: 'mask', mask })
    }
    expect(maskOfValue(maskValue(undefined))).toEqual({ kind: 'mask', mask: undefined })
    expect(maskValue({ keys: ['01'] })).toBe('chosen')
    expect(maskOfValue('chosen')).toEqual({ kind: 'keep' })
    expect(maskOfValue('choose')).toEqual({ kind: 'choose' })
  })

  it('names each kind, with its count where the picker shows one', () => {
    expect(maskName(undefined, ctx, null)).toBe('The whole device')
    expect(maskName(undefined, { ...ctx, region: true }, null)).toBe('The whole region')
    expect(maskName({ group: 'wasd' }, ctx, 4)).toBe('Movement keys')
    expect(maskName({ region: 'lid' }, ctx, 3)).toBe('Lid')
    expect(maskName({ saved: 'k1' }, ctx, 4)).toBe('Game keys')
    expect(maskName({ saved: 'zz' }, ctx, 0)).toBe('Deleted mask')
    expect(maskName({ keys: ['01', '02'] }, ctx, null)).toBe('2 keys')

    expect(maskLabel({ group: 'numberRow' }, ctx, 13)).toBe('Number row · 13 keys')
    expect(maskLabel({ keys: ['01'] }, ctx, 1)).toBe('Chosen keys · 1 key')
    expect(maskLabel({ region: 'lid' }, { ...ctx, lights: 'zones' }, 3)).toBe('Lid · 3 zones')
    expect(maskLabel({ lights: [3, 7] }, { ...ctx, lights: 'zones' }, null)).toBe(
      'Chosen zones · 2 zones',
    )
  })

  it('names the groups in French', () => {
    showIn('fr')
    expect(maskLabel({ group: 'wasd' }, ctx, 4)).toBe('Touches de déplacement · 4 touches')
    expect(maskName(undefined, ctx, null)).toBe("Tout l'appareil")
  })

  it('counts the layers and saved layers naming a saved mask, on every device', () => {
    const stacks: StackRecord[] = [
      { ...keyboard, stack },
      { ...lid, stack: [{ id: 'c', effect: 'x', mask: { saved: 'k1' } }] },
    ]
    const saved: SavedLayerRecord[] = [
      { name: 'One', effect: 'x', mask: { saved: 'k1' } },
      { name: 'Two', effect: 'x', mask: { group: 'wasd' } },
    ]
    expect(usageOf('k1', stacks, saved)).toBe(3)
    expect(usageOf('k2', stacks, saved)).toBe(0)
  })
})

describe('rows', () => {
  it('says a failed layer before anything else, and nothing to light when its mask resolves to none', () => {
    const failed = { layer: 'a', running: false, effectId: null, error: 'boom' }
    expect(rowState(failed, [1, 2])).toBe('failed')
    expect(rowState(undefined, [])).toBe('nothing')
    // Still resolving: no state is guessed.
    expect(rowState(undefined, null)).toBe('default')
    expect(rowState(undefined, [4])).toBe('default')
  })

  it('reads mask, blend and opacity on one line', () => {
    expect(layerLine(eq, ctx, 13)).toBe('Number row · Screen 100%')
    expect(layerLine({ ...red, opacity: 80 }, ctx, 4)).toBe('Game keys · Cover 80%')
    showIn('fr')
    expect(layerLine(red, ctx, 4)).toBe('Game keys · Couvrir 100 %')
  })

  it('lists the top first, with the removed layer where it stood', () => {
    const top = stackItems(stack, null)
    expect(top.map((i) => (i.kind === 'layer' ? `${i.layer.id}${i.position}` : 'x'))).toEqual([
      'b2',
      'a1',
    ])
    // Removed between the two: at 1, counted from the bottom.
    const holes = stackItems(stack, 1).map((i) => (i.kind === 'layer' ? i.layer.id : 'x'))
    expect(holes).toEqual(['b', 'x', 'a'])
    expect(stackItems([eq], 5).map((i) => i.kind)).toEqual(['removed', 'layer'])
  })

  it('stops at eight effects, the base included', () => {
    expect(atLimit(new Array(6).fill(eq))).toBe(false)
    expect(atLimit(new Array(7).fill(eq))).toBe(true)
  })

  it('moves a layer one place, and refuses past either end', () => {
    expect(movedOrder(stack, 'a', 1)).toEqual(['b', 'a'])
    expect(movedOrder(stack, 'a', -1)).toBeNull()
    expect(movedOrder(stack, 'b', 1)).toBeNull()
    expect(droppedOrder([eq, red, { id: 'c', effect: 'x' }], 'c', 'a')).toEqual(['c', 'a', 'b'])
    expect(droppedOrder(stack, 'a', 'a')).toBeNull()
    expect(inOrder(stack, ['b', 'a']).map((l) => l.id)).toEqual(['b', 'a'])
  })

  it('trades effects with the base, the layer keeping its place, mask and look', () => {
    const layer: LayerRecord = {
      id: 'l2',
      effect: 'shipped:Ripples',
      values: { ring: '#40c8ff' },
      bindings: { ring: 'signal:build' },
      mask: { group: 'wasd' },
      blend: 'add',
      opacity: 60,
    }
    const base = { effect: 'shipped:Bubbles', values: { speed: 3 }, bindings: {} }
    expect(swappedWithBase(layer, base)).toEqual({
      layer: {
        id: 'l2',
        effect: 'shipped:Bubbles',
        values: { speed: 3 },
        bindings: {},
        mask: { group: 'wasd' },
        blend: 'add',
        opacity: 60,
      },
      base: { effect: 'shipped:Ripples', values: { ring: '#40c8ff' }, bindings: { ring: 'signal:build' } },
    })
    expect(swappedWithBase({ id: 'l1', effect: 'shipped:Rain' }, base).base).toEqual({
      effect: 'shipped:Rain',
      values: {},
      bindings: {},
    })
  })

  it('says the base and its layers on the device card', () => {
    expect(withLayers('Diagonal wave', 0)).toBe('Diagonal wave')
    expect(withLayers('Diagonal wave', 1)).toBe('Diagonal wave + 1 layer')
    expect(withLayers('Diagonal wave', 2)).toBe('Diagonal wave + 2 layers')
    showIn('fr')
    expect(withLayers('Vague diagonale', 2)).toBe('Vague diagonale + 2 calques')
  })

  it('finds and replaces a target’s stack in the file, a region apart from its device', () => {
    const stacks: StackRecord[] = [{ ...keyboard, stack }]
    expect(stackIn(stacks, keyboard)).toBe(stack)
    expect(stackIn(stacks, { ...keyboard, region: 'lid' })).toEqual([])
    const both = withStack(stacks, lid, [eq])
    expect(stackIn(both, lid)).toEqual([eq])
    expect(withStack(both, keyboard, [])).toEqual([{ vid: 1, pid: 3, region: 'lid', stack: [eq] }])
  })
})

describe('card', () => {
  it('names the row it frames', () => {
    expect(cardKicker(BASE, stack, null, false)).toBe('Base')
    expect(cardKicker(BASE, stack, null, true)).toBe('Base · hardware effect')
    expect(cardKicker(layerRow('b'), stack, null, false)).toBe('Layer 2 of 2')
    expect(cardKicker(NEW, stack, null, false)).toBe('New layer')
    expect(cardKicker(NEW, stack, 'CI on Escape', false)).toBe('New layer · from “CI on Escape”')
    expect(cardKicker(layerRow('a'), stack, 'CI on Escape', false)).toBe(
      'Layer 1 of 2 · replaced by “CI on Escape”',
    )
    showIn('fr')
    expect(cardKicker(layerRow('a'), stack, null, false)).toBe('Calque 1 sur 2')
  })

  it('previews what the device does not run, and only that', () => {
    expect(previews(BASE, 'x', null, null, true)).toBe(false)
    expect(previews(BASE, 'x', null, null, false)).toBe(true)
    expect(previews(NEW, null, null, null, false)).toBe(false)
    expect(previews(NEW, 'x', null, null, false)).toBe(true)
    expect(previews(layerRow('a'), eq.effect, eq, null, false)).toBe(false)
    expect(previews(layerRow('a'), 'other', eq, null, false)).toBe(true)
    expect(previews(layerRow('a'), eq.effect, eq, 'Saved', false)).toBe(true)
  })

  it('labels the sending button by its outcome', () => {
    expect(applyLabel(BASE, true)).toBe('effects.apply')
    expect(applyLabel(BASE, false)).toBe('effects.applied')
    expect(applyLabel(layerRow('a'), true)).toBe('layers.apply.toLayer')
    expect(applyLabel(layerRow('a'), false)).toBe('effects.applied')
    expect(applyLabel(NEW, false)).toBe('layers.apply.add')
    expect(applyLabel(NEW, true)).toBe('layers.apply.add')
  })

  it('says one state', () => {
    expect(previewState(true, true)).toBe('hardware')
    expect(previewState(false, true)).toBe('live')
    expect(previewState(false, false)).toBe('preview')
  })
})

describe('preview', () => {
  const edited = {
    effect: 'shipped:Rain',
    values: { speed: 2 },
    bindings: {},
    mask: { group: 'wasd' },
    look: { blend: 'cover' as const, opacity: 100 },
  }

  it('puts the gallery’s effect in the selected layer’s place, the rest around it', () => {
    const layers = previewLayers(stack, layerRow('a'), edited, false)
    expect(layers).toEqual([
      { id: 'a', effect: 'shipped:Rain', values: { speed: 2 }, mask: { group: 'wasd' }, blend: 'cover' },
      red,
    ])
  })

  it('adds the new layer on top, under an id the stack does not use', () => {
    const layers = previewLayers([{ ...eq, id: 'new' }], NEW, edited, false)
    expect(layers.map((l) => l.id)).toEqual(['new', 'new2'])
    expect(freshId([])).toBe('new')
  })

  it('draws only this layer, covering at full opacity', () => {
    const solo = previewLayers([eq, { ...red, opacity: 40, follows: { source: 'sound:beat', floor: 0 } }], layerRow('b'), null, true)
    expect(solo).toEqual([{ id: 'b', effect: red.effect, mask: { saved: 'k1' }, blend: 'cover' }])
  })

  it('leaves the stack as it is for the base', () => {
    expect(previewLayers(stack, BASE, null, true)).toEqual(stack)
  })

  it('starts again for an effect, a mask or the order, not for a look', () => {
    const key = previewKey(keyboard, 'wave', stack)
    expect(previewKey(keyboard, 'wave', [eq, { ...red, opacity: 10 }])).toBe(key)
    expect(previewKey(keyboard, 'wave', [red, eq])).not.toBe(key)
    expect(previewKey(keyboard, 'wave', [eq, { ...red, mask: { group: 'wasd' } }])).not.toBe(key)
    expect(previewKey(lid, 'wave', stack)).not.toBe(key)
  })

  it('changes one part of a layer and keeps the rest', () => {
    const l: LayerRecord = { ...red, values: { colour: 1 }, opacity: 50 }
    expect(withLook(l, { blend: 'add' })).toEqual({
      id: 'b',
      effect: red.effect,
      values: { colour: 1 },
      mask: { saved: 'k1' },
      blend: 'add',
    })
    expect(withMask(l, undefined)).toEqual({
      id: 'b',
      effect: red.effect,
      values: { colour: 1 },
      blend: 'cover',
      opacity: 50,
    })
    expect(withSettings(l, {}, { colour: 'signal:ci' })).toEqual({
      id: 'b',
      effect: red.effect,
      bindings: { colour: 'signal:ci' },
      mask: { saved: 'k1' },
      blend: 'cover',
      opacity: 50,
    })
  })

  it('writes a record without the defaults', () => {
    expect(
      layerRecord('z', { effect: 'e', values: {}, bindings: {}, mask: undefined, look: { blend: 'screen' } }),
    ).toEqual({ id: 'z', effect: 'e' })
  })
})

describe('saving', () => {
  it('refuses a name taken, ignoring case, but not the one being renamed', () => {
    expect(nameTaken(' game KEYS ', ['Game keys'])).toBe(true)
    expect(nameTaken('Game keys', ['Game keys'], 'game keys')).toBe(false)
    expect(nameTaken('Other', ['Game keys'])).toBe(false)
  })

  it('proposes the effect and the mask, within the limit', () => {
    expect(proposedName('Fixed colour', 'Game keys')).toBe('Fixed colour · Game keys')
    expect(proposedName('x'.repeat(70), 'y')).toHaveLength(64)
  })

  it('lists what a saved layer keeps, the link to a saved mask said', () => {
    const items = layerSaveItems('Fixed colour', red, ctx, 9)
    expect(items).toEqual([
      { label: 'Effect', value: 'Fixed colour, with its settings' },
      { label: 'Mask', value: 'Game keys · 9 keys', note: 'Linked: it will follow the mask\'s changes.' },
      { label: 'Blend', value: 'Cover' },
      { label: 'Opacity', value: '100%' },
    ])
    expect(maskSaveItems(9, 'keys')).toEqual([{ label: 'Keys', value: '9 keys' }])
  })

  it('says what the opacity follows', () => {
    expect(opacityText({ follows: { source: 'sound:beat', floor: 0 } })).toBe('100%, follows the sound')
    expect(opacityText({ opacity: 50, follows: { source: 'signal:lux', floor: 10 } })).toBe(
      '50%, follows the signal “lux”',
    )
    showIn('fr')
    expect(opacityText({ opacity: 50 })).toBe('50 %')
  })

  it('keeps every declared value and no id', () => {
    const saved = layerToSave({ ...red, values: { colour: 1 }, bindings: {} }, { colour: 1, speed: 1 })
    expect(saved).toEqual({
      effect: red.effect,
      values: { colour: 1, speed: 1 },
      mask: { saved: 'k1' },
      blend: 'cover',
    })
  })

  it('describes a saved layer in the gallery', () => {
    const saved: SavedLayerRecord = { name: 'CI on Escape', effect: 'x', mask: { group: 'escape' }, blend: 'cover' }
    expect(savedLine(saved, 'Breathing', ctx, 1)).toBe('Breathing · Escape · Cover 100%')
  })
})
