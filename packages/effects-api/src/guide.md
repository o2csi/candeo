# Writing a Candeo effect

This guide is for an assistant connected to Candeo through MCP. It describes the
effects API installed with this version of Candeo: API version 1. The
declaration itself, every type and helper with its documentation, is the
resource `candeo://effects-api` (or the tool `get_effects_api`).

## What an effect is

An effect is one TypeScript module. Its file name is its name. It
default-exports an object, and Candeo calls its `render` function about 30 times
a second with the layout, the time and a frame to paint:

```ts
import { defineEffect, hsv } from '@candeo/effects-api'

export default defineEffect({
  description: { en: 'Hues sweep across the keyboard', fr: 'Les teintes balaient le clavier' },
  params: {
    speed: { kind: 'number', label: { en: 'Speed', fr: 'Vitesse' }, min: 0, max: 360, default: 90 },
  },
  render({ layout, time, frame, params }) {
    for (const key of layout.keys) {
      frame.set(key, hsv(time * params.speed + key.col * 12, 1, 1))
    }
  },
})
```

- **One import**, `@candeo/effects-api`. There is no other module, no
  `node_modules`, no network, no file system, no timers. `Math`, `Date` and the
  language itself are there.
- **`defineEffect`** types `render`'s argument from `params`; it does nothing at
  run time.
- **`time`** is in seconds since the effect started, from the real clock: animate
  on it. `frameIndex` counts frames, which are not guaranteed.
- **`layout.keys`** holds only the lights. Each has `row` and `col` in the matrix,
  and `x`, `y`, `w`, `h`, its rectangle in key units. A wide key is one light;
  matrix holes have none. `center(key)` and `bounds(layout)` help with the
  rectangles.
- **`frame.set(key, colour)`** paints one light, `frame.fill(colour)` all of them.
  A light not painted in a frame is black. Colours are `{ r, g, b }` from 0 to
  255; `rgb`, `hsv`, `mix` and `lerp` build them.
- **Text shown to people** — `description`, each parameter's `label`, a
  `choice`'s option labels — is a string or a map of languages:
  `{ en: '…', fr: '…' }`.

## Settings

`params` declares what the person can adjust. Each is a `ParamSpec`: `number`
(`min`, `max`, `step`, `default`), `color`, `boolean`, `choice` (`options`) or
`text` (`maxLength`). `render` receives them in `params`, already typed. A
setting may follow a signal, the sound or the computer instead of its value:
the person binds it in Candeo, and the effect reads it in `params` without
knowing. Prefer a setting to a hard-coded constant the person might want to
change.

## What an effect reads

Besides time and its settings, an effect reads nothing unless it declares it in
`inputs`:

| Input | Gives | Note |
|---|---|---|
| `keys` | `presses`: the keys pressed in the last 10 seconds | Key presses are read only while such an effect runs, and Candeo says so. |
| `clock` | `clock`: the local date and time | For a clock face; animate on `time`. |
| `signals` | `signals`: every value other software sent | Prefer a setting bound to a signal. |
| `audio` | `audio`: level, peak, 16 bands, beats | The sound is captured only while such an effect runs. |
| `system` | `system`: processor, memory, graphics load and temperatures | `null` where the computer does not say. |

Declare only what the effect uses: the person sees what an effect reads before
applying it.

## Devices

- **`kinds`**: the lights the effect makes sense on, `['keys']` (the default),
  `['zones']` — a ring, a logo, a strip — or `'all'`.
- **`requires`**: what it needs, `['matrix']` when it reads `row` and `col`,
  `['geometry']` when it reads the rectangles. A device without it is not
  offered the effect.

`get_layout` gives a device's lights, or, without a device, the default layout
effects are written against. An effect must work on any layout it declares
itself fit for: measure from `layout.rows`, `layout.cols` or `bounds(layout)`,
never from fixed numbers.

## Limits

- Loading the module, top level included, must finish within 2 seconds.
- A frame must render within 10 ms. Thirty failed frames in a row, errors or
  overruns, stop the effect.
- The effect's JavaScript has 32 MB of memory.
- An effect declaring an `apiVersion` above 1 does not load in this version.

Keep state between frames only when the effect needs it: an effect computed
from `time` alone is easier to reason about, and renders the same at any frame
rate.

## The library

`list_effects` lists every effect with what it declares; `get_effect` gives one
with its source. The effects shipped with Candeo (`shipped:…`) are working
examples of everything above: reading two or three of them is the fastest way to
see the API in use.

## With this version

This version of Candeo's MCP server reads; it validates, previews and saves
nothing yet. Give the effect to the person: in Candeo, *Effects › New effect*
opens the editor, which checks the TypeScript, previews it on the keyboard drawn
on screen without touching the device, and saves it. Applying it to the device
is the person's own gesture, *Apply*.
