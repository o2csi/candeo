# Keyboards beyond Razer

A study, 2026-09-27: can the keyboards of other brands be described the way
the Razer ones are ([`device-sdk.md`](device-sdk.md) §9), and what would it
take?

**Sources.** OpenRGB at commit `0f8f2dcc` (2026-09-25), its controllers read as
data, nothing of it built or run; the Wooting RGB SDK at `98364bc`; the HID
Lighting and Illumination page, [HUTRR84]; Microsoft's Dynamic Lighting
documentation. What follows is how those sources drive each keyboard. Nothing
was sent to a device, and nothing here is established until a device says so.

[HUTRR84]: https://www.usb.org/sites/default/files/hutrr84_-_lighting_and_illumination_page.pdf

---

## 1. What decides it

The Razer keyboards were cheap to add because one of them is verified: the
protocol is known, only the matrix and a few bytes come from OpenRGB. For any
other brand, the protocol itself comes from OpenRGB's code. Three questions
decide each family:

- **Can a definition express it?** Fixed reports, rows or chunks of lights,
  `open` and `close`, a take-over, an `xor` or `sum` byte — and no reading of a
  reply, no colour planes, no CRC16.
- **Must the keyboard change mode, and what does that cost?** Candeo never puts
  a Razer in driver mode (§6 of the SDK); a mode whose effect on keys, Fn
  layers or macros is unknown is probed before it is sent.
- **Does anything persist?** Nothing sent thirty times a second may write to
  flash.

---

## 2. Family by family

| Family | Keyboards | Frame | Mode | Persists | Fits |
|---|---|---|---|---|---|
| **Razer** extended matrix | ~40 | a feature report per row, XOR byte | none | nothing | yes — done |
| **ASUS** ROG/TUF (`C0 81`) | 23 per key (Scope, Strix Flare, Falchion, Azoth, Claymore, TUF K3/K7…) | 64-byte writes of 15 × `address r g b` | none; the Claymore takes `41 01`, `41 03` | save `50 55`, never per frame | yes, one definition per locale |
| **Logitech** HID++ feature `0x8081` | G815, G915 (TKL), G515, PRO X Rapid… | 20-byte `11 FF {feature} 1F` × 4 keys, then `7F` | take-over sets the firmware effect off | the take-over, with persist = 1 | yes, per model and link — see below |
| Logitech feature `0x8080` | G810, G610, G512, G910, G Pro | 64-byte report `12` per key type, commit on another collection | none | nothing | no: two collections |
| **SteelSeries** Apex per key | 14 (Apex 5, 7, Pro, Pro TKL, 9…) | one 642-byte feature report `3A n (usage r g b)…` | Gen3: `4B`; leaving: `3B` or `41` | nothing | yes, with a report on leaving |
| **HyperX** Alloy Origins | 7 (Origins, 60, 65, Elite 2) | `04 F2`, then 9 × 16 `81 r g b` | none; the firmware reverts without a fresh image | nothing | yes, with a keep-alive |
| HyperX Origins 2 65 | 1 | output id `44`, 4 rows of 20 | open `01 04 00` | nothing | yes |
| HyperX Alloy Elite, FPS, Origins Core | 3 | one colour channel per report | — | unknown | no |
| **Corsair** V1 (K70, K95, Strafe…) | 21 | 12 writes, colour planes, a commit per channel | software mode and undocumented "key mapping" packets, never restored | hardware modes write a file, not per frame | no |
| Corsair V2 (K70 Pro, K100, K60…) | 17 | a handle opened, its reply read, a stream cut mid-light | software mode, never restored; reverts after about a minute | nothing per frame | no |
| **Wooting** V2/V3 | ~10 | one 256- or 2047-byte report, RGB565 | colour init; the SDK resets on leaving | nothing | no: RGB565, a report id per report |
| **Roccat** / Turtle Beach Vulcan | 11 | 65-byte stream, colour planes in blocks, two collections | direct mode | on firmware modes only | no |
| **Ducky**, **Cooler Master** V2 | 2 + ~18 | one stream cut across 8–10 reports, mid-light | `41 01` (Ducky); unknown | none seen | no: needs a stream |
| **Keychron** stock QMK (`3434:*`) | many | `A8 0A {start} n` + 9 colours in **HSV**, an echo awaited | VIA per-key effect | on request only | no: HSV and a start |
| QMK with OpenRGB's firmware | added by hand | `08 n (address r g b)…` | none | nothing | yes, for a custom firmware |
| MSI laptops, Keychron K3 v2, ASUS Cerberus | — | zones only, or a profile rewritten per change | — | the last two write profiles | excluded |

Across them:

- **Where OpenRGB reads the device, a definition cannot.** ASUS reads the
  locale, SteelSeries the firmware generation, Corsair and Roccat the layout.
  A definition then exists per locale or per generation, or draws the superset,
  as the Razer ones draw ISO.
- **Most brands write where Candeo does not.** OpenRGB sends ASUS, Logitech,
  Corsair, Ducky, Cooler Master and Roccat reports with `hid_write`, the
  interrupt pipe. Candeo's `output` is `HidD_SetOutputReport`, the control
  pipe. Whether each firmware takes the one for the other is unknown.
