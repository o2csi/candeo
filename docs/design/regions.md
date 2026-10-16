# Regions — one effect per part of a device

A device is one target today: one effect for all its lights. On the Alienware
17 R5 every light sits on one controller, the keyboard's four zones included, so
the keyboard cannot run one effect while the lid runs another; the m18 R1's ring
and logo are one target too. This note settles how a definition splits a device
into **regions**, each running its own effect, and what stays the device's
(#329).

Status: implemented. The mockup of *Devices* and *Effects* was approved on
2026-10-16 and is not published; the decisions below were taken on it. The code
came in two pull requests, built side by side from the contract in §11; §12 says
what the window decided beyond it.

## 1. What a region is

A definition may declare `regions`: named groups of its lights, **each light in
exactly one**.

```json
"regions": [
  { "id": "keyboard", "name": { "en": "Keyboard", "fr": "Clavier" }, "lights": [4, 5, 6, 7, 12] },
  { "id": "lid", "name": { "en": "Lid", "fr": "Capot" }, "lights": [0, 1, 10, 11] },
  { "id": "base", "name": { "en": "Base", "fr": "Base" }, "lights": [8, 9, 2] },
  { "id": "power", "name": { "en": "Power button", "fr": "Bouton d'alimentation" }, "lights": [3] }
]
```

| Definition | Regions, by address |
|---|---|
| `alienware-17-r5-zones.json` | *Keyboard*: the four keyboard zones and the media keys (4–7, 12) · *Lid*: the alien head, the logo, both edges (0, 1, 10, 11) · *Base*: both sides and the touchpad (8, 9, 2) · *Power button* (3) |
| `alienware-m18-r1-zones.json` | *Ring*: both halves (0, 1) · *Logo* (2) |

The m18's ring halves make one region: one half lit alone spreads through the
whole ring, and the halves show only when both are lit
([`alienware-m18-r1.md`](../protocol/alienware-m18-r1.md) §2). Two regions there
would offer a split the hardware does not keep.

- **Lights are named by their address.** It is unique and mandatory in a
  definition (load refuses an address given twice), it is what the survey found
  and what the notes cite (*2 is taken for the touchpad*), and it does not move
  when the items are reordered. A position moves with the order; a light's
  `name` is optional free text for whoever reads the file.
- **Every light in exactly one region.** A light in none would stay dark once
  the device is split, with nothing on screen to say why; a light in two would
  have two effects fighting over it. Load refuses both, naming the address.
- **At least two regions.** One region is the device itself, and *Split* would
  split nothing.
- **A region's name is text for people, as a firmware effect's is**: a string,
  the same in every language, or a map of languages with English shown where
  the interface's language is missing (`$defs/text` in the schema). The device's
  own `name` is a plain string because it is a product name, never translated;
  a light's `name` is a plain string because Candeo never shows it. A region's
  name is shown — on the card in *Devices*, in *Effects*, in the tray — and
  only the definition's author knows what the parts of their device are called,
  so the catalogs cannot name it, as they cannot name a firmware's effects
  ([`device-sdk.md`](device-sdk.md) §7).
- **A region's id is what everything else names it by**: settings, commands,
  the tray. Changing it orphans what was kept for the region, as changing a
  firmware effect's id does. Lower case letters, digits and hyphens: it travels
  in the tray's menu ids, whose separator is `:`.

**The term.** *Region* (*région*). *Zone* already names a light of a zones
device — the 17 R5 has thirteen —, and a region of zones called a zone would
make every sentence about it ambiguous.

### What an effect sees

A region's effect sees the region as a device of its own: its lights in
`layout.keys`, hence its own `bounds` and `center`, which the API computes from
them. Nothing else moves:

- **Positions and coordinates stay the device's.** `key.index` is the light's
  place in the device's frame, and its rectangle is where it sits on the device:
  a region's frame and the device's share their positions, with nothing to
  translate (§7), and the simulator draws the region where it is.
- **The grid stays the device's.** A region of a grid keeps its rows and
  columns: an effect counting from the first column counts from the device's.
  A definition's regions do not need more; a region drawn by the user may
  (§10).
- **The effects API does not change.** An effect cannot tell a region from a
  device and has no need to; `apiVersion` stays 1.
- **`kinds` and `requires` are checked against the region.** With a
  definition's regions the answer is the device's — the kind of lights is the
  definition's, and a region keeps its grid —, but the check takes the region,
  so that a region drawn later may offer less without another change.

## 2. Splitting a device, in *Devices*

- **A *Split into regions* checkbox** on the device's card, off by default.
  Offered only where the definition in force declares regions, on the card of a
  plugged-in device, controlled or not, where *Control* and *Release* are. A
  native checkbox with the amber tick of *Settings* and *Automations*: the
  application has no toggle switch.
- **Ticked, the card lists the regions** in the definition's order, each with
  its pictogram and its number of lights (*5 zones*). Listed, not edited:
  drawing regions is later (§10).
- **The pictogram** is the device's silhouette drawn from its definition's
  rectangles and outline: the region's lights filled, the others faint, thin
  lights thickened — a pictogram, not the data. The same one marks the region in
  *Effects*.
- **Ticking gives every region the device's applied effect**, with its
  settings, its bindings and what its brightness follows. Splitting changes how
  the device is driven, not what it shows, so it goes on showing the same effect
  until someone changes a region. What does change, said here rather than on
  screen: a spatial effect now measures each region on its own — a radial wave
  starts from each region's centre —, and every region's time starts from zero.
- **Unticking gives the whole device back what it had before it was split**:
  its last applied effect, with its settings, and what its brightness followed.
  A split is a detour, and the choices made for the whole device before it are
  not overwritten by it. The regions' choices are forgotten: ticking again starts
  from the device's effect, as the first time did.
- **A firmware effect driving the device when it is split is turned off**: a
  split device lights only what its regions run. None of the definitions
  declaring regions has one today.
- **Ticking or unticking is a gesture** on the device: like applying an effect,
  it ends an interruption under way
  ([`inputs-and-automations.md`](inputs-and-automations.md) §3.3).
- **The definition decides.** A device is split when its settings say so *and*
  the definition in force declares regions. A definition chosen without regions
  leaves the device whole, the split kept in the file for when one with regions
  drives it again. A region the settings name and the definition no longer
  declares is ignored; a region the definition declares and the settings do not
  name runs nothing.

## 3. An effect per region, in *Effects*

**The devices column.**

- **A split device becomes a header** — its pictogram, its status dot, its name,
  its number of regions (*4 regions*) — above one card per region, in the
  definition's order. The header is not selectable; its regions are, and the
  amber frame moves from the device to the region.
- **A region's card** shows its pictogram, its name, and in amber the effect it
  runs, as a device's line does today. A region running nothing says *no
  effect*, and its lights are black in the device's frame.
- **The selected region's card carries what its brightness follows** (§4); the
  header carries the brightness slider while one of its regions is selected,
  as only the selected card carries it today.
- **Collapsed**, the column keeps the device's pictogram with its status dot,
  and below it each region's pictogram alone, as an effect keeps its swatch.

**The effects and their settings.**

- **The list is filtered for the selected region** (§1), and **has no
  *Hardware* section**: firmware effects light the whole device, and a firmware
  knows nothing of regions.
- ***Applied* reads for the selected region**, as it reads for the selected
  device today. *Apply* and *Stop* act on that region; the others go on.
- **Each region keeps its parameters and bindings per effect**: two regions
  running the same effect can bind it to different signals. A binding follows
  the effect on the device today
  ([`inputs-and-automations.md`](inputs-and-automations.md) §2.3.1); on a split
  device it follows the effect on the region. The saved line names both:
  *Settings saved for Alienware 17 R5 · Keyboard*.

**The preview.**

- **The drawing stays the whole device**: the selected region in full colour,
  the others faded, with their real frames — what the device shows now.
  Selecting an effect the region does not run previews it on the region alone;
  the rest of the drawing keeps the device's frames.
- **The region's bounds and centre** are drawn over it, a dashed frame and a
  cross: what the effect measures as its device, and where a radial wave
  starts.
- **A line under the drawing** says which region, and its share of the lights:
  *Region Keyboard · 5 zones of 13*.

## 4. What stays the device's

| Per device, split or not | Why |
|---|---|
| *Control*, *Release*, and the state: controlled, not open, unplugged | adoption decides what Candeo may write to; a region is not opened on its own |
| The brightness slider | a hardware command where the definition has one, and the ceiling a follow scales under (`inputs-and-automations.md` §2.2.2) |
| *Send to the device*, the output toggle | one frame leaves per device (§7) |
| Firmware effects | offered only while the device is whole: the firmware lights all of it |
| Automations | a rule interrupts the whole device, at first |
| The definition chosen | it is what declares the regions |

**What the brightness follows becomes the region's.** The follow — *Value*,
*Signal*, *Sound*, *System* — is a factor the engine applies to each frame
before it leaves, and scaling a region's lights by their own factor costs what
scaling them all does. So the keyboard can follow the sound while the lid
follows the CPU temperature. The slider stays the device's: it is the ceiling,
written to the hardware where a command exists. Ticking *Split* copies the
device's follow to every region; unticking gives the device back its own, as for
the effect. A firmware effect still disables the switch; on a split device only
a rule can bring one, and nothing of ours is then drawn to scale.

**A region's follow scales its lights, whatever draws them**: its own effect, or
a rule's effect over the whole device. A follow belongs to the lights, as the
device's holds today when a rule takes over.

**Automations, at first, interrupt the whole device.**

- A rule names devices, as it does today. Its effect runs on the whole layout,
  the regions' effects set aside, and the interruption shows in the header,
  under the device's name, with *Resume* — not under each region.
- **A split device's resting state is always its regions**: when the rule ends,
  every region starts its applied effect again, its time from zero, as an
  interrupted device's does (`inputs-and-automations.md` §3.4).
- Applying an effect on a region during an interruption ends it, as any gesture
  does, and the device goes back to its regions, that one with its new effect.
- A rule showing a firmware effect lights the whole device for its duration;
  giving back restarts the regions.
- Rules aimed at a region are later (§10).

## 5. The tray

On a split device, the device's submenu offers its regions: **one submenu per
region**, named in the interface's language, listing the effects that apply to
it, the one it runs ticked. Choosing one starts it on that region with the
settings saved for it there, as *Apply* would. The interruption line,
*Resume*, *Send to the device* and *Turn off* stay at the device's level.
*Turn off* leaves every region with no effect, the split kept, then turns the
device off as today.

## 6. Words

- ***Region*** / ***région*** for a part of a device that runs its own effect;
  ***Split into regions*** / ***Scinder en régions*** for the checkbox. *Zone*
  stays the name of a light on a zones device.
- **Strings that say *keyboard* for the device the frames go to** move to *the
  device* (*l'appareil*): the *Lid* region is no keyboard, and neither is the
  m18's ring.

  | Key | English, reworded |
  |---|---|
  | `effects.preview.device` | Frames from the engine, exactly the ones sent to the device. |
  | `effects.preview.apply` | Preview: nothing is sent to the device. “Apply” sends this one there. |
  | `effects.preview.elsewhere` | Preview: nothing is sent to the device, where “{name}” keeps running. |
  | `effects.preview.illustration` | Run by the firmware. This is the application's drawing of what it does, not the device's own frames. |
  | `effects.previewError` | Error in the previewed effect, with no consequence for the device: {error} |
  | `effects.footerPreview` | the preview runs as soon as an effect is selected · “Apply” sends to the device |
  | `editor.noKeyboard` | No device connected: drawn from the default layout. |
  | `settings.startup.resumeDetail` | When Candeo starts or a device is plugged back in, the effect applied on it starts again. |
  | `devices.warnings.firmwareDiffers` | … but if the device does not obey, this is the first lead. |
  | `errors.deviceNotFound` | Device not found: plug it in and try again. |
  | `tray.sendToKeyboard`, renamed `tray.sendToDevice` | Send to the device |

  French follows with *l'appareil* (*Envoyer à l'appareil*). Kept:
  *Keyboard shortcuts*, the computer's keyboard, and the *Games* lead, the
  keyboard someone plays on.

## 7. The engine

**Still one loop and one frame per device.** A device's render thread runs a
list of **parts**, one effect each, on the whole layout or on a region's. Each
frame:

1. each part renders on its own layout, with its own parameters, bindings and
   inputs, under its own frame budget;
2. the device's frame starts black and takes, at each position, the colour of
   the part that owns it;
3. each region's lights are scaled by the region's follow — the whole frame by
   the device's follow while it is whole;
4. the frame is sent once, to the device and to the simulator.

A whole device is the case of one part owning every position: the path is the
same, and every engine test of today covers it.

**Why compose before sending, in one thread:**

- **The family keeps the pace.** A definition's `pace` — at most one frame every
  100 ms on the Alienware zones, an unchanged frame skipped — is held by the
  family, which remembers the last frame it sent and when (`Template::frame` in
  `definition.rs`). Two writers on one device would each find the other's frame
  too soon or changed, and frames would be lost at random.
- **One writer by construction**, not by vigilance: there is one.
- **The regions' frames are of the same instant.**

**Changing one region leaves the others running.** Applying an effect on a
region hands its JavaScript to the device's thread, which builds the new context
between two frames — a QuickJS context lives and dies on the thread that runs
it — and answers the waiting command. Restarting the device's loop with all its
regions would be simpler, and applying an effect on the lid would restart the
keyboard's from time zero: a starry sky redrawn, a wave jumping.

*Rejected*: a loop per region writing into a shared frame, and a writer sending
it at the device's pace. It keeps today's loop as it is, and costs a thread per
region plus one, a frame shared between threads, regions' frames from different
instants, and the rule "never a device's own loop and its writer at once" —
during an interruption — held by vigilance.

**A part's failure stays the part's.**

- A region's effect failing thirty frames in a row stops alone: its lights go
  black, its card says why, the other regions go on. The device is turned off
  only when no part runs any more, as today.
- A region's new effect that does not load leaves the region running the one it
  ran, and the command returns the error, as *Apply* does today.
- Stopping the last region's effect ends the loop on a black frame: a region
  with no effect is dark (§3). A whole device's *Stop* does not change.

**What it costs.** The parts render one after the other in the device's thread.
An ordinary effect takes 0.23 ms a frame
([`effects-runtime.md`](effects-runtime.md) §8): four regions, about a
millisecond of the 33. Each part keeps its own 10 ms budget, so four effects at
the edge of theirs would slow the device below 30 frames a second without
stopping it. Loading a region's effect holds the others for the load:
milliseconds, two seconds at worst.

**The preview of a region** runs the effect on the region's layout alone, with no
output, and borrows the region's follow. Its frame has the device's length,
black outside the region; the window lays it over the device's own frames (§3).

## 8. `settings.json`

- **A new key, `splitDevices`**, per model as `activeEffects` is: a device's
  entry while it is split, holding each region's applied effect and follow, and
  the parameters and bindings kept per region and effect (§11 b).
- **The device's own records are not touched while it is split**: its
  `activeEffects` entry, its `effectParams`, the `dimming` of its `devices`
  entry. They are what unticking gives back.
- **A key of its own rather than a `region` field on `effectParams`.** An older
  build ignores a field it does not know: a region's record would read as the
  device's, beside the device's own for the same effect, and its next write
  would keep it without the region.
- **An older build drops the key when it rewrites the file**: `Settings` keeps
  no key it does not know, and the Store, MSI and NSIS builds share the file
  ([`msix.md`](msix.md) §2). A device split in a newer build then comes back
  whole — with the effect and follow it had before it was split, which the split
  never touched. Accepted: it costs ticking again and choosing the regions'
  effects again, and keeping it would need every older build to know the key.
- **Same economy as the rest of the file**: no entry while the device is whole,
  no region record holding nothing.
- **No version bump.** The key is optional and needs no migration;
  `SETTINGS_VERSION` counts shapes that cannot be told from the content.

## 9. The brightness slider, checked on 2026-10-16

*Effects* shows the brightness slider for every selected device, whereas some
definitions have no brightness command. **It is a bug, not a software
dimming.**

- The slider is shown for any selected device and enabled whenever it is open
  (`EffectsView.vue`). Nothing could tell it otherwise: neither `LayoutInfo` nor
  `DeviceInfo` says whether a definition has `brightness` reports.
- On a definition without them — the 17 R5, the m18 R1's zones — moving it
  calls `set_brightness`, which the family refuses (`Error::NoBrightness`,
  shown as an unexpected error), while the level is still written to
  `settings.json` and fails again, in the log, at every open.
- Nothing scales frames by it. The engine's factor (`runtime/dimming.rs`:
  `level_factor`, `sound_factor`, `signal_factor`) comes from the follow source
  alone, from its floor to 1, and never reads the slider.
