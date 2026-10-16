// Writes the definitions of the Alienware and Dell machines whose lights
// Alienware's AW-ELC drives, from alienfx-tools' map of each
// (`docs/design/device-sdk.md` §9, #328):
//
//   node crates/candeo-device/devices/unverified/alienfx.mjs --from <devices.csv>
//   node crates/candeo-device/devices/unverified/alienfx.mjs
//
// The first reads alienfx-tools' `alienfx-gui/Mappings/devices.csv` (MIT) into
// the `machines` of `alienfx.facts.json`; the second writes the definitions
// from those facts, without the network.
//
// The protocol is verified, one degree above the families that have none: the
// AW-ELC, `187c:0550` and `0551`, lights the 17 R5 and the m18 R1 through the
// same live preview. What changes from one machine to the next is which id
// lights what, and only the system's product name tells the machines apart
// (#326). alienfx-tools labels its maps by hand — "Alienware m15R1" — so each
// definition names the machine as SMBIOS does, from a DMI dump whose source the
// facts give.
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const root = join(here, '../../../..')
const factsPath = join(here, 'alienfx.facts.json')

/** The AW-ELC as alienfx-tools writes it: vendor 0x187c, products 0x0550 and 0x0551, in decimal. */
const AW_ELC = { vid: 6268, pids: [1360, 1361] }

/** alienfx-tools' flag for a power button, which the firmware drives with the power state. */
export const POWER = 1
/** Its flag for an indicator, which it keeps dark when the lights are off. */
export const INDICATOR = 2

const hex = (n, width = 2) => n.toString(16).padStart(width, '0')

// ------------------------------------------------------------------ the CSV

/**
 * A line's fields, split as alienfx-tools' `LoadCSV` splits them: quoted, and
 * separated by `','`. The empty fields a spreadsheet leaves at the end are
 * dropped; a line that is not quoted is no line.
 */
export function fields(line) {
  const text = line.trim().replace(/,+$/, '')
  if (text.length < 2 || !text.startsWith("'") || !text.endsWith("'")) return null
  return text.slice(1, -1).split("','")
}

/** Cells as ranges: `[40, 41, 42, 80]` is `"40-42 80"`. */
export function ranges(cells) {
  const out = []
  for (const c of cells) {
    const last = out.at(-1)
    if (last && c === last[1] + 1) last[1] = c
    else out.push([c, c])
  }
  return out.map(([a, b]) => (a === b ? `${a}` : `${a}-${b}`)).join(' ')
}

/** The cells a range string names, in its order. */
export function cells(text) {
  return text.split(' ').flatMap((part) => {
    const [a, b = a] = part.split('-').map(Number)
    return Array.from({ length: b - a + 1 }, (_, i) => a + i)
  })
}

/**
 * The machines of a `devices.csv`. `'3'` starts one by its label; `'2'` gives
 * it a grid, its id, columns, rows and name; `'0'` a device, vendor and product
 * in decimal; `'1'` a light of the last device: its id, its flags in the low
 * word (the high word is a key's scan code), its name, then pairs of a grid and
 * a cell of it. As in alienfx-tools, a blank line ends nothing, and what comes
 * before the first machine is skipped.
 */
export function machines(csv) {
  const out = []
  csv.split(/\r?\n/).forEach((raw, i) => {
    const f = fields(raw)
    const machine = out.at(-1)
    if (!f || (!machine && f[0] !== '3')) return
    const line = i + 1
    switch (f[0]) {
      case '3':
        out.push({ label: f[1], line, grids: [], devices: [] })
        break
      case '2':
        machine.grids.push({ id: Number(f[1]), cols: Number(f[2]), rows: Number(f[3]), name: f[4] })
        break
      case '0':
        machine.devices.push({ vid: Number(f[1]), pid: Number(f[2]), name: f[3], lights: [] })
        break
      case '1': {
        const device = machine.devices.at(-1)
        if (!device) throw new Error(`line ${line}: a light before any device`)
        const data = Number(f[2])
        const light = { id: Number(f[1]), name: f[3], flags: data & 0xffff }
        if (data >>> 16) light.scancode = data >>> 16
        const pairs = f.slice(4)
        const grids = new Set(pairs.filter((_, j) => j % 2 === 0).map(Number))
        if (grids.size > 1) throw new Error(`line ${line}: cells on ${grids.size} grids`)
        if (grids.size === 1) {
          light.grid = [...grids][0]
          light.cells = ranges(pairs.filter((_, j) => j % 2 === 1).map(Number))
        }
        device.lights.push(light)
        break
      }
    }
  })
  return out
}

