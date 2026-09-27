// Reads which key sits at each matrix position of a Razer keyboard from
// OpenRGB's tables, and writes it as the `matrix` of each model in
// `variants.json` (`docs/design/device-sdk.md` §9):
//
//   node crates/candeo-device/devices/unverified/openrgb.mjs <OpenRGB checkout>
//
// OpenRGB describes each keyboard as edits to a standard layout, which its
// KeyboardLayoutManager applies. Nothing of OpenRGB is run: its tables are read
// as data, and the edits are applied here the way KeyboardLayoutManager.cpp
// (GPL-2.0-or-later) applies them. Checked against the verified DeathStalker V2
// Pro on 2026-09-27: the same key at the same place for all 105 lights OpenRGB
// lists, Enter's upper arm being the one it leaves out (below).
//
// The layout drawn is ISO, like the verified definition: the ANSI keys are the
// same lights but one, the backslash above Enter, where ISO has Enter's upper
// arm. Lit as Enter, it lights on both.
//
// It also checks that each model's transaction byte and interface are the ones
// OpenRGB uses, and says so when they differ.
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

import { token } from './keys.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const checkout = process.argv[2]
if (!checkout) {
  console.error('usage: node openrgb.mjs <OpenRGB checkout>')
  process.exit(2)
}
const read = (path) => readFileSync(join(checkout, path), 'utf8')
const razer = 'Controllers/RazerController'

// ---------------------------------------------------------------- C tables

/** The tokens of C source without its comments: names, numbers, strings, punctuation. */
function tokens(src) {
  const code = src.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/\/\/[^\n]*/g, ' ')
  return code.match(/"(?:[^"\\]|\\.)*"|[A-Za-z_]\w*(?:::[A-Za-z_]\w*)*|0x[0-9A-Fa-f]+|\d+|[{}(),;=*&]/g)
}

/** A brace initializer from `toks[at] === '{'`, as nested arrays; one-token elements unwrapped. */
function brace(toks, at) {
  const out = []
  let cur = []
  let i = at + 1
  const flush = () => {
    if (cur.length) out.push(cur.length > 1 ? cur : cur[0])
    cur = []
  }
  for (;;) {
    const t = toks[i]
    if (t === '{') {
      const [sub, next] = brace(toks, i)
      cur.push(sub)
      i = next
      continue
    }
    if (t === '}') {
      flush()
      return [out, i + 1]
    }
    if (t === ',') flush()
    else cur.push(t)
    i++
  }
}

/** Every `type NAME {…}` or `type NAME = {…}` of a source, by name. */
function initializers(src, type) {
  const toks = tokens(src)
  const found = {}
  for (let i = 0; i < toks.length - 3; i++) {
    if (toks[i] !== type || !/^[A-Za-z_]/.test(toks[i + 1])) continue
    let j = i + 2
    if (toks[j] === '=') j++
    if (toks[j] === '{') found[toks[i + 1]] = brace(toks, j)[0]
  }
  return found
}

/** A table that may hold one entry, unwrapped by `brace`, or none. */
const list = (x) => (x === undefined || x === null ? [] : Array.isArray(x) && Array.isArray(x[0]) ? x : Array.isArray(x) && x.length ? [x] : [])

const klmSrc = read('KeyboardLayoutManager/KeyboardLayoutManager.cpp')
const zones = {}
{
  const toks = tokens(klmSrc)
  for (const zone of ['keyboard_zone_main', 'keyboard_zone_fn_row', 'keyboard_zone_extras', 'keyboard_zone_numpad']) {
    let i = toks.indexOf(zone)
    while (toks[i] !== '{') i++
    zones[zone] = brace(toks, i)[0]
  }
}
const overlays = initializers(klmSrc, 'keyboard_keymap_overlay')

const SIZE = { KEYBOARD_SIZE_EMPTY: 0, KEYBOARD_SIZE_FULL: 15, KEYBOARD_SIZE_TKL: 7, KEYBOARD_SIZE_SEVENTY_FIVE: 3, KEYBOARD_SIZE_SIXTY: 1 }
const OPCODE = {
  KEYBOARD_OPCODE_INSERT_SHIFT_RIGHT: 0,
  KEYBOARD_OPCODE_SWAP_ONLY: 1,
  KEYBOARD_OPCODE_REMOVE_SHIFT_LEFT: 2,
  KEYBOARD_OPCODE_INS_SHFT_ADJACENT: 3,
  KEYBOARD_OPCODE_INSERT_ROW: 4,
  KEYBOARD_OPCODE_REMOVE_ROW: 5,
  KEYBOARD_OPCODE_ADD_ALT_NAME: 6,
}
const num = (t) => (t.startsWith('0x') ? parseInt(t, 16) : Number(t))

