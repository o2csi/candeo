// The pure parts of `alienfx.mjs`, and the files it wrote: reading
// alienfx-tools' CSV, naming the lights, drawing them, and the frames.
//
//   node --test crates/candeo-device/devices/unverified/
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { test } from 'node:test'
import { fileURLToPath, pathToFileURL } from 'node:url'

import {
  accounted,
  around,
  awElc,
  cells,
  columns,
  definition,
  draw,
  english,
  fields,
  machines,
  onKeyboard,
  ranges,
  reports,
} from './alienfx.mjs'

const here = dirname(fileURLToPath(import.meta.url))

// The shape of alienfx-tools' file, with made-up machines.
const CSV = [
  "'3','Test laptop'",
  "'2','0','10','2','Keyboard'",
  "'2','1','3','3','Tron'",
  "'0','6268','1360','Main lights'",
  "'1','0','0','Alienhead','1','4'",
  "'1','1','1','Power Button'",
  "'1','2','0','KB Left','0','0','0','1','0','10','0','11'",
  "'1','3','0','KB Right','0','2','0','3','0','4','0','12','0','13','0','14'",
  '',
  "'1','4','0','Ring Top'",
  "'3','Other machine',,,,",
  "'0','3426','2588','RGB Keyboard'",
  "'1','0','2097152','Space','0','5'",
  "'0','6268','1361','Tron, both halves'",
  "'1','0','2','Tron Bottom'",
].join('\r\n')

test('a line is split as alienfx-tools splits it', () => {
  assert.deepEqual(fields("'1','2','0','KB Left','0','40'"), ['1', '2', '0', 'KB Left', '0', '40'])
  assert.deepEqual(fields("'3','Alienware m18R1',,,,,"), ['3', 'Alienware m18R1'], 'the empty fields dropped')
  assert.deepEqual(fields("'0','6268','1360','Tron (Not set, do manual set!)'").at(-1), 'Tron (Not set, do manual set!)')
  assert.equal(fields(''), null)
  assert.equal(fields('not quoted'), null)
})

test('cells are written as ranges and read back', () => {
  assert.equal(ranges([40, 41, 42, 80, 82, 83]), '40-42 80 82-83')
  assert.deepEqual(cells('40-42 80 82-83'), [40, 41, 42, 80, 82, 83])
  assert.deepEqual(cells(ranges([7])), [7])
})

test('the machines, their grids, devices and lights', () => {
  const [laptop, other] = machines(CSV)
  assert.equal(laptop.label, 'Test laptop')
  assert.equal(laptop.line, 1)
  assert.deepEqual(laptop.grids[0], { id: 0, cols: 10, rows: 2, name: 'Keyboard' })
  const [lights] = laptop.devices.map((d) => d.lights)
  assert.deepEqual(lights[0], { id: 0, name: 'Alienhead', flags: 0, grid: 1, cells: '4' })
  assert.deepEqual(lights[1], { id: 1, name: 'Power Button', flags: 1 })
  assert.equal(lights[3].cells, '2-4 12-14')
  assert.equal(lights[4].name, 'Ring Top', 'a blank line ends nothing')
  assert.equal(other.label, 'Other machine')
  const space = other.devices[0].lights[0]
  assert.deepEqual([space.flags, space.scancode], [0, 32], 'the flags in the low word, a scan code in the high one')
})

test('only the AW-ELC maps are kept, with the grids their lights use', () => {
  const maps = awElc(machines(CSV))
  assert.deepEqual(
    maps.map((m) => [m.label, m.pid, m.device]),
    [
      ['Test laptop', '0550', 'Main lights'],
      ['Other machine', '0551', 'Tron, both halves'],
    ],
  )
  assert.deepEqual(maps[0].grids.map((g) => g.name), ['Keyboard', 'Tron'])
  assert.deepEqual(maps[1].grids, [])
})

