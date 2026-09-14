# The effects library — effects are files

Status: **proposed**. Replaces the uid-based version accepted in #83, which was
set aside during implementation review for a simpler model. Covers the shipped
effects and the identity and format of #44.

## Why

candeo ships five effects as JavaScript compiled into the binary, and treats
them as a second kind of effect. That kind costs a special case almost
everywhere: reserved ids, refused deletion, copy-before-editing, manifests
written twice, swatches kept in memory, `EffectKind::Builtin` checks in storage,
listing and the gallery.

Meanwhile #44 needs an identity and a single-file format for effects that come
from elsewhere. The first design answered with a generated uid written into the
source. In review it proved heavier than the need: a uid the user can edit is
a way to overwrite another effect by pasting its code, and it adds a second
identity next to a name that is already unique.

**Decision: an effect is a `.ts` file in the effects folder, and its file name
is its name.** candeo is a tool for power users: listing a folder is the whole
library, adding an effect is saving a file there, and naming it is renaming
the file.

## 1. Identity: the file name

```text
app_data_dir()/effects/
  Radial wave.ts
  Breathing.ts
  My effect.ts
```

- The name shown everywhere is the file name without `.ts`. Sources no longer
  declare `name`.
- It is the key of everything that refers to an effect: `activeEffects` and
  `effectParams` in `settings.json`, the engine, the tray menu, editor drafts.
- **Allowed names** follow the Windows rules on every system, so a folder copied
  between machines means the same thing: no `< > : " / \ | ? *` or control
  characters, no leading or trailing space or dot, not a reserved device name
  (`CON`, `NUL`, `COM1`…), at most 64 characters. Two names that differ only by
  case are refused.
- **Renaming from the app** renames the file and moves its settings and draft.
  **Renaming outside the app** makes a new effect: the settings of the old name
  stay in `settings.json`, unused. See §5.
- **Hardware effects** (`wave`, `off`, `spectrumCycle`, defined in the front
  end) move to ids with a prefix no file name can hold, `hardware:wave`, so a
  file named `wave.ts` cannot collide with them. The tray menu reads an effect
  id as everything after the product id, so the `:` does not break it.

## 2. Format: one self-contained `.ts` file

```ts
import { defineEffect, hsv } from '@candeo/effects-api'

export default defineEffect({
  description: { en: 'A hue wave spreading in circles', fr: 'Une onde de teinte en cercles' },
  kinds: ['keyboard'],
  params: {
    speed: { kind: 'number', label: { en: 'Speed', fr: 'Vitesse' }, min: 0, max: 400, default: 120 },
  },
  render({ layout, time, frame, params }) { … },
})
```

- **Text fields** — `description` and parameter `label` — accept a string or a
  map of languages. The display picks the current language, then English, then
  the first entry. The name is not translated: it is a file name.
- **`kinds`** declares the device kinds the effect is meant for, as decided in
  #44 §5 and `device-sdk.md` §2 (`['keyboard']`, `'all'`…). Shipped effects
  declare it now. Only keyboards exist today, so a missing `kinds` is read as
  `['keyboard']`; the obligation and the gallery filter come with the second
  kind.
- `author` and `version` stay as in #44 §2, optional.
- `name` in an existing source is **ignored**. `EffectModule` keeps it as a
  deprecated optional property, so an old source still type-checks and the
  editor strikes it through instead of refusing to save.
- Defaults no longer need to be literals: the manifest is read by running the
  module (§3), not from the syntax tree.

## 3. Compiling and the cache

The Rust side cannot strip TypeScript types, and the webview already can: the
editor loads the TypeScript compiler for Monaco. The main window is created at
startup and closing it only hides it, so a webview is always there while candeo
runs, tray-only use included.

1. **Rust lists the folder**: for each `.ts`, its name, the SHA-256 of its bytes,
   and whether the cache holds a result for that hash.
2. **The webview compiles what is stale**, with `transpileModule`, at startup
   and on **Refresh** in the gallery. The compiler is loaded only when at
   least one file is stale.
3. **Rust receives the JavaScript**, loads it once in QuickJS with the time
   budget swatch sampling already uses, reads the manifest the module declares
   (`__candeo_manifest`), checks `apiVersion`, samples the swatch, and writes
   `app_cache_dir()/effects/<name>.json`: hash, JavaScript, manifest, swatch.
4. A file that fails to compile or load is cached **with its error**, for that
   hash: it is listed with an error state, opens in the editor, and is not
   retried until it changes.

