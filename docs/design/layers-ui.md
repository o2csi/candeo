# Layers in the window — the interface

The interface of layers, step 2 of [`composition.md`](composition.md) §5.1:
where the stack is shown, how a layer is added, masked, blended, ordered and
removed, the words, what accessibility requires, and what step 2 must pass. The
model, the engine and `settings.json` are settled in `composition.md`; this note
changes none of them. It replaces `composition.md` §3.10's placement of the
stack, for the reasons of §2.

Status: proposed on 2026-11-06, for the maintainer's review before step 2 is
built. A clickable prototype went with the review; §9 lists what it leaves
open.

## 1. How it was worked out

- **A critique of the *Effects* view as it is** (2026-11-06, the dev build on
  the DeathStalker V2 Pro and the m18 R1's zones split into regions), and of
  §3.10 against it: §2.
- **An accessibility review** against WCAG 2.2 AA, with the contrast of the
  tokens measured (`styles/tokens.css`): §6.
- **The words**, English and French, checked against the catalogs' existing
  terms and AGENTS.md's rules: §5.
- **The components**, reusing what exists before adding anything: §4.
- **The requirements and their acceptance**, P0 to P2: §7.

No telemetry exists or will: §8 says how the design is checked instead.

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
3. **The detail panel already runs long**: title, three lines of notes, preview,
   presets, settings, actions. The layer's own controls (mask, blend, opacity)
   belong there; the list of layers does not.

The stack shows only for the selected target, as the brightness block does: one
open card at a time keeps the column short.

## 3. The flows

### 3.1 Add a layer — *WASD in red over my effect*

1. The device is selected; its card lists *+ Add a layer* above the base.
2. *Add a layer* is selected: it reads *New layer — choose an effect*; the line
   above the title says *DeathStalker V2 Pro · New layer*; *Apply* becomes *Add
   the layer*, disabled.
3. *Fixed colour* in the gallery: the preview shows it over the stack, on the
   whole device, with *screen* at 100 % (decision 5). *Add the layer* is enabled.
4. The *Layer* section, under the preview: *Mask* → *Movement keys · 4 keys*,
   *Blend* → *Cover*. The preview follows at once; nothing is sent.
5. *Add the layer*: it runs on the device, on top, selected; the card's line
   reads *Diagonal wave + 1 layer*.

Choosing the effect before or after the mask both work: the new layer's look is
kept while the gallery is browsed.

### 3.2 Change what a layer runs

Select its row, pick another effect: the preview shows it in that row's place,
the rest of the stack around it. *Apply to this layer* sends it; the mask, blend
and opacity stay. Selecting never writes, as today.

### 3.3 The mask

A native `select`, grouped:

- *The whole device* (*The whole region* in a region's stack);
- **Key groups** this layout has, each with its count: *Number row · 13 keys*.
  A group with no key here is absent, not disabled;
- **Regions**, on a whole device whose definition declares them: *Lid · 3
  zones*;
- the custom selection when there is one: *5 keys*;
- *Choose keys…* (*Choose zones…* on a zones device).

**Choose keys** turns the preview into a selection:

- a bar above it: the hint, the live count, *Select none*, *Cancel*, *Done*;
- a click toggles one key; a drag paints from the first key's new state;
- selected keys keep their colour, with the accent outline and a check; the
  others fade;
- arrows move between keys, Space toggles, Enter is *Done*, Escape *Cancel*;
- *Done* is disabled at zero keys: *Select at least one key*;
- the rest of the panel is inert until the selection ends.

Out of selection, the selected layer's keys are outlined in dashes over the
final frame, and a line under the preview says *Mask: Movement keys · 4 of 106
keys*. *Only this layer* draws that layer alone over black, in the preview only.

### 3.4 Blend and opacity

- **Blend**: four segments, *Cover*, *Add*, *Screen*, *Multiply*, the selected
  one's sentence under them (§5).
- **Opacity**: a slider with its value, and what it follows, with the brightness
  follow's *Value / Signal / Sound / System* switch.
- Live on the device when the layer runs there, saved at once (#218).

### 3.5 Order and removal

- **Move up / Move down** on each layer row, with the rules' grip for dragging.
  The base never moves; *Add a layer* stays on top.
- **Remove** takes the row away at once and leaves *Layer removed · Undo* in its
  place until the next change to that stack. Undo puts it back with its place,
  mask and settings. No confirmation: nothing is deleted from the library.
- A live region says *Layer moved to position 2 of 3*, *Layer removed*.

### 3.6 The base

The bottom row, tagged *base*, is the applied effect as today. Selecting it
targets it; *Apply* keeps its label. A **hardware effect** applied there sets the
layers aside (`composition.md` §3.2): the rows stay, dimmed, under *A hardware
effect runs: the layers wait for one of Candeo's effects*.

### 3.7 Split devices

Each region's card carries its own stack when selected; the split device's
header carries none. A whole device whose definition has regions offers them as
masks (§3.3). The two ways of `composition.md` §3.5, as the prototype's third
board shows.

### 3.8 Elsewhere

- **The device's card and the tray** say *Diagonal wave + 2 layers*. The tray
  changes the base and keeps the layers (decision 9).
- **The copied diagnostic** lists the layers, as step 1 made it.
- **Automations** is step 3's.

## 4. Components

No new token. Everything reuses `tokens.css`; the RGB spectrum stays the
keyboard's.

| Component | New or reused | What it is |
|---|---|---|
| `LayerStack` | new | the rows under the selected target's card; group named *Layers of {target}* |
| `LayerRow` | new | swatch, effect, line `{mask} · {blend} {opacity}`; a button with `aria-pressed`, as a region; Move up/down and Remove, 24 × 24, shown on hover and focus, always in the DOM |
| `LayerSection` | new | mask `select`, blend radio group, opacity slider and follow, under the preview, above Presets; absent for the base |
| `ReorderControls` | extracted | the rules' grip and *Move up* / *Move down*, shared by `AutomationsView` and `LayerStack` |
| `BrightnessFollow` | reused | gains `subject` (`brightness` or `opacity`) for its labels |
| `KeyboardSimulator` | extended | `selecting` (positions, or `null`), emits `toggle` and `paint`; keys become toggle buttons named by legend |
| Region card, `EffectPresets`, `EffectParamsForm` | reused | unchanged; the last two receive the selected layer's values |

**`LayerRow` states**, each said in words, colour as a second channel:

| State | Line | Look |
|---|---|---|
| default | `{mask} · {blend} {opacity}` | plain |
| selected | same | accent frame and `--accent-soft`, as a selected region |
| failed | *Stopped on an error* | `--bad` |
| mask lights nothing here | *Nothing to light here* | `--warn`, swatch hatched |
| waiting for a library effect | the sentence of §3.6 under the rows | rows at 60 % |
| removed | *Layer removed · Undo* | `--raised-2` |
| base | *base* | no tools |
| *Add a layer* | — | dashed `--line-strong` border, muted text |
| *Add a layer*, selected | *New layer · {effect}* | accent frame |
| *Add a layer* at the limit | *Eight effects at most, the base included* | plain text, not a button |

## 5. Words

The settled terms stay: *layer* / *calque*, *mask* / *masque*, *Add a layer* /
*Ajouter un calque*, *Cover* / *Couvrir*, *Add* / *Additionner*, *Screen* /
*Éclaircir*, *Multiply* / *Teinter* (decision 10). New ones:

| English | Français | Why |
|---|---|---|
| base | base | one word in both; *Background* / *Fond* clashes with effects' own *background* colour, *Applied effect* with the button's *Applied* |
| Apply to this layer | Appliquer à ce calque | *Apply* labelled by its outcome |
| Add the layer | Ajouter le calque | the new layer's *Apply* |
| New layer | Nouveau calque | the target before it exists |
| Layer · Blend · Opacity | Calque · Mélange · Opacité | the section and its labels |
| The whole device / region | Tout l'appareil / Toute la région | mask |
| Choose keys… / Choose zones… | Choisir les touches… / Choisir les zones… | mask |
| Only this layer | Ce calque seul | preview |
| Remove · Layer removed · Undo | Retirer · Calque retiré · Annuler | *Delete* / *Supprimer* is for files and rules; nothing is deleted |
| Done · Cancel · Select none | Valider · Annuler · Tout désélectionner | selection bar |
| {effect} + {n} layers | {effect} + {n} calques | card and tray |

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

*Movement keys* may add the four letters of the system's keyboard layout —
*(ZQSD)* on AZERTY, *(WASD)* on QWERTY — if §9's question 2 says so. The
interface language does not decide them: a French interface on a QWERTY
keyboard would otherwise say ZQSD over W, A, S and D.

**States**: *Nothing to light here* / *Rien à éclairer ici*; in the panel, *This
mask lights nothing on this device: choose another.* / *Ce masque n'éclaire rien
sur cet appareil : choisissez-en un autre.*; *Stopped on an error* / *Arrêté sur
une erreur*.

## 6. Accessibility

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
| Row tools and segments at least 24 × 24 px, rows at least 32 px high; the follow switch raised from about 20 px to 24 | 2.5.8 |
| Selecting keys by click, one key at a time, with painting by drag only as a shortcut | 2.5.7 |
| *Undo* kept until the next change to the stack, not on a timer | 2.2.1 |
| *Apply* labelled by its outcome; selecting never writes | 2.4.6, 3.2.2 |

**Roles**: the stack a group named *Layers of {target}*, rows buttons with
`aria-pressed` and their position in their name (*Equalizer, number row, screen
100 %, layer 2 of 3*); blend a radio group described by its sentence; opacity a
native range; the mask a native `select`; the selection a group of toggle
buttons named by legend, its bar a toolbar with a live count.

**Keyboard**: Tab reaches each row, then its move and remove buttons; Enter or
Space selects. In the blend group, arrows change the mode. In the selection,
arrows move between keys, Space toggles, Enter is *Done*, Escape *Cancel*.

## 7. Requirements

**P0, step 2 does not ship without them**

1. **The stack under its target.** *Given* a controlled device with two layers,
   *when* it is selected, *then* its card lists *Add a layer*, the two layers top
   first, and the base, and no other card shows a stack.
2. **Selecting is a preview.** *Given* a layer selected, *when* an effect is
   picked in the gallery, *then* the preview shows it in that layer's place, the
   device's frames do not change, and the button reads *Apply to this layer*.
3. **Adding.** *Given* *Add a layer* selected and an effect picked, *when* *Add
   the layer* is pressed, *then* the layer runs on top with *screen* at 100 % and
   the chosen mask, is selected, and is in `settings.json` under `layers`.
4. **Masks.** Every key group this layout has, with its count; regions on a whole
   device that declares them; *Choose keys…* with click, keyboard and *Done* /
   *Cancel*; no group without a key here; *Done* refused at zero keys.
5. **Blend and opacity** live on the device, saved at once; opacity follows a
   value as brightness does.
6. **Order and removal**: *Move up* / *Move down* and drag; *Remove* with *Undo*
   restoring place, mask and settings; the base fixed at the bottom.
7. **Limits and states**: the eighth effect is the last (*Add a layer* becomes
   the sentence); failed, nothing to light and waiting rows as §4.
8. **Split devices**: a stack per selected region; regions offered as masks on a
   whole device that declares them.
9. **Card and tray** say *{effect} + {n} layers*; the tray changes the base only.
10. **Words in both catalogs**, §5's; the guards on catalog keys pass.
11. **Accessibility** as §6, each requirement with its test in
    `LayerStack.test.ts`, `LayerSection.test.ts` and `KeyboardSimulator.test.ts`.

**P1, soon after**

- Painting keys by drag in the selection, if step 2 ships with click only.
- The movement keys named with the system layout's letters (§9, question 2).
- A short sentence the first time a layer is added, explaining *screen*: only if
  the first moderated check shows people expect *cover*.

**P2, designed for, not built**

- Saved stacks (`composition.md` §4, idea 9): a *Save the stack* beside *Add a
  layer*, as presets are saved.
- Overlay rules (step 3) shown in the stack, read-only, as *over Escape, while
  build is failed*.
- Simulated inputs under the preview (idea 8).

## 8. How it is checked

Candeo measures nothing about its users, so the design is checked by hand,
before the pull request merges, on the dev build:

| Task, from the devices column | Done when |
|---|---|
| Put red on the movement keys over the current effect | under 30 s, without help, nothing applied by mistake on the way |
| Make the number row follow the music over the effect | the right group found from its name |
| Put the red layer under the music layer | by buttons, then by dragging |
| Remove a layer, then bring it back | *Undo* found without looking for it |
| All of the above with the keyboard only | every step possible, focus always visible |
| The same in French, and in the light theme | no truncated label, contrast as §6 |

Each task is run by the maintainer and by someone who has not seen the design;
what fails comes back here before the code changes.

## 9. Open questions

1. **The stack in the devices column** (§2), rather than in the detail panel as
   `composition.md` §3.10 had it. *Maintainer.*
2. **Naming the movement keys by the system layout** — *(ZQSD)*, *(WASD)* — read
   locally on Windows from the scancodes (`MapVirtualKeyEx`), nothing on other
   systems; or the neutral name alone. *Maintainer, engineering.*
3. ***base* for the bottom row**, the same word in both languages. *Maintainer.*
4. **A new layer covers the whole device** until a mask is chosen: right for a
   layer chosen by its effect, wrong for *WASD in red*, which then shows red
   everywhere for a moment in the preview. The alternative opens *Mask* first.
   *Maintainer, after the moderated check.*
5. **Painting by drag in step 2**, or click only with drag in a follow-up.
   *Engineering.*