/** A key of a table: its place, and what it is — `KEY_EN_…` or a quoted name; '' for none. */
function key([, row, col, , name, , op]) {
  const what = name === 'KEY_EN_UNUSED' ? '' : name.startsWith('"') ? JSON.parse(name) : name
  return { row: num(row), col: num(col), what, op: OPCODE[op] }
}

// ------------------------------------------------ KeyboardLayoutManager

/** The standard layout of a size, edited the way KeyboardLayoutManager does it. */
class Layout {
  constructor(layout, size, regional) {
    this.keys = []
    this.rows = this.cols = 0
    if (size === 0) return
    if (size & 1) this.insertAll(zones.keyboard_zone_main)
    if (size & 2) this.insertAll(zones.keyboard_zone_fn_row)
    if (size & 4) this.insertAll(zones.keyboard_zone_extras)
    if (size & 8) this.insertAll(zones.keyboard_zone_numpad)
    this.edit(list(overlays[{ ANSI: 'ansi_qwerty', ISO: 'iso_qwerty' }[layout]][1]).map(key))
    for (const k of regional[`KEYBOARD_LAYOUT_${layout}_QWERTY`] ?? []) this.swap(k)
    if (size === SIZE.KEYBOARD_SIZE_SIXTY) {
      this.removeRow(0)
      this.swap(key(zones.keyboard_zone_fn_row[0]))
    }
    this.measure()
  }

  measure() {
    this.rows = Math.max(0, ...this.keys.map((k) => k.row)) + 1
    this.cols = Math.max(0, ...this.keys.map((k) => k.col)) + 1
  }

  insertAll(entries) {
    for (const e of entries) this.insert(key(e))
    this.measure()
  }

  edit(keys) {
    for (const k of keys) {
      if (k.op === 0) this.insert(k)
      else if (k.op === 1) this.swap(k)
      else if (k.op === 2) this.remove(k)
      else if (k.op === 4) {
        if (this.insertRow(k.row)) this.swap(k)
      } else if (k.op === 5) this.removeRow(k.row)
      // 3 is not implemented by OpenRGB either; 6 only names a key again.
    }
    this.measure()
  }

  /** Before the first key at or after the place; a named key shifts its row's next keys right. */
  insert(k) {
    const km = this.keys
    let i = 0
    while (i < km.length && !(k.row < km[i].row || (k.row === km[i].row && k.col <= km[i].col))) i++
    if (i === km.length) {
      km.push({ ...k })
      return
    }
    if (k.what !== '') km.splice(i++, 0, { ...k })
    for (; i < km.length; i++) {
      if (km[i].row === k.row && km[i].col >= k.col) km[i].col++
      if (km[i].row > k.row) break
    }
  }

  /** Replaces the key at the place, removes it for '', or adds one where none is. */
  swap(k) {
    const km = this.keys
    if (!km.length) {
      km.push({ ...k })
      return
    }
    for (let i = 0; i < km.length; i++) {
      if (km[i].row === k.row && km[i].col === k.col) {
        if (k.what === '') km.splice(i, 1)
        else km[i].what = k.what
        return
      }
      if (km[i].row > k.row || (km[i].row === k.row && km[i].col > k.col)) {
        // KeyboardLayoutManager inserts one place early: only the order of the
        // list changes, which later edits walk, so it is kept.
        if (k.what !== '') km.splice(i === 0 ? 0 : i - 1, 0, { ...k })
        return
      }
    }
  }

  /** Removes the key at the place, or the gap there, shifting its row's next keys left. */
  remove(k) {
    const km = this.keys
    for (let i = 0; i < km.length; i++) {
      const at = km[i].row === k.row && km[i].col === k.col
      if (at || (km[i].row === k.row && k.col < km[i].col)) {
        if (at) km.splice(i, 1)
        for (; i < km.length && km[i].row === k.row; i++) km[i].col--
        return
      }
    }
  }

