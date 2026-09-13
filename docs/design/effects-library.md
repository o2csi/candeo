# The effects library — one kind of effect

Status: **accepted**. Covers the redesign of the shipped effects and the identity
and format of #44, which turn out to be one project.

## Why

candeo ships five effects as JavaScript compiled into the binary, and treats
them as a second kind of effect. That kind costs a special case almost
everywhere:

- ids reserved at install, and a refusal when a name derives to one;
- deletion refused, and a "copy before editing" flow in the editor;
- manifests written twice, in Rust and in the module, kept equal by a test;
- swatches kept in memory instead of on disk;
- `EffectKind::Builtin` checks in storage, listing and the gallery.

Meanwhile #44 needs a stable identity and a single-file exchange format for
effects that come from elsewhere. A shipped effect is exactly that: an effect
that comes from elsewhere, the application being its author.

**Decision: every effect is an ordinary library effect, in the portable format.**
The application seeds its effects into the library like an import it performs
itself. The special kind disappears.

## 1. Identity: a `uid`, not a name

As decided in #44 §1:

- `id = derive_id(name)` stops being the identity. A stable `uid` is.
- Saving again or renaming keeps the `uid`; duplicating or "save as" creates a
  new one; importing a known `uid` offers an update.
- Remembered settings (`activeEffects`, `effectParams`) are indexed by `uid`, so
  renaming no longer orphans them.

