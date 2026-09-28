// Temperature — the whole device from a cool colour to a hot one as the computer
// heats up (#242, `docs/design/inputs-and-automations.md` §2.4).
//
// The form people ask for most: one colour sliding on a gradient, readable
// across the room. Where the processor's temperature cannot be read without a
// driver or administrator rights, its power stands in: it follows load and
// heat within a second. Nothing read at all, the device stays at the cool
// colour, dimmed, rather than going dark and looking broken.

import { defineEffect, mix, rgb } from '@candeo/effects-api'

const COOL = { r: 0, g: 90, b: 255 }
const HOT = { r: 255, g: 30, b: 0 }

export default defineEffect({
  description: {
    en: 'The keyboard from cool to hot as the graphics card or the processor heats up',
    fr: 'Le clavier du froid au chaud à mesure que la carte graphique ou le processeur chauffe',
  },
  kinds: 'all',
  inputs: ['system'],
  params: {
    follows: {
      kind: 'choice',
      label: { en: 'Follows', fr: 'Suit' },
      options: [
        { value: 'gpu', label: { en: 'Graphics card', fr: 'Carte graphique' } },
        { value: 'cpu', label: { en: 'Processor', fr: 'Processeur' } },
      ],
      default: 'gpu',
    },
    cool: { kind: 'color', label: { en: 'Cool', fr: 'Froid' }, default: COOL },
    hot: { kind: 'color', label: { en: 'Hot', fr: 'Chaud' }, default: HOT },
  },
  render({ layout, frame, params, system }) {
    const cool = params.cool ?? COOL
    const hot = params.hot ?? HOT
    const level =
      params.follows === 'cpu' ? (system.cpuTemp ?? system.cpuPower) : system.gpuTemp
    const colour = level === null ? mix(rgb(0, 0, 0), cool, 0.3) : mix(cool, hot, level)
    for (const key of layout.keys) frame.set(key, colour)
  },
})
