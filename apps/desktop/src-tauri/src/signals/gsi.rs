//! Valve's game state integration (`docs/design/game-state-integration.md`):
//! what a game posts, turned into signals, and the file that asks it to post.
//!
//! Everything deciding is pure — a payload in, signals out; a library list in,
//! folders out; a port and a token in, a file's text out — so it is tested
//! without Steam or a game. Only [`folder`] reads the disk.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value};

/// How long a game's values live: the file asks for a heartbeat every five
/// seconds, so a game that quits lets them expire rather than freezing the
/// keyboard (§2).
pub const TTL_SECONDS: u32 = 15;

/// The file a connected game reads, in its `cfg` folder.
pub const FILE: &str = "gamestate_integration_candeo.cfg";

/// A game whose integration Candeo reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Game {
    Cs2,
    Dota2,
}

impl Game {
    pub const ALL: [Game; 2] = [Game::Cs2, Game::Dota2];

    /// In the route, the signals' names and the window: `cs2`.
    pub fn id(self) -> &'static str {
        match self {
            Game::Cs2 => "cs2",
            Game::Dota2 => "dota2",
        }
    }

    pub fn from_id(id: &str) -> Option<Game> {
        Game::ALL.into_iter().find(|game| game.id() == id)
    }

    /// Its name, as its maker writes it: not translated.
    pub fn name(self) -> &'static str {
        match self {
            Game::Cs2 => "Counter-Strike 2",
            Game::Dota2 => "Dota 2",
        }
    }

    /// Its id in Steam's libraries.
    fn app(self) -> &'static str {
        match self {
            Game::Cs2 => "730",
            Game::Dota2 => "570",
        }
    }

    /// Its `cfg` folder, from a library's root: there when the game is installed.
    fn cfg(self) -> &'static str {
        match self {
            Game::Cs2 => "steamapps/common/Counter-Strike Global Offensive/game/csgo/cfg",
            Game::Dota2 => "steamapps/common/dota 2 beta/game/dota/cfg",
        }
    }

    /// Where in its `cfg` folder it looks for the file: Dota 2 in a folder of
    /// its own, which a fresh install does not have yet.
    fn place(self) -> Option<&'static str> {
        match self {
            Game::Cs2 => None,
            Game::Dota2 => Some("gamestate_integration"),
        }
    }

    /// The launch option without which it reads no file: Dota 2 asks for one
    /// since 2022, the integration costing it time on every frame.
    pub fn launch_option(self) -> Option<&'static str> {
        match self {
            Game::Cs2 => None,
            Game::Dota2 => Some("-gamestateintegration"),
        }
    }

    /// The parts of its state the file asks for: what [`translate`] reads.
    fn data(self) -> &'static [&'static str] {
        match self {
            Game::Cs2 => &[
                "provider",
                "round",
                "player_id",
                "player_state",
                "player_weapons",
            ],
            Game::Dota2 => &["provider", "map", "player", "hero"],
        }
    }
}

/// The token a payload carries, from the file's `auth` block: the game cannot
/// set a header.
pub fn token(body: &Value) -> Option<&str> {
    body["auth"]["token"].as_str()
}

/// The signals a payload gives, flat and named `<game>.<value>`: what the store
/// takes. An empty string erases a value that no longer holds.
pub fn translate(game: Game, body: &Value) -> Map<String, Value> {
    match game {
        Game::Cs2 => cs2(body),
        Game::Dota2 => dota2(body),
    }
}

/// The values of the player at the keyboard.
const CS2_PLAYER: [&str; 5] = ["health", "armor", "ammo", "flashed", "burning"];

fn cs2(body: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    let mut put = |name: &str, value: Value| {
        out.insert(format!("cs2.{name}"), value);
    };
    let percent = |value: i64, of: i64| Value::from((value * 100 / of).clamp(0, 100));

    let player = &body["player"];
    let id = player["steamid"].as_str();
    // While you spectate a teammate, the payload describes them: nothing of it
    // is yours, and your values go rather than stay frozen.
    let mine = id.is_some() && id == body["provider"]["steamid"].as_str();
    if mine && player["state"].is_object() {
        let state = &player["state"];
        let read = |name: &str| state[name].as_i64();
        if let Some(health) = read("health") {
            put("health", percent(health, 100));
        }
        if let Some(armor) = read("armor") {
            put("armor", percent(armor, 100));
        }
        // The game counts these from 0 to 255.
        if let Some(flashed) = read("flashed") {
            put("flashed", percent(flashed, 255));
        }
        if let Some(burning) = read("burning") {
            put("burning", percent(burning, 255));
        }
        let active = player["weapons"]
            .as_object()
            .and_then(|weapons| weapons.values().find(|w| w["state"] == "active"));
        let clip =
            active.and_then(|w| Some((w["ammo_clip"].as_i64()?, w["ammo_clip_max"].as_i64()?)));
        match clip {
            Some((left, max)) if max > 0 => put("ammo", percent(left, max)),
            // A knife or a grenade in hand: no clip to show.
            _ => put("ammo", Value::from("")),
        }
    } else if player.is_object() {
        for name in CS2_PLAYER {
            put(name, Value::from(""));
        }
    }

    let round = &body["round"];
    if round.is_object() {
        if let Some(phase) = round["phase"].as_str() {
            put("phase", Value::from(phase));
        }
        put(
            "bomb",
            Value::from(round["bomb"].as_str().unwrap_or("none")),
        );
    }
    out
}

