// Regenerates the application icons from the two drawings next to this file.
//
// `tauri icon` renders a single source at every size. `candeo.svg` blurs its
// glow away below 48 px, so the sizes a notification area or a file list shows
// come from `candeo-small.svg`, drawn on a 32 px grid. The window and tray icon
// is the first entry of `icon.ico` on Windows and `32x32.png` on Linux: both
// come from the small drawing.
//
// Run from apps/desktop: pnpm icons
import { execSync } from 'node:child_process'
import { copyFileSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const icons = dirname(here)

const FROM_SMALL = ['32x32.png', 'Square30x30Logo.png', 'Square44x44Logo.png']
const FROM_FULL = [
  '128x128.png',
  '128x128@2x.png',
  'icon.icns',
  'icon.png',
  'Square71x71Logo.png',
  'Square89x89Logo.png',
  'Square107x107Logo.png',
  'Square142x142Logo.png',
  'Square150x150Logo.png',
  'Square284x284Logo.png',
  'Square310x310Logo.png',
  'StoreLogo.png',
]
// The order `tauri icon` writes: 32 first, since that entry becomes the window
// and tray icon.
const ICO_ORDER = [32, 16, 24, 48, 64, 256]
const ICO_SMALL = new Set([16, 24, 32])

function render(source, out) {
  execSync(`pnpm exec tauri icon "${join(here, source)}" -o "${out}"`, { stdio: 'inherit' })
}

// ICO: a 6-byte header, a 16-byte entry per image, then the image data.
function readIco(file) {
  const buf = readFileSync(file)
  const images = new Map()
  for (let i = 0; i < buf.readUInt16LE(4); i++) {
    const at = 6 + 16 * i
    const size = buf.readUInt32LE(at + 8)
    const offset = buf.readUInt32LE(at + 12)
    images.set(buf[at] || 256, {
      entry: buf.subarray(at, at + 8),
      data: buf.subarray(offset, offset + size),
    })
  }
  return images
}

function writeIco(file, images) {
  const header = Buffer.alloc(6 + 16 * images.length)
  header.writeUInt16LE(1, 2)
  header.writeUInt16LE(images.length, 4)
  let offset = header.length
  images.forEach(({ entry, data }, i) => {
    const at = 6 + 16 * i
    entry.copy(header, at)
    header.writeUInt32LE(data.length, at + 8)
    header.writeUInt32LE(offset, at + 12)
    offset += data.length
  })
  writeFileSync(file, Buffer.concat([header, ...images.map((image) => image.data)]))
}

const work = mkdtempSync(join(tmpdir(), 'candeo-icons-'))
try {
  const full = join(work, 'full')
  const small = join(work, 'small')
  render('candeo.svg', full)
  render('candeo-small.svg', small)

  for (const name of FROM_FULL) copyFileSync(join(full, name), join(icons, name))
  for (const name of FROM_SMALL) copyFileSync(join(small, name), join(icons, name))

  const fullIco = readIco(join(full, 'icon.ico'))
  const smallIco = readIco(join(small, 'icon.ico'))
  writeIco(
    join(icons, 'icon.ico'),
    ICO_ORDER.map((size) => {
      const image = (ICO_SMALL.has(size) ? smallIco : fullIco).get(size)
      if (!image) throw new Error(`icon.ico has no ${size} px image`)
      return image
    }),
  )
} finally {
  rmSync(work, { recursive: true, force: true })
}
