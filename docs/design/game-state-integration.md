# Game state integration

What happens in a game — your health, the ammo left in the clip, a flashbang,
the bomb — shown on the keyboard. Status: proposed, Counter-Strike 2 first;
Dota 2 is the next table (§6).

## Why this needs a design

A first test on 2026-09-26 went through a PowerShell script: Counter-Strike 2
sent its state to the script, which passed it on to Candeo as signals, and an
effect drew health and ammo on a DeathStalker V2 Pro. Health and ammo showed as
expected. Asking everyone to run a script is not a feature, and the decisions
below shape the signals API, the settings, a new route and a shipped effect.

## 1. What Valve's game state integration is

Valve's own mechanism, in Counter-Strike 2 and Dota 2:

- a file `gamestate_integration_<name>.cfg` in the game's `cfg` folder names a
  URI and what to send;
- the game **POSTs its state as JSON** to that URI when it changes, throttled,
  and again at a heartbeat;
- it is read by the game at launch.

Nothing reads the game's memory or its screen: it is the game telling what it
chooses to tell, which is why tools built on it are safe with anti-cheat.
Candeo uses nothing else — no memory reading, no screen analysis, ever.

## 2. Received as signals

The state becomes **signals** (`inputs-and-automations.md` §2.3), so everything
signals already do follows without a line more: a rule, a setting following a
value, *Status row*, an effect reading signals.

- **A route on the signals listener**: `POST /gsi/<game>`, `/gsi/cs2` for now,
  on the same port and the same addresses — loopback unless someone ticked an
  interface.
- **A translation per game**, a pure function from the game's JSON to flat
  signals named `<game>.<value>`, tested against captured payloads.
- **Only the player at the keyboard**: a payload whose `player.steamid` is not
  `provider.steamid` — spectating a teammate after dying — says nothing of
  yours, and gives no player values. The Steam id is compared, never kept.
- **Fifteen seconds of life**, and the file asks for a heartbeat every five: a
  game that quits lets its signals expire instead of freezing the keyboard.
- **Kept in memory only**, like every signal: nothing of the game or the player
  reaches the disk or the log.

Counter-Strike 2:

| Signal | Value |
|---|---|
| `cs2.health`, `cs2.armor` | 0 to 100 |
| `cs2.ammo` | the clip of the weapon in hand, 0 to 100 % |
| `cs2.flashed`, `cs2.burning` | 0 to 100, from the game's 0 to 255 |
| `cs2.bomb` | `none`, `planted`, `defused` or `exploded` |
| `cs2.phase` | `freezetime`, `live` or `over` |

## 3. Authentication

The game cannot set a header: the file's `auth` block is sent in the body, as
`"auth": { "token": "…" }`. A route under `/gsi/` accepts that token only, and
`/signals` only its `Authorization: Bearer` one:

- **the game's token is its own**, `gameToken` in the signals settings, made
  when a first game is connected. The file sits in the game's folder, readable
  by anything the user runs; what it can do is post that game's state, not set
  any signal;
- **renewing one leaves the other**, so a renewed signals token breaks no game.

A web page is refused on both, as today: a browser says where it comes from.

## 4. Connecting a game

*Settings › Signals* gains **Games**, a row per supported game:

- **not found** — Steam does not have it;
- **Connect** — writes the file into the game's `cfg` folder, pointing at this
  computer's port with the game token, and turns reception on for this computer
  if it was off;
- **connected** — the file is there and says what Candeo expects; *Disconnect*
  removes it.

The game reads its file at launch: connecting while it runs takes effect at its
next launch, and the row says so. When the port or the game token changes, the
files of connected games are written again.

Finding the game: Steam's folder — on Windows `SteamPath` under
`HKCU\Software\Valve\Steam`, on Linux `~/.steam/steam` or
`~/.local/share/Steam` — then `steamapps/libraryfolders.vdf`, which lists every
library and the apps in it (`730` is Counter-Strike 2, `570` Dota 2). The folder
is `steamapps/common/Counter-Strike Global Offensive/game/csgo/cfg/` for
Counter-Strike 2, `steamapps/common/dota 2 beta/game/dota/cfg/gamestate_integration/`
for Dota 2.

## 5. The shipped effect

*Counter-Strike 2*, the one tested on 2026-09-26: health on the number row from
green to red, the clip on the function row, a flash turning the keyboard white,
fire turning it orange, and the whole keyboard pulsing red while the bomb is
planted. Rules add the rest — a flash of colour on a kill, a round won.

## 6. Adding a game

Another of Valve's game state integrations is a translation table and a
folder: Dota 2 first — health, mana, the day and night cycle, Roshan. Nothing
else changes.

Games without it are another mechanism, decided when asked: League of Legends
serves its state locally over HTTPS, to be polled. Screen analysis and memory
reading stay out, whatever the game.

## 7. Order of work

1. The route, the translation and the game token, with their tests.
2. Finding Steam's libraries and writing the file; *Games* in Settings.
3. The shipped effect, a page on the site, and the Store's *What's new*.