/// What the hero at the keyboard is in.
const DOTA2_HERO: [&str; 6] = ["health", "mana", "respawn", "stunned", "silenced", "smoked"];

fn dota2(body: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    let mut put = |name: &str, value: Value| {
        out.insert(format!("dota2.{name}"), value);
    };
    let percent = |value: &Value| value.as_i64().map(|v| Value::from(v.clamp(0, 100)));
    // A state that holds while it lasts, and goes with it: a rule reads it as
    // *set*.
    let flag = |on: bool| Value::from(if on { "yes" } else { "" });

    let hero = &body["hero"];
    // Spectating, `hero` describes the players of both teams: nothing of it is
    // yours, and your values go rather than stay frozen.
    if let Some(health) = percent(&hero["health_percent"]) {
        put("health", health);
        if let Some(mana) = percent(&hero["mana_percent"]) {
            put("mana", mana);
        }
        let dead = hero["alive"].as_bool() == Some(false);
        put(
            "respawn",
            match hero["respawn_seconds"].as_i64() {
                Some(seconds) if dead => Value::from(seconds.max(0)),
                _ if dead => Value::from(0),
                _ => Value::from(""),
            },
        );
        for name in ["stunned", "silenced", "smoked"] {
            put(name, flag(hero[name].as_bool() == Some(true)));
        }
    } else if hero.is_object() {
        for name in DOTA2_HERO {
            put(name, Value::from(""));
        }
    }

    let map = &body["map"];
    if map.is_object() {
        if let Some(state) = map["game_state"].as_str() {
            let phase = state
                .trim_start_matches("DOTA_GAMERULES_STATE_")
                .to_lowercase();
            put("phase", Value::from(phase));
        }
        if let Some(day) = map["daytime"].as_bool() {
            let night = !day || map["nightstalker_night"].as_bool() == Some(true);
            put("daytime", Value::from(if night { "night" } else { "day" }));
        }
    }
    out
}

/// The file asking `game` to post its state to Candeo, on this computer.
pub fn file_text(game: Game, port: u16, token: &str) -> String {
    let data: String = game
        .data()
        .iter()
        .map(|part| format!("    \"{part}\" \"1\"\n"))
        .collect();
    format!(
        "\"Candeo\"\n{{\n  \"uri\" \"http://127.0.0.1:{port}/gsi/{id}\"\n  \"timeout\" \"1.0\"\n  \"buffer\" \"0.1\"\n  \"throttle\" \"0.1\"\n  \"heartbeat\" \"5.0\"\n  \"auth\"\n  {{\n    \"token\" \"{token}\"\n  }}\n  \"data\"\n  {{\n{data}  }}\n}}\n",
        id = game.id()
    )
}

// ---------------------------------------------------------------- Steam's libraries

/// A node of Valve's key-value format: a text, or a block of keyed nodes.
#[derive(Debug, PartialEq)]
enum Node {
    Text(String),
    Block(Vec<(String, Node)>),
}

impl Node {
    fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Block(entries) => entries
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v),
            Node::Text(_) => None,
        }
    }

    fn entries(&self) -> &[(String, Node)] {
        match self {
            Node::Block(entries) => entries,
            Node::Text(_) => &[],
        }
    }
}

/// Reads Valve's key-value text: quoted keys, then a quoted value or a block in
/// braces. What it cannot read ends the block, rather than the program.
fn parse(text: &str) -> Node {
    let mut tokens = tokens(text).into_iter().peekable();
    Node::Block(block(&mut tokens))
}

#[derive(Debug, PartialEq)]
enum Token {
    Text(String),
    Open,
    Close,
}

fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => out.push(Token::Open),
            '}' => out.push(Token::Close),
            '"' => {
                let mut value = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => {
                            if let Some(escaped) = chars.next() {
                                value.push(escaped);
                            }
                        }
                        _ => value.push(c),
                    }
                }
                out.push(Token::Text(value));
            }
            _ => {}
        }
    }
    out
}

