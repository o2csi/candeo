# Layers in the window — the interface

The interface of layers, step 2 of [`composition.md`](composition.md) §5.1:
where the stack is shown, how the detail panel frames what is selected, how a
layer is added, masked, blended, ordered and removed, how layers and masks are
saved and reused, the words, what accessibility requires, and what step 2 must
pass. The model and the engine are settled in `composition.md`; this note adds
two keys to `settings.json`, for saved layers and saved masks (§5.5), and
replaces `composition.md` §3.10's placement of the stack, for the reasons of §2.

Status: decided on 2026-11-06. The maintainer reviewed a clickable prototype
twice; the second round reshaped the detail panel (§3) and added saved layers
and saved masks (§5). §11 records the decisions taken on the questions the
first round left open.

## 1. How it was worked out

- **A critique of the *Effects* view as it is** (2026-11-06, the dev build on
  the DeathStalker V2 Pro and the m18 R1's zones split into regions), and of
  §3.10 against it: §2.
- **An accessibility review** against WCAG 2.2 AA, with the contrast of the
  tokens measured (`styles/tokens.css`): §8.
- **The words**, English and French, checked against the catalogs' existing
  terms and AGENTS.md's rules: §7.
- **The components**, reusing what exists before adding anything: §6.
- **The requirements and their acceptance**, P0 to P2: §9.
- **Two rounds on a prototype.** The first settled the stack's place. The second
  found the detail panel cluttered: three *Save* buttons in three places, none
  saying what it kept, and six sentences of explanation around the controls.
  §3 is the answer: one card per selected row, each *Save* inside what it covers,
  one state line instead of the sentences.

No telemetry exists or will: §10 says how the design is checked instead.

## 2. Where the stack lives

**In the devices column, under its target, as regions are.** The selected
device's card, or the selected region's, lists its layers between the
brightness and the next card: *Add a layer* on top, the layers top first, the
base last.

| | In the devices column (chosen) | In the detail panel (§3.10) |
|---|---|---|
| What the columns mean | left: *where*, every target; middle: *what*; right: *how* | *where* split between the left column and a block on the right |
| Precedent | a region is chosen on the left and is what the gallery targets ([`regions.md`](regions.md) §3) | none: a new kind of target in a new place |
| Seeing the target while choosing an effect | always: the column does not scroll with the panel | the block sits under the preview, often out of view |
| The panel's title | the effect shown, under a line naming the target | the effect shown; the target is somewhere below |
| Width | 264 px: a swatch, a name and one line per row | wide, but shared with the preview, presets and settings |
| Collapsed column | the card alone, as today | unaffected |

Three findings decided it:

1. **Two places choosing a target** would have the gallery depend on a selection
   made to its right, below a preview whose title names the gallery's choice.
2. **Regions already answer the question**: the selected region is chosen on the
   left, and *Apply* sends there. A layer is one more target of the same kind.
3. **The detail panel already runs long**. The layer's own controls (mask,
   blend, opacity) belong there; the list of layers does not.

The stack shows only for the selected target, as the brightness block does: one
open card at a time keeps the column short.

## 3. The detail panel

Top to bottom: the preview, the card of the selected row, the action bar.

**The preview** keeps its frame and gains a header line:

- **One state, in words**: *Live on the device* (`--ok`), *Preview · nothing is
  sent* (`--accent`) while the gallery's choice differs from what runs, *Hardware
  effect · approximate preview* when the firmware draws. It replaces the notes
  the panel had: the line under the title, the gallery's footnote, the line
  under the preview naming the mask, and the action bar's hint.
- ***Only this layer*** on the right, for a layer or a new layer with an effect.
- **During a key selection** the header becomes the selection bar (§4.3).

**The card** frames the selected row: the base, a layer, or the new layer.

- **Its header**: the effect's swatch, a line naming the row — *Base*, *Layer 2
  of 3*, *New layer*, *New layer · from "CI on Escape"* — the effect's name as
  the title, and, for a layer, ***Save the layer…***.
- ***Mask and blend***, for a layer only: mask, blend, opacity and what it
  follows (§4.3, §4.4).
- ***Effect***: the effect's origin (*Built-in*, *Yours*), *View code* and
  *Duplicate*, then the presets with *Save as preset…*, then its settings.
