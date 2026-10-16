# Alienware 17 R5 — lighting, as surveyed

Established on 2026-10-16, firmware not read, on one machine.

| What | Device | Reports |
|---|---|---|
| Every light: keyboard zones, lid, base, touchpad | `187c:0550`, *AW-ELC* | output report, 33 bytes |

The machine has no per-key keyboard: its keyboard is four zones of the same
controller as the lights around it.

## 1. The same controller as the m18 R1's zones

`187c:0550` declares vendor usage page `0xff00`, usage `0x0001`, and an output
report of 33 bytes, as the m18 R1's `187c:0551` does. The framing of
[`alienware-m18-r1.md`](alienware-m18-r1.md) §2 — the live preview, opened with
`03 21 00 01 ff ff`, zones selected by id with `03 23`, a colour with `03 24`,
shown with `03 21 00 03 00 ff` — lights this machine unchanged, sent through the
control endpoint at no more than ten changes a second. It stores nothing in the
device.

`T-Troll/alienfx-tools` (MIT) waits for a *ready* status between reports on
`0550` and not on `0551`. Candeo reads no status, and every zone lit all the same.

## 2. Which id is which

Found by lighting one id at a time in white, everything else dark, three
seconds each, and watching the machine.

| Id | Light |
|---|---|
| 0 | the alien head on the back of the lid |
| 1 | the logo under the screen |
| 2 | nothing seen; taken for the touchpad, which lights when touched |
| 3 | the power button |
| 4 | the keyboard's numeric keypad |
| 5, 6, 7 | the rest of the keyboard, in three zones |
| 8 | the outer strip on the left side of the base |
| 9 | the same on the right side |
| 10 | the outer strip on the left edge of the lid |
| 11 | the same on the right edge |
| 12 | the media keys, left of the keyboard |
| 13 to 15 | nothing seen |

The power button took the colour written live, unlike the m18 R1's, which
keeps pulsing with the power state.

## 3. What this leaves open

- The **firmware version**, and where to read it.
- The order of zones 5, 6 and 7 from left to right, which the definition draws
  as 5, 6, 7.
- Whether id 2 is the touchpad, and what it shows when not touched.
- Whether the colours hold through sleep, and on battery.
- `alienfx-tools` lists the 17 R5 as its *API v3*, twelve-byte reports; this
  machine has the 33-byte AW-ELC. Another revision of the model may carry the
  older controller.

## Method

The device was found by its product id on the machine, then driven by a
definition in `Documents\candeo\devices` drawing ids 0 to 15 as sixteen lights,
and an effect lighting one at a time. Nothing was captured: the reports are the
ones the m18 R1's survey established.