fn block(tokens: &mut std::iter::Peekable<std::vec::IntoIter<Token>>) -> Vec<(String, Node)> {
    let mut entries = Vec::new();
    while let Some(token) = tokens.next() {
        let Token::Text(key) = token else {
            break;
        };
        match tokens.next() {
            Some(Token::Text(value)) => entries.push((key, Node::Text(value))),
            Some(Token::Open) => entries.push((key, Node::Block(block(tokens)))),
            _ => break,
        }
    }
    entries
}

/// Where a game is, in the libraries `libraryfolders.vdf` lists: the root of
/// the one holding its app.
fn library_of(vdf: &str, app: &str) -> Option<PathBuf> {
    let root = parse(vdf);
    root.get("libraryfolders")?
        .entries()
        .iter()
        .map(|(_, library)| library)
        .find(|library| {
            library
                .get("apps")
                .is_some_and(|apps| apps.get(app).is_some())
        })
        .and_then(|library| match library.get("path")? {
            Node::Text(path) => Some(PathBuf::from(path)),
            Node::Block(_) => None,
        })
}

/// The folder where `game` looks for the file, or `None` when Steam does not
/// have the game. It may not exist yet: [`write`] makes it.
pub fn folder(game: Game) -> Option<PathBuf> {
    let steam = steam()?;
    let vdf = std::fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")).ok()?;
    let cfg = library_of(&vdf, game.app())?.join(game.cfg());
    cfg.is_dir().then(|| match game.place() {
        Some(place) => cfg.join(place),
        None => cfg,
    })
}

/// Writes the file into `folder`, making the folder first where the game has
/// none yet.
pub fn write(folder: &Path, text: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    std::fs::write(folder.join(FILE), text)
}

/// Whether the file in `folder` is there, and says what Candeo expects now.
pub fn file_state(folder: &Path, expected: &str) -> (bool, bool) {
    match std::fs::read_to_string(folder.join(FILE)) {
        Ok(text) => (true, text.replace("\r\n", "\n") == expected),
        Err(_) => (false, false),
    }
}

#[cfg(windows)]
fn steam() -> Option<PathBuf> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};

    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(Some(0)).collect() };
    let key = wide(r"Software\Valve\Steam");
    let name = wide("SteamPath");
    let mut buffer = [0u16; 520];
    let mut size = std::mem::size_of_val(&buffer) as u32;
    // SAFETY: `size` is the buffer's size in bytes; both strings end in NUL.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    let chars = (size as usize / 2).saturating_sub(1);
    (status == 0).then(|| PathBuf::from(String::from_utf16_lossy(&buffer[..chars])))
}

