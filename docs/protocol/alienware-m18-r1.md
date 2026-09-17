# Alienware m18 R1 — lighting, as surveyed

**Established on 2026-09-17**, on an Alienware m18 R1, by watching what Alienware
Command Center writes on the USB bus (USBPcap, then the reports read back out of
the capture). **Nothing was written to either device**: every fact below is an
observation of the maker's own software.

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
| `cc 8b 01 ff` | brightness | `ff` while the lighting was at full |
| `cc 8c 05 00`, `cc 8c 06 00`, `cc 8c 07 00` | three maps of 60 bytes, each `00` or `01` | which positions carry a key, most likely |
| `cc 8c 01 01` | a mode, carrying `ff ff 00 ff ff` | not established |
| `cc 8c 13 00` | zeroes | not established |

**One frame is eight colour reports**: 8 × 15 = 120 groups, for **110 addressed
keys**. Command Center sent about 91 frames a minute while an animation ran, and
resent the whole frame each time.

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
of their own. The gaps are not explained yet: the full map needs a pass lighting
one row at a time.

Colours are plain `R, G, B`. When a key changes, Command Center sends it first,
then every other key in index order.

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

## 3. What this leaves open

- The **firmware version** of each device, and where to read it.
- The **rest of the key map**: which index each remaining key holds, and what the
  gaps are.
- What `cc 8c 01 01`, `cc 8c 13 00` and the three `00`/`01` maps mean.
- Which **zones** the AW-ELC addresses, and what `02 82 00 0f` selects.
- Whether writing works while Command Center's lighting agent runs: it rewrites
  the whole keyboard about once a second, so it would have to be stopped.

## Method

Capture with USBPcap on the root hub carrying both devices, while Command Center
changed the lighting; the reports were then pulled out of the capture by matching
the `SET_REPORT` setup packet and reading the bytes that follow. Lighting one key
at a time, with everything else off, is what turns a stream of colours into a key
index.