- **Logitech:** the feature index in each report is fixed per model, link
  (cable or receiver) and probably firmware; OpenRGB's newer driver asks the
  device for it. The device answers a commit sent too soon with *busy*, which
  Candeo would not see: a paced frame, not a fast one. OpenRGB's older take-over
  sets the effect off with persist = 1, a flash write each time Candeo would
  take the keyboard: a definition would send it with persist = 0, untried.
- **SteelSeries** Gen1 reads the unit's serial number: a definition never asks
  it, and nothing logs it.

---

## 3. What would widen the format

By what each opens, the cheapest first:

1. **A `write` wire** — `hid_write`, the interrupt pipe. Asked by most
   families above. Small.
2. **Reports on leaving** — what gives the lighting back to the firmware when
   Candeo lets a device go: SteelSeries `3B`/`41`, the Claymore's `41 00`,
   Wooting's reset. Small, and what *hand back* already means for Razer.
3. **A keep-alive** — the last image sent again after so many milliseconds
   without a change: HyperX, Corsair V2. Small; `skipUnchanged` gives way to it.
4. **A stream** — one buffer of colours cut across reports whatever falls
   where, with a packet index and a 16-bit length: Ducky, Cooler Master V2,
   Roccat, Corsair V2, Wooting's ARM boards. Medium.
5. **HSV colours and a `{start}`** — Keychron's stock firmware, Vial. Medium;
   Keychron sells a great many keyboards.
6. **Colour planes, RGB565, a report id or a collection per report** — each
   for one or two families. Last.

---

## 4. LampArray, the standard way

Windows 11's Dynamic Lighting rests on a HID standard, the Lighting and
Illumination page `0x59` ([HUTRR84]). A LampArray keyboard **describes
itself**: how many lamps, each one's position in micrometres, the key it sits
under as a HID usage, how many levels each colour has. The host then sends
lamps and colours, eight or so per report, and a *complete* flag; *autonomous
mode* off hands the lamps to the host, on gives them back to the firmware.

- **Here:** the DeathStalker V2 Pro exposes a collection on page `0x59` next to
  its Razer interfaces, and describes itself through it: 105 lamps with their
  positions in millimetres and their keys, read on 2026-09-27 (its survey,
  §13). Its updates, though, are taken and not shown: Razer's firmware keeps
  the lamps until Synapse hands them over, with a switch that is not known yet.
  A standard in the descriptor is not a standard in use.
- **Elsewhere:** other Razer keyboards (the DeathStalker V2, Huntsman V3 Pro,
  BlackWidow V4 Pro, Ornata V3, per Razer's support pages); laptops such as
  Acer's Predator Helios 16 and ASUS's Vivobook S14/S16, natively. Logitech
  answers through G HUB, software rather than firmware. HP/HyperX, ASUS ROG and
  SteelSeries are announced partners; no model was found.
- **It does not fit a file per model**, and needs none: a built-in family,
  like the `razer` inspection, would match page `0x59` whatever the vendor,
  read the report descriptor and every lamp once on open, and build the layout
  from them — micrometres to key units, HID usages to scancodes through a fixed
  table. A file per model could still add names and shapes.
- **Windows' own API** (`Windows.Devices.Lights.LampArray`) needs Dynamic
  Lighting on, gives way to the foreground application, and needs package
  identity to light in the background. Raw HID is the path on both systems;
  the API may come later for when Dynamic Lighting is on.
- **Risks.** A maker holding the lamps until its own software releases them,
  as Razer does; racing Windows when Dynamic Lighting is on; a keyboard driven by
  two protocols at once, so one is chosen per device; no colour read back, so a
  report a device drops is dropped silently; lamps are points, so rectangles
  are guessed from their spacing; about thirteen reports per frame for a full
  keyboard.
- **Cost:** one to two weeks. The descriptor parser, the lamp reading and the
  layout are pure functions; Microsoft's example descriptor is a ready test
  fixture.

OpenRGB's own LampArray driver takes shortcuts a definition should not: it
never checks the lamp id a device answers with, ignores each colour's level
count, which the standard says makes a device drop the whole report, and sends
frames without waiting the device's minimum interval.

---

## 5. Proposed order

1. **LampArray, verified on the DeathStalker.** A read-only probe first — the
   attributes and every lamp, `Get` feature reports — then one frame with
   Dynamic Lighting off. It is the one path verifiable here, and it serves every
   brand that ships the standard.
2. **The `write` wire, reports on leaving, the keep-alive**, then unverified
   definitions — *protocol unverified*, one degree below the Razer ones, whose
   protocol is verified on a sibling — for ASUS ROG/TUF, SteelSeries Apex,
   HyperX Origins and Logitech G815/G915. Their key maps are C++ arrays, per
   model: each family gets its generator, as `openrgb.mjs` is Razer's.
3. **HSV and `{start}`** for Keychron's stock firmware.
4. **Not now:** Corsair, until someone probes what its software mode does to
   the keys; Wooting and Roccat, until a stream exists; the Keychron K3 v2, MSI
   and the Cerberus, which write profiles or have no keys to light.

The site would tell the two degrees apart: *to verify* for a model whose
protocol a sibling proved, *protocol to verify* for the others.
