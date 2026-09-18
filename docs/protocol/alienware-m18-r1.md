# Alienware m18 R1 — lighting, as surveyed

**Established on 2026-09-17**, on an Alienware m18 R1, by watching what Alienware
Command Center writes on the USB bus (USBPcap, then the reports read back out of
the capture), and then by **writing to the keyboard** to confirm what had been
read. The AW-ELC was only ever listened to.

Firmware versions are **not established yet**: neither device was asked for one.
Until they are, treat every identifier here as true of this machine on this date,
as [`deathstalker-v2-pro.md`](deathstalker-v2-pro.md) does for its own version.

Two devices carry the lighting, and they speak differently.

| What | Device | Reports |
|---|---|---|
| The keys, one by one | `0d62:aab0`, the keyboard's vendor interface | feature report `0xcc`, 64 bytes |
| The zones around them | `187c:0551`, *AW-ELC* | output report, 33 bytes |

## 1. The keyboard: one colour per key

The vendor interface declares usage page `0xff89`. Lighting goes through
**`SET_REPORT` on the control endpoint**, feature report id `0xcc`, 64 bytes:

```
bmRequestType 0x21  bRequest 0x09  wValue 0x03cc  wIndex 0x0000  wLength 0x0040
```

`GET_REPORT` with the same report id reads 64 bytes back, so a write can be
checked rather than hoped for — the property that took a second survey to find on
the Razer keyboard.

### The commands seen

| First four bytes | What it does | Payload |
|---|---|---|
| `cc 8c 02 00` | **colours** | 15 groups of `index, R, G, B` |
| `cc 94 00 00` | opens a frame | zeroes |
| `cc 93 00 00` | closes it | zeroes |
| `cc 83 38 9c <level>` | **brightness**, `00` dark to `ff` full | only after a frame has been opened and closed |
| `cc 8b 01 ff` | commits a frame | always that same `ff`, never a level |
| `cc 8c 05 00`, `cc 8c 06 00`, `cc 8c 07 00` | three maps of 60 bytes, each `00` or `01` | which positions carry a key, most likely |
| `cc 8c 01 01` | a mode, carrying `ff ff 00 ff ff` | not established |
| `cc 8c 13 00` | zeroes | not established |

**One frame is eight colour reports**: 8 × 15 = 120 groups, for **110 addressed
keys**. Command Center sent about 91 frames a minute while an animation ran, and
resent the whole frame each time.

### Brightness, and what it is not

Established on 2026-09-18, on a keyboard lit white.

- `cc 8b 01 <level>` is **not** brightness. Written at `ff`, `80`, `40`, `10` and
  `00`, each report acknowledged, nothing changed. Command Center only ever
  writes it with `ff`, which is what it is: the commit of a frame.
- `cc 83 38 9c <level>` **is** brightness, `00` dark to `ff` full, the whole
  surface at once.
- It lands **only after `cc 94` then `cc 93`**. Sent on its own the report is
  acknowledged and nothing changes; sent after that pair, the level takes. The
  pair carries no colour, so nothing else moves.

The command did not come from the capture — Command Center never varied a level
while it was watched. It came from another project's source for this family of
keyboards, and every byte above was then written to this keyboard here. See §5.

### Key indexes

Indexes run from 1 to 136 with gaps: **110 are ever addressed**. Lighting one key
at a time gave these anchors:

| Key | Index |
|---|---|
| Esc | 1 |
| F1 | 2 |
| The four media keys above the numeric keypad | 17, 18, 19, 20 |
| `²` (below Esc) | 21 |
| Tab | 41 |
| A (the keyboard is AZERTY) | 43 |
| Enter | 75 |
| Left Ctrl | 101 |
| Space | 108 |
| Numeric keypad `0` | 118 |

They read as a walk **row by row, left to right, the numeric keypad included**,
starting at Esc. The media keys are addressed like any other key, with no command
of their own.

### A grid of twenty

Writing whole ranges of indexes, each in its own colour, and reading the keyboard
off a photograph, gives the shape: **every row holds exactly twenty indexes**.

| Indexes | Row |
|---|---|
| 1–20 | Esc, F1–F12, the media keys |
| 21–40 | the digits, `²` to Backspace |
| 41–60 | Tab, A Z E R T Y U I O P |
| 61–80 | Q S D F G H J K L M, Enter |
| 81–100 | W X C V B N and the punctuation |
| 101–120 | Ctrl, Fn, Windows, Alt, Space, AltGr |
| 121–136 | the arrows |