- The schema says a definition without `brightness` "offers no brightness
  slider", and so does `Lighting::brightness`.

The fix — the slider hidden where the definition has no brightness, the follow
switch kept, scaling from 100 % there — is a pull request of its own, outside
this work. For regions it decides one thing: a split device's header shows the
slider under the same condition as a whole device's card.

## 10. Later

- **Regions drawn by the user**, in the preview, down to a single light. They
  would be kept in `settings.json`, where a definition's are not; and a region
  drawn as a block of keys needs a grid of its own, counted from its own first
  row and column, where a definition's region keeps the device's.
- **Automations aimed at a region**: a rule naming a device's region rather than
  the device.
- Layers and masks — several effects over the same lights — are not planned.

## 11. Implementation contract

Precise enough for the Rust part and the window to be built side by side. Names
given here are the contract; names not given are each pull request's choice.

### a. The definition

**Schema**, `crates/candeo-device/devices/device-definition.schema.json`, a new
top-level property:

```json
"regions": {
  "type": "array",
  "minItems": 2,
  "description": "Parts of the device that can each run their own effect once someone splits it, every light in exactly one. Absent: the device is not offered *Split into regions*.",
  "items": {
    "type": "object",
    "additionalProperties": false,
    "required": ["id", "name", "lights"],
    "properties": {
      "id": {
        "type": "string",
        "pattern": "^[a-z][a-z0-9-]{0,31}$",
        "description": "What settings and commands name it by: `keyboard`. Changing it orphans what was kept for the region."
      },
      "name": { "$ref": "#/$defs/text", "description": "What the interface calls it: `{ \"en\": \"Lid\", \"fr\": \"Capot\" }`." },
      "lights": {
        "type": "array",
        "minItems": 1,
        "uniqueItems": true,
        "items": { "type": "integer", "minimum": 0, "maximum": 255 },
        "description": "The addresses of its lights, as `lights.items` gives them."
      }
    }
  }
}
```

**Checks at load**, in `parse`, after the schema; each refusal one sentence
naming what is wrong, as the other refusals do:

| Refused | Sentence |
|---|---|
| an id given twice | region “lid” is given twice |
| an address no light has | region “lid”: no light has address 14 |
| an address in two regions | address 3 is in “base” and in “power” |
| a light in no region | address 2 is in no region |

**Types**, in `candeo-device`:

```rust
/// A part of a device that runs its own effect once the device is split.
pub struct Region {
    /// What settings and commands name it by: `keyboard`.
    pub id: &'static str,
    pub name: Text,
    /// The frame positions of its lights, ascending.
    pub positions: &'static [u16],
    /// The device as the region's effect sees it (§1).
    pub layout: &'static Layout,
}