**Where the `uid` lives: in the source**, `defineEffect({ uid, … })`, like every
other manifest field (#44 §2, "one file, one truth").

- A **shipped** effect carries a fixed `uid`, written once in its source: every
  installation recognizes it, which is what makes updates possible.
- A **user** effect gets one at its first save: the editor inserts
  `uid: '…'` into the source before installing. The author sees it and can
  keep it when sharing.
- Format: a lowercase UUID v4. It passes `validate_id` (`[a-z0-9-]`, at most
  64 characters) and contains no `:`, which the tray menu uses as a separator.

**The directory keeps a readable name** derived from the title, with a numeric
suffix on collision (#44). Nothing decides on it any more: the `uid` does.

**Hardware effects** (`wave`, `off`, defined in the front end) currently share
the id space: a user effect named "Wave" collides with them. They move to a
namespace of their own, `hardware:wave`, outside the library.

## 2. Format: one self-contained `.ts` file

Unchanged from #44 §2:

- no `manifest.json` shipped: the manifest is read from the source;
- no `.js` shipped: the bytes executed derive from the text that can be read;
- the local `manifest.json` and `swatch.json` stay, as **caches** written at
  install so that listing starts no engine.

New manifest fields, in `defineEffect({…})`: `uid`, `author` (free text, a
claim and never shown as verified), `version` (the effect's, not the API's).

**Every default must be a literal.** The manifest is read from the syntax tree,
without running the module. Three shipped effects declare defaults through
constants (`respiration`, `balayage`, `degrade-fixe`) and cannot be saved from
the editor today: they are rewritten with literal defaults.

## 3. Shipping the effects

The sources move to `packages/effects/*.ts`, next to the effects API. Adding
that folder to the `include` of `apps/desktop/tsconfig.json` makes `vue-tsc`
type-check them, as it already does `example.ts`.

**Compiling without a window.** The seeded effects must run at first launch,
with the window closed, so their JavaScript has to exist before the app
starts. Three ways:

| Option | Cost | Limit |
|---|---|---|
| **A. Generated `.js` committed next to each `.ts`**, produced by a `pnpm` script with the same `transpileModule` as the editor; the `web` CI job regenerates and fails on any difference | small; no new dependency | a generated file in the tree |
| B. Type stripping in Rust (`oxc`) | one heavy dependency | also enables importing without a window |
| C. Shipped sources restricted to TypeScript that is valid JavaScript | none | no types in the effects meant to be read and copied |

**Decision: A.** The CI check keeps the executed bytes equal to what a reviewer
reads, which is the principle of #44 §2. B stays open for the day import must
work without a window. The Rust `rust` CI job has no Node, which rules out
running `tsc` from `build.rs`.

Either way, the shipped sources and their JavaScript are **embedded in the
binary**: seeding happens at first launch, window closed and possibly offline.

## 4. Seeding and updates

At startup, for each shipped effect, keyed by `uid`:

| Library state | Action |
|---|---|
| Absent, never deleted | install it |
| Present, source equal to the version shipped last time | update it, **silently**, if the shipped version changed |
| Present, source modified by the user | keep the user's version; mark "update available" |
| Deleted by the user | leave it deleted |

To tell these apart, an installed effect records its **origin**: the shipped
`uid` and the hash of the source it was installed from. A deletion of a shipped
effect is remembered in `settings.json`, so it does not come back at the next
launch.

**No "Restore shipped effects" action.** candeo is open source: a deleted shipped
effect comes back by importing its file from the repository, once import exists
(#44 §3–4). Until then it stays deleted on that machine, which costs one effect,
never user work. The action is trivial to add later, since the sources are
embedded anyway; it is left out to keep the interface for what is frequent.

## 5. What disappears, what stays

Disappears:

- `builtins/mod.rs`, `EffectKind`, the reserved-id refusal, the refused
  deletion, the memory-only swatches, the duplicated Rust manifests;
- copy-on-open in the editor (`renameInSource` for built-ins, "copie de X");
- the `intégrés` / `à vous` split in the gallery.

Stays, for every effect:

- **Duplicate**: a new `uid`, the name suffixed, the source otherwise identical;
- delete, which stops the loops running it and forgets its settings;
- edit in place, since the source is the user's.

## 6. Names and labels in two languages

`AGENTS.md` currently says built-in names come from the i18n catalogs. With no
built-ins, that rule has nothing left to apply to. Instead, **manifest text
fields accept a string or a map of languages**:

```ts
name: { en: 'Radial wave', fr: 'Onde radiale' },
params: {
  speed: { kind: 'number', label: { en: 'Speed', fr: 'Vitesse' }, … },
}
```

A plain string stays valid for effects written in one language. The display
picks the current language, then English, then the first entry. Shipped effects
use maps; `AGENTS.md` changes accordingly once this is accepted.

## 7. Migration of existing installations

One pass at the first launch of the new version, idempotent:

1. **Shipped ids** map to their fixed `uid` through a table in the code
   (`onde-radiale`, `onde-matricielle`, `respiration`, `balayage`,
   `degrade-fixe`).
2. **User effects** in `effects/<id>/` get a `uid` minted and written into their
   `source.ts`; their directory is kept.
3. **`settings.json`**: every `activeEffects` and `effectParams` entry is
   rewritten from id to `uid`, and a schema version is recorded.
4. Front-end drafts stored under `candeo:brouillon:<id>` are renamed.

The migration is tested on a fixture of a real pre-migration data folder.

## 8. Out of scope here

- Import and export UI, and the edge cases of #44 §4 (newer `apiVersion`,
  out-of-bounds writes): they build on this identity and format.
- Device kinds and capabilities (#44 §5, #34).
- The effects reimplemented from the OpenRGB Effects Plugin: they are written
  directly in this format once it lands.

## Order of work

1. `uid` and origin in the manifest, settings keyed by `uid`, and the migration.
2. Shipped effects as `.ts` in `packages/effects`, generated JavaScript and its
   CI check; seeding and updates.
3. Remove the built-in special cases (Rust and front end); Duplicate.
4. Localized manifest text (with #73).
5. Import and export (#44 §3–4).

## Decisions taken on review

1. Compiling shipped effects: **A**, generated JavaScript committed and checked
   by CI.
2. Updating an unmodified shipped effect: **silent**.
3. No "Restore shipped effects" action for now: a deleted shipped effect comes
   back through import from the repository.