/**
 * The AW-ELC maps among those machines, one per machine and device, with the
 * grids their lights are placed on.
 */
export function awElc(all) {
  return all.flatMap((m) =>
    m.devices
      .filter((d) => d.vid === AW_ELC.vid && AW_ELC.pids.includes(d.pid))
      .map((d) => ({
        label: m.label,
        line: m.line,
        device: d.name,
        pid: hex(d.pid, 4),
        grids: m.grids.filter((g) => d.lights.some((l) => l.grid === g.id)),
        lights: d.lights,
      })),
  )
}

// ------------------------------------------------------------------ names

/** A keyboard zone, by the word alienfx-tools gives its place, left to right. */
const ZONES = [
  [/^left$/, 'left'],
  [/^(?:left mid|center left)$/, 'centre left'],
  [/^middle$/, 'centre'],
  [/^(?:right mid|center right)$/, 'centre right'],
  [/^right$/, 'right'],
  [/^numpad$/, 'numeric keypad'],
]

/**
 * Every other light alienfx-tools names on these machines: the name written
 * here, a number the name carries being added after a comma, and the place it
 * is drawn. A name not here stops the generator: a new map is read before it
 * is written.
 */
const NAMES = [
  [/^(?:back ?)?alien ?head$/, 'alien head on the lid', 'lid'],
  [/^logo$/, 'logo', 'lid'],
  [/^lid logo$/, 'logo on the lid', 'lid'],
  [/^lid light$/, 'light on the lid', 'lid'],
  [/^back$/, 'back', 'lid'],
  [/^power(?: ?button ?(\d+)?)?$/, 'power button', 'power'],
  [/^alien ?head\/power button$/, 'alien head and power button', 'power'],
  [/^touchpad ?(\d+)?$/, 'touchpad', 'touchpad'],
  [/^haptic$/, 'haptic', 'touchpad'],
  // The ring of light at the back, which alienfx-tools calls the Tron.
  [/^(?:tron|ring) top$/, 'ring, top', 'rear', { half: 'top' }],
  [/^(?:tron|ring) bottom$/, 'ring, bottom', 'rear', { half: 'bottom' }],
  [/^tron$/, 'ring', 'rear'],
  [/^tron full$/, 'ring, full', 'rear'],
  [/^tron back ?left$/, 'ring, back left', 'rear', { span: [0, 0.5] }],
  [/^tron back ?right$/, 'ring, back right', 'rear', { span: [0.5, 1] }],
  [/^left of tron$/, 'left of the ring', 'rear', { span: [0, 0.5] }],
  [/^right of tron$/, 'right of the ring', 'rear', { span: [0.5, 1] }],
  [/^ring ?(\d+)$/, 'ring', 'rear'],
  [/^rear vents ?(\d+)$/, 'rear vents', 'rear'],
  [/^ex(?:ha|ah)ust lights ?(\d+)$/, 'exhaust lights', 'rear'],
  // Numbers alone: the G5 5500's twelve-zone light bar.
  [/^(\d+)$/, 'light bar', 'front'],
  // A desktop's.
  [/^front ring$/, 'front ring', 'ring'],
  [/^ring led ?(\d+)$/, 'front ring', 'ring'],
  [/^ambient led ?(\d+)$/, 'ambient', 'ambient'],
  [/^alienware$/, 'Alienware logo', 'logo'],
  [/^internal ambient$/, 'inside, ambient', 'inside', { shape: 'rect' }],
  [/^fan ?light$/, 'fan', 'inside'],
  [/^cooling hexagon$/, 'cooler, hexagon', 'inside'],
  [/^cooling logo$/, 'cooler, logo', 'inside'],
]