// `Layout` gains, in the file's order; empty for a definition declaring none:
pub regions: &'static [Region],
// and `Layout::region(&self, id: &str) -> Option<&'static Region>`.
```

**A region's layout** is the device's with: `keys` only the region's;
`matrix` holding `EMPTY` outside the region; no `firmware_effects`; no
`regions`. Everything else is the device's — `name`, `vid`, `pid`, `port`,
`lighting`, `lights`, `grid`, `rows`, `cols`, `outline` — so `led_count()` is the
device's and a region's frame has the device's length. Built and leaked in
`parse` with the device's layout; every `Layout { .. }` literal gains
`regions: &[]`.

**Data**: `alienware-17-r5-zones.json` and `alienware-m18-r1-zones.json` gain
the regions of §1. `site/devices-guide.html` documents the property.

### b. `settings.json`

```json
"splitDevices": [
  {
    "vid": 6268,
    "pid": 1360,
    "regions": [
      { "region": "keyboard", "effect": "shipped:Radial wave",
        "dimming": { "source": "sound:volume", "floor": 20 } },
      { "region": "lid", "effect": "shipped:Breathing",
        "dimming": { "source": "system:cpuTemp", "floor": 20 } },
      { "region": "base", "effect": "shipped:Starry night" }
    ],
    "effectParams": [
      { "region": "keyboard", "effect": "shipped:Radial wave",
        "values": { "speed": 120 }, "bindings": { "hue": "sound:tone" } }
    ]
  }
]
```

- An entry exists **while the model is split**, keyed by `vid` and `pid`
  without serial, as `activeEffects` is: every engine command targets a model.
  `{ "vid", "pid" }` alone is a split device whose regions run nothing.
- `regions[]`: a region's applied effect (`effect`, an effect key) and what its
  brightness follows (`dimming`, the shape of a `devices` entry's). A record
  with neither is dropped; `regions` and `effectParams` are left out when
  empty.
- `effectParams[]`: the shape of `effectParams`, plus `region`. An empty
  `values` and no `bindings` drops the record.
- **Ticking** writes the entry: every region of the definition gets the
  device's applied effect, a copy of the device's `effectParams` record for that
  effect, and a copy of the device's `dimming`. **Unticking** removes the
  entry. Neither touches the device's own records.
- **Deleting or renaming an effect** reaches the regions' records too
  (`forget_effect`, `rename_effects`, `effect_references`). Resetting the
  configuration drops the key with the rest.
- Rust: `Settings` gains `#[serde(skip_serializing_if = "Vec::is_empty")]
  split_devices: Vec<SplitDeviceRecord>`, with `SplitDeviceRecord { vid, pid,
  regions: Vec<RegionRecord>, effect_params: Vec<RegionEffectParamsRecord> }`,
  `RegionRecord { region, effect: Option<String>, dimming: Option<Dimming> }`
  and `RegionEffectParamsRecord { region, effect, values, bindings }`, all
  `camelCase`.
- TypeScript, in `api/candeo.ts`, mirrored the same way: `SplitDeviceRecord`,
  `RegionRecord`, `RegionEffectParamsRecord`, and `Settings.splitDevices?`.

### c. Commands and types

**A region is named by its device and its id**: the `DeviceRef` `{ vid, pid }`
of its model, and the id its definition gives it, `"keyboard"`. Commands keep
their `device` argument and gain a separate `region`, so that `DeviceRef` stays
the key of every device table (`docs/api/commands.md`, *Designating a device*)
and a call without a region means exactly what it means today.

In `api/types.ts`:

```ts
/** A part of a device that runs its own effect once the device is split. */
export interface RegionInfo {
  /** What settings and commands name it by: `keyboard`. */
  id: string
  /** As its definition writes it: a string, or one per language. */
  name: Text
  /** The frame positions of its lights — `KeyInfo.index` values — ascending. */
  positions: number[]
  /** What `applies` checks an effect against for it: the device's, today. */
  lights: 'keys' | 'zones'
  capabilities: string[]
}