  insertRow(row) {
    if (row >= this.rows) return false
    const i = this.keys.findIndex((k) => row <= k.row)
    if (i >= 0) for (const k of this.keys.slice(i)) k.row++
    return true
  }

  removeRow(row) {
    if (row >= this.rows) return
    this.keys = this.keys.filter((k) => k.row !== row)
    for (const k of this.keys) if (k.row > row) k.row--
  }

  /** What is at a place, as `GetKeyNameAt` finds it: the first key there. */
  at(row, col) {
    return this.keys.find((k) => k.row === row && k.col === col)?.what ?? ''
  }
}

// ---------------------------------------------------------------- models

const devicesSrc = read(`${razer}/RazerDevices.cpp`)
const pids = Object.fromEntries(
  [...read(`${razer}/RazerDevices.h`).matchAll(/#define\s+(RAZER_\w+_PID)\s+(0x[0-9A-Fa-f]+)/g)].map((m) => [m[1], m[2]]),
)
const layouts = initializers(devicesSrc, 'keyboard_keymap_overlay_values')
const razerZones = initializers(devicesSrc, 'razer_zone')
const devices = Object.values(initializers(devicesSrc, 'razer_device'))
const detectors = [...read(`${razer}/RazerControllerDetect.cpp`).matchAll(/^REGISTER_HID_DETECTOR_IPU\s*\(\s*"[^"]*",[^,]*\w+,\s*RAZER_VID,\s*(RAZER_\w+_PID),\s*(0x[0-9A-Fa-f]+)/gm)]

const ref = (x) => (Array.isArray(x) ? x[1] : x)

/** The keyboard zone of a model, laid out in ISO: its rows as tokens. */
function matrix(device) {
  const [, , , , , , , zoneRefs, layoutRef] = device
  const zone = razerZones[ref(zoneRefs[0])]
  const [rows, cols] = [num(zone[2]), num(zone[3])]
  const [size, values = [], edits] = layouts[ref(layoutRef)]
  const regional = {}
  for (const [name, keys] of list(values[1])) regional[name] = list(keys).map(key)
  const lay = (layout) => {
    const l = new Layout(layout, SIZE[size.split('::').pop()], regional)
    l.edit(list(edits).map(key))
    return l
  }
  const iso = lay('ISO')
  const ansi = lay('ANSI')
  const grid = []
  for (let r = 0; r < rows; r++) {
    const row = []
    for (let c = 0; c < cols; c++) {
      let t = token(iso.at(r, c))
      // Enter's upper arm, lit on the verified DeathStalker V2 Pro, which
      // OpenRGB leaves out of ISO: where ANSI has its backslash.
      if (t === '.' && ansi.at(r, c) === 'KEY_EN_ANSI_BACK_SLASH') t = 'enter'
      row.push(t)
    }
    grid.push(row.join(' '))
  }
  return grid
}

const facts = JSON.parse(readFileSync(join(here, 'variants.json'), 'utf8'))
for (const v of facts.variants) {
  const device = devices.find((d) => pids[d[1]] && parseInt(pids[d[1]], 16) === parseInt(v.pid, 16))
  if (!device) throw new Error(`${v.pid}: not in OpenRGB's tables`)
  v.matrix = matrix(device)
  const transaction = num(device[4]).toString(16).padStart(2, '0')
  if (transaction !== v.transaction) console.warn(`${v.file}: OpenRGB sends ${transaction}, the facts say ${v.transaction}`)
  const iface = detectors.filter((m) => pids[m[1]] && parseInt(pids[m[1]], 16) === parseInt(v.pid, 16)).map((m) => num(m[2]))
  if (!iface.includes(v.interface)) console.warn(`${v.file}: OpenRGB opens interface ${iface.join(' or ')}, the facts say ${v.interface}`)
  console.log(`${v.file}: ${v.matrix.length}×${v.matrix[0].split(' ').length}`)
}

const { formatJson } = await import(pathToFileURL(join(here, '../../../../apps/desktop/src/editor/jsonFormat.ts')).href)
writeFileSync(join(here, 'variants.json'), formatJson(JSON.stringify(facts)))