/** A name as the patterns read it: lower case, `_` and `-` as spaces. */
const plain = (name) => name.trim().toLowerCase().replace(/[_-]/g, ' ').replace(/\s+/g, ' ')

/**
 * Whether alienfx-tools lays the light out as a key range: named after the
 * keyboard (`KB Left`), or named by its place alone (`Left`) on a grid it calls
 * a keyboard — where the m15 R6's power button also has a cell.
 */
export function onKeyboard(light, grids) {
  const name = plain(light.name)
  if (/^kb /.test(name)) return true
  const grid = grids.find((g) => g.id === light.grid)
  return Boolean(grid && /keyboard/i.test(grid.name) && ZONES.some(([pattern]) => pattern.test(name)))
}

/**
 * What a light is called here, where it is drawn, and its rank there: a
 * keyboard zone by its place on alienfx-tools' grid, others by the number in
 * their name.
 */
export function english(light, grids = []) {
  const name = plain(light.name)
  if (onKeyboard(light, grids)) {
    const word = name.replace(/^kb /, '')
    const zone = ZONES.findIndex(([pattern]) => pattern.test(word))
    if (zone < 0) throw new Error(`“${light.name}”: no keyboard zone of that name`)
    return { name: `keyboard, ${ZONES[zone][1]}`, place: 'keyboard', rank: zone }
  }
  for (const [pattern, written, place, extra] of NAMES) {
    const found = pattern.exec(name)
    if (!found) continue
    const number = found[1] === undefined ? undefined : Number(found[1])
    return {
      name: number === undefined ? written : `${written}, ${number}`,
      place,
      rank: number ?? 0,
      ...extra,
    }
  }
  throw new Error(`“${light.name}”: no name for it here`)
}

// ---------------------------------------------------------------- drawing

/** Two decimals: what a drawing needs, and no float noise in the file. */
const round = (n) => Math.round(n * 100) / 100
const rect = (x, y, w, h) => [x, y, w, h].map(round)

/**
 * `[x, y, w, h]` cut across into parts of the given weights, a gap between
 * them: the keyboard's zones, a strip's segments.
 */
export function across([x, y, w, h], weights, gap = 0.1) {
  const total = weights.reduce((a, b) => a + b, 0)
  const room = w - gap * (weights.length - 1)
  let at = x
  return weights.map((weight) => {
    const width = (room * weight) / total
    const part = rect(at, y, width, h)
    at += width + gap
    return part
  })
}

/** `n` discs of `size` in a row, centred on `cx`. */
export function discs(cx, y, size, n, gap = 0.2) {
  const width = n * size + (n - 1) * gap
  return Array.from({ length: n }, (_, i) => rect(cx - width / 2 + i * (size + gap), y, size, size))
}

/**
 * `n` points evenly spaced along an ellipse, by length rather than by angle so
 * that neighbours stay apart where it curves most: the first at the top, then
 * clockwise.
 */
export function around(cx, cy, rx, ry, n) {
  const steps = 3600
  const at = (t) => [cx + rx * Math.sin(t), cy - ry * Math.cos(t)]
  const length = [0]
  for (let i = 1; i <= steps; i++) {
    const [x0, y0] = at(((i - 1) / steps) * 2 * Math.PI)
    const [x1, y1] = at((i / steps) * 2 * Math.PI)
    length.push(length[i - 1] + Math.hypot(x1 - x0, y1 - y0))
  }
  const whole = length[steps]
  let i = 0
  return Array.from({ length: n }, (_, k) => {
    while (length[i] < (whole * k) / n) i++
    return at((i / steps) * 2 * Math.PI)
  })
}

/** A laptop from the front, lid open, its back seen through the hinge. */
const LAPTOP = {
  outline: [
    { rect: [0.5, 0, 15, 8], r: 0.5 },
    { rect: [0, 9, 16, 9.5], r: 0.8 },
  ],
  lid: [7.2, 2.4, 1.6, 2],
  rear: [1, 8.1, 14, 0.8],
  power: { cx: 8, y: 9.4, size: 0.8 },
  keyboard: [1.6, 10.5, 13.8, 4.6],
  touchpad: [6.2, 15.6, 3.6, 2],
  front: [1, 17.95, 14, 0.3],
}