#[cfg(not(windows))]
fn steam() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    [".steam/steam", ".local/share/Steam"]
        .into_iter()
        .map(|dir| home.join(dir))
        .find(|dir| dir.join("steamapps/libraryfolders.vdf").is_file())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const ME: &str = "76561190000000001";

    /// A payload as Counter-Strike 2 posts it, trimmed to what is read: a round
    /// with the bomb planted, and the player holding a rifle half full.
    fn payload() -> Value {
        json!({
            "provider": { "name": "Counter-Strike: Global Offensive", "appid": 730, "steamid": ME },
            "round": { "phase": "live", "bomb": "planted" },
            "player": {
                "steamid": ME,
                "state": { "health": 64, "armor": 100, "flashed": 255, "burning": 0 },
                "weapons": {
                    "weapon_0": { "name": "weapon_knife_t", "state": "holstered" },
                    "weapon_1": { "name": "weapon_ak47", "ammo_clip": 15, "ammo_clip_max": 30, "state": "active" }
                }
            },
            "auth": { "token": "secret" }
        })
    }

    #[test]
    fn a_payload_becomes_the_players_signals() {
        let signals = translate(Game::Cs2, &payload());
        assert_eq!(
            Value::Object(signals),
            json!({
                "cs2.health": 64, "cs2.armor": 100, "cs2.flashed": 100, "cs2.burning": 0,
                "cs2.ammo": 50, "cs2.phase": "live", "cs2.bomb": "planted"
            })
        );
        assert_eq!(token(&payload()), Some("secret"));
    }

    /// Dead, you watch a teammate: their health is not yours.
    #[test]
    fn a_spectated_player_erases_yours() {
        let mut body = payload();
        body["player"]["steamid"] = json!("76561190000000002");
        let signals = translate(Game::Cs2, &body);
        for name in CS2_PLAYER {
            assert_eq!(signals[&format!("cs2.{name}")], json!(""), "{name}");
        }
        assert_eq!(signals["cs2.bomb"], json!("planted"));
    }

    #[test]
    fn a_knife_in_hand_erases_the_clip_and_no_bomb_says_none() {
        let mut body = payload();
        body["player"]["weapons"]["weapon_0"]["state"] = json!("active");
        body["player"]["weapons"]["weapon_1"]["state"] = json!("holstered");
        body["round"] = json!({ "phase": "freezetime" });
        let signals = translate(Game::Cs2, &body);
        assert_eq!(signals["cs2.ammo"], json!(""));
        assert_eq!(signals["cs2.bomb"], json!("none"));
        assert_eq!(signals["cs2.phase"], json!("freezetime"));
    }

    /// In the menus there is no round and no player: nothing is said.
    #[test]
    fn the_menus_say_nothing() {
        let body = json!({ "provider": { "steamid": ME } });
        assert!(translate(Game::Cs2, &body).is_empty());
    }

    #[test]
    fn the_file_points_at_this_computer_with_the_game_token() {
        let text = file_text(Game::Cs2, 7317, "abc");
        assert!(
            text.contains("\"uri\" \"http://127.0.0.1:7317/gsi/cs2\""),
            "{text}"
        );
        assert!(text.contains("\"token\" \"abc\""));
        let parsed = parse(&text);
        let data = parsed.get("Candeo").and_then(|c| c.get("data")).unwrap();
        assert_eq!(data.entries().len(), Game::Cs2.data().len());
    }

    /// A payload as Dota 2 posts it during a match, trimmed to what is read.
    fn dota2_payload() -> Value {
        json!({
            "provider": { "name": "Dota 2", "appid": 570 },
            "map": {
                "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS",
                "daytime": true,
                "nightstalker_night": false
            },
            "player": { "steamid": ME, "kills": 3 },
            "hero": {
                "alive": true, "respawn_seconds": 0,
                "health_percent": 64, "mana_percent": 30,
                "stunned": true, "silenced": false, "smoked": false
            }
        })
    }

    #[test]
    fn a_dota2_payload_becomes_the_heros_signals() {
        let signals = translate(Game::Dota2, &dota2_payload());
        assert_eq!(
            Value::Object(signals),
            json!({
                "dota2.health": 64, "dota2.mana": 30, "dota2.respawn": "",
                "dota2.stunned": "yes", "dota2.silenced": "", "dota2.smoked": "",
                "dota2.phase": "game_in_progress", "dota2.daytime": "day"
            })
        );
    }

    #[test]
    fn a_dead_hero_waits_and_nightstalker_brings_the_night() {
        let mut body = dota2_payload();
        body["hero"]["alive"] = json!(false);
        body["hero"]["respawn_seconds"] = json!(23);
        body["map"]["nightstalker_night"] = json!(true);
        let signals = translate(Game::Dota2, &body);
        assert_eq!(signals["dota2.respawn"], json!(23));
        assert_eq!(signals["dota2.daytime"], json!("night"));
    }

    #[test]
    fn a_spectated_match_erases_your_hero() {
        let mut body = dota2_payload();
        body["hero"] = json!({ "team2": { "player0": { "health_percent": 90 } } });
        let signals = translate(Game::Dota2, &body);
        for name in DOTA2_HERO {
            assert_eq!(signals[&format!("dota2.{name}")], json!(""), "{name}");
        }
        assert_eq!(signals["dota2.phase"], json!("game_in_progress"));
    }

    #[test]
    fn dota2_asks_for_its_launch_option_and_a_folder_of_its_own() {
        let text = file_text(Game::Dota2, 7317, "abc");
        assert!(
            text.contains("\"uri\" \"http://127.0.0.1:7317/gsi/dota2\""),
            "{text}"
        );
        assert_eq!(Game::Dota2.launch_option(), Some("-gamestateintegration"));
        assert_eq!(Game::Cs2.launch_option(), None);
        assert_eq!(Game::from_id("dota2"), Some(Game::Dota2));

        let folder = std::env::temp_dir()
            .join(format!("candeo-gsi-{}", std::process::id()))
            .join("gamestate_integration");
        write(&folder, &text).unwrap();
        assert_eq!(file_state(&folder, &text), (true, true));
        std::fs::remove_dir_all(folder.parent().unwrap()).unwrap();
    }

    /// `libraryfolders.vdf` as Steam writes it, paths escaped, two libraries.
    #[test]
    fn the_game_is_found_in_the_library_holding_it() {
        let vdf = r#""libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"apps"
		{
			"228980"		"1234"
		}
	}
	"1"
	{
		"path"		"D:\\Games\\SteamLibrary"
		"apps"
		{
			"730"		"38000000000"
		}
	}
}"#;
        assert_eq!(
            library_of(vdf, "730"),
            Some(PathBuf::from(r"D:\Games\SteamLibrary"))
        );
        assert_eq!(library_of(vdf, "570"), None);
        assert_eq!(library_of("not a library", "730"), None);
    }
}
