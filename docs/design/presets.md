# Presets — named settings for one effect

An effect's settings are kept per device, and per region of a split device
([`regions.md`](regions.md) §3): the values someone set, and the parameters
reading a signal, the sound or the computer. Nothing lets someone keep two looks
of one effect — *slow blue*, *fast red* — and go back to one without moving
every slider again. This note settles **presets**, the first increment of #360;
composition, layers and masks are decided in [`composition.md`](composition.md).

Status: implemented.

## 1. What a preset is

| Term | What it is | Where it lives |
|---|---|---|
| **Effect** | a TypeScript file returning colours, which declares its parameters | the library's folders ([`effects-sources.md`](effects-sources.md)) |
| **Settings** | the values and bindings an effect runs with on one device or region | `effectParams`, `splitDevices[].effectParams` |
| **Preset** | named settings for one effect, offered on every device and region | `presets` |
| **Region** | a part of a device running its own effect | its definition, and `splitDevices` ([`regions.md`](regions.md)) |
| **Composition** | several effects layered over one device or region | not built: #360, second increment, decided in [`composition.md`](composition.md) |
| **Saved setup** | effects assigned to several devices at once | #367 |

A preset is data: no code, no new runtime, no new effect. **Picking one changes
the settings, never which effect runs.** Saving or picking a preset is not a
hardware write (#360): *Apply* stays the one gesture that sends an effect to a
device.

## 2. Where presets live, and why per effect

**One list at the root of `settings.json`, each preset naming its effect,
offered on every device and every region.**

- **A parameter is the effect's, not the device's.** The effect declares it, the
  same on every device; *slow blue* made on the keyboard is wanted on the lid
  region, and on the second keyboard. Tuning per device and per region exists
  already: it is the settings a preset fills (§5).
- **Not per device or per region.** A region's records go when its device is made
  whole ([`regions.md`](regions.md) §2): presets kept there would vanish with a
  split. A preset per device would also be copied once per device to be reused.
- **Not in the effect's file.** A built-in's file is rewritten by updates and not
  saved by Candeo; a preset is the person's, as a rule is.
- **Not in the window's storage.** Rust will read them for the tray and for rules
  (§7), and the window's storage is per window, not per user.

## 3. The shape

```json
"presets": [
  {
    "effect": "shipped:Radial wave",
    "name": "Slow blue",
    "values": { "speed": 0.4, "colour": { "r": 0, "g": 80, "b": 255 }, "rings": 3 },
    "bindings": { "hue": "sound:tone" }
  }
]
```

- **`values` holds every parameter the effect declared** when the preset was
  saved, where `effectParams` keeps only what departs from the declared values.
  A preset is a look someone saw and named: a later version of the effect
  changing a default must not change it.
- **`bindings`** as in `effectParams`: by parameter, absent when none.
- **In the order they were saved**, a new one last. Nothing reorders them.
- **Fields this version does not know are kept** and written back (#373), as a
  device entry's are.
- **No version bump.** The key is optional and needs no migration;
  `SETTINGS_VERSION` counts shapes that cannot be told from the content. A build
  released before 0.22.1 drops the key when it rewrites the file (#373), as it
  drops `splitDevices`: accepted, the presets are lost there and nothing else.

## 4. Names

- **Unique within an effect, ignoring case.** *Calm* and *calm* side by side
  would be one choice in a list. Two effects may each have a *Calm*.
- **Trimmed, 1 to 64 characters**, any text otherwise: a preset is not a file,
  so none of a file name's rules apply.
- **A preset is known by its effect and its name.** Nothing refers to one yet
  (§7), so it needs no identifier to survive a rename.

## 5. In *Effects*

A **Presets** block opens the settings column of a library effect that declares
parameters, above its settings: the effect's presets in their order, *Save as
preset*, and when there is none, a line saying so.

- **Save as preset** names the settings shown — values and bindings, those of
  the selected device or region. A name already taken for this effect is refused
  in one sentence.
- **Picking** a preset — clicking its name — is a gesture on the settings, as
  moving every slider at once: its values and bindings replace the effect's on
  the selected device or region, are saved there at once, go to the preview, and
  reach the device only when this effect already runs there, the rule every
  setting follows (#218). It neither starts, stops nor switches an effect.
- **The preset the settings hold is marked, not stored**: the one whose values and
  bindings, read against what the effect declares today, are the settings shown.
  Moving a slider unmarks it. A stored *preset in use* would be stale at the first
  slider.
- **Update** replaces a preset's values and bindings with the settings shown:
  offered on a preset the settings do not hold.
- **Rename**, in place: Enter keeps the name, Escape goes back.
- **Delete**, after a question in the preset's row: nothing brings a preset back.
- **Duplicate** is picking a preset, then saving under another name: no gesture
  of its own.
- **Without a controlled device** nothing can be picked: settings are kept per
  device or region. Saving still names the declared values shown.
- **While the preview has stopped on an error** picking is off, as the settings
  form is.
- **Not for a firmware effect**: its id is a firmware's, not the library's, and
  its settings are the colour or two sent with it. Not for an effect without
  parameters either.

## 6. Checks, and effects that change

- **Values are checked when a preset is picked**, against what the effect
  declares then, by the rule kept settings follow (`merge` in `useSettings`,
  `starting_params` in Rust): a value for a parameter no longer declared is
  dropped, one that no longer fits takes the default, and a parameter added since
  takes its default. A binding on a parameter no longer declared, or a sound on a
  parameter that cannot follow one, is dropped (`declaredBindings`).
- **Rust checks on saving** the effect's key (a library effect), the name, and the
  bindings (`checked_bindings`). Values stay raw JSON, as `effectParams`'s do: the
  Rust side does not interpret them.
- **An effect deleted takes its presets with it** (`forget_effect`): the file holds
  no key the library does not know. **Renamed**, its presets follow it
  (`rename_effects`), migrations included; a name that would then appear twice
  for the effect takes a number, *Calm (2)*.
- **An effect gone from the folder keeps its presets**, as it keeps its settings:
  putting the file back finds them, and *Forget its settings* forgets them too.
- **Resetting the configuration deletes them**, as it deletes rules: they live in
  `settings.json`, and the confirmation says so.

## 7. Later

- **A rule's *show*.** A rule carries its own values and bindings (§3.2 of
  [`inputs-and-automations.md`](inputs-and-automations.md)). Picking a preset
  there would copy them into the rule, with no reference: a preset renamed or
  deleted would leave the rule as it is. A change to the rule's settings form.
- **The tray.** A preset under each effect of a device's submenu needs Rust to
  pick it — `starting_params` over the preset's values, written to the device's
  settings — and menu ids naming it.
- **Import and export**, as a file carrying its own version. Never code.
- **An agent** (#359, #365) choosing an effect and one of its presets as data: the
  commands below are that contract.
- **A layer** of a composition (#360) naming an effect and a preset.

## 8. Commands

Each answers with the effect's presets after the change, in their order, so the
window needs no second read. `effect` is a library effect's key.

| Command | Does | Refuses |
|---|---|---|
| `save_preset(effect, name, values, bindings)` | adds a preset, last | `presetNameEmpty`, `presetNameTooLong { max }`, `presetExists { name }`, `bindingInvalid` |
| `update_preset(effect, name, values, bindings)` | replaces its values and bindings, in place | `presetNotFound { name }`, `bindingInvalid` |
| `rename_preset(effect, from, to)` | renames it, in place; a change of case alone is a rename | `presetNotFound`, `presetExists`, and the name's refusals |
| `delete_preset(effect, name)` | deletes it; one already gone is no error | — |

`Settings` gains `presets?: PresetRecord[]`, absent while there is none:
`PresetRecord { effect, name, values, bindings? }`. Picking a preset needs no
command: the window writes the settings with `remember_effect_params` and
`remember_effect_bindings`, as *Restore the effect's values* does.

## 9. Words

***Preset*** / ***préréglage***. *Save as preset* / *Enregistrer comme
préréglage*; *Update* / *Mettre à jour*; *Rename* / *Renommer*; *Delete* /
*Supprimer*. Picking a preset is not *applying* one: *Apply* / *Appliquer* is
what sends an effect to a device, and a preset's name says what clicking it does
in its tooltip, *Use these settings* / *Reprendre ces réglages*.