/** A desktop's front, and the side window when lights are inside. */
const DESKTOP = {
  outline: [{ rect: [0, 0, 9, 18], r: 0.6 }],
  side: { rect: [10, 0, 7, 18], r: 0.4 },
  power: { cx: 4.5, y: 1, size: 1 },
  ring: { one: [2, 3.5, 5, 10], cx: 4.5, cy: 8.5, rx: 2.5, ry: 4.5, size: 0.4 },
  ambient: { cx: 4.5, cy: 8.5, rx: 3.5, ry: 5.8, size: 0.4 },
  logo: [2.5, 16, 4, 0.8],
  inside: { x: 12.3, y: 1.5, size: 2.4, step: 4.25 },
}

/** The lights of a place, in their order. */
function ranked(lights) {
  return [...lights].sort((a, b) => a.rank - b.rank || a.id - b.id)
}

/** The ring at the back: halves stacked, sides by halves, or segments in turn. */
function rear(lights, [x, y, w, h]) {
  const halves = lights.filter((l) => l.half)
  const spans = lights.filter((l) => l.span)
  const rest = lights.filter((l) => !l.half && !l.span)
  if ([halves, spans, rest].filter((g) => g.length).length > 1) {
    throw new Error(`rear: ${lights.map((l) => l.name).join(', ')} mix halves, sides and segments`)
  }
  const out = new Map()
  for (const l of halves) {
    const top = l.half === 'top'
    out.set(l, { rect: rect(x, top ? y : y + h / 2, w, h / 2), shape: top ? 'archUp' : 'archDown' })
  }
  for (const l of spans) {
    const [a, b] = l.span
    out.set(l, { rect: rect(x + a * w + (a ? 0.05 : 0), y, (b - a) * w - 0.05, h) })
  }
  const parts = across([x, y, w, h], rest.map(() => 1))
  ranked(rest).forEach((l, i) => out.set(l, { rect: parts[i] }))
  return out
}

/**
 * Where each light is drawn: `lights` carry what `english` gives, plus their
 * id; the keyboard zones also their columns on alienfx-tools' grid, `from`
 * and `to`, which order them and give their widths.
 */
export function draw(kind, lights) {
  const by = Map.groupBy(lights, (l) => l.place)
  const out = new Map()
  const set = (l, item) => out.set(l, item)
  if (kind === 'laptop') {
    for (const [place, group] of by) {
      const list = ranked(group)
      switch (place) {
        case 'lid':
          across(LAPTOP.lid, list.map(() => 1)).forEach((r, i) => set(list[i], { rect: r, shape: 'disc' }))
          break
        case 'power': {
          const p = LAPTOP.power
          discs(p.cx, p.y, p.size, list.length).forEach((r, i) => set(list[i], { rect: r, shape: 'disc' }))
          break
        }
        case 'keyboard': {
          // Ordered and sized by the columns they cover on alienfx-tools'
          // grid; without one, by the place their names give, all as wide.
          const gridded = group.every((z) => z.from !== undefined)
          const zones = gridded ? [...group].sort((a, b) => a.from - b.from || a.to - b.to) : list
          const weights = zones.map((z) => (gridded ? z.to - z.from + 1 : 1))
          across(LAPTOP.keyboard, weights).forEach((r, i) => set(zones[i], { rect: r }))
          break
        }
        case 'touchpad':
        case 'front':
          across(LAPTOP[place], list.map(() => 1), place === 'front' ? 0.1 : 0.05).forEach((r, i) =>
            set(list[i], { rect: r }),
          )
          break
        case 'rear':
          for (const [l, item] of rear(group, LAPTOP.rear)) set(l, item)
          break
        default:
          throw new Error(`a laptop has no ${place}: ${list.map((l) => l.name).join(', ')}`)
      }
    }
    return { items: out, outline: LAPTOP.outline }
  }
  if (kind !== 'desktop') throw new Error(`a ${kind}: neither a laptop nor a desktop`)
  for (const [place, group] of by) {
    const list = ranked(group)
    switch (place) {
      case 'power': {
        const p = DESKTOP.power
        discs(p.cx, p.y, p.size, list.length).forEach((r, i) => set(list[i], { rect: r, shape: 'disc' }))
        break
      }
      case 'ring':
      case 'ambient': {
        const e = DESKTOP[place]
        if (list.length === 1 && e.one) {
          set(list[0], { rect: e.one, shape: 'disc' })
          break
        }
        around(e.cx, e.cy, e.rx, e.ry, list.length).forEach(([x, y], i) =>
          set(list[i], { rect: rect(x - e.size / 2, y - e.size / 2, e.size, e.size), shape: 'disc' }),
        )
        break
      }
      case 'logo':
        across(DESKTOP.logo, list.map(() => 1)).forEach((r, i) => set(list[i], { rect: r }))
        break
      case 'inside': {
        const s = DESKTOP.inside
        list.forEach((l, i) =>
          set(l, { rect: rect(s.x, s.y + i * s.step, s.size, s.size), shape: l.shape ?? 'disc' }),
        )
        break
      }
      default:
        throw new Error(`a desktop has no ${place}: ${list.map((l) => l.name).join(', ')}`)
    }
  }
  const outline = by.has('inside') ? [...DESKTOP.outline, DESKTOP.side] : DESKTOP.outline
  return { items: out, outline }
}