The numeric keypad does not have a range of its own: each row runs on into it.

**The gaps are wide keys.** Tab is 41 and A is 43, with 42 addressed by nobody:
a key wider than one cell takes its cell and leaves the next empty. So the
keyboard is a **seven by twenty matrix with empty cells**, 110 keys in 140 cells —
the same shape as the Razer's 6 × 22 carrying 106 keys, which the application
already models.

Colours are plain `R, G, B`. When a key changes, Command Center sends it first,
then every other key in index order.

### Brightness cannot be read, and a dark keyboard is usually it

Established on 2026-09-18. Writing two levels and reading the collection back
gives the **same** answer for both — the echo of the last command, `cc 93 …`,
and once the leftover bytes of the previous colour frame. Nothing in it follows
the level. A host therefore knows only what it wrote itself, and a level set by
other software is invisible.

Which matters more than it sounds: **a keyboard that looks dead is usually a
keyboard at brightness zero**. Colours written to it are accepted, acknowledged,
and show nothing. It happened twice here — once after the maker's software took
the device, once from a probe that sent the brightness command with its level
byte left at zero — and both times writing `cc 83 38 9c ff` brought everything
back. Before concluding that a device is stuck, set its brightness.

### An address is not a position

Established on 2026-09-18, by lighting single addresses and reading the keyboard,
key by key, until every one answered.

The device names a key by its **address**. An image names a key by its
**position** in the seven by twenty grid. The two are not the same number, and
taking one for the other lights the wrong key: past the first gap, every key
falls one cell short of where it should be. The map lives in
`crates/candeo-device/src/layout.rs`, position by position, with a marker where
the grid carries no key.

**Backspace is 36, not 34.** Addresses 34 and 35 are acknowledged and drive
nothing. It is the one address that does not follow its neighbours, and it was
found by lighting the orphans one at a time.

## 2. AW-ELC: the zones

`187c:0551` declares vendor usage page `0xff00` and a report descriptor of 34
bytes: one input report of 33 bytes, one output report of 33 bytes, no report id.
It is driven through the control endpoint all the same:

```
bmRequestType 0x21  bRequest 0x09  wValue 0x0200  wIndex 0x0000  wLength 0x0021
```

Every report starts with `03`, then a command byte. **The device answers**
`GET_REPORT` with the same command, its high bit set: `83 24` for `03 24`.

| Command | Seen as | What it carries |
|---|---|---|
| `03 24` | 450 of 750 reports | up to three blocks of 8 bytes: `02 82 00 0f` then `R, G, B` |
| `03 23 01 00` | 150 | two bytes that change with the effect |
| `03 21 00 01` | 75 | `ff ff`, which looks like a mask of zones |
| `03 21 00 03` | 75 | `00 ff` |

While a rainbow animation ran, the colours went past in the clear — `ff0000`,
`ffa500`, `ffff00`, `008000`, `00bfff`, `0000ff`, `800080` — rewritten every
150 ms.

### Writing to it, established on 2026-09-18

Three things had to be right at once, and getting any one of them wrong looks
exactly like a device that ignores everything.

**The control endpoint, not the interrupt one.** A plain HID write leaves through
the interrupt endpoint; this device acts only on `SET_REPORT` of its output
report, which is what the maker's software sends. Verified by capturing our own
writes and comparing them with Command Center's: same bytes, different path,
no effect. With `hidapi`, that is `send_output_report`, not `write`.

**The transaction that applies, not the one that stores.** Two families sit side
by side and only one shows:

| Family | What it does |
|---|---|
| `03 22 00 04/01/02 00 <tx>` | writes the configuration the device keeps |
| `03 21 00 04/01/02/06 00 <tx>` | the transaction whose colours are **lit** |

A change through `03 22` alone is accepted, answers nothing, and shows nothing.

**A transaction is numbered.** `<tx>` walks (`5e`, `5f`, `60`…). Reusing a number
the device has already seen looks like a repeat.

One zone, lit, is then:

```
03 21 00 04 00 <tx>          opens
03 21 00 01 00 <tx>
03 23 01 00 <count> <ids…>   selects zones by id
03 24 <mode> <hi lo> <hi lo> <R G B> [more entries]
03 21 00 02 00 <tx>          commits
03 21 00 06 00 <tx>
```

`<mode>` is `00` for a fixed colour, `01` and `02` for the animated ones, which
carry several colour entries; the two pairs of bytes are durations
(`07d0` = 2000, `03e8` = 1000).

### It can stop applying, for everyone