/** One region of a split device. */
export interface RegionRef extends DeviceRef {
  region: string
}

// LayoutInfo gains:  regions: RegionInfo[]   — empty when the definition declares none
// DeviceInfo gains:  regions: RegionInfo[]
//                    split: boolean           — its settings say so and its definition declares regions
```

In `api/candeo.ts`:

```ts
export interface RegionEngineStatus {
  region: string
  running: boolean
  effectId: string | null
  /** Error coming from the region's effect. Already readable. */
  error: string | null
}

// DeviceEngineStatus gains:  regions?: RegionEngineStatus[]
// PreviewStatus gains:       region?: string
```

`regions` is present while the device's loop runs its regions, one per region of
the definition in its order; the device's own `running` is then true while one of
them runs, and its `effectId` and `error` are null. During an interruption the
loop runs one part on the whole device, `regions` is absent, and the device's
fields describe the rule's effect, as today. `deviceError`, `reachingKeyboard`,
`toKeyboard` and `interruption` are always the device's.

**Commands that gain `region: string | null`**, absent or null meaning the whole
device:

| Command | With a region |
|---|---|
| `start_effect(device, region, id, params, bindings)` | starts it on the region, the others untouched, and remembers it as the region's applied effect |
| `stop_effect(device, region)` | the region runs nothing, its lights black; forgets its applied effect |
| `set_effect_params(device, region, params)` | live, the region's part |
| `set_effect_bindings(device, region, bindings)` | live, the region's part |
| `remember_effect_params(device, region, effect, params)` | `splitDevices[].effectParams` |
| `remember_effect_bindings(device, region, effect, bindings)` | `splitDevices[].effectParams` |
| `set_dimming(device, region, dimming)` | live, the region's follow |
| `remember_dimming(device, region, dimming)` | `splitDevices[].regions[].dimming` |
| `start_preview(device, region, id, params, bindings)` | the region's layout and its follow; `device` is then required |

On a split device these require a region, and on a whole one they refuse one —
except `start_preview`, which may borrow a split device's whole layout. A region
on a whole device, none on a split one, or an id the definition does not declare
is `Failure::unexpected`, with an English detail: the window never sends one
knowingly, and a race with the tray ends at the next state-changed event.

**New command**: `set_split(device: DeviceRef, split: boolean)`. Writes or
removes the `splitDevices` entry (§11 b), ends an interruption under way, and,
when the device is open and controlled, runs what it now has: every region with
its copied effect, or the whole device with its applied effect — nothing when it
has none. Turns off a firmware effect on ticking. Emits the state-changed event
and rebuilds the tray menu. `Failure::unexpected` when the definition in force
declares no regions.

**Changed behaviour**:

- `get_layout(device)` answers for a device that is not open too, from the
  definition in force (`find_layout`), as `start_preview` already does: the
  window draws pictograms and regions for a device it has not opened. Its callers
  today ask only for an open device, except *Automations*, which then filters
  its pickers for a closed device too instead of offering every effect.
- `set_effect(device, …)`, a firmware effect, is `Failure::unexpected` on a split
  device.
- `delete_effect(id)`, `rename_effect(from, to)` and `forget_effect_settings(id)`
  reach the regions' records too, and `reset_settings` drops `splitDevices`.
- `subscribe_frames(device)` is unchanged: the device's composed frames.
  `subscribe_preview_frames` is unchanged: a region's preview sends frames of
  the device's length, black outside the region.

**Wrappers in `api/candeo.ts`** take a `DeviceRef | RegionRef` where a command
gains a region, and send `{ device: { vid, pid }, region: target.region ?? null,
… }`; `setSplit(device, split)` is new.

### d. The engine

**Parts.** A device's loop holds parts. A part is one effect on one layout —
the device's, or a region's `Region::layout` — owning that layout's lit
positions, with its own state: `stop`, `params`, `bindings`, `effect_id`,
`error`, `reads_private`, its QuickJS context, budget, clock, frame index, error
count and input leases. The device keeps what is the device's: `to_keyboard`,
`frames`, `device_error`, `reaching`, `device_failures`. Today's `Shared` splits
along that line.

**A frame**: each part renders under `FRAME_BUDGET`; the device frame starts
black and takes each position from the part owning it; then the follow scales
it — per region, by `(device, Some(region))`, for a split device, whatever part
drew the lights; as a whole, by `(device, None)`, otherwise; then one `emit`.

**Engine API** — today's methods keep their meaning on a whole device:

```rust
pub struct EffectStart { pub effect_id: String, pub js: String, pub params: String, pub bindings: Bindings }