/** The first and last column a light covers on its grid. */
export function columns(light, grids) {
  const grid = grids.find((g) => g.id === light.grid)
  if (!grid || !light.cells) return {}
  const cols = cells(light.cells).map((c) => c % grid.cols)
  return { from: Math.min(...cols), to: Math.max(...cols) }
}

// ------------------------------------------------------------ the frames

/**
 * The reports an image gives, worked out here from the verified definition's
 * reports and not by the interpreter that replays them: lights of one colour
 * selected together, four at a time, in the order the colours first appear;
 * black written without its zeros, as the verified files write it.
 */
export function reports(frame, addresses, colours) {
  const groups = []
  addresses.forEach((address, i) => {
    const colour = colours[i] ?? '000000'
    const group = groups.find((g) => g.colour === colour)
    if (group) group.addresses.push(address)
    else groups.push({ colour, addresses: [address] })
  })
  const [select, paint] = frame.each.send
  if (!paint.endsWith('{r} {g} {b}')) throw new Error(`${paint}: not the colour report expected`)
  const out = [...frame.open]
  for (const { colour, addresses: all } of groups) {
    for (let at = 0; at < all.length; at += frame.each.chunk) {
      const chunk = all.slice(at, at + frame.each.chunk)
      out.push(select.replace('{count}', hex(chunk.length)).replace('{addresses}', chunk.map((a) => hex(a)).join(' ')))
      const rgb = colour === '000000' ? '' : [0, 2, 4].map((i) => colour.slice(i, i + 2)).join(' ')
      out.push(paint.replace('{r} {g} {b}', rgb).trim())
    }
  }
  return [...out, ...frame.close]
}

// ------------------------------------------------------------ definitions

const REFERENCE = ['ff0000', '00ff00', '0000ff']
const REPORT = 'https://github.com/o2csi/candeo/issues/new?template=device-report.yml'

/** The verified definition of the same controller, by product id. */
const VERIFIED = { '0550': 'alienware-17-r5-zones.json', '0551': 'alienware-m18-r1-zones.json' }