Seen twice on 2026-09-18, hours apart: the device goes on **acknowledging every
write and applying none**. Not our doing, and not a question of who is talking to
it — the maker's own software, on its own reports, could no longer change a zone
either, while it kept changing the keyboard in the same session. The state was
there before anything of ours had ever written to this device.

What does *not* clear it: writing anything else, waiting, closing and reopening
the collection, stopping the maker's agent, restarting its application. What is
left is a power cycle.

So a host that drives these zones cannot take silence for success, and cannot
read a refusal as its own mistake. The reports here are the ones the maker sends;
when they stop working, the device is the thing to reset.

### Which zone is which

Established by writing one zone at a time, each in its own colour, and reading
the machine — with the colours **rewritten every 150 ms**, because the maker's
lighting agent restarts on its own and repaints within a second otherwise.

| Id | Light |
|---|---|
| 0 | the ring around the rear connectors, upper half |
| 1 | the same ring, lower half |
| 2 | the logo on the lid |
| 4 | the power button |

Id `3` lights nothing on this machine. Ids 0 and 1 are always addressed together
by the maker's software, which is why the ring looked like one light until each
half was written alone.

**The power button is not only ours.** It pulses by itself, and more slowly on
battery than on mains: that state is the device's own, not something a host
writes. Anything Candeo sets there is overwritten when the firmware pulses.

## 3. Writing works, and what it costs

The keyboard **accepts the reports above** from anything that opens its collection:
brightness, a frame opened with `cc 94`, colours in `cc 8c 02 00`, closed with
`cc 93`. The colours stay until someone writes again, and the device answers each
report — `cc 94 …` after opening a frame, `cc 93 …` after closing one.

**Command Center only writes when it animates.** On a fixed colour, or while
someone edits a profile, it falls silent and what we write stays; on an animated
effect it rewrites the whole keyboard about once a second and wins. So sharing a
keyboard is a question of **what the other software is doing**, not of whether it
runs — and Candeo will have to say so rather than fight for the device.

**It keeps up.** Written as fast as one process can, from a single open handle,
the keyboard took **147 frames in 5 seconds** — about 29 frames a second, 353
reports a second, every one acknowledged, and the alternation was visible as
flicker. An animation at 8 frames a second travels across the keys smoothly.
Command Center's 1.5 frames a second is its own choice, not the device's limit.

One caveat on measuring this: **opening the device costs more than writing to
it**. A frame written from a freshly started process takes about 250 ms, most of
it the opening; from a handle already open, about 34 ms. A survey that reopens
the device per frame measures its own startup.

## 4. What this leaves open

- The **firmware version** of each device, and where to read it.
- What `cc 8c 01 01`, `cc 8c 13 00` and the three `00`/`01` maps mean.
- What the two duration pairs of `03 24` really measure, and what modes `01` and
  `02` do beyond carrying several colours.
- Whether the power button can be held against its own pulsing, and where that
  pulse is configured.
- What the keyboard does with a **faster stream** than twelve frames a second,
  and whether it keeps its colours when the machine sleeps.

## 5. What others have published

Looked for on 2026-09-18, once the keyboard was already lighting, to check this
reading against anyone else's.

- **No published description of this protocol was found.** Searches for the
  report ids and the byte sequences came back to this repository's own issue.
- `tr1xem/alienfx-linux` (GPL-3.0) names feature report `0xcc` the lighting
  interface of the per-key notebook keyboards, what it calls API v5. It
  corroborates the shape without giving the commands.
- `T-Troll/AlienFX-SDK`, whose licence we could not establish, carries that API's
  command table, brightness included; `T-Troll/alienfx-tools` documents it from
  the outside as a hardware dimming level from 0 to 255. That is what put
  `cc 83 38 9c` under our eyes — and what sent us back to the keyboard, where
  every byte of §1 was written and read back.
- **No key map** for this model or a sibling was found: the one this application
  carries is this survey's own.
- Nobody tells the firmware to stop its own animation. What these projects do is
  stop Command Center's Windows service — a choice about a machine, not a report
  to send.

## Method

Capture with USBPcap on the root hub carrying both devices, while Command Center
changed the lighting; the reports were then pulled out of the capture by matching
the `SET_REPORT` setup packet and reading the bytes that follow. Lighting one key
at a time, with everything else off, is what turns a stream of colours into a key
index.

The grid came the other way around: **writing** ranges of indexes, each range in
its own colour, then reading the keyboard off a photograph. Five indexes per
colour is fine enough to count, and a wide key shows up as a dark cell inside a
lit group.
