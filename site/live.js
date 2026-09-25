// The keyboard at the top of the landing page runs the example further down it:
// the same source, the engine's own helpers and the layout the app draws for
// the DeathStalker V2 Pro, all written out by `build.mjs`. Thirty frames a
// second, as the engine sends them; one still frame for whoever asked for less
// motion. Without this script the figure stays hidden rather than empty.
import effect from './live/rainbow.mjs'

/** Around the keys, in keyboard pitch units: the frame of the keyboard. */
const PAD = 0.4
/** Around the frame: room for the light the keys throw on the desk. */
const GLOW = 1.1
/** How many columns of light that glow is averaged into. */
const BANDS = 24
/** Between two keycaps. */
const GAP = 0.08
const FRAME_MS = 1000 / 30
const DARK = { r: 0, g: 0, b: 0 }

const figure = document.querySelector('.live')
const canvas = figure.querySelector('canvas')
const context = canvas.getContext('2d')
const layout = await (await fetch('live/layout.json')).json()
// Shown once there is something to draw; the canvas keeps its proportions from
// its attributes until the first frame sets its size.
figure.hidden = false

const keysWidth = Math.max(...layout.keys.map((key) => key.x + key.w))
const keysHeight = Math.max(...layout.keys.map((key) => key.y + key.h))
const width = keysWidth + 2 * (PAD + GLOW)
const height = keysHeight + 2 * (PAD + GLOW)
const still = matchMedia('(prefers-reduced-motion: reduce)')
const colors = []
const frame = { set: (key, color) => (colors[key.index] = color) }

let scale = 1
let visible = false
let running = false
let last = -Infinity

function rounded(x, y, w, h, r) {
  context.beginPath()
  context.roundRect(x, y, w, h, r)
  context.fill()
}

function draw(time) {
  effect.render({ layout, time, frame, params: {} })
  const ratio = devicePixelRatio
  context.setTransform(scale * ratio, 0, 0, scale * ratio, 0, 0)
  context.clearRect(0, 0, width, height)
  context.translate(GLOW, GLOW)

  // The light on the desk: columns of the keys' average colour, blurred, of
  // which the frame drawn over them leaves only what spills around it. The blur
  // is in device pixels, whatever the transform: scaled by hand.
  context.shadowBlur = 1.3 * scale * ratio
  const band = keysWidth / BANDS
  for (let i = 0; i < BANDS; i++) {
    const inside = layout.keys.filter((key) => Math.floor((key.x + key.w / 2) / band) === i)
    const sum = { r: 0, g: 0, b: 0 }
    for (const key of inside) {
      const color = colors[key.index] ?? DARK
      sum.r += color.r
      sum.g += color.g
      sum.b += color.b
    }
    const n = inside.length || 1
    const [r, g, b] = [sum.r / n, sum.g / n, sum.b / n].map(Math.round)
    context.shadowColor = `rgb(${r} ${g} ${b} / 55%)`
    context.fillStyle = `rgb(${r} ${g} ${b})`
    rounded(PAD + i * band, PAD, band, keysHeight, 0)
  }

  context.shadowBlur = 0
  context.fillStyle = '#0b0a0f'
  rounded(0, 0, keysWidth + 2 * PAD, keysHeight + 2 * PAD, 0.35)

  context.translate(PAD, PAD)
  context.shadowBlur = 0.45 * scale * ratio
  for (const key of layout.keys) {
    const { r, g, b } = colors[key.index] ?? DARK
    context.shadowColor = `rgb(${r} ${g} ${b} / 80%)`
    context.fillStyle = `rgb(${r} ${g} ${b})`
    rounded(key.x + GAP, key.y + GAP, key.w - 2 * GAP, key.h - 2 * GAP, 0.12)
  }
  // The keycap over the light: what makes a key read as a key, not a pixel.
  context.shadowBlur = 0
  context.fillStyle = 'rgb(10 9 14 / 30%)'
  for (const key of layout.keys) {
    rounded(key.x + 0.17, key.y + 0.14, key.w - 0.34, key.h - 0.34, 0.08)
  }
}

function tick(now) {
  if (!visible || still.matches) {
    running = false
    return
  }
  if (now - last >= FRAME_MS) {
    last = now
    draw(now / 1000)
  }
  requestAnimationFrame(tick)
}

/** One loop at most, and none off screen: scrolling back must not start a second. */
function start() {
  if (still.matches) {
    draw(0)
  } else if (visible && !running) {
    running = true
    requestAnimationFrame(tick)
  }
}

new ResizeObserver(() => {
  scale = canvas.clientWidth / width
  canvas.width = Math.round(canvas.clientWidth * devicePixelRatio)
  canvas.height = Math.round(height * scale * devicePixelRatio)
  draw(last === -Infinity ? 0 : last / 1000)
}).observe(canvas)

new IntersectionObserver(([entry]) => {
  visible = entry.isIntersecting
  start()
}).observe(canvas)

still.addEventListener('change', start)