test('the lights are named in plain words, and placed', () => {
  const keyboard = [{ id: 1, cols: 16, rows: 7, name: 'RGB Keyboard' }]
  const on = (name, extra = {}) => english({ name, ...extra }, keyboard)
  assert.deepEqual(on('KB Left'), { name: 'keyboard, left', place: 'keyboard', rank: 0 })
  assert.equal(on('KB Center-Left').name, 'keyboard, centre left')
  assert.equal(on('KB RIGHT MID').name, 'keyboard, centre right')
  assert.equal(on('KB numpad').name, 'keyboard, numeric keypad')
  assert.equal(on('Center-Right', { grid: 1, cells: '25-28' }).name, 'keyboard, centre right', 'its place alone, on a keyboard grid')
  assert.equal(on('PowerButton', { grid: 1, cells: '15' }).place, 'power', 'a cell on the keyboard grid is no zone')
  assert.throws(() => on('Right'), /no name for it here/, 'a place alone, off a keyboard grid, says nothing')
  assert.equal(on('Right of Tron').name, 'right of the ring')
  assert.deepEqual(on('Ring Top'), { name: 'ring, top', place: 'rear', rank: 0, half: 'top' })
  assert.equal(on('TRON-BackLeft').name, 'ring, back left')
  assert.equal(on('BackAlienHead').name, 'alien head on the lid')
  assert.equal(on('Exahust Lights 2').name, 'exhaust lights, 2', 'the typo read')
  assert.deepEqual(on('Ring_LED25'), { name: 'front ring, 25', place: 'ring', rank: 25 })
  assert.deepEqual(on('01'), { name: 'light bar, 1', place: 'front', rank: 1 })
  assert.equal(on('Alienhead/Power Button').name, 'alien head and power button')
  assert.equal(on('Power Button 2').name, 'power button, 2')
  assert.throws(() => on('Light 104'), /no name for it here/)
  assert.throws(() => on('KB Somewhere'), /no keyboard zone/)
  assert.equal(onKeyboard({ name: 'Left' }, keyboard), false, 'on no grid')
})

/** No two items of a drawing overlap, and none sticks out at the top or left. */
function apart(items) {
  const rects = [...items.values()].map((i) => i.rect)
  for (const [x, y] of rects) assert.ok(x >= 0 && y >= 0)
  rects.forEach((a, i) =>
    rects.slice(i + 1).forEach((b) => {
      const clear = a[0] + a[2] <= b[0] || b[0] + b[2] <= a[0] || a[1] + a[3] <= b[1] || b[1] + b[3] <= a[1]
      assert.ok(clear, `${a} and ${b} overlap`)
    }),
  )
}

test("the keyboard zones keep the order and widths of alienfx-tools' grid", () => {
  const grids = [{ id: 0, cols: 10, rows: 2, name: 'Keyboard' }]
  const zone = (id, name, at) => {
    const light = { id, name, grid: 0, cells: at }
    return { ...light, ...english(light, grids), ...columns(light, grids) }
  }
  // Listed right to left, as on the 17 R5: drawn left to right all the same.
  const right = zone(1, 'KB Right', '6-9 16-19')
  const left = zone(2, 'KB Left', '0-5 10-15')
  assert.deepEqual([left.from, left.to, right.from, right.to], [0, 5, 6, 9])
  const { items } = draw('laptop', [right, left])
  const [l, r] = [items.get(left).rect, items.get(right).rect]
  assert.ok(l[0] < r[0], 'left first')
  assert.ok(Math.abs(l[2] / r[2] - 6 / 4) < 0.01, 'as wide as their columns')
  apart(items)
})

test('a laptop: lid, the back through the hinge, keyboard, touchpad, front', () => {
  const lights = [
    'Alienhead',
    'Power Button 1',
    'Power Button 2',
    'Ring Top',
    'Ring Bottom',
    'KB left',
    'KB middle',
    'KB right',
    'KB numpad',
    'Touchpad1',
    'Touchpad2',
    '01',
    '02',
  ].map((name, id) => ({ id, name, ...english({ name }) }))
  const { items, outline } = draw('laptop', lights)
  assert.equal(items.size, lights.length)
  apart(items)
  const at = (name) => items.get(lights.find((l) => l.name === name))
  assert.equal(at('ring, top').shape, 'archUp')
  assert.equal(at('ring, bottom').rect[1], at('ring, top').rect[1] + at('ring, top').rect[3], 'the halves meet')
  const [lid, base] = outline.map((o) => o.rect)
  assert.ok(at('alien head on the lid').rect[1] + at('alien head on the lid').rect[3] <= lid[1] + lid[3])
  assert.ok(at('ring, top').rect[1] >= lid[1] + lid[3] && at('ring, bottom').rect[1] + 0.4 <= base[1], 'between lid and base')
  assert.ok(at('light bar, 1').rect[0] < at('light bar, 2').rect[0])
  assert.throws(() => draw('laptop', [{ id: 0, ...english({ name: 'ALIENWARE' }) }]), /a laptop has no logo/)
})