The effects folder holds only the files people write. The engine and the tray
run an effect only when its cache matches the file's current hash, so they
never run code that differs from the file on disk. A file dropped in while
candeo runs appears after Refresh; watching the folder can come later.

What goes away: the manifest reader on the syntax tree (`editor/effect.ts`), the
`source.ts` / `effect.js` / `manifest.json` / `swatch.json` directory per
effect, and option A of the first design (generated JavaScript committed and
checked by CI).

## 4. Shipped effects

The sources move to `packages/effects/<Name>.ts`, type-checked by `vue-tsc`, and
are embedded in the binary. They carry **no type annotations**: `defineEffect`
already infers what `render` receives, and the Rust tests run them as they are,
without a TypeScript compiler, to check the waves' geometry and the swatches. A
test fails the day one of them would need its types stripped. At startup they are copied into the effects folder
**once**, and `settings.json` records what was copied:
`shippedEffects: { "Radial wave": "<sha-256>" }`.

Shipped effects are named in English: a name is a file name and is not
translated, and English is the reference language of the interface.

| State at startup | Action |
|---|---|
| Not recorded, no file with that name | copy it, record its hash |
| Not recorded, a file with that name exists | leave the file; record it as not ours |
| Recorded, file unchanged, shipped version changed | overwrite it **silently**, record the new hash |
| Recorded, file modified | leave it |
| Recorded, file missing (deleted or renamed) | leave it; never copied again |

A deleted shipped effect comes back by downloading its file from the repository
into the folder. The gallery's **Built-in** section lists the files whose name
is recorded as shipped, modified or not; a renamed one becomes the user's.

## 5. Library actions

- **Refresh**, and **Open folder**, in the effects column.
- **Rename** from the editor header: renames the file, moves its settings and
  its draft. The name is edited there only; sources have no `name`.
- **Duplicate**: a copy named with a copy suffix in the interface language —
  `<name> (copie)`, `(copie 2)`… while the interface is French — ready at once,
  since its cache is copied too, and the user's even when the original was
  shipped.
- **Delete**: stops the loops running it, removes the file and its cache,
  forgets its settings.
- **Missing effect**: when `activeEffects` or `effectParams` name an effect the
  folder no longer holds, the gallery says so once — "Radial wave" is no
  longer in the folder — with **Forget its settings**. Nothing is purged
  automatically: putting the file back under that name restores everything.

## 6. Migration from the directory layout

One pass at the first launch of the new version, idempotent, recorded as
`version: 1` in `settings.json`:

1. Each `effects/<id>/` becomes `effects/<name>.ts`, `<name>` being the
   manifest's name made valid (forbidden characters replaced by `-`, a suffix
   ` (2)` on collision). The directory is removed once the file is written.
2. `activeEffects` and `effectParams` are rewritten from old ids to names:
   installed effects through step 1, shipped effects through a table
   (`onde-radiale` → `Radial wave`, `onde-matricielle` → `Diagonal wave`,
   `respiration` → `Breathing`, `balayage` → `Sweep`, `degrade-fixe` →
   `Fixed gradient`).
3. Shipped effects are then copied as in §4.
4. Drafts stored under `candeo:brouillon:<id>` are renamed when the editor
   opens, from the same table.

Tested on a fixture shaped like a data folder of the directory layout.

## 7. What disappears, what stays

Disappears: `builtins/mod.rs`, reserved ids, the refused deletion, memory-only
swatches, the Rust manifest copies and their test, copy-on-open in the editor,
`derive_id`, per-effect directories.

`EffectKind` stays, with another meaning: `builtin` marks a file recorded as
shipped, which is what the gallery's Built-in section lists. It decides nothing
else.

Stays: one engine, one API, the swatch sampled by running the effect, per-device
settings, preview versus Apply.

## 8. Out of scope

- Watching the folder for changes.
- An "update available" mark for modified shipped effects.
- The `kinds` filter and obligation (with the second device kind, #34).
- The OpenRGB Effects Plugin reimplementations: written directly in this format
  once it lands.

## Order of work

1. File-named effects, the compile pass and cache, Refresh, the migration,
   settings keyed by name.
2. Shipped effects as `packages/effects/*.ts`, seeding; removal of the built-in
   special cases.
3. Rename, Duplicate, Open folder, the missing-effect notice.
4. Localized `description` and `label` (with #73).
