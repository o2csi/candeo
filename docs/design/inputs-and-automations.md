# Inputs and automations

Lighting that reacts to more than key presses — the time, the music playing,
signals sent by other software — and rules that interrupt the configured effect
for a while. Status: proposed. Each part lands in its own pull request, in the
order at the end.

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

```ts
render({ signals }) // { doorbell: 'ring', ci: 'failed', volume: 0.4 }
```

- **Why it matters most**: once other software can push values, every
  integration Candeo does not write becomes a script — Home Assistant, a CI job,
  a mute state, a recording state.
- **The local API**:
  - **Off by default**, enabled in Settings.
  - Listens on loopback only.
  - Every request carries a token, shown in Settings with a button to generate
    a new one.
  - Listening on the local network is a second, explicit choice, for Home
    Assistant on another machine.
  - Values are strings, numbers or booleans; a signal expires after a lifetime
    its sender can set (default 60 s), so a crashed sender does not leave the
    keyboard red forever.
- **Also from the command line**: `candeo signal ci=failed`, which talks to the
  running instance through the single-instance channel, for scripts that would
  rather not handle HTTP.
- **Home Assistant**: the HTTP API is enough for its `rest_command`. MQTT, with
  Candeo as a client of the broker Home Assistant already runs, needs no inbound
  port and could expose Candeo as an entity. It is a later step, if HTTP proves
  awkward there.
- Signals are also **automation triggers** (§3): "when `doorbell` becomes `ring`".

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
  | `idle` | no key pressed for 10 minutes (reuses key capture) |
  | `app` | an application in the foreground (later) |

- **Duration for these triggers**:
  - `for: { seconds }` ends the interruption after a time (a flash);
  - `while` lasts as long as the trigger holds (a signal that keeps its value,
    idleness).
- **Action**: any effect of the library or a hardware effect, including *Off*,
  with its own settings. A rule does not borrow the device's saved settings for
  that effect: the hourly clock and the clock applied by hand need not look the
  same.
- **Priority**: the order of the list. When two rules are active on one device,
  the higher one runs; when it ends, the next active one, or the applied effect.

### 3.3 What someone does during an interruption

- **Applying an effect by hand** makes it the new applied effect, and ends the
  current interruption: a gesture always wins over a rule. The rule triggers
  again at its next occurrence.
- **Resume** on the device card and in the tray ends the interruption now.
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
- **Resume and a gesture dismiss a run, not a rule.** A run is occurrences of one
  rule that keep touching — less than a tick and a half apart. Dismissed, it stays
  dismissed while it continues, so Resume holds on every second for a second; after
  a gap the rule comes back, the next hour or the next night.
- **Rules are raw JSON in the file, read one by one.** A broken rule stays as
  written and does nothing; saving from the tab refuses a new broken rule — an
  expression that is not cron typed in the advanced field — not one already there.
- **Expressions are read by `croner`**, backwards from now on the local clock,
  daylight-saving changes included; **local time comes from `chrono`**: the
  standard library has no time zones, and `time`, already in the tree, refuses the
  local offset in a multithreaded process on Linux.
- **Pause automations** lives in `preferences`, so a "do not disturb" set for a
  game survives a restart in the middle of it; it is in the tray and in the tab.

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
5. **External signals**: the local API, the command line, `signal` triggers,
   the Home Assistant example (#108).
6. **`idle` trigger**, then system metrics, if asked.

## 5. Out of scope

- Several effects composed on one device (layers, masks).
- Screen colors and game integrations beyond what signals carry.
- macOS.