impl Engine {
    /// One effect on the whole device, replacing whatever ran: today's `start`.
    /// `split` says whose follows scale the frame (an interruption of a split device).
    pub fn start(&self, device: DeviceRef, start: EffectStart, layout: &'static Layout,
                 out: Box<dyn DeviceOut>, split: bool) -> Result<(), String>;
    /// The device's regions, each with its effect or none, replacing whatever ran.
    /// A region whose effect does not load runs nothing and reports it.
    pub fn start_regions(&self, device: DeviceRef, layout: &'static Layout, out: Box<dyn DeviceOut>,
                         regions: Vec<(&'static Region, Option<EffectStart>)>)
        -> Vec<(&'static str, Result<(), String>)>;
    /// One region, the others untouched. `Err(NotRunningRegions)` when the loop
    /// does not run the device's regions: the caller then calls `start_regions`.
    pub fn start_region(&self, device: DeviceRef, region: &str, start: EffectStart) -> Result<(), RegionStart>;
    /// The region runs nothing; the last one stopped ends the loop on a black frame.
    pub fn stop_region(&self, device: DeviceRef, region: &str);
    pub fn set_params(&self, device: DeviceRef, region: Option<&str>, params: String);
    pub fn set_bindings(&self, device: DeviceRef, region: Option<&str>, bindings: Bindings);
    pub fn set_dimming(&self, device: DeviceRef, region: Option<&str>, dimming: Option<Dimming>);
    pub fn set_dimmings(&self, all: HashMap<(DeviceRef, Option<String>), Dimming>);
    pub fn start_preview(&self, layout_of: DeviceRef, region: Option<&str>, start: EffectStart,
                         layout: &'static Layout) -> Result<(), String>;
}
// enum RegionStart { NotRunningRegions, Failed(String) }
```

- **`start_region` reaches the running thread** through a channel the thread
  reads between two frames: *replace this part*, with a sender for the verdict.
  The thread builds the context; on success the part is replaced, on failure the
  old one goes on and the error comes back. The command holds the device's
  `thread` lock while it waits, as `start` does: starts on one device are
  serialised, and no other device waits. `stop_region` goes the same way.
- `stop`, `stop_all`, `stop_everywhere` (the parts running the effect),
  `rename_everywhere` (the parts' ids) and `device_status` (with `regions`) cover
  parts. `MAX_CONSECUTIVE_ERRORS` stops one part; `turn_off` happens when none
  is left.
- `dimming::Table` is keyed by `(DeviceRef, Option<String>)`; `Settings::dimmings`
  returns the regions' entries beside the devices'.
- The preview stays one part, with `NoOutput`, on the region's layout when given
  one, reading the region's follow.

**Who calls what**, in `runtime` and `automations`:

| Path | Whole device | Split device |
|---|---|---|
| `start_effect` | `start` | `start_region`, else `start_regions` with the others' applied effects |
| `stop_effect` | `stop` | `stop_region` |
| `resume_applied`, `resume_waiting` | `activeEffects` | each region's applied effect, unless the loop already runs them |
| `resting_state` | as today | always `Resting::Applied` |
| `give_back` | `activeEffects` | `start_regions` from the regions' records |
| `interrupt` | `start` / firmware | `start(.., split: true)` / firmware |
| tray `start_saved` | `run_with` | per region, with the region's settings |
| tray *Turn off* | as today | every region's `effect` cleared, then *Off* |

Every reader of `activeEffects` asks first whether the device is split.

**The tray** encodes a region's effect as `regionEffect:<vid>:<pid>:<region>:<effect>`,
`<vid>:<pid>` as `hex_id` writes them, the effect last since its key holds a
`:`; a region id cannot.

### e. Two pull requests

**1. Rust — definitions, settings, engine, tray, commands.**

- (a) to (d); `docs/api/commands.md` for every command changed or added;
  `site/devices-guide.html` for `regions`.
- Catalogs: only the keys of §6 on Rust's side — `tray.sendToKeyboard`
  renamed `tray.sendToDevice` and `devices.warnings.firmwareDiffers`, which Rust
  renders, and `errors.deviceNotFound`, whose code it sends — in English and
  French.
- Tests, without hardware: the refusals of (a); a region's layout (its keys, its
  `led_count`, its bounds); the built-in definitions' regions; the composition
  and the follow per region over a rule's frame, as pure functions; one region
  replaced while another keeps its clock; a part failing alone; the settings
  transitions (tick, untick, forget, rename) and a file without the key; the
  tray's ids with a region.
- **May assume** that no window sends a region yet: every call without one must
  behave as today, byte for byte, and the current window ignores the fields it
  adds. Nothing becomes visible but the reworded strings until the window
  offers *Split*.

**2. Window — *Devices*, *Effects*, preview, catalogs.**

- The types and wrappers of (b) and (c); the checkbox and the regions' list in
  *Devices*; the header, region cards, follow per region, filtered list without
  *Hardware*, *Apply* and *Stop* per region, the saved line, in *Effects*; the
  pictogram; the preview laid over the device's frames, `KeyboardSimulator`
  gaining `focus?: readonly number[]` — the positions drawn in full colour, the
  others faded, the focus's bounds and centre drawn over them.
- Catalogs: every other key of §6, and the new ones — *Split into regions* /
  *Scinder en régions*, *{n} region* / *{n} région* with its plural, the line
  under the drawing in a `keys` and a `zones` form, the simulator's label for a
  region, and the name of a region with its device, *{device} · {region}* — in
  English and French. A region running nothing reuses `effects.deviceIdle`.
- The Store *What's new* line, in English and French, under
  `packaging/store/news/`.
- Tests next to their modules, mocking `api/candeo.ts`: the selection of a
  region, the overlay of a preview frame on the device's, `applies` per region,
  settings read and written per region.
- **May assume** (c) exactly, and that the built-in 17 R5 and m18 R1 zones
  declare their regions. Built against mocks, it merges after the Rust pull
  request, rebased on it, and is tried in the running app then.

The first refers to #329; the second fixes it, and turns this note's status to
implemented, with what the implementation decided beyond it.

## 12. What the window decided beyond this note

- **The editor applies to the region chosen in *Effects*.** The region chosen
  on each split device is the whole window's choice, as the device is: the
  editor's *Apply* and *Stop* act on it, and its simulator draws the region
  over the device as *Effects* does. Without it, the editor would send a split
  device a command for all of it, which Rust refuses. The choice lives for the
  session, per device; it is not saved.
- **A region whose effect failed** says *error* on its card, in red, the reason
  on hover and in the settings column while it is selected.
- **During an interruption** each region's line names the effect it goes back
  to; the header says the rule.
- **Without a brightness command**, the header has no slider, and the selected
  region's card puts the *Brightness* heading over what it follows, as a whole
  device's card does (#334).
- **The pictogram** thickens a light to 6 % of the device's longest side, about
  a pixel at its size. The layouts it is drawn from are read once per
  definition in force.
- **The preview of a region** keeps both channels open, the preview's and the
  device's, one owner each, and lays the first over the second; while the
  device's loop runs nothing, the other regions are black. The device's channel
  is opened again when what its loop runs changes.
- **The line under the drawing** counts the region's lights out of the device's
  (`lightCount`).
- ***Settings › Signals*** lists a region's settings bound to a signal under
  *device · region*, after the devices'.