- **A new layer without an effect** shows its header alone: *Choose an effect*,
  over *An effect or one of your layers, in the gallery.*

**Each *Save* sits inside what it keeps**, so the place answers the question
before the dialog does: *Save the layer…* in the card's header, which frames
both sections; *Save as preset…* in *Effect*, which is all a preset keeps; a
mask's actions in its own menu (§5.4). *View code* and *Duplicate* move from the
action bar into *Effect* for the same reason: they concern the effect alone.
The base's card is today's panel in this frame: no *Mask and blend*, no *Save
the layer…*.

**The action bar** keeps the button that sends — *Apply*, *Applied*, *Apply to
this layer*, *Add the layer* — and *Cancel preview* while there is one.

## 4. The flows

### 4.1 Add a layer — *WASD in red over my effect*

1. The device is selected; its card lists *+ Add a layer* above the base.
2. *Add a layer* is selected: the row reads *New layer — choose an effect*; the
   card reads *New layer* over *Choose an effect*; *Apply* becomes *Add the
   layer*, disabled.
3. *Fixed colour* in the gallery: the preview shows it over the stack, on the
   whole device, with *screen* at 100 % (decision 5). The state line says
   *Preview · nothing is sent*; *Add the layer* is enabled.
4. *Mask and blend*: *Mask* → *Movement keys · 4 keys*, *Blend* → *Cover*. The
   preview follows at once; nothing is sent.
5. *Add the layer*: it runs on the device, on top, selected; the device's card
   reads *Diagonal wave + 1 layer*.

Choosing the effect before or after the mask both work: the new layer's look is
kept while the gallery is browsed. *Your layers*, above the effects in the
gallery, offers a saved layer in the same place (§5.3).

### 4.2 Change what a layer runs

Select its row, pick another effect: the preview shows it in that row's place,
the rest of the stack around it. *Apply to this layer* sends it; the mask, blend
and opacity stay. Selecting never writes, as today.

### 4.3 The mask

A native `select`, grouped:

