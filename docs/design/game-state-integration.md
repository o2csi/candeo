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

Dota 2 — the hero's values go the same way while you spectate, `hero` then
describing both teams rather than yours:

| Signal | Value |
|---|---|
| `dota2.health`, `dota2.mana` | 0 to 100 |
| `dota2.respawn` | the seconds left before respawning, while dead; erased alive |
| `dota2.stunned`, `dota2.silenced`, `dota2.smoked` | `yes` while it lasts, erased after |
| `dota2.daytime` | `day` or `night`, Night Stalker's night included |
| `dota2.phase` | the match's state, `DOTA_GAMERULES_STATE_` left out: `hero_selection`, `pre_game`, `game_in_progress`, `post_game`… |

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

## 4. Turning games on, and the Games tab

Most people using Candeo do not play, and their screens stay as they are:

- **Settings has one switch per integration**: *Valve's game state
  integration*, for Counter-Strike 2 and, later, Dota 2. Another game's API —
  League of Legends' — would be a switch of its own. It is `valveGames` in the
  signals settings; off, `/gsi/` refuses every post.
- **On, a *Games* tab appears** beside the others, and everything about games
  lives there. A card per game shows:
  - where it stands: not found, not connected, connected, or *playing* — its
    values arriving now;
  - **Connect**, which writes the game's file into its `cfg` folder, pointing at
    this computer's port with the game token, and turns reception on if it was
    off; **Disconnect** removes the file. The game reads its file at launch, so
    connecting while it runs takes effect at its next launch, and the card says
    so. When the port or the game token changes, the files of connected games
    are written again;
  - its values as they arrive — the simplest way to see it works;
  - **while playing**: which effect to show on which devices during a match,
    and the device's own effect back after it. It is a rule — *while
    `cs2.phase` is set* — carrying the game it belongs to, and listed in
    *Automations* like any other so that its priority stays visible there,
    with its switch locked on: it runs as long as the game is connected, since
    switched off there it would leave the card showing a lighting that never
    comes.
    Connecting a game is wanting its lighting during a match, so **Connect**
    makes it, with the game's effect on the devices Candeo controls where it
    applies (`effects-library.md`), and **Disconnect** removes it; the card
    changes its effect and devices, offering only the devices where the effect
    applies. A game
    connected without it — no device controlled then, or the rule deleted in
    *Automations* — says its lighting stays as it is, and makes it on request.
    There is no box to tick: without the rule, a connected game would light
    nothing.

A signal condition with no value to match is met by any value, which is what
*set* means; it is useful beyond games — *while `call` is set*.

Finding the game: Steam's folder — on Windows `SteamPath` under
`HKCU\Software\Valve\Steam`, on Linux `~/.steam/steam` or
`~/.local/share/Steam` — then `steamapps/libraryfolders.vdf`, which lists every
library and the apps in it (`730` is Counter-Strike 2, `570` Dota 2). The folder
is `steamapps/common/Counter-Strike Global Offensive/game/csgo/cfg/` for
Counter-Strike 2, `steamapps/common/dota 2 beta/game/dota/cfg/gamestate_integration/`
for Dota 2.

## 5. Effects for a game, and the gallery

*Counter-Strike 2*, shipped, is the one tested on 2026-09-26: health on the
number row from green to red, the clip on the function row, a flash turning
the keyboard white, fire turning it orange, and the whole keyboard pulsing
while the bomb is planted. Every colour is a setting.

Effects for games would soon outnumber the others in the gallery, which is
where someone picks the lighting of their desk. So:

- **an effect says which game it is for**, `game: 'cs2'` in its declaration,
  like its `inputs`;
- **the gallery keeps them apart**: a *Games* group, folded, after the others,
  and only while games are on;
- **their place is the game's card**, where *while playing* picks one.

## 6. Adding a game

Another of Valve's game state integrations is a translation table and a
folder. Dota 2 was the second, on 2026-09-27, with two differences from
Counter-Strike 2:

- **its file goes in a folder of its own**, `game/dota/cfg/gamestate_integration/`,
  which a fresh install does not have: *Connect* makes it;
- **it reads no file unless started with `-gamestateintegration`**, a launch
  option Valve made necessary in March 2022, the integration costing time on
  every frame. Candeo does not edit Steam's launch options, which Steam
  rewrites while it runs: the card says to add it.

Roshan is not in what a player's integration posts, and is left out.

Games without it are another mechanism, decided when asked: League of Legends
serves its state locally over HTTPS, to be polled. Screen analysis and memory
reading stay out, whatever the game.

## 7. Order of work

1. The route, the translation and the game token, with their tests.
2. Finding Steam's libraries and writing the file.
3. The switch in Settings, the *Games* tab, and *while playing* as a rule.
4. `game` in an effect's declaration, and the gallery's *Games* group.
5. The shipped effect, a page on the site, and the Store's *What's new*.