/** One machine's definition, from the facts. */
export function definition(facts, v, verified) {
  const map = facts.machines.find((m) => m.label === v.label)
  if (!map) throw new Error(`${v.file}: no map labelled “${v.label}”`)
  if (map.pid !== v.pid) throw new Error(`${v.file}: ${v.pid}, the map says ${map.pid}`)
  const sibling = verified[VERIFIED[map.pid]]

  const power = map.lights.filter((l) => l.flags & POWER)
  const kept = map.lights
    .filter((l) => !(l.flags & POWER))
    .map((l) => ({ ...l, ...english(l, map.grids), ...columns(l, map.grids), alienfx: l.name }))
  const { items, outline } = draw(v.kind, kept)
  const lights = [...kept].sort((a, b) => a.id - b.id)
  const named = (l) => `id ${l.id}, “${l.alienfx ?? l.name}”`

  const product = facts.productNames[v.productName.from]
  const notes = [
    `Unverified: never tried on this machine. Written on ${facts.derived} by alienfx.mjs from ${facts.source}`,
    `Its map there: “${map.label}”, line ${map.line}, device “${map.device}”. Which id lights what is that map's; the names are its own, in plain words.`,
    `The protocol is verified: the AW-ELC's live preview, as ${VERIFIED[map.pid]} sends it, which stores nothing in the device.`,
    `Matched on the machine (#326): SMBIOS names it “${v.machine}”, as ${product}: ${v.productName.at}.`,
    ...power.map((l) => `Left out: ${named(l)}, which alienfx-tools marks as the power button: the firmware drives it, as on the m18 R1.`),
    ...lights
      .filter((l) => l.place === 'power')
      .map((l) => `alienfx-tools does not mark ${named(l)}, as the power button: it is drawn, as the 17 R5's is.`),
    ...lights
      .filter((l) => l.flags & INDICATOR)
      .map((l) => `alienfx-tools marks ${named(l)}, as an indicator, which it keeps dark when the lights are off; here it follows the effect.`),
    ...(v.notes ?? []),
    v.kind === 'laptop'
      ? 'Drawn from the front, lid open, the lights at the back seen through the hinge. Where a light sits is not in the source: the keyboard zones keep the order and widths of alienfx-tools\' grid, the rest is drawn where its name puts it.'
      : 'Drawn as the case\'s front, the lights inside on a side window. Where a light sits is not in the source: it is drawn where its name puts it, the numbered ones in their order around an oval.',
    `Does it light? Say so: ${REPORT}`,
  ]

  const colours = REFERENCE.slice(0, lights.length)
  return {
    $schema: sibling.$schema,
    name: v.name,
    notes,
    match: { ...sibling.match, pid: map.pid, machine: v.machine },
    report: sibling.report,
    frame: sibling.frame,
    lights: {
      kind: 'zones',
      items: lights.map((l) => {
        const { rect: r, shape } = items.get(l)
        return { name: l.name, address: l.id, ...(shape && shape !== 'rect' ? { shape } : {}), rect: r }
      }),
    },
    outline,
    examples: [{ colours, reports: reports(sibling.frame, lights.map((l) => l.id), colours) }],
  }
}

/** Every map read is written, or said why not: a new one is never passed over. */
export function accounted(facts) {
  const written = new Set(facts.variants.map((v) => v.label))
  const left = new Set(facts.left.map((l) => l.label))
  return facts.machines.map((m) => m.label).filter((label) => !written.has(label) && !left.has(label))
}

// ------------------------------------------------------------------ main

async function main() {
  const { formatJson } = await import(pathToFileURL(join(root, 'apps/desktop/src/editor/jsonFormat.ts')).href)
  const facts = JSON.parse(readFileSync(factsPath, 'utf8'))
  const from = process.argv.indexOf('--from')
  if (from > 0) {
    facts.machines = awElc(machines(readFileSync(process.argv[from + 1], 'utf8')))
    writeFileSync(factsPath, formatJson(JSON.stringify(facts)))
    console.log(`${facts.machines.length} AW-ELC maps`)
  }
  const unaccounted = accounted(facts)
  if (unaccounted.length) throw new Error(`maps neither written nor left out: ${unaccounted.join(', ')}`)
  const verified = Object.fromEntries(
    Object.values(VERIFIED).map((f) => [f, JSON.parse(readFileSync(join(here, '..', f), 'utf8'))]),
  )
  for (const v of facts.variants) {
    const d = definition(facts, v, verified)
    writeFileSync(join(here, v.file), formatJson(JSON.stringify(d)))
    console.log(`${v.file}: ${d.lights.items.length} lights`)
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) await main()