- *The whole device* (*The whole region* in a region's stack);
- **Key groups** this layout has, each with its count: *Number row · 13 keys*.
  A group with no key here is absent, not disabled;
- **Regions**, on a whole device whose definition declares them: *Lid · 3
  zones*;
- **Your masks** (§5.4), each with its count here: *Game keys · 9 keys*;
- the chosen keys, when the layer has some: *Chosen keys · 5 keys*;
- *Choose keys…* (*Choose zones…* on a zones device).

Beside it, a **⋯ menu** when the mask is a saved one or chosen keys (§5.4); none
for the whole device, a group or a region.

**Choose keys** turns the preview into a selection:

- the preview's header becomes the bar: *Click the keys to light:*, the live
  count, *Select none*, *Cancel*, *Done*;
- a click toggles one key; a drag paints from the first key's new state;
- selected keys keep their colour, with the accent outline and a check; the
  others fade;
- arrows move between keys, Space toggles, Enter is *Done*, Escape *Cancel*;
- *Done* is disabled at zero keys;
- the rest of the panel is inert until the selection ends.

Out of selection, the selected layer's keys are outlined in dashes over the
final frame. *Only this layer* draws that layer alone over black, in the preview
only.

### 4.4 Blend and opacity

- **Blend**: four segments, *Cover*, *Add*, *Screen*, *Multiply*, the selected
  one's sentence under them (§7).
- **Opacity**: a slider with its value, and what it follows, with the brightness
  follow's *Value / Signal / Sound / System* switch.
- Live on the device when the layer runs there, saved at once (#218).

### 4.5 Order and removal

- **Move up / Move down** on each layer row, with the rules' grip for dragging.
  The base never moves; *Add a layer* stays on top.
- **Remove** takes the row away at once and leaves *Layer removed · Undo* in its
  place until the next change to that stack. Undo puts it back with its place,
  mask and settings. No confirmation: nothing is deleted from the library.
- A live region says *Layer moved to position 2 of 3*, *Layer removed*.

### 4.6 The base

The bottom row, tagged *base*, is the applied effect as today. Selecting it
targets it; *Apply* keeps its label. A **hardware effect** applied there sets the
layers aside (`composition.md` §3.2): the rows stay, dimmed, under *A hardware
effect runs: the layers wait for one of Candeo's effects*.

### 4.7 Split devices

Each region's card carries its own stack when selected; the split device's
header carries none. A whole device whose definition has regions offers them as
masks (§4.3). The two ways of `composition.md` §3.5, as the prototype's board
*Split device* shows.

### 4.8 Elsewhere

- **The device's card and the tray** say *Diagonal wave + 2 layers*. The tray
  changes the base and keeps the layers (decision 9).
- **The copied diagnostic** lists the layers, as step 1 made it.
- **Automations** is step 3's.

## 5. Saved layers and saved masks

A layer set up on one keyboard is wanted on another, or again later; a set of
keys drawn once is wanted under several layers. Both are saved, as presets are,
and offered where they are used: no separate library screen.

### 5.1 What each keeps, and how it is put back

| | Preset | Saved layer | Saved mask |
|---|---|---|---|
| Keeps | an effect's settings | the effect and its settings, the mask, the blend, the opacity and what it follows | keys, or lights |
| Put back | as a copy | as a copy | **linked** |
| Changing it afterwards | changes nothing already set | changes nothing already set | changes every layer using it |
| Deleting it | changes nothing | changes nothing | each layer using it keeps its keys |
| On another device | wherever the effect runs | any keyboard when its mask is keys or a group; the same model when it is lights or a region | the same |

**Why the difference**: a mask is a selection that is refined, and refining it
should reach every layer drawn through it — *Game keys* gains Shift, every layer
on them follows. A layer's look is tuned per device, and a later change to the
saved one should not reach into layers someone already adjusted. A saved layer
whose mask is a saved mask keeps the link: putting it back links the new layer
to the same mask.

Applying a layer to several devices at once is a saved setup, #367's.

### 5.2 Saving a layer

*Save the layer…*, in the card's header (§3), opens a dialog:

- **Name**, filled with *{effect} · {mask}*, selected, unique among saved layers
  ignoring case, 1 to 64 characters.
- **What is saved**, a list with a check per line, so that nobody has to guess
  whether the blend goes with it: *Effect* — *Fixed colour, with its settings*;
  *Mask* — *Game keys · 9 keys*, and under it *Linked: it will follow the mask's
  changes* when the mask is a saved one; *Blend* — *Cover*; *Opacity* — *100 %*,
  or *100 %, follows the sound*.
- *It will appear in the gallery, under Your layers.*
- *Cancel* and *Save*.

It saves what the device runs, not a preview: the button is absent while the
row previews another effect.

### 5.3 Your layers, in the gallery

- **Above the effects**, under *Your layers*, when the target is a layer or *Add
  a layer*; absent for the base, which has no mask or blend. Each item: the
  effect's swatch framed in dashes, the name, and *{effect} · {mask} · {blend}
  {opacity}*.
- **Picking one previews it whole**: effect, settings, mask, blend and opacity,
  in the selected row's place. The card's header says *New layer · from
  "{name}"* or *Layer 2 of 3 · replaced by "{name}"*; *Mask and blend* shows its
  values, read-only until it is added. *Add the layer* or *Apply to this layer*
  puts a copy there.
- **A ⋯ menu on each item**, shown on hover and focus, always in the DOM:
  *Rename…* and *Delete*. Deleting changes no layer; *Undo* stays in the item's
  place until the next change, as a removed layer's does.

### 5.4 Saved masks

- **Your masks** in the mask's `select` (§4.3), each with its count on this
  target. A saved mask that lights nothing here is absent, as a group is.
- **The ⋯ menu** beside the `select`, for a saved mask: *Used by 3 layers*, then
  *Edit keys…*, *Rename…*, *Delete*. For chosen keys not yet saved: *Edit keys…*
  and *Save as mask…*.
- ***Save as mask…*** opens the dialog of §5.2 with the name alone and *Keys —
  9 keys*, and *It will appear in the Mask menu, under Your masks, and this
  layer will use it.* The layer then names the saved mask.
- ***Edit keys…*** opens the selection on the mask's keys; the bar reads *Edit
  "Game keys", used by 3 layers:*, and *Done* becomes *Update the mask*. Every
  layer naming it follows at once, on every device.
- ***Delete*** writes the mask's keys into each layer and saved layer naming it,
  then leaves *"Game keys" deleted · 3 layers keep its keys · Undo* under the
  `select`, until the next change. Undo puts the mask and its links back.
- ***Rename…*** changes the name only: layers name a saved mask by its id.

### 5.5 In `settings.json`

```json
"savedMasks": [
  { "id": "k1", "name": "Game keys", "keys": ["11", "1e", "1f", "20", "12", "13", "21", "39", "2a"] },
  { "id": "k2", "name": "Lid logo", "vid": 6268, "pid": 2615, "lights": [3, 7] }
],
"savedLayers": [
  {
    "name": "CI on Escape",
    "effect": "shipped:Breathing",
    "values": { "colour": { "r": 242, "g": 163, "b": 60 }, "speed": 1 },
    "mask": { "group": "escape" },
    "blend": "cover"
  },
  {
    "name": "Equalizer on game keys",
    "effect": "shipped:Equalizer",
    "values": { "bands": 16 },
    "mask": { "saved": "k1" }
  }
]
```

- **A saved mask** has a short `id`, unique, what layers name it by; a `name`,
  unique ignoring case, 1 to 64 characters; and a mask's fields beside them, as
  `composition.md` §3.3 writes them. `vid` and `pid` come with `lights` or
  `region`, which mean something on one definition only.
- **A layer's mask may name a saved one**: `{ "saved": "k1" }`, and nothing else
  beside it. It is resolved when the stack starts and whenever the saved mask
  changes; the engine only ever sees the mask it resolves to. A name nobody
  saved, read from the file, lights nothing, and the row says so (§6).
- **A saved layer** is a layer record without `id`, with a `name` unique ignoring
  case, 1 to 64 characters; `values` keeps every value the effect declared, as a
  preset does, so that a later default does not change it.
- **Same checks as layers** (`composition.md` §3.11), when read and when saved,
  each refusal one sentence with its negative test: an unknown effect (kept,
  marked missing), a mask that names nothing, a `saved` beside another field, an
  unknown blend, an opacity outside 0–100, a name empty, too long or taken.
- **Deleting or renaming an effect** reaches saved layers as it reaches presets.
  Resetting the configuration drops both keys.
- **No version bump**: both keys are optional. A build since #373 writes them
  back without knowing them; an older one drops them when it writes the file.

## 6. Components

No new token. Everything reuses `tokens.css`; the RGB spectrum stays the
keyboard's.

| Component | New or reused | What it is |
|---|---|---|
| `LayerStack` | new | the rows under the selected target's card; group named *Layers of {target}* |
| `LayerRow` | new | swatch, effect, line `{mask} · {blend} {opacity}`; a button with `aria-pressed`, as a region; Move up/down and Remove, 24 × 24, shown on hover and focus, always in the DOM |
| `DetailCard` | new | the card of §3: header with *Save the layer…*, then its sections as slots |
| `LayerLook` | new | *Mask and blend*: `MaskPicker`, blend radio group, opacity slider and follow; absent for the base |
| `MaskPicker` | new | the mask `select`, its ⋯ menu, the deleted mask's *Undo* |
| `SaveDialog` | new | name and *What is saved*, for a layer and for a mask |
| `SavedLayers` | new | *Your layers* in the gallery, each item with its ⋯ menu |
| `ReorderControls` | extracted | the rules' grip and *Move up* / *Move down*, shared by `AutomationsView` and `LayerStack` |
| `BrightnessFollow` | reused | gains `subject` (`brightness` or `opacity`) for its labels |
| `KeyboardSimulator` | extended | `selecting` (positions, or `null`), emits `toggle` and `paint`; keys become toggle buttons named by legend |
| Region card, `EffectPresets`, `EffectParamsForm` | reused | the last two move into the card's *Effect* and receive the selected row's values |

**`LayerRow` states**, each said in words, colour as a second channel:

| State | Line | Look |
|---|---|---|
| default | `{mask} · {blend} {opacity}` | plain |
| selected | same | accent frame and `--accent-soft`, as a selected region |
| failed | *Stopped on an error* | `--bad` |
| mask lights nothing here | *Nothing to light here* | `--warn`, swatch hatched |
| waiting for a library effect | the sentence of §4.6 under the rows | rows at 60 % |
| removed | *Layer removed · Undo* | `--raised-2` |
| base | *base* | no tools |
| *Add a layer* | — | dashed `--line-strong` border, muted text |
| *Add a layer*, selected | *New layer · {effect}* | accent frame |
| *Add a layer* at the limit | *Eight effects at most, the base included* | plain text, not a button |

## 7. Words

The settled terms stay: *layer* / *calque*, *mask* / *masque*, *Add a layer* /
*Ajouter un calque*, *Cover* / *Couvrir*, *Add* / *Additionner*, *Screen* /
*Éclaircir*, *Multiply* / *Teinter* (decision 10). New ones:

| English | Français | Where |
|---|---|---|
| base | base | the bottom row; one word in both: *Background* / *Fond* clashes with effects' own *background* colour, *Applied effect* with the button's *Applied* |
| Apply to this layer · Add the layer | Appliquer à ce calque · Ajouter le calque | the action bar, labelled by outcome |
| Cancel preview | Annuler l'aperçu | the action bar |
| Live on the device · Preview · nothing is sent · Hardware effect · approximate preview | En direct sur l'appareil · Aperçu · rien n'est envoyé · Effet matériel · aperçu approximatif | the preview's state line |
| Only this layer | Ce calque seul | the preview's header |
| Base · Layer {n} of {total} · New layer | Base · Calque {n} sur {total} · Nouveau calque | the card's header |
| New layer · from "{name}" · replaced by "{name}" | Nouveau calque · depuis « {name} » · remplacé par « {name} » | the card's header, a saved layer previewed |
| Choose an effect · An effect or one of your layers, in the gallery. | Choisissez un effet · Un effet ou un de vos calques, dans la galerie. | the new layer's card |
| Mask and blend · Mask · Blend · Opacity | Masque et mélange · Masque · Mélange · Opacité | the card's section |
| Effect · Built-in · Yours | Effet · Intégré · À vous | the card's section and the effect's origin |
| The whole device / region | Tout l'appareil / Toute la région | mask |
| Chosen keys · {n} keys | Touches choisies · {n} touches | mask |
| Choose keys… / Choose zones… | Choisir les touches… / Choisir les zones… | mask |
| Click the keys to light: · Select none · Cancel · Done | Cliquez les touches à éclairer : · Tout désélectionner · Annuler · Valider | the selection bar |
| Remove · Layer removed · Undo | Retirer · Calque retiré · Annuler | *Delete* / *Supprimer* is for what the library holds; a removed layer is in no library |
| {effect} + {n} layers | {effect} + {n} calques | card and tray |
| Save the layer… · What is saved · It will appear in the gallery, under Your layers. | Enregistrer le calque… · Ce qui est enregistré · Il apparaîtra dans la galerie, sous « Vos calques ». | the save dialog |
| Effect: {effect}, with its settings · Linked: it will follow the mask's changes. · {opacity} %, follows {source} | {effect}, avec ses réglages · Lié : il suivra les modifications du masque. · {opacity} %, suit {source} | *What is saved* |
| Your layers · Your masks | Vos calques · Vos masques | the gallery, the mask's `select` |
| Save as mask… · Edit keys… · Rename… · Delete | Enregistrer comme masque… · Modifier les touches… · Renommer… · Supprimer | the mask's ⋯ menu, a saved layer's |
| Used by {n} layers | Utilisé par {n} calques | the mask's ⋯ menu |
| Edit "{name}", used by {n} layers: · Update the mask | Modifier « {name} », utilisé par {n} calques : · Mettre à jour le masque | the selection bar on a saved mask |
| "{name}" deleted · {n} layers keep its keys | « {name} » supprimé · {n} calques gardent ses touches | under the mask's `select` |

**Blend sentences**:

| Mode | English | Français |
|---|---|---|
| Cover | Its colours replace what is under it. | Ses couleurs remplacent ce qui est dessous. |
| Add | Adds its light to what is under it. Black changes nothing. | Ajoute sa lumière à ce qui est dessous. Le noir ne change rien. |
| Screen | Brightens what is under it, softer than Add. Black changes nothing. | Éclaircit ce qui est dessous, plus doucement qu'Additionner. Le noir ne change rien. |
| Multiply | Tints or darkens what is under it. White changes nothing. | Teinte ou assombrit ce qui est dessous. Le blanc ne change rien. |

**Key groups**, named by what the keys are, not by their legends, since a group
is a physical position (`composition.md` §3.3):

| Group | English | Français |
|---|---|---|
| `escape` | Escape | Échap |
| `wasd` | Movement keys | Touches de déplacement |
| `arrows` | Arrow keys | Flèches |
| `functionRow` | Function keys | Touches de fonction |
| `numberRow` | Number row | Rangée des chiffres |
| `numpad` | Numeric keypad | Pavé numérique |
| `modifiers` | Shift, Ctrl, Alt and Windows | Maj, Ctrl, Alt et Windows |
| `navigation` | Navigation keys | Touches de navigation |
| `space` | Space bar | Barre d'espace |

**States**: *Nothing to light here* / *Rien à éclairer ici*; in the card, *This
mask lights nothing on this device: choose another.* / *Ce masque n'éclaire rien
sur cet appareil : choisissez-en un autre.*; *Stopped on an error* / *Arrêté sur
une erreur*.

## 8. Accessibility

Against WCAG 2.2 AA. The tokens pass for text: `--text-muted` 5.5:1 on
`--raised` in the dark theme, `--accent` 8.1:1 dark and 5.4:1 light. Borders do
not: `--line-strong` is 1.75:1 dark and 1.64:1 light, below the 3:1 a control
needs when its border is what identifies it.

| Requirement | Criterion |
|---|---|
| Reordering by buttons as well as by dragging; focus stays on the moved row | 2.1.1, 2.5.7 |
| Selected keys marked by the accent outline and a check, not by fading alone | 1.4.1, 1.4.11 |
| States in words, colour as a second channel | 1.4.1, 1.3.1 |
| Controls identified by their text or the accent, never by a `--line` border alone | 1.4.11 |
| Row tools, ⋯ buttons and segments at least 24 × 24 px, rows at least 32 px high; the follow switch raised from about 20 px to 24 | 2.5.8 |
| Selecting keys by click, one key at a time, with painting by drag only as a shortcut | 2.5.7 |
| *Undo* kept until the next change, not on a timer | 2.2.1 |
| *Apply* labelled by its outcome; selecting never writes | 2.4.6, 3.2.2 |
| The dialog takes focus to its name, keeps it inside, gives it back to the button that opened it | 2.4.3, 2.1.2 |

**Roles**: the stack a group named *Layers of {target}*, rows buttons with
`aria-pressed` and their position in their name (*Equalizer, number row, screen
100 %, layer 2 of 3*); the preview's state a `status`; blend a radio group
described by its sentence; opacity a native range; the mask a native `select`;
each ⋯ a button with `aria-haspopup="menu"`, `aria-expanded` and a name saying
whose actions they are (*Actions of the mask "Game keys"*), its list a `menu` of
`menuitem`s; the save dialog a modal `dialog` named by its title; the selection a
group of toggle buttons named by legend, its bar a toolbar with a live count.

**Keyboard**: Tab reaches each row, then its move and remove buttons; Enter or
Space selects. In the blend group, arrows change the mode. In a ⋯ menu, arrows
move, Enter runs, Escape closes and returns to the button. In the dialog, Enter
saves and Escape cancels. In the selection, arrows move between keys, Space
toggles, Enter is *Done*, Escape *Cancel*.

## 9. Requirements

**P0, step 2 does not ship without them**

1. **The stack under its target.** *Given* a controlled device with two layers,
   *when* it is selected, *then* its card lists *Add a layer*, the two layers top
   first, and the base, and no other card shows a stack.
2. **Selecting is a preview.** *Given* a layer selected, *when* an effect is
   picked in the gallery, *then* the preview shows it in that layer's place, the
   device's frames do not change, the state line reads *Preview · nothing is
   sent*, and the button reads *Apply to this layer*.
3. **Adding.** *Given* *Add a layer* selected and an effect picked, *when* *Add
   the layer* is pressed, *then* the layer runs on top with *screen* at 100 % and
   the chosen mask, is selected, and is in `settings.json` under `layers`.
4. **The card.** The base, a layer and a new layer each show §3's card; *Save
   the layer…* only on a layer that previews nothing; *View code*, *Duplicate*
   and *Save as preset…* in *Effect*; the action bar holds only the sending
   button and *Cancel preview*.
5. **Masks.** Every key group this layout has, with its count; regions on a whole
   device that declares them; saved masks lighting something here; *Choose
   keys…* with click, drag, keyboard and *Done* / *Cancel*; no group without a
   key here; *Done* refused at zero keys.
6. **Blend and opacity** live on the device, saved at once; opacity follows a
   value as brightness does.
7. **Order and removal**: *Move up* / *Move down* and drag; *Remove* with *Undo*
   restoring place, mask and settings; the base fixed at the bottom.
8. **Limits and states**: the eighth effect is the last (*Add a layer* becomes
   the sentence); failed, nothing to light and waiting rows as §6.
9. **Split devices**: a stack per selected region; regions offered as masks on a
   whole device that declares them.
10. **Card and tray** say *{effect} + {n} layers*; the tray changes the base only.
11. **Saving a layer**: the dialog lists the effect, the mask (with *Linked* for
    a saved one), the blend and the opacity with what it follows; the saved
    layer appears under *Your layers*; picking it previews it whole; adding it
    puts a copy; renaming or deleting it changes no layer.
12. **Saved masks**: chosen keys saved as a mask and the layer linked to it;
    *Edit keys…* changes every layer naming it, on every device; *Delete*
    leaves each of them its keys, with *Undo* bringing the link back; *Rename…*
    changes no link.
13. **`settings.json`**: `layers`, `savedLayers` and `savedMasks` as
    `composition.md` §3.11 and §5.5 write them, each check with its negative
    test, without hardware.
14. **Words in both catalogs**, §7's; the guards on catalog keys pass.
15. **Accessibility** as §8, each requirement with its test next to its
    component.

**P1, soon after**

- The movement keys named with the system layout's letters (§11, decision 2).
- A short sentence the first time a layer is added, explaining *screen*: only if
  using it shows people expect *cover*.

**P2, designed for, not built**

- Saved stacks (`composition.md` §4, idea 9): a *Save the stack* beside *Add a
  layer*, as a layer is saved.
- Overlay rules (step 3) shown in the stack, read-only, as *over Escape, while
  build is failed*.
- Simulated inputs under the preview (idea 8).

## 10. How it is checked

Candeo measures nothing about its users, so the design is checked by hand,
before the pull request merges, on the dev build:

| Task, from the devices column | Done when |
|---|---|
| Put red on the movement keys over the current effect | under 30 s, without help, nothing applied by mistake on the way |
| Make the number row follow the music over the effect | the right group found from its name |
| Put the red layer under the music layer | by buttons, then by dragging |
| Remove a layer, then bring it back | *Undo* found without looking for it |
| Save the red layer, then put it on another keyboard | the dialog read without hesitation on what it keeps |
| Save chosen keys as a mask, add a key to it, see both layers using it follow | from the ⋯ menu, without help |
| All of the above with the keyboard only | every step possible, focus always visible |
| The same in French, and in the light theme | no truncated label, contrast as §8 |

Each task is run by the maintainer and by someone who has not seen the design;
what fails comes back here before the code changes.

## 11. Decisions

Taken by the maintainer on 2026-11-06, after the two rounds on the prototype.

1. **The stack lives in the devices column** (§2), not in the detail panel as
   `composition.md` §3.10 had it.
2. **The movement keys keep their neutral name** in step 2. Adding the system
   layout's letters — *(ZQSD)*, *(WASD)*, read on Windows from the scancodes
   with `MapVirtualKeyEx` — waits for use to ask for it (P1).
3. ***base*** names the bottom row, the same word in both languages.
4. **A new layer covers the whole device** until a mask is chosen, as decision 5
   of `composition.md` has it; whether *Mask* should come first is judged in use.
5. **Painting by drag comes with step 2**, beside the click.
6. **The detail panel is a card per selected row** (§3), each *Save* inside what
   it keeps, one state line instead of explanations.
7. **Saved layers are copied, saved masks linked** (§5.1), both offered where
   they are used, under *Your layers* and *Your masks*, without a library
   screen.
