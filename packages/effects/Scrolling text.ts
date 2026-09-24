// Scrolling text — any words across the keyboard, as Clock shows the time.
//
// Typed in the settings, or sent by another program: switch *Text* to a signal
// and the keyboard shows what it says — a build's state, a message, a
// temperature. The same small display as Clock: the API's 3×5 font (`banner`)
// over the five top rows, the bottom row dark. Upper case only; what the font
// cannot draw is a space.
//
// Three displays, each tried on a keyboard against the others before shipping
// (#217), and shared with Clock:
// - Smooth scrolling, right to left in physical key units, each key lit by how
//   much of its width the letters cover: the text glides instead of jumping
//   from key to key across the staggered rows, and a wide key such as Shift
//   does not light whole for a thin stroke.
// - Sharp scrolling, each key on or off by where its centre falls.
// - Still: text that fits stays centred; longer text comes a page of whole
//   words at a time. Still text reads best on a grid this coarse, and a status
//   sent by a signal is often a word or two.

import { banner, bounds, center, defineEffect, mix } from '@candeo/effects-api'

const TEXT = 'CANDEO'
const COLOR = { r: 90, g: 200, b: 255 }
const BACKGROUND = { r: 4, g: 6, b: 12 }
const NOTHING = ['', '', '', '', '']

// Pages are worked out once per text and width, not at every frame.
let cached = { text: '', room: 0, pages: [NOTHING] }

/** The text in pages of whole words that fit `room` columns, each as banner lines. */
function pagesOf(text = '', room = 0) {
  if (cached.text === text && cached.room === room) return cached.pages
  const pages = []
  let current = ''
  for (const word of text.split(/\s+/).filter(Boolean)) {
    const tried = current ? current + ' ' + word : word
    if (current && banner(tried)[0].length > room) {
      pages.push(current)
      current = word
    } else {
      current = tried
    }
  }
  if (current) pages.push(current)
  cached = { text, room, pages: pages.length ? pages.map((page) => banner(page)) : [NOTHING] }
  return cached.pages
}

export default defineEffect({
  description: {
    en: 'Any text across the keyboard, scrolling or still, typed here or sent by a signal',
    fr: "N'importe quel texte sur le clavier, qui défile ou fixe, tapé ici ou envoyé par un signal",
  },
  kinds: ['keyboard'],
  params: {
    // As long as a signal's value, so a bound message is never cut.
    text: { kind: 'text', label: { en: 'Text', fr: 'Texte' }, maxLength: 256, default: TEXT },
    display: {
      kind: 'choice',
      label: { en: 'Display', fr: 'Affichage' },
      options: [
        { value: 'smooth', label: { en: 'Smooth scrolling', fr: 'Défilement lissé' } },
        { value: 'sharp', label: { en: 'Sharp scrolling', fr: 'Défilement net' } },
        { value: 'still', label: { en: 'Still, a few words at a time', fr: 'Fixe, mot par mot' } },
      ],
      default: 'smooth',
    },
    color: { kind: 'color', label: { en: 'Letters', fr: 'Lettres' }, default: COLOR },
    background: { kind: 'color', label: { en: 'Background', fr: 'Fond' }, default: BACKGROUND },
    speed: {
      kind: 'number',
      label: {
        en: 'Scrolling speed (keys per second)',
        fr: 'Vitesse du défilement (touches par seconde)',
      },
      min: 1,
      max: 20,
      step: 1,
      default: 6,
    },
    hold: {
      kind: 'number',
      label: { en: 'Seconds per page, when still', fr: 'Secondes par page, en fixe' },
      min: 0.5,
      max: 5,
      step: 0.5,
      default: 1.5,
    },
  },
  render({ layout, time, frame, params }) {
    const color = params.color ?? COLOR
    const background = params.background ?? BACKGROUND
    const text = params.text ?? TEXT
    const area = bounds(layout)

    // On or off, by the text column each key's centre falls in.
    const sharp = (lines = NOTHING, left = 0) => {
      const width = lines[0].length
      for (const key of layout.keys) {
        // The function row is the text's first line; the bottom row has none.
        const line = key.row
        const column = Math.floor(center(key).x - left)
        const lit =
          line >= 0 && line < 5 && column >= 0 && column < width && lines[line][column] === '#'
        frame.set(key, lit ? color : background)
      }
    }
    // By how much of each key's width, in text columns, the lit pixels cover.
    const smooth = (lines = NOTHING, left = 0) => {
      const width = lines[0].length
      for (const key of layout.keys) {
        const line = key.row
        if (line < 0 || line >= 5) {
          frame.set(key, background)
          continue
        }
        const keyWidth = key.w ?? 1
        const from = center(key).x - keyWidth / 2 - left
        const to = from + keyWidth
        const last = Math.min(width, Math.ceil(to))
        let lit = 0
        for (let column = Math.max(0, Math.floor(from)); column < last; column++) {
          if (lines[line][column] === '#') lit += Math.min(to, column + 1) - Math.max(from, column)
        }
        frame.set(key, mix(background, color, lit / keyWidth))
      }
    }

    const display = params.display ?? 'smooth'
    if (display === 'still') {
      const pages = pagesOf(text, Math.floor(area.w))
      const lines = pages[Math.floor(time / Number(params.hold ?? 1.5)) % pages.length]
      sharp(lines, area.x + (area.w - lines[0].length) / 2)
      return
    }
    // The text comes in past the right edge and leaves past the left one, then
    // comes round again: a lap is the keyboard's width plus the text's own.
    const lines = banner(text)
    const left = area.x + area.w - ((time * Number(params.speed ?? 6)) % (area.w + lines[0].length))
    if (display === 'sharp') sharp(lines, left)
    else smooth(lines, left)
  },
})
