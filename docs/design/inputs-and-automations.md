# Inputs and automations

Lighting that reacts to more than key presses — the time, the music playing,
signals sent by other software — and rules that interrupt the configured effect
for a while. Status: steps 1, 2, 3 and 6 of §4 are implemented (#102, #105,
#106, #179); sound (#107) and external signals (#108) are proposed. Each part
lands in its own pull request, in the order at the end.

## Why this needs a design

Two different things are asked, and they must not be confused:

- **Inputs** feed an effect while it runs: a spectrum drawn from the music, the
  hour drawn on the number row. The effect stays the one applied; it only sees
  more than `time`.
- **Automations** change *which* effect runs: at every full hour, show the clock
  for ten seconds; when the doorbell rings, flash the keyboard. They interrupt
  the configured effect and give it back.

Both would otherwise grow one feature at a time, each with its own capture, its
own settings and its own idea of what the device is doing. This document fixes
the shared shapes first.

## 1. Inputs: the shape key presses already set

`docs/design/key-input.md` settled the pattern, and every input reuses it:

| Rule | For every input |
|---|---|
| Declared | an effect lists what it reads: `inputs: ['keys', 'clock', 'audio']` |
| Captured only when needed | a source runs while at least one loop — a device's or the preview — runs an effect declaring it; the last guard dropped stops it |
| Compact, per frame | the effect receives a small snapshot with each frame, next to `time`, never a stream it has to buffer |
| Same in the preview | the preview loop receives the same snapshot, so the simulator shows what the keyboard shows |
| Nothing kept, nothing written | only what the next frame needs stays in memory; values never reach the log or the diagnostic, only when a source starts and stops |
| Visible | the gallery marks an effect by what it reads ("reacts to key presses", "reacts to sound"…) |
| Testable without hardware | each source sits behind a channel, so engine tests inject values |

An effect that does not declare an input receives an empty or neutral value, and
nothing is captured on its behalf. The swatch is sampled with neutral inputs, so
a reactive effect draws something at rest.

The effect API version stays 1 until the first release (`key-input.md` §1).

## 2. Sources

### 2.1 Clock

**Shipped** (#105): the input and the *Clock* effect are in the application; the
rules that show it on the hour are §3.

```ts
render({ clock }) // { year, month, day, weekday, hours, minutes, seconds, ms }
```

The local wall-clock time, read once per frame. `time` stays the seconds since
the effect started; `clock` is what a clock face needs. Nothing to capture and
nothing private, so no guard: every effect declaring `clock` gets it.

**How it is handed over.** The host reads the instant — milliseconds since the
epoch, `std::time::SystemTime` and no date crate — and passes it with the frame;
the split into fields happens in the bootstrap, where `Date` already knows the
machine's time zone. So a test renders any instant it likes, the device loop and
the preview see the same one, and an effect declaring nothing is given zeros —
which is also how a swatch is sampled.

**Showing the hour** takes two pieces:

- **The effect** draws the time. It is an ordinary effect with
  `inputs: ['clock']`: applied by hand, it shows the time all along. The shipped
  *Clock* reads the keyboard as a small display — each key a pixel, digits in a
  3×5 font on the five top rows, function keys included, the space bar's row
  left dark — and scrolls the hour across it, in physical key units so a digit
  keeps its width over the staggered rows.
- **When and for how long** is an **automation** (§3): `0 * * * *` *for 10
  seconds* shows the clock on the hour and gives the keyboard back; `* * * * * *`
  *for 1 second* never gives it back, which is the continuous display again, on
  top of whatever is applied.

Later, if asked: sunrise and sunset, which need a location.

### 2.2 Sound playing

```ts
render({ audio }) // { level, peak, bands: number[16], beat }
```

- **Source**: what the computer plays, not the microphone.
  - On Windows, WASAPI loopback on the default output device, through `cpal`
    (or the `wasapi` crate if loopback turns out incomplete there).
  - On Linux, the monitor source of the default sink, through PipeWire or
    PulseAudio.
- **Analysis in Rust, once per frame**, whatever the number of effects reading it:
  - an FFT (`rustfft`) over the latest window of about 40 ms;
  - `level` and `peak` in 0..1;
  - 16 logarithmically spaced `bands`, normalized in 0..1 with a short decay so
    bars do not flicker;
  - `beat`, true on the frame an onset is detected (spectral flux over a moving
    threshold).
- **Privacy**: samples never leave the analysis; only these numbers reach the
  effect. Nothing is recorded, and the log says only when capture starts and
  stops.
- **Silence** is `level: 0` and flat bands, the same as no capture: an effect
  cannot tell "nothing plays" from "capture unavailable", and does not need to.
  The gallery says when capture failed.
- **Shipped effects**: a spectrum across the columns, and a pulse on the beat.

The microphone is the same pipeline on an input device; it comes only if an
effect needs it (a "microphone muted" light is better served by signals, §2.3).

### 2.3 External signals

Named values that other software sends to Candeo:

```text
POST http://127.0.0.1:<port>/signals   { "doorbell": "ring", "ci": "failed", "volume": 0.4 }
```

They reach an effect **through its parameters**, not as a bag of names it has to
read (§2.3.1):

```text
Fixed gradient · colour ← signal "status"   ·   Ripples · speed ← signal "volume"
```

**Why it matters most**: once other software can push values, every integration
Candeo does not write becomes a script — Home Assistant, a mute state, a
recording state, a machine joining the network.

**The sender says *what*, Candeo says *where*.** A request names values and
nothing else: no device, no zone, no effect, no colour. Five reasons, and they
all point the same way.

- A signal is **a fact about the world**, not a lighting instruction. The sender
  knows the fact; it knows nothing of which keyboard is plugged in, which effect
  is running or which zone is free. Home Assistant has no idea what `1532:658`
  is, and must not have to learn.
- **The mapping from fact to light already exists**: it is the rule (§3), which
  names the devices, the effect, its settings, the duration and the priority. A
  sender choosing the device would put the decision in two places, and the rule
  list would stop being the single readable answer to "why is my keyboard red".
- It **survives the hardware**. Unplug one keyboard, buy another: the sender is
  unchanged. A rule naming a device that is not there simply does not fire.
- The **blast radius of the token** is not the same. A token that protects
  "set a named value" is not a token that protects "drive my hardware from the
  network" — which would also mean validating effect names, parameters and
  geometry over HTTP, the whole gallery exposed.
- A signal naming a device would **bypass the resolver**, and with it the
  priority between rules that §3.1 exists to keep.

Whoever wants direct control is served by §2.3.1, without giving any of this up:
a parameter of the running effect reads the value, and the person chose which
effect and which device. The sender supplies the value, the person supplies the
place.

- **What a request may carry**:
  - Strings, numbers and booleans, **flat**. No objects, no arrays: a rule
    compares one equality and an effect reads one scalar; nested, both would
    need a path language nobody asked for.
  - Bounds, so a broken sender cannot grow the process: at most 64 signals, a
    name of at most 64 characters, a string value of at most 256.
  - **Nothing is whitelisted.** A signal no rule and no effect mentions lights
    nothing — and still shows in the panel below. That is the discovery loop:
    send it, see it, then write the rule.
- **A signal's lifetime**: 60 s by default, set per send, `0` meaning "until
  erased"; an empty value erases it. The default protects against a sender that
  dies, which is right for a watcher that re-sends in a loop anyway, and wrong
  on its own for a state pushed once — `build=failed` must not go out by itself
  while the build is still broken.
- **The local API**:
  - **Off by default**, enabled in Settings, and on loopback when it is.
  - One endpoint, `POST /signals`, taking a flat object. No path naming a
    device.
  - Every request carries a token, shown in Settings with a button to generate
    a new one.
  - **Listening on the local network is the main path, not an afterthought**:
    Home Assistant runs on another machine. It stays a separate switch, and on
    Windows the inbound rule is scoped to the **Private** profile, so a public
    network refuses the port without anyone remembering to turn the switch off —
    which matters on a laptop.
- **Seeing what arrived**: Settings lists the signals held right now — name,
  value, time left — and sends a test one. Without it, a rule names a signal
  nobody can see, and nothing is debuggable.
- **Also from the command line**: `candeo signal ci=failed`, which talks to the
  running instance through the single-instance channel, for scripts that would
  rather not handle HTTP. It needs no port, no token and no firewall rule, and
  **it covers every sender running on this machine** — which is most of them.
  A hosted CI runner is not one of them: it cannot reach this loopback, so the
  sender there is a local watcher, not the job.
- **Home Assistant**: the HTTP API is enough for its `rest_command`. MQTT, with
  Candeo as a client of the broker Home Assistant already runs, needs no inbound
  port and could expose Candeo as an entity. It is a later step, if HTTP proves
  awkward there.
- Signals are also **automation triggers** (§3): "when `doorbell` becomes `ring`".

**Three pull requests, in this order.** The command line first: it makes signals
work end to end with no port open, and it answers "a long job has finished"
straight away. Then the HTTP API, the token and the Settings block, which is
what Home Assistant on another machine needs. Then the parameter bindings of
§2.3.1.

#### 2.3.1 How a signal reaches an effect: it binds to a parameter

**Signals are the one source that does not follow §1**, and this is why: an
effect declares nothing, reads nothing and is not marked in the gallery.

**The arrow points from the effect to the signal.** A signal does not find an
effect to drive; a **parameter of an effect names the signal it reads**. Instead
of a fixed value, a parameter is bound: *Fixed gradient*, whose colour reads
`status`; *Ripples*, whose speed reads `volume`.

A first version of this section gave the effect the whole bag —
`inputs: ['signals']`, `render({ signals })` — and it was wrong in three ways:

- **The effect's author chose the names.** Someone would have to send `status`
  because it is written in that effect's source. The coupling is invisible and
  it points the wrong way.
- **Every effect would parse strings.** What is `"red"`? Each one would carry
  its own vocabulary, its own conversion and its own handling of nonsense.
- **Nothing would be typed**, so the gallery could show nothing and the preview
  could draw nothing.

Binding a parameter puts the conversion **in one place, once, typed**, against
the four `ParamSpec` kinds that already exist: `color` takes `#rrggbb`, `number`
takes a number and is clamped to its declared `min`/`max`, `boolean` takes true
or false, `choice` takes one of its options. **A value that is missing or does
not convert leaves the configured value in place** — so nothing goes dark,
nothing needs a special case, and the preview and the swatch draw at rest.

**That conversion belongs to the bootstrap, not to Rust**, and the storage
module says why: `EffectParamsRecord.values` is a raw JSON map on purpose,
because typing parameters there "would create a second source of truth, which
would diverge at the first parameter type added". Rust therefore cannot turn
`"#ff0000"` into a colour. The bootstrap can: `__candeo_manifest` already
carries `effect.params`, so the specs are there, next to where presses and the
clock are already decoded.

So the host **never touches `params`**. It passes the bound values that exist
alongside it, raw — `{ "colour": "#ff0000" }` — and a signal that is absent or
expired is simply not in it. The bootstrap converts each against its spec: it
replaces the value on success, and on failure or absence the configured value is
still there, never having been overwritten. The fallback above falls out of that
rather than being written.

And every shipped effect becomes signal-driven without a line of code. There is
no *Status* effect to write.

**Words belong to rules, values belong to bindings.** `{"status": "red"}` where
red is a state someone chose is a rule — *when `status` is `red`, show a red
Fixed gradient on the ring* — where the word is written by the person who reads
it, and where priority between rules applies. A colour the sender computed is a
value, and travels as `#ff0000` into a bound parameter. **No mapping table in a
binding** (`failed → red`): that would be a small language, and a second place
where lighting is decided — the very thing this section refuses of the API.

**Where a binding is written: in the parameter's own row, nowhere new.**
`EffectParamsForm` is already the gallery's form *and* a rule's, so the
affordance appears in both by existing once. The control is the pattern §3.2
already settled for cron: **one of the two holds the parameter at a time**, a
value or a signal, the other shown disabled. The name is picked from the signals
held right now — the same list as the Settings panel — or typed, since a
parameter is often bound before its sender ever runs; a name never received is
shown as such and not as an error, as a rule naming a deleted effect stays and
does nothing (§3.4).

**A binding is not a rule, and must not grow into one.**

| | A rule | A binding |
|---|---|---|
| Decides | **which** effect runs | **one value** of the effect already running |
| Carries | a trigger, devices, a duration, a priority, an order | none of these |
| Reads as | a sentence | a field |

Giving bindings an ordered list with priorities would rebuild the scheduler to
fill in a box. A binding **follows the effect, not the device**:
`EffectParamsRecord` is already keyed on (vid, pid, effect), so bindings live in
that same record beside `values`, created and erased with it. A rule carries its
own in `show`, exactly as it already carries its own `params` (§3.2).

**Where it costs.** Parameters are already read afresh on every frame, inside
the render loop, so the bound values are gathered in that one place. **One
argument** is added to the host's render entry point, next to presses and the
clock — an earlier draft of this section said none, and was wrong: the
conversion needs the specs, which are in the bootstrap. Nothing else moves: no
manifest flag, no gallery badge, and **nothing at all in
`packages/effects-api`**. The work is in the settings form.

The bag stays possible later, for an effect drawing sixteen values at once. It
is not the answer here, and it must not be the main mechanism.

### 2.4 Later, if asked

| Source | What an effect would see | Why later |
|---|---|---|
| System | CPU load per core, memory, network rates (`sysinfo`, once a second) | easy; temperatures and GPU need WMI or NVML, sometimes rights |
| Screen colors | average colors along the screen's edges | screen capture (Windows Graphics Capture, a PipeWire portal): costly, and the most sensitive input there is |
| Games | health, ammunition, round state | per-game integrations (CS2 and Dota "Game State Integration" post JSON locally): signals (§2.3) can carry them first |

## 3. Automations

**Shipped with the `cron` trigger** (#106): the resolver and the scheduler in
`src-tauri/src/automations/`, `rules` in `settings.json`, the Automations tab,
interruptions on the device card and in the tray. What the implementation had to
decide beyond this section is written at the end of it, in §3.6.

### 3.1 The principle: one effect per device, interrupted

A device still runs **exactly one effect** at a time. What changes is where that
effect comes from:

- the **applied effect** — the one chosen in the gallery, remembered in
  `activeEffects` — is the device's resting state;
- an **interruption** replaces it for a while, then gives it back.

At any moment the engine runs the interruption with the highest priority that is
active for the device, or else the applied effect. An interruption never
rewrites `activeEffects`: when it ends, the device goes back to what someone
chose, with its settings.

### 3.2 Rules

A rule is one sentence someone can read back:

> **Every hour** · **on** *DeathStalker V2 Pro* · **show** *Clock* · **for**
> *10 seconds*

```json
"rules": [{
  "id": "…", "name": "Hourly clock", "enabled": true,
  "devices": [{ "vid": 5426, "pid": 658 }],
  "when": { "kind": "cron", "expr": "0 * * * *" },
  "show": { "effect": "shipped:Clock", "params": {} },
  "for": { "seconds": 10 }
}]
```

- **When: a cron expression**, read against the local clock, and a duration. An
  occurrence starts each time the expression matches and lasts `for` seconds
  (10 unless said):

  | Rule | Expression | For |
  |---|---|---|
  | The hour | `0 * * * *` | 10 s |
  | Every quarter of an hour | `*/15 * * * *` | 10 s |
  | The night | `0 22 * * *` | 9 h |
  | Office hours, weekdays | `0 9-18 * * 1-5` | 10 s |
  | A flash every thirty seconds | `*/30 * * * * *` | 5 s |

  Five fields, or six with seconds first. **A first version had a trigger of its
  own** — `every` N seconds, `aligned` on the clock, `between` two times — and it
  was replaced before release: it said less than cron (no days, no fixed times)
  with more to explain and maintain. Cron's only loss is "every 7 seconds from when
  the rule was switched on", which nobody asked for.

  - **Back-to-back occurrences are one run.** When an occurrence starts before the
    last one ended — every second for a second — the interruption continues and the
    effect is not restarted: a steady display, not a flicker.
  - **A time window is a start and a duration**: at 22:00 for nine hours, show Off,
    turns the keyboard off at night.

- **The interface writes the common expressions as chips** — every minute, every
  quarter of an hour, every hour, at a time; every day, weekdays, weekend, chosen
  days — and an **advanced** field takes any expression, with its format explained
  beside it. One of the two holds a rule at a time; the other is shown disabled.

- **Other triggers, with their own pull requests**:

  | Trigger | Examples |
  |---|---|
  | `signal` | `doorbell` becomes `ring`; `ci` equals `failed` while it does |
  | `idle` | nobody has used the computer for 10 minutes — shipped (#179), see §3.6 |
  | `app` | an application in the foreground (later) |

- **Duration for these triggers**: `for: { seconds }` ends the interruption
  after a time (a flash). **There is no second duration.** A first version of
  this section announced a `while`, and the implementation of `idle` (#179)
  found it was not needed: "as long as the trigger holds" is a property of the
  occurrence, not of the rule — the resolver answers with an **open**
  occurrence, known to last only until the next look, and the scheduler reads a
  string of them as one run (§3.6). A `signal` trigger says "while `ci` is
  `failed`" exactly that way, and adds no field to a rule.
- **Action**: any effect of the library or a hardware effect, including *Off*,
  with its own settings. A rule does not borrow the device's saved settings for
  that effect: the hourly clock and the clock applied by hand need not look the
  same.
- **Priority**: the order of the list. When two rules are active on one device,
  the higher one runs; when it ends, the next active one, or the applied effect.
  A rule hidden by a higher one is not postponed: its occurrence goes by unseen.
  The hourly clock placed under the night never shows at night.

### 3.3 What someone does during an interruption

- **Applying an effect by hand** makes it the new applied effect, and ends the
  current interruption: a gesture always wins over a rule. It ends every rule
  under way on the device, not only the one on screen: otherwise the night the
  hourly clock was hiding would take the keyboard back a second later. Each rule
  triggers again at its next occurrence.
- **Resume** on the device card and in the tray ends the interruption now, the
  same way.
- **Pause automations**, in the tray and in Settings, suspends every rule until
  it is turned back on: a "do not disturb" for a meeting or a game.

### 3.4 Engine (Rust)

Rules must work with the window closed, so they live in Rust, next to the
engine:

- A **resolver**, a pure function, decides for each device what should run and
  why: `(now, rules, signals, idle times, applied effects, paused) → { effect,
  params, reason, until }`. It holds all the logic above and is tested without a
  clock or a device.
- A **scheduler** thread calls it on each event that can change the answer — a
  signal received, a rule edited, a device opened — and on a one-second tick for
  schedules and durations. When the answer changes for a device, it starts the
  new effect on that device through the same path as `start_effect`, without
  writing `activeEffects`.
- Going back restarts the applied effect: its `time` starts again from zero.
  Keeping the interrupted loop alive in the background would mean two loops per
  device, which §3.1 excludes.
- `engine_status` gains `interruption: { rule, until } | null` per device, and
  the state-changed event tells the window and the tray.
- Rules are written in `settings.json` under `rules`, validated when read. A rule
  naming a deleted effect stays, marked broken, and does nothing.

### 3.5 Interface

- **An Automations tab** in the rail, between Effects and Devices:
  - Each rule is a sentence of chips — *every hour* / *on weekdays* / *on* / *show*
    / *for* — each opening a small picker, or an advanced cron field (§3.2); the durations take presets
    and any number of seconds. The effect picker is the gallery's list; its
    settings form is the gallery's.
  - A switch per rule, the order changed by dragging, and a **Try** button that
    triggers a rule once, now.
  - Empty state: examples to start from (hourly clock, night off, and a doorbell
    flash once signals exist), each creating a disabled rule to adjust.
- **The device card** in the gallery shows an interruption where it shows the
  running effect: "Clock — for 7 s, then Ripples", with **Resume**.
- **The tray**: the device submenu names the interruption the same way, and a
  **Pause automations** check item sits above *Open window*.
- **The simulator** shows what the device shows, interruption included, since it
  draws the device loop's frames.

### 3.6 What the implementation decided

- **The applied effect to go back to may be a firmware one.** Candeo remembers a
  library effect in `activeEffects`, but nothing remembers a firmware effect set
  from the gallery. So before a first interruption, a device no host loop drives
  has its current effect read back — `0x0f`/`0x82`, the read the inspection
  already makes on opening — and that effect is set again when the rule ends. An
  effect that cannot be read back (Static, Breathing) gives way to *Off*: dark is
  better than the rule's last frame, frozen.
- **Resume and a gesture dismiss runs, not rules.** A run is occurrences of one
  rule that keep touching — less than a tick and a half apart. Dismissed, it stays
  dismissed while it continues, so Resume holds on every second for a second; after
  a gap the rule comes back, the next hour or the next night. **Every run under way
  on the device is dismissed**, found by asking the resolver at the gesture: a first
  version dismissed only the run on screen, and a lower rule under way took the
  device back at the next tick.
- **Rules are raw JSON in the file, read one by one.** A broken rule stays as
  written and does nothing; saving from the tab refuses a new broken rule — an
  expression that is not cron typed in the advanced field — not one already there.
- **Expressions are read by `croner`**, backwards from now on the local clock,
  daylight-saving changes included; **local time comes from `chrono`**: the
  standard library has no time zones, and `time`, already in the tree, refuses the
  local offset in a multithreaded process on Linux.
- **Pause automations** lives in `preferences`, so a "do not disturb" set for a
  game survives a restart in the middle of it; it is in the tray and in the tab.
- **`idle` reads the system's idle time, not key capture** (#179), unlike the
  first plan in §3.2. Capture reads keys system-wide, which Candeo does only while
  an effect asks for them (`key-input.md`); an idle rule would keep it on for a
  question that needs no key, and would take someone using only the mouse for
  absent. Windows answers with `GetLastInputInfo`, read once per tick. Linux has
  no single source, so the trigger is unavailable there and the tab says so
  (#183).
- **An idle occurrence is open.** It starts at the instant the threshold was
  crossed — the same instant at every look while nothing is touched, so the
  scheduler reads it as one run — and is known to last only until the next look.
  No end is announced; it ends at the first input, within a second. Minutes only:
  the use is "when I am away", and the tick is a second anyway.

## 4. Order of work

Each step is one pull request, with its issue:

1. **Resume the applied effect** when a device opens — at startup, on adoption,
   on replug (#81) — behind a Settings option, on by default (#102).
2. **Clock input** and the shipped *Clock* effect (#105).
3. **Automations with the `cron` trigger**: the resolver, the scheduler,
   `rules` in `settings.json`, the Automations tab, interruptions on the device
   card and in the tray. The periodic clock and the night window come with it
   (#106).
4. **Sound input** and two shipped effects (#107).
5. **External signals** (#108), in three pull requests (§2.3): the store, the
   `signal` trigger and `candeo signal name=value`; then the HTTP API, its
   token, the Settings block and the Home Assistant example; then the parameter
   bindings of §2.3.1.
6. **`idle` trigger** (#179), on Windows; Linux follows (#183). Then system
   metrics, if asked.

## 5. Out of scope

- Several effects composed on one device (layers, masks).
- Screen colors and game integrations beyond what signals carry.
- macOS.