test('a desktop: numbered lights around an oval, the inside on a side window', () => {
  const lights = [
    ...Array.from({ length: 34 }, (_, i) => `Ring_LED${i + 1}`),
    ...Array.from({ length: 34 }, (_, i) => `Ambient_LED${i + 1}`),
    'PowerButton1',
    'PowerButton2',
    'ALIENWARE',
    'Internal_Ambient',
    'Fanlight',
  ].map((name, id) => ({ id, name, ...english({ name }) }))
  const { items, outline } = draw('desktop', lights)
  assert.equal(items.size, lights.length)
  apart(items)
  assert.equal(outline.length, 2, 'the side window')
  const first = items.get(lights[0]).rect
  assert.ok(Math.abs(first[0] + first[2] / 2 - 4.5) < 0.01, 'the first at the top')
  assert.deepEqual(draw('desktop', [{ id: 0, ...english({ name: 'Front Ring' }) }]).outline.length, 1)
})

test('points around an ellipse are evenly spaced', () => {
  const points = around(0, 0, 2, 5, 20)
  const gaps = points.map((p, i) => Math.hypot(...p.map((v, k) => v - points[(i + 1) % 20][k])))
  assert.ok(Math.max(...gaps) - Math.min(...gaps) < 0.05, `${gaps}`)
  assert.deepEqual(points[0].map((v) => Math.round(v * 1000) / 1000), [0, -5])
})

const FRAME = JSON.parse(readFileSync(join(here, '..', 'alienware-17-r5-zones.json'), 'utf8')).frame

test('the reports select lights by colour, four at a time, as the verified file gives them', () => {
  assert.deepEqual(reports(FRAME, [0, 1, 2, 3, 4, 5], ['ff0000', '000000', 'ff0000']), [
    '03 21 00 01 ff ff',
    '03 23 01 00 02 00 02',
    '03 24 00 07 d0 00 fa ff 00 00',
    '03 23 01 00 04 01 03 04 05',
    '03 24 00 07 d0 00 fa',
    '03 21 00 03 00 ff',
  ])
  // The 17 R5's own second reference frame, worked out again here.
  const r5 = JSON.parse(readFileSync(join(here, '..', 'alienware-17-r5-zones.json'), 'utf8'))
  const addresses = r5.lights.items.map((i) => i.address)
  assert.deepEqual(reports(FRAME, addresses, r5.examples[1].colours), r5.examples[1].reports)
})

const facts = JSON.parse(readFileSync(join(here, 'alienfx.facts.json'), 'utf8'))
const verified = Object.fromEntries(
  ['alienware-17-r5-zones.json', 'alienware-m18-r1-zones.json'].map((f) => [
    f,
    JSON.parse(readFileSync(join(here, '..', f), 'utf8')),
  ]),
)

test('every map read is written or said why not, once per machine', () => {
  assert.deepEqual(accounted(facts), [])
  assert.deepEqual(accounted({ ...facts, left: [] }).includes('Aurora R16'), true)
  const named = facts.variants.map((v) => v.machine)
  assert.equal(new Set(named).size, named.length, 'one definition per machine')
  for (const v of facts.variants) {
    assert.equal(v.file, `${v.name.toLowerCase().replace(/\s+/g, '-')}.json`, v.file)
    assert.ok(facts.productNames[v.productName.from], `${v.file}: where its product name comes from`)
  }
})

test('a light marked as the power button is left out, and the machine is matched', () => {
  const m15 = facts.variants.find((v) => v.machine === 'Alienware m15')
  const d = definition(facts, m15, verified)
  assert.deepEqual(d.match, { vid: '187c', pid: '0550', usagePage: 'ff00', usage: '0001', machine: 'Alienware m15' })
  assert.ok(!d.lights.items.some((i) => i.address === 1), 'its power button')
  assert.ok(d.notes.some((n) => n.startsWith('Left out: id 1, “Power Button”')))
  assert.deepEqual(d.frame, verified['alienware-17-r5-zones.json'].frame)
  const m16 = definition(facts, facts.variants.find((v) => v.pid === '0551'), verified)
  assert.equal(m16.match.pid, '0551')
  assert.ok(m16.notes.some((n) => n.includes('choose it on the device')))
})

test('the files are what the generator writes from the facts', async () => {
  const { formatJson } = await import(
    pathToFileURL(join(here, '../../../../apps/desktop/src/editor/jsonFormat.ts')).href
  )
  for (const v of facts.variants) {
    const written = readFileSync(join(here, v.file), 'utf8').replaceAll('\r\n', '\n')
    assert.equal(written, formatJson(JSON.stringify(definition(facts, v, verified))), `${v.file}: run alienfx.mjs`)
  }
})
