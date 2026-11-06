# Composition — several effects over the same lights

A device runs one effect, or one effect per region of a split device
([`regions.md`](regions.md)), and no two effects ever light the same key. That
is what these ask for: a spectrum on the number row over your own effect, a red
Escape key while a build is broken, the Counter-Strike 2 health bar over the
rest of your lighting. Each needs a second effect on lights the first one
already draws. This note
settles **composition**, the second increment of
[#360](https://github.com/o2csi/candeo/issues/360): what other products do, what
Candeo already has, the model for layers, ideas the products compared do not
have, and the first version.

Status: decided on 2026-11-06. The maintainer took every recommendation of the
study; §6 lists the decisions, and §5.1 the three pull requests that build the
first version, in their order. The first, the engine, is built (§7); layers in
the settings and the window, and overlay rules, are not.

## 1. Terms

The terms of [#360](https://github.com/o2csi/candeo/issues/360), kept apart. The
first four exist today.

| Term | What it is | Where it lives |
|---|---|---|
| **Effect** | a TypeScript file returning colours, which declares its parameters | the library ([`effects-library.md`](effects-library.md)) |
| **Settings** | the values and bindings an effect runs with on one device or region | `effectParams`, `splitDevices[].effectParams` |
| **Preset** | named settings for one effect, offered everywhere ([`presets.md`](presets.md)) | `presets` |
| **Region** | a part of a device that its definition declares, each light in exactly one region; split, each region runs its own effect | the definition, `splitDevices` ([`regions.md`](regions.md)) |
| **Target** | what a stack runs on: a whole device, or one region of a split device | — |
| **Layer** | an effect with its settings, drawn over what is under it through a mask, a blend mode and an opacity | §3.1 |
| **Mask** | the lights of the target a layer may touch | §3.3 |
| **Stack** | the base, the applied effect as today, and the layers above it, on one target | §3.2 |
| **Overlay** | a layer a rule adds while it is under way, instead of interrupting the device | §3.6 |
| **Saved stack** | a named stack, picked again later, as a preset is | after the first version: §4, idea 9 |
| **Saved setup** | effects assigned to several devices at once | [#367](https://github.com/o2csi/candeo/issues/367) |
| **Spatial scene** | devices placed on one canvas, one animation sampled across them | not planned (#360) |

*Composition* names the feature, not an object: a stack is what is stored. A
region is not a layer. Regions share a device's lights out without overlap;
layers overlap by design. §3.5 says how the two meet.

## 2. What exists

### 2.1 Elsewhere

From the [comparison](../comparison/rgb-controllers.md) (§4.2 to §4.6 and §6.1
there, with their sources; nothing was run):

| Product | Layers | Blend modes | What a layer covers | When it shows | Transitions, sharing |
|---|---|---|---|---|---|
| **Artemis** | layers and nested folders, opacity, shapes, transforms | yes | an explicit selection of LEDs; *adaption hints* remap it on other hardware | display conditions per layer or folder: always, once, on a condition, on an event; node scripts for conditions and data bindings | profiles fade in and out; exported as a zip |
| **Hypercolor** | an ordered stack per zone, a GPU compositor with a CPU fallback | eleven | the zone | scenes, activated by hand, the API or MCP | scenes cross-fade; backups exported and imported |
| **KeyGlow** | bottom to top, each with an opacity | normal, add, multiply, screen | key names, or portable groups (`@wasd`, `@arrows`, `@fn_row`) narrowed to the keys the keyboard has | always; reactive and music effects return transparency; shortcut lights drawn on top while a modifier is held | profiles are JSON, shared through a gallery |
| **OpenRGB** (Effects plugin) | a *Layers* effect stacking layer groups | ten | zones or segments; a *Mask* effect | always | *Patterns* save parameters |
| **SignalRGB** | no layer stack offered to the user | — | — | game integrations compose a background, events and a HUD inside the effect (paid) | presets shared as a link |
| **Dynamic Lighting** | `ZIndex` settles which effect wins a lamp | none documented | lamps one by one | — | — |

**What this says.** A layer with a selection, an opacity and a few blend modes
is what every product with layers has: offering it puts Candeo level with them,
it does not set it apart. They differ on three points:

- **how a selection survives another keyboard**: KeyGlow's named groups,
  Artemis's adaption hints;
- **what makes a layer appear**: Artemis's conditions are the richest, at the
  cost of a node editor; the others show layers always, or switch whole scenes;
- **how much of it is code**: Artemis's brushes are C# plugins, Hypercolor's
  effects HTML pages, KeyGlow's effects Python in its own source. The layers
  themselves are data everywhere.

Not relevant to Candeo, and left out of this note: transforms and shapes
(Artemis), because a Candeo effect already decides what it draws; a GPU
compositor (Hypercolor) for 132 lights; nested folders; ten blend modes made for
images; a keyframe timeline.

### 2.2 In Candeo today

- **One loop per device runs *parts*** ([`regions.md`](regions.md) §7, `runtime/mod.rs`):
  each part is one effect on one layout, with its own QuickJS runtime, clock,
  parameters, bindings and inputs. The loop renders them one after the other,
  then `compose` builds the device's frame: black, then each position copied from
  the part that owns it. Follows scale the frame, and it leaves once. A command
  replaces or stops one region's part between two frames, the others keeping
  their clock (`Request::Replace`, `Request::Stop`).
- **Regions** split a definition's lights into groups that do not overlap; the
  17 R5 and the m18 R1's zones declare them.
- **Presets** keep named settings for one effect; picking one changes settings,
  never what runs.
- **Rules interrupt a whole device** ([`inputs-and-automations.md`](inputs-and-automations.md) §3):
  one effect at a time, the highest active rule in the list wins, a rule hidden
  by a higher one goes by unseen. When the rule ends, the applied effect starts
  again **from time zero**: a starry sky redrawn, a wave jumping. *Resume*, a
  gesture and *Pause automations* end or suspend runs.
- **Any setting follows a value** — a signal, the sound, the computer — and so
  does a device's or a region's brightness, as a factor from a floor to 1
  applied to the frame (`runtime/dimming.rs`).
- **Inputs**: key presses of that keyboard, by scancode (`key-input.md`); the
  sound playing; the computer's load and temperatures; the clock; signals from
  other software and from Counter-Strike 2 and Dota 2
  ([`game-state-integration.md`](game-state-integration.md)).
- **The preview is a separate loop** that never writes to a device; the
  simulator draws engine frames, never its own.
- **An effect returns colours, never transparency.** Every frame starts from
  black (`bootstrap.js`), and black is a colour like any other.

What is missing is precisely two effects on the same lights. The engine already
runs several effects per device in one thread; composition changes how their
frames are combined, and who decides that they run.

## 3. A model for layers

### 3.1 A layer

| Field | What it is | Default |
|---|---|---|
| `effect` | a library effect's key, `shipped:Equalizer` | — |
| `values`, `bindings` | its settings, as `effectParams` keeps them; a preset copied into them when picked | the declared values |
| `mask` | the lights of the target it may touch (§3.3) | the whole target |
| `blend` | how its colour combines with what is under it (§3.4) | `screen` |
| `opacity` | 0 to 100 | 100 |
| `follows` | what its opacity follows: a signal, a sound or a system source with a floor, the shape of a brightness follow | none: the opacity set |

- **A layer's effect sees its mask as its device**, as a region's effect sees the
  region ([`regions.md`](regions.md) §1): its lights in `layout.keys`, hence its
  own `bounds` and `center`, positions and grid kept. An *Equalizer* on the
  number row spreads its sixteen bands along that row; a *Radial wave* on WASD
  starts from the cluster's centre; *Ripples* on WASD answers presses on those
  four keys only, since a press on a key the layout lacks is dropped
  (`key-input.md` §1). No effect needs a change, and `apiVersion` stays 1.
- **`kinds` and `requires` are checked against the mask's layout**, as against
  a region's: an effect requiring `matrix` stays unavailable on a zones device.
- **Settings are the layer's own.** Two layers may run the same effect with
  different colours, so a layer does not share the device's `effectParams` for
  that effect. A preset picked for a layer is copied, as it will be for a rule
  ([`presets.md`](presets.md) §7): renaming or deleting the preset leaves the
  layer as it is.
- **Screen by default**, so that adding a layer never hides what was there: a
  black background stays transparent (§3.4). *Cover* at 100 % on the whole
  target would make the base invisible, which is what *Apply* already does.

### 3.2 Order

- **Bottom to top**, as KeyGlow and image editors have it: the base first, then
  each layer over the result. The interface lists the top first.
- **The base is the applied effect**, the one *Apply* sets today, on the whole
  target, composed over black. It has no mask, blend or opacity. Without a base,
  layers are drawn over black.
- **A firmware effect cannot carry layers.** The firmware lights the device by
  itself and Candeo sends no frame to compose with. Applying one sets the
  layers aside, kept and not run, until a library effect is applied again; a
  split device offers no firmware effect anyway.
- **Composed in one pass, in a fixed order**, colours kept as numbers until the
  last layer and rounded once: the same parts' frames always give the same
  bytes, which is what lets a test compare the preview's frame with the
  device's (§3.9).

### 3.3 Masks

A mask is a union of the following, resolved against the target's layout when
the stack starts and whenever the definition in force changes.

| Kind | Written | Carries to another keyboard | Notes |
|---|---|---|---|
| **The whole target** | absent | yes | the default |
| **A key group** | `{ "group": "wasd" }` | yes, narrowed to the keys it has | by scancode, defined in Rust, named in the catalogs |
| **Keys** | `{ "keys": ["01", "e01d"] }` | yes, the keys it has | drawn in the preview (§3.10), kept by scancode as definitions write them |
| **Lights** | `{ "lights": [3, 7] }` | no: same definition only | for zones and for keys without a scancode (Fn), by address as regions name them |
| **A region** | `{ "region": "lid" }` | same definition only | on a whole device whose definition declares regions (§3.5) |

**Groups by scancode, not by legend.** A scancode names the physical key
whatever its legend: the keys under a gamer's left hand are `11 1e 1f 20`,
engraved ZQSD on AZERTY and WASD on QWERTY (`Key.scancode` in
[`effects-api`](../../packages/effects-api/src/index.ts)). A group defined that
way means the same cluster on every keyboard and every national layout.
KeyGlow's selector names keys, its groups included (comparison §4.3); this is
where Candeo's would differ.

| Group | Scancodes | DeathStalker V2 Pro | m18 R1 keyboard |
|---|---|---|---|
| `escape` | `01` | 1 of 1 | 1 of 1 |
| `wasd` | `11 1e 1f 20` | 4 of 4 | 4 of 4 |
| `arrows` | `e048 e04b e050 e04d` | 4 of 4 | 4 of 4 |
| `functionRow` | `3b`…`44`, `57`, `58` | 12 of 12 | 12 of 12 |
| `numberRow` | `29`, `02`…`0d` | 13 of 13 | 13 of 13 |
| `numpad` | `45 e035 37 4a 47 48 49 4e 4b 4c 4d 4f 50 51 e01c 52 53` | 17 of 17 | 17 of 17 |
| `modifiers` | Shift, Ctrl, Alt and Windows, both sides | 7 of 8 | 7 of 8 |
| `navigation` | Insert, Home, Page Up, Delete, End, Page Down | 6 of 6 | 2 of 6 |
| `space` | `39` | 1 of 1 | 1 of 1 |

Counted from the two built-in keyboard definitions on 2026-11-06: neither has a
right Windows key, and the m18 R1 keeps only Delete and End of the navigation
block. That is the case narrowing is for.

- **A mask that resolves to no light on the target is refused** when the stack
  is saved, in one sentence naming the group or the keys. A stack that meets
  another definition later (a definition chosen, a saved stack picked there)
  keeps the layer and marks it *nothing to light here* rather than dropping it.
- **Groups are unavailable on a zones device**: its lights carry no scancode. The
  picker shows only the groups with at least one key on the layout, each with
  its count.
- **Later**: masks *except* a group; masks computed every frame — the keys
  pressed in the last seconds, the keys a signal names. OpenRGB's *Mask* effect
  is the nearest. Not needed for anything in §5.

### 3.4 Blend modes and transparency

An effect cannot say *transparent* today, and the first version does not change
that. Three blend modes make a colour transparent by themselves, which covers
every example in this note:

| Mode | Result, per channel, 0 to 255 | Transparent where the layer is | Good for |
|---|---|---|---|
| **Cover** (*normal*) | the layer's colour | nowhere: the mask decides | a bar, a status key, a flash |
| **Add** | the sum, clipped at 255 | black | light added: sparks, ripples, beats |
| **Screen** | 255 − (255 − a)(255 − b) / 255 | black | the same, softer, never clipping |
| **Multiply** | a × b / 255 | white | tinting or darkening what is under: a warm evening layer, dimming all but WASD |

Then `opacity` and `follows` mix the result with what was under it:
`under + (blended − under) × opacity × factor`, inside the mask; outside, the
layer changes nothing.

- **Four, KeyGlow's set.** *Lighten* and *darken* cost one line each and can
  follow if asked. Overlay, dodge, burn, difference and their kind (OpenRGB has
  ten, Hypercolor eleven) are image-editing modes: on a keyboard their effect is
  hard to predict and to name.
- **Computed on the bytes sent**, as the brightness follow already scales them.
  LED brightness is not linear in those bytes, so a 50 % opacity may not look
  half-way on the device. Blending in linear light would need each device's
  curve, which nobody has measured; it waits for that measurement, made on the
  hardware.
- **An effect's own transparency, later.** KeyGlow's reactive effects return
  transparency. In Candeo it would be an optional third argument,
  `frame.set(key, colour, alpha)`, and the bootstrap would hand Rust four bytes
  per light instead of three. An older Candeo would ignore the argument and draw
  the layer opaque without a word, so an effect using it declares `apiVersion` 2
  and an older Candeo refuses it instead. It waits for an effect that cannot be
  written with a black background under *screen*.

### 3.5 Regions and layers

- **A region stays a partition, and a mask is a selection.** Regions decide what
  runs where without overlap, with the follow, the tray and the settings they
  already have. Turning them into layers with region masks would be the same
  pixels, and would rewrite a shipped feature and its settings for nothing.
- **A split device has one stack per region.** The target of a stack is a whole
  device or one region: on the 17 R5, the Lid region runs Breathing with a
  Temperature layer on its logo, the Keyboard region its own stack.
- **A region is also a mask** on a device left whole: the 17 R5 whole, with a
  layer on its Lid region only. It costs nothing, since the region's lights are
  known. Both ways exist: a stack per region on a split device, a region mask on
  a whole one.
- **The follow scales the lights after the stack**: a region's brightness
  follow still scales its lights whatever drew them
  ([`regions.md`](regions.md) §4), layers and overlays included.

### 3.6 Rules: interrupt, or add a layer

Today a rule replaces the whole device. An **overlay rule** adds a layer on top
of what runs, for as long as it is under way, and takes it away after:

> **While** `build` **is** `failed` · **on** *DeathStalker V2 Pro* · **over**
> *Escape* · **show** *Breathing*, red

- **The base keeps running, and keeps its clock.** Nothing restarts when the
  rule ends: what §3.4 of `inputs-and-automations.md` accepts for an
  interruption — the applied effect starting again from zero — does not happen.
- **Priority stays the order of the list.** The highest interrupting rule under
  way replaces the device's stack, as today. Overlay rules under way are drawn
  over whatever runs, in the list's order, the highest on top. An overlay listed
  below the interrupting rule under way goes by unseen, as a lower rule does
  today; one listed above it draws over it, so a doorbell can flash over the
  hourly clock.
- **Over a firmware effect nothing is drawn** (§3.2): an overlay under way waits
  for frames from Candeo, as a rule hidden by a higher one does. A night shown as
  the firmware's *Off* stays dark; a night shown as a dim library effect can take
  a doorbell's flash.
- **What ends one.** Its trigger, as for any rule; *Resume*, which ends every run
  under way on the device, overlays included; *Pause automations*. Applying
  another effect by hand changes the base and **leaves overlays**: they hide
  nothing the gesture chose.
- **Where it shows.** The device's card and the tray name it as they name an
  interruption: *Build failed — over Escape*, with *Resume*.
- **On a split device**, overlays are drawn over the composed device frame; their
  mask may name a region or keys anywhere. Most of what rules aimed at a region
  ([`regions.md`](regions.md) §10) were for comes this way, as overlays.
- **Games.** The *while playing* rule of a game's card can be an overlay: the
  Counter-Strike 2 effect on the function and number rows, over the person's
  own effect. The card offers both: *the whole keyboard*, as today, or *the two
  top rows, over my effect*.

**In the file, an overlay rule carries `layer` instead of `show`:**

```json
{
  "id": "…", "name": "Build failed", "enabled": true,
  "devices": [{ "vid": 5426, "pid": 658 }],
  "when": { "kind": "signal", "name": "build", "equals": "failed" },
  "layer": {
    "effect": "shipped:Breathing",
    "params": { "color": { "r": 255, "g": 0, "b": 0 } },
    "mask": { "group": "escape" },
    "blend": "cover",
    "opacity": 100
  }
}
```

A `show` next to an unknown `over` field would read, in a build released before
overlays, as an interruption of the whole keyboard: Escape's warning would turn
every key red. Without `show`, that build finds the rule unreadable and leaves it
as written, doing nothing (`resolver::parse`). Rules are read one by one, so the
others keep working there.

### 3.7 What a frame costs

**Measured on 2026-11-06** on the Alienware m18 R1 (Core i9-13980HX), release
profile, with the engine's load and render path as the headless transpilation
proof of concept copies it (`tools/transpile-poc/src/engine.rs`, the
application's own `api.js` and `bootstrap.js`). Each of the 26 shipped effects,
transpiled with Oxc, on each built-in layout: loaded five times, 30 frames to
warm up, then 900 frames in a row with every input fed (`scenario::active`,
repeated ten times). Two runs, with Candeo and other programs running on the
same laptop, so the figures lean high; the ranges are theirs. *Sweep*, at 0.19
to 0.23 ms, matches what [`effects-runtime.md`](effects-runtime.md) §8 measured
for an ordinary effect. The harness, about 150 lines over the proof of
concept's `engine` and `scenario` modules, was not committed: it timed `load`,
`render` and a blend loop, nothing else.

| Layout | Lights | Mean frame, over the 26 | Median effect | Heaviest | Worst 99th percentile | The 26 in a row |
|---|---|---|---|---|---|---|
| DeathStalker V2 Pro | 132 | 0.41–0.43 ms | 0.35–0.38 ms | Noise map, 1.23–1.35 ms; Aurora 1.2 ms | 1.9–2.2 ms | 10.6–11.2 ms |
| m18 R1 keyboard | 140 | 0.41 ms | 0.34–0.35 ms | Noise map, 1.19–1.56 ms | 2.0–2.1 ms | 10.7 ms |
| 17 R5 zones | 13 | 0.07 ms | 0.06 ms | Aurora, Noise map, 0.16–0.19 ms | 0.33–0.38 ms | 1.7–1.8 ms |
| m18 R1 zones | 3 | 0.03 ms | 0.02–0.03 ms | Aurora, 0.06 ms | 0.17–0.18 ms | 0.7–0.8 ms |

| Other costs | Measured |
|---|---|
| Loading one effect: runtime, context, module | 0.9 to 1.9 ms, median of five per effect and layout |
| Blending in Rust, over 140 lights, a mask and an opacity per layer | 2–2.5 µs for 2 layers, 5–11 µs for 8, 9–18 µs for 16 |
| Memory per part, from `effects-runtime.md` §8 | 0.17 MB for a stateless effect; 32 MB ceiling each |

**What it gives.** On the DeathStalker the HID write takes 13.1 ms on average and
14.4 ms at worst of the 33.3 ms period, which leaves about 19 ms before the loop
slips (`runtime/mod.rs`, `FRAME_BUDGET`). Rendering is the sum of the parts;
blending is three orders of magnitude below it.

- **Eight effects per target, base included**: eight of the heaviest at their
  99th percentile take about 17.6 ms, within the 19; eight median ones, about
  3 ms. A ninth is refused, in one sentence.
- **Overlays come on top**: at most four drawn at once per device, the highest
  in the list; others go by unseen. Twelve of the heaviest effects, at their
  peaks, take about 26 ms: the device would drop below 30 frames a second for
  those frames without stopping, as [`regions.md`](regions.md) §7 already
  accepts for four regions at the edge of their budget.
- **Zones devices are cheap**: a stack of eight on the 17 R5 renders in about
  1.5 ms, and its definition's pace sends a frame every 100 ms at most anyway.
- **An overlay starting holds the device's frame for its load**, 1 to 2 ms here,
  two seconds at worst under `LOAD_BUDGET`: well within one frame.
- **The preview doubles it while it runs**: it runs its own copy of the stack
  being edited, on its own thread.
- **One runtime per part.** Each is capped at 32 MB, so a device's ceiling
  grows with its parts, within the limits above. One runtime per device, a
  context per part, would cap the device as a whole, but one part running out of
  memory would then fail its neighbours.

### 3.8 The engine

- **A part gains a mask, a blend, an opacity and a follow.** `owns` becomes the
  mask's positions; the base's mask is its target's. `compose` becomes a blend
  over each target's parts in order, then the overlays, then the follows, then
  one `emit`: one writer, one frame per device, as `regions.md` §7 requires.
- **Adding, replacing or removing a layer** is a request read between two frames,
  as replacing a region's part is today: the other parts keep their clock. A
  layer that fails to load leaves the stack as it was and the command returns
  the error.
- **A layer failing thirty frames in a row stops alone**: it draws nothing more,
  its row says why, the rest goes on, as a region's part does.
- **Rules**: the resolver answers, per device, the interruption to run *and* the
  overlays to draw, both from the same pure function and the same tests. The
  scheduler adds and removes overlay parts with the same requests.
- **No second renderer**: no compositing in Vue, no shader, no HTML (#360).

### 3.9 Preview and applied output

- **The same compose function** builds the preview's frame and the device's.
  The preview loop runs the stack being edited, with no output, and borrows the
  target's follow, as a region's preview does.
- **What the simulator shows is the final frame**: the stack composed and
  scaled. The selected layer's mask is drawn over it, as a region's bounds are
  ([`regions.md`](regions.md) §3); *solo* draws that layer alone, in the preview
  only.
- **When the preview runs exactly what the device runs**, the simulator shows
  the device's frames, as it does today for a single effect.
- **Identical frames, tested**: the same stack, given the same instants and
  inputs, renders the same bytes in a preview loop and in a device loop. The
  clocks differ in the application — each loop starts its parts' time at load —
  so the test injects them, as the engine tests already inject presses.

### 3.10 The interface, in words

> [`layers-ui.md`](layers-ui.md) designs this interface in full and moves the
> stack into the devices column, under its target, as regions are; it keeps
> the words below. This section stays as the study wrote it.

**In *Effects***, the settings column of a device or a region gains a
**Layers** block, above Presets:

- **The stack, top first**: each layer's swatch, effect, mask (*Escape*, *WASD*,
  *5 keys*), blend and opacity; the base at the bottom, named as today. One row is
  selected; the base is, unless someone picks another.
- **The selected row is what the gallery targets**, as the selected region is
  for a split device: selecting an effect previews it in that row, the rest of
  the stack around it; *Apply* puts it there. A last row, ***Add a layer***,
  targets a new layer on top. *Apply* stays the one gesture that changes what
  runs, and selecting stays a preview.
- **The selected layer's settings** fill the column below, with its Presets
  block: the effect's parameters, then its mask, blend, opacity and what the
  opacity follows, with the *Value* / *Signal* / *Sound* / *System* switch of a
  brightness. Like any setting, they are live on the device when the layer runs
  there and saved at once (#218).
- **The mask**: a picker with *The whole device* (or region), the groups this
  layout has with their counts, the regions, and *Select keys*. *Select keys*
  turns the preview into a selection: a click toggles a key, a drag paints, Enter
  keeps, Escape cancels. The keys selected are drawn in full, the others faded.
- **Reordering** by dragging the rows, as rules are; **removing** from the row,
  without a question: adding it back is one gesture.
- **The tray** picks the base, as today, and leaves the layers.

**In *Automations***, a rule's sentence gains a chip after the device: *over the
whole device* (an interruption, as today) or *over* a group or keys (an
overlay), with the blend and opacity under it.

**Words, settled before the strings** (AGENTS.md):

| English | French |
|---|---|
| *layer* | *calque* |
| *mask* | *masque* |
| *Add a layer* | *Ajouter un calque* |
| *Cover* | *Couvrir* |
| *Add* | *Additionner* |
| *Screen* | *Éclaircir* |
| *Multiply* | *Teinter* |
| *over*, the rule's chip | *par-dessus* |

- **The French modes say what they do**, not the image editors' names; they are
  infinitives, as the English names are verbs, and each is a choice of the
  layer, said in its row and in its settings.
- ***Additionner*, not *Ajouter***, which the study proposed. *Ajouter* is the
  interface's verb for adding something — *Ajouter les exemples* in
  *Automations*, *Ajouter un calque* in the same block — and a mode named like
  the button above it would read as an action.

### 3.11 `settings.json`

```json
"layers": [
  {
    "vid": 5426, "pid": 658,
    "stack": [
      {
        "id": "a3",
        "effect": "shipped:Equalizer",
        "values": { "background": { "r": 0, "g": 0, "b": 0 } },
        "mask": { "group": "numberRow" },
        "follows": { "source": "sound:volume", "floor": 0 }
      },
      {
        "id": "b7",
        "effect": "shipped:Status row",
        "mask": { "group": "functionRow" },
        "blend": "cover",
        "opacity": 80
      }
    ]
  }
]
```

- **A key of its own, the base left where it is.** The base stays in
  `activeEffects` (or the region's record), its settings in `effectParams`:
  an older build sees the base alone and runs it, which is the right way to fall
  short. A stack nested in `activeEffects` would be dropped or misread there.
- **Per model, as `activeEffects`**, keyed by `vid` and `pid` without serial;
  `region` added for a region's stack. A region's stacks go when its device is
  made whole, with the rest of the region's records ([`regions.md`](regions.md)
  §2).
- **`stack`** bottom to top, the base excluded. `id` is short, unique within the
  stack, and what commands and MCP name a layer by. `values` keeps what departs
  from the declared values, as `effectParams` does; a saved stack keeps every
  value, as a preset does (§4, idea 9).
- **Same economy as the rest of the file**: no entry without a layer, no field at
  its default — the first layer above is drawn with *screen* at 100 %.
- **Checked when read and when saved**, each refusal one sentence: an effect the
  library does not know (the layer kept, marked missing, as a rule naming a
  deleted effect is); a mask that resolves to nothing; a group, region or blend
  nobody knows; an opacity outside 0–100; more than eight effects; an effect
  whose `kinds` or `requires` the mask's layout does not meet. Each has its
  negative test, without hardware.
- **Deleting or renaming an effect** reaches the layers, as it reaches presets
  and regions (`forget_effect`, `rename_effects`). Resetting the configuration
  drops the key.
- **No version bump**: the key is optional and needs no migration.

### 3.12 What an agent could do with it

The MCP proposal ([`mcp-architecture.md`](mcp-architecture.md)) lets an agent
write an effect in TypeScript. A stack lets it do something simpler and safer:
**assemble effects that already exist, as data**, which #360 asks for.

- **Read**: the target's layout, with the groups it has and their keys; the
  stack running; the effects and presets that apply there.
- **Preview a stack**, with simulated inputs — `build` set to `failed`, the
  sound at 0.8 — and get its frames back: nothing reaches a device, nothing
  changes a rule.
- **Propose**: *apply this stack*, *add this overlay rule*, each confirmed by the
  person before any hardware write or rule change, as the proposal requires.
- **The same validation** as the window's commands, in Rust: an agent cannot
  write a stack the window would refuse.

> *Show my CI on Escape, the CPU's temperature along the function row, over
> Aurora.* — a base, one layer with *Temperature* on `functionRow`, one overlay
> rule on `build`; previewed with `build` failed, then proposed.

The comparison's list of Hypercolor's MCP tools has scenes to activate, not
stacks to assemble ([§6.4](../comparison/rgb-controllers.md#64-agent-and-mcp-interfaces));
SignalRGB announces an MCP endpoint without documenting its tools, and the
others have none.

## 4. Ideas the products compared do not have, or not this way

*Not found* below means not among the features the comparison records, with
its sources; it is not proof of absence. Cost: **S**, days, mostly what §3
builds anyway; **M**, a pull request of its own with a design paragraph;
**L**, a feature with its own design note.

| # | Idea | Elsewhere | Why it is worth it | What it takes | Cost |
|---|---|---|---|---|---|
| 1 | **Overlay rules** instead of interruptions (§3.6) | Artemis's display conditions per layer are the nearest, through its data model and node scripts; SignalRGB's game HUDs are inside paid effects | the base keeps running and its clock; a CI, a doorbell, a game, Home Assistant light a few keys over the person's own effect; one sentence in the rule list, its priority visible | `layer` in rules, overlays from the resolver, parts added and removed between frames | **M** |
| 2 | **Press to acknowledge**: an overlay on a key stays until that key is pressed | not found as a feature; Artemis's Input plugin and event conditions might be assembled into it | a notification that waits for you — the washing machine done, a message, a failed deploy — and goes when you press the key it lit | key capture leased while such an overlay shows, only its scancodes compared; a press dismisses the run, as *Resume* does. Capture stays off otherwise (`key-input.md` §3) | **M** |
| 3 | **Transitions**: a cross-fade when the base changes, a fade when an overlay comes and goes | Hypercolor's scenes cross-fade; Artemis's profiles fade | no jump when a rule ends or an effect is applied | an overlay's fade is an opacity ramp (**S**); a base cross-fade keeps the old part alive for 0.3–1 s, two parts at once (**M**). Zones devices paced at 100 ms get three to ten steps | **S**/**M** |
| 4 | **Groups by scancode** across keyboards and national layouts (§3.3) | KeyGlow's portable groups, over key names | WASD is the same cluster on AZERTY; a stack moves between the DeathStalker and the m18 unchanged | a table in Rust, names in the catalogs. A variant by legend — *the vowels*, *the letters of a word* — needs the system layout (`key-input.md` §6): **M** | **S** |
| 5 | **A gauge anywhere**: a shipped *Gauge* effect drawing a bound number as a bar across its mask Candeo's Counter-Strike 2 effect and SignalRGB's game integrations draw bars inside their own effect | any signal, sound or system value becomes a bar on any row — the CPU on the function row, a Home Assistant sensor on the numpad — over the person's effect | one shipped effect; masks do the rest. *Status row* becomes a layer on the function row the same way, unchanged | **S** |
| 6 | **Opacity that follows** the sound, the computer or a signal, per layer | Artemis's data bindings, through node scripts | a spectrum that appears only while music plays; a heat layer that rises with the load | the brightness follow, per layer | **S** |
| 7 | **Time-based layers**: an overlay on a schedule, a warm *multiply* layer through the evening | Artemis's time conditions and keyframes | lighting that changes with the day without a rule taking the keyboard | a cron overlay already is one (**S**); a ramp over the hours — 20:00 to 22:00 — is a new follow source, the time of day (**M**). Sunset needs a location (`inputs-and-automations.md` §2.1) | **S**/**M** |
| 8 | **A preview with simulated inputs**: the stack as it will look when `build` is `failed`, when the CPU is hot, when music plays | not found | an overlay can be judged before its sender exists; the agent of §3.12 shows its proposal the same way | the preview loop takes overridden values for its parts; a few controls under the preview | **M** |
| 9 | **Saved stacks**, picked like presets, exported as a file | KeyGlow's profiles, Artemis's zips, Hypercolor's backups | a look made once, carried to another device, shared | a `stacks` list as `presets` is, every value kept; export and import as a file with its own version, never code, effects named by key and missing ones reported | **S** save, **M** files |
| 10 | **Composed by an agent**, as data (§3.12) | Hypercolor's MCP activates scenes; SignalRGB's is undocumented | *show my CI on Escape over Aurora* without a line of TypeScript, previewed before anything lights | the MCP server of #359; this schema and its validation | **S** on top of #359 |
| 11 | **The shortcuts of the application in front**: holding Ctrl lights C, V, X, Z, S in Word, other keys in Blender | KeyGlow lights shortcuts while a modifier is held; Artemis can condition a layer on the focused window | learning an application's keys; striking on a keyboard that can light each key | an `app` trigger, planned and not built; the modifiers held, which capture tracks but does not report (`key-input.md` §6); shortcut maps as data, per application — bundled for a few, written by the person, or by an agent. The foreground process name is read locally and never logged | **L** |

**Not proposed**: a heat map of the keys typed most, which would keep presses
over time where `key-input.md` §3 keeps nothing; layers drawn from the screen,
which waits for screen capture (`inputs-and-automations.md` §2.5); a canvas
shared across devices, which #360 keeps out.

**Cheapest for the effect**: ideas 4, 5 and 6 come almost free with layers.
**The most striking**: 1, 2 and 10. Of the three, only 2 is no product's
feature; 1 and 10 have neighbours, and what Candeo would add is the rule list
and the data an agent assembles. 11 is striking too, and the most expensive.

## 5. The plan

### 5.1 The first version

**Layers on one target, and overlay rules.** In order, each a pull request:

1. **The engine and a two-layer proof of concept**, as #360 asks: parts with a
   mask, a blend and an opacity; the blend function, pure and tested; a test
   fixture rendering two layers on a per-key keyboard and on a zones device, with
   deterministic order and the frame budget of §3.7 checked; the same frame from a
   preview loop and a device loop with injected clocks. No interface yet.
2. **Layers in settings and in *Effects***: `layers` in `settings.json` with its
   checks and negative tests; the Layers block; masks by group, by keys drawn in
   the preview, by region and by lights; the four blend modes; opacity and what it
   follows. Whole devices and regions alike.
3. **Overlay rules**: `layer` in rules, the resolver's overlays, the *over* chip
   in *Automations*, the device's card and the tray; *Resume* and *Pause* as in
   §3.6. The Counter-Strike 2 card offering *the two top rows, over my effect*.

Shipped with it, at almost no cost: a *Gauge* effect (idea 5). Each step users
can see adds its Store *What's new* line, and layers go into the Store listings,
the README and the site before the release that ships them (AGENTS.md).

**Left out of the first version**: effects returning transparency; transitions;
press to acknowledge; saved stacks and files; simulated inputs; MCP tools, which
wait for #359; the tray's editing of layers; masks computed per frame.

### 5.2 After it, in the order the value comes

1. **Press to acknowledge** (idea 2), on top of overlay rules.
2. **Fades for overlays** (idea 3, the **S** half), then the base's cross-fade.
3. **Saved stacks** (idea 9), then their files.
4. **Simulated inputs in the preview** (idea 8), which the MCP tools reuse.
5. **MCP composition tools** (idea 10), with #359.
6. **Effects returning transparency**, if an effect needs it.
7. **Application shortcuts** (idea 11), after the `app` trigger.

### 5.3 How it answers #360

| Acceptance in #360 | Here |
|---|---|
| A decision record separating effect, preset, layer, composition, region and setup | §1, §3.5 |
| A preset that adds no runtime and changes no device unexpectedly | shipped: [`presets.md`](presets.md) |
| A two-layer proof of concept with deterministic order and realistic budgets | §5.1 step 1, §3.2, §3.7 |
| The same composition, the same frame in the preview and on the device | §3.9 |
| A schema, with negative tests for capabilities, unknown ids and invalid masks | §3.11 |
| The pointer-driven interface and MCP on one schema | §3.10, §3.12 |

## 6. Decisions

Taken by the maintainer on 2026-11-06, each as the study recommended it; only
the French name of *Add* moved when the words were settled (§3.10).

1. **The model**: a stack per target, a whole device or one region of a split
   device; the base is today's applied effect, where it is kept today, and the
   layers live under a key of their own, `layers` (§3.8, §3.11).
2. **A layer's effect sees its mask as its device**, as a region's effect sees
   the region (§3.1). No effect changes, and `apiVersion` stays 1.
3. **Overlay rules are in the first version**, its third step (§5.1).
4. **Applying an effect by hand leaves overlays**: it changes the base, and the
   overlays under way go on drawing over it (§3.6).
5. **A new layer blends with *screen***, so that adding one never hides what is
   under it (§3.1).
6. **Four blend modes**, *cover*, *add*, *screen* and *multiply*, computed on the
   bytes sent; blending in linear light waits for a measurement of the devices'
   response (§3.4).
7. **The limits**: eight effects per target, the base included; four overlays
   drawn at once per device, the highest in the list; one runtime per part
   (§3.7).
8. **Both stacks per region and region masks**: a split device has a stack per
   region, and a region is a mask on a device left whole (§3.5).
9. **The tray changes the base and keeps the layers** (§3.10).
10. **The words** of §3.10: *layer* / *calque*, *mask* / *masque*, *over* /
    *par-dessus*, and the four modes named by what they do: *Cover* /
    *Couvrir*, *Add* / *Additionner*, *Screen* / *Éclaircir*, *Multiply* /
    *Teinter*.
11. **Press to acknowledge comes in a second version**, first in the order of
    §5.2, which stands as written.

## 7. The engine, as step 1 built it

What §5.1 step 1 settled in `apps/desktop/src-tauri/src/runtime/`, for step 2
to call. No command, setting or screen reaches it yet, and every call made
before layers behaves as before, byte for byte.

**Types**, in `runtime::composition`, pure and tested without a loop:

| Type | What it is |
|---|---|
| `Blend` | `cover`, `add`, `screen` (the default) or `multiply`; `apply` is §3.4's formula on one channel |
| `Look` | `blend`, `opacity` from 0 to 100, and `follows`, a brightness follow's `Dimming` |
| `Mask` | `group`, `keys` (scancodes as definitions write them), `lights` (addresses) and `region`: their union, the whole target when it names nothing. `resolve(device, target)` gives its positions, or a `MaskError`: a group or a region nobody knows, a text that is no scancode, nothing to light |
| `LayerStart` | an `id`, the `EffectStart`, its `Mask` and its `Look` |
| `StackStart` | `base`, an `Option<EffectStart>`, and `layers`, bottom to top. `check` refuses a ninth effect and an id given twice |
| `Refusal` | `TooManyEffects`, `LayerTwice`, `Order`, or `Mask` |
| `MAX_EFFECTS` | 8, the base included |

`Blend`, `Look` and `Mask` read and write as §3.11 writes them, unknown fields
refused, so that step 2's `layers` records are made of them. The key groups are
`candeo_device::groups::KEY_GROUPS`, by scancode, checked against both built-in
keyboards; their names for people come into the catalogs with step 2. A mask's
layout, the device narrowed to its lights, is built once per set of lights
(`Layout::masked`).

**The `Engine`**. A target is named by its region, `None` for the whole device:

| Method | What it does |
|---|---|
| `start_stack(device, layout, out, stack, split)` | a whole device's stack, replacing what ran; `start` is it with a base alone |
| `start_regions(device, layout, out, stacks)` | a stack per region, in the definition's order; a region given none runs nothing |
| `start_base(device, region, start)` | the base, between two frames, its layers keeping their clocks: *Apply* and the tray on a target with layers |
| `stop_base(device, region)` | the layers go on, over black |
| `start_layer(device, region, layer)` | on top, or in place of the layer with its id: another effect, or another mask |
| `stop_layer(device, region, id)`, `order_layers(device, region, ids)` | `ids` names each layer of the stack once |
| `set_look(device, region, id, look)` | blend, opacity and follow, live |
| `set_params`, `set_bindings(device, region, layer, …)` | a base's with `layer` `None`, a layer's otherwise |
| `start_preview_stack(layout_of, region, stack, layout)` | the preview of a stack; `start_preview` is it with a base alone. `set_preview_params`, `set_preview_bindings` and `set_preview_look` name a layer the same way |

- **Refused before anything stops**: a stack or a set of region stacks that
  `check` refuses comes back as a `Refusal`, and what ran goes on.
- **A change between two frames** answers `Ok`, or a `PartError`: `NotRunning`
  when the loop does not run that target's stack — nothing runs, a split device
  asked for its whole stack or a whole one for a region — and the caller starts
  the whole composition; `Refused`, with a mask lighting nothing or a ninth
  effect among the reasons; `Failed`, with the effect's load error. In every
  case the stack goes on as it was.
- **A start's verdict**, `StackVerdict`, holds the base's and each layer's. A
  target whose base does not load runs nothing, its layers included, as a
  device whose effect did not load ran nothing before layers. A layer that does
  not load runs nothing and keeps its row, with why; one whose mask lights
  nothing on this device runs nothing and has no row: step 2 says *nothing to
  light here* from the verdict, or from `Mask::resolve`.
- **The status**: `EngineStatus`, `RegionEngineStatus` and `PreviewStatus` gain
  `layers`, each a `LayerEngineStatus` — `layer`, `running`, `effectId`,
  `error` — bottom to top, left out when empty. A layer stopped after thirty
  failed frames keeps its row with its error. The copied diagnostic lists them.
- **The engine knows no rules.** A whole device's stack may be a rule's: before
  `start_base` or `start_layer`, the command ends an interruption and starts
  the applied stack whole, as `start_effect` ends one today.
- **Identical frames** are tested with a clock that gives every part's frame
  `n` the instant `n / 30` s, in a preview loop and a device loop.
- **Not in step 1**: overlays and their limit of four per device, which come
  with overlay rules (step 3) as a list drawn over the targets' stacks; the
  TypeScript mirrors of the new status fields, with step 2.
