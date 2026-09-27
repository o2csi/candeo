//! External signals (#108): named values other software sends, which rules read
//! (`docs/design/inputs-and-automations.md` §2.3).
//!
//! - [`store`] holds the values, and is pure.
//! - [`http`] answers the API; [`interfaces`] says where it listens.
//! - This module ties them to the application. The listeners follow the
//!   settings, and the addresses of the ticked interfaces as they change; every
//!   change to what is held wakes the automations and tells the window.
//!
//! # Locks
//!
//! Each table is held for as long as it takes to read or change it, and never
//! across another: a request reads the token, then changes the store, then
//! notifies, one after the other.

pub mod gsi;
pub mod http;
pub mod interfaces;
pub mod store;

use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::failure::Failure;
use crate::CmdResult;
use http::{Listener, Request, Response};
use interfaces::NetworkInterface;
use store::{Held, Refusal, SignalView, Store};

/// What is held, shared with the effects engine: a render loop reads the values
/// bound to its parameters on every frame, and has no application handle to ask
/// for them (`docs/design/inputs-and-automations.md` §2.3.1).
pub type SharedStore = Arc<Mutex<Store>>;

/// The port, unless something else holds it; editable in Settings.
pub const DEFAULT_PORT: u16 = 7317;

/// Emitted whenever what is held changes, and when a value expires: the Settings
/// list follows without polling.
pub const CHANGED: &str = "candeo://signals-changed";

/// How often the listeners check that the ticked interfaces still have the
/// addresses they listen on, and try again an address that was refused.
const FOLLOW: Duration = Duration::from_secs(10);

/// The API as `settings.json` keeps it. Off, it listens nowhere.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct SignalsConfig {
    pub enabled: bool,
    pub port: u16,
    /// Made the first time the API is turned on, and renewed from Settings.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub token: String,
    /// The interfaces listened on besides loopback, **by name**.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub interfaces: Vec<String>,
    /// What games post their state with, written into their file: made when a
    /// first game is connected, and apart from `token` so that renewing one
    /// leaves the other (`docs/design/game-state-integration.md` §3).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub game_token: String,
    /// Valve's game state integration turned on in Settings: the Games tab
    /// shows, and `/gsi/` takes posts (§4). Off, it refuses them.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub valve_games: bool,
}

impl Default for SignalsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_PORT,
            token: String::new(),
            interfaces: Vec::new(),
            game_token: String::new(),
            valve_games: false,
        }
    }
}

impl SignalsConfig {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// The API's state, as Settings shows it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SignalsApi {
    pub enabled: bool,
    pub port: u16,
    pub token: String,
    pub interfaces: Vec<String>,
    /// The addresses actually listened on now.
    pub listening: Vec<String>,
    /// The port is held by something else on loopback: nothing answers.
    pub port_in_use: bool,
    pub valve_games: bool,
}

#[derive(Default)]
pub struct Signals {
    store: SharedStore,
    /// The settings the listeners were last set from: a request reads its token
    /// here rather than from the disk.
    config: Mutex<SignalsConfig>,
    listeners: Mutex<Vec<Listener>>,
    /// Addresses that could not be listened on, and why.
    refused: Mutex<BTreeMap<SocketAddr, String>>,
}

impl Signals {
    /// Signals holding `store`, the one the effects engine reads.
    pub fn sharing(store: SharedStore) -> Self {
        Self {
            store,
            ..Self::default()
        }
    }
}

/// Starts following the settings and the interfaces: a thread of its own, for as
/// long as the process runs, like the automations.
pub(crate) fn start(app: &AppHandle) {
    let app = app.clone();
    // A change of settings reconciles at once, from its command; this thread
    // only follows what changes by itself — an address, a port freed.
    let spawned = std::thread::Builder::new()
        .name("candeo-signals".into())
        .spawn(move || loop {
            reconcile(&app);
            std::thread::sleep(FOLLOW);
        });
    if let Err(e) = spawned {
        tracing::error!("signals not started, the API will not listen: {e}");
    }
}

/// Makes the listeners match the settings and the addresses of the ticked
/// interfaces: stops those no longer wanted, starts the missing ones.
pub(crate) fn reconcile(app: &AppHandle) {
    let Some(signals) = app.try_state::<Signals>() else {
        return;
    };
    let config = match crate::storage::store(app).and_then(|s| s.read_settings()) {
        Ok(settings) => settings.signals,
        Err(e) => {
            tracing::debug!("signals API left as it is, settings not read: {e}");
            return;
        }
    };
    *signals.config.lock().unwrap() = config.clone();

    let wanted: BTreeSet<SocketAddr> = if config.enabled && !config.token.is_empty() {
        // Listed only when someone ticked one: loopback needs no list.
        let up = if config.interfaces.is_empty() {
            Vec::new()
        } else {
            interfaces::list()
        };
        interfaces::addresses(config.port, &config.interfaces, &up)
    } else {
        BTreeSet::new()
    };

    let mut refused = BTreeMap::new();
    {
        let mut listeners = signals.listeners.lock().unwrap();
        listeners.retain(|listener| {
            let keep = wanted.contains(&listener.addr);
            if !keep {
                tracing::info!(on = %logged(&listener.addr), "signals API stops listening");
            }
            keep
        });
        for &addr in &wanted {
            if listeners.iter().any(|listener| listener.addr == addr) {
                continue;
            }
            let serving = app.clone();
            match Listener::start(addr, move |request| serve(&serving, request)) {
                Ok(listener) => {
                    tracing::info!(on = %logged(&addr), "signals API listening");
                    listeners.push(listener);
                }
                Err(e) => {
                    refused.insert(addr, e);
                }
            }
        }
    }

    let mut previous = signals.refused.lock().unwrap();
    for (addr, reason) in &refused {
        // Said once, not every ten seconds while it lasts. Loopback in IPv6 is
        // refused on a system without IPv6, which is nobody's problem.
        if previous.get(addr) != Some(reason) && !addr.is_ipv6() {
            tracing::warn!(on = %logged(addr), "signals API not listening: {reason}");
        }
    }
    *previous = refused;
}

/// Where a listener listens, as the log says it. An interface's address
/// identifies the machine and its network, and no address reaches the log
/// (AGENTS.md, Privacy); loopback's says nothing about anyone.
fn logged(addr: &SocketAddr) -> String {
    if addr.ip().is_loopback() {
        addr.to_string()
    } else {
        format!("a network interface, port {}", addr.port())
    }
}

/// Answers one request, on the listener's thread.
fn serve(app: &AppHandle, request: &Request) -> Response {
    let Some(signals) = app.try_state::<Signals>() else {
        return Response {
            status: 503,
            body: json!({ "error": "Candeo is starting" }),
        };
    };
    let (token, game_token) = {
        let config = signals.config.lock().unwrap();
        let games = config.valve_games.then(|| config.game_token.clone());
        (config.token.clone(), games)
    };
    let now = chrono::Local::now().timestamp_millis();
    let (response, changed) = {
        let mut store = signals.store.lock().unwrap();
        if http::for_game(request.url) {
            http::game(request, game_token.as_deref(), &mut store, now)
        } else {
            http::handle(request, &token, &mut store, now)
        }
    };
    if changed {
        notify(app);
    }
    response
}

/// What is held changed: the window follows, and the automations decide now
/// rather than at the next second.
fn notify(app: &AppHandle) {
    let _ = app.emit(CHANGED, ());
    crate::automations::wake(app);
}

/// What is held now, for the resolver. Values that expired go first, and the
/// window hears of it.
pub(crate) fn held(app: &AppHandle) -> BTreeMap<String, Held> {
    let Some(signals) = app.try_state::<Signals>() else {
        return BTreeMap::new();
    };
    let now = chrono::Local::now().timestamp_millis();
    let (held, pruned) = {
        let mut store = signals.store.lock().unwrap();
        let pruned = store.prune(now);
        (store.held().clone(), pruned)
    };
    if pruned {
        let _ = app.emit(CHANGED, ());
    }
    held
}

/// A new token: 32 random bytes, in hexadecimal.
fn new_token() -> CmdResult<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|e| Failure::unexpected(format!("no randomness for a token: {e}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn view(app: &AppHandle, config: &SignalsConfig) -> SignalsApi {
    let (listening, port_in_use) = match app.try_state::<Signals>() {
        Some(signals) => {
            let listening = signals
                .listeners
                .lock()
                .unwrap()
                .iter()
                .map(|listener| listener.addr.to_string())
                .collect();
            let loopback = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), config.port);
            let in_use = signals.refused.lock().unwrap().contains_key(&loopback);
            (listening, in_use)
        }
        None => (Vec::new(), false),
    };
    SignalsApi {
        enabled: config.enabled,
        port: config.port,
        token: config.token.clone(),
        interfaces: config.interfaces.clone(),
        listening,
        port_in_use,
        valve_games: config.valve_games,
    }
}

#[tauri::command]
pub fn get_signals_api(app: AppHandle) -> CmdResult<SignalsApi> {
    let config = crate::storage::store(&app)?.read_settings()?.signals;
    Ok(view(&app, &config))
}

/// Turns the API on or off, and says where it listens. The token is made the
/// first time it is turned on.
#[tauri::command]
pub fn set_signals_api(
    app: AppHandle,
    enabled: bool,
    port: u16,
    interfaces: Vec<String>,
) -> CmdResult<SignalsApi> {
    // Below 1024, Linux asks for privileges no desktop application has.
    if port < 1024 {
        return Err(Failure::new("signalsPortInvalid").with("port", port));
    }
    let store = crate::storage::store(&app)?;
    let mut settings = store.read_settings()?;
    let config = &mut settings.signals;
    config.enabled = enabled;
    config.port = port;
    config.interfaces = interfaces;
    if enabled && config.token.is_empty() {
        config.token = new_token()?;
    }
    store.write_settings(&settings)?;
    tracing::info!(
        enabled,
        port,
        interfaces = settings.signals.interfaces.len(),
        "signals API set"
    );
    reconcile(&app);
    // A connected game posts to the port its file names.
    rewrite_games(&settings.signals);
    Ok(view(&app, &settings.signals))
}

/// Turns Valve's game state integration on or off: the Games tab follows.
#[tauri::command]
pub fn set_valve_games(app: AppHandle, on: bool) -> CmdResult<SignalsApi> {
    let store = crate::storage::store(&app)?;
    let mut settings = store.read_settings()?;
    settings.signals.valve_games = on;
    store.write_settings(&settings)?;
    tracing::info!(on, "Valve's game state integration set");
    reconcile(&app);
    Ok(view(&app, &settings.signals))
}

/// A new token: every sender holding the old one is refused from now on.
#[tauri::command]
pub fn renew_signals_token(app: AppHandle) -> CmdResult<SignalsApi> {
    let store = crate::storage::store(&app)?;
    let mut settings = store.read_settings()?;
    settings.signals.token = new_token()?;
    store.write_settings(&settings)?;
    tracing::info!("signals token renewed");
    reconcile(&app);
    Ok(view(&app, &settings.signals))
}

#[tauri::command]
pub fn list_network_interfaces() -> Vec<NetworkInterface> {
    interfaces::list()
}

#[tauri::command]
pub fn list_signals(app: AppHandle) -> Vec<SignalView> {
    let now = chrono::Local::now().timestamp_millis();
    match app.try_state::<Signals>() {
        Some(signals) => signals.store.lock().unwrap().views(now),
        None => Vec::new(),
    }
}

/// Sets a value from Settings, as a sender would: to try a rule before any
/// sender exists.
#[tauri::command]
pub fn send_signal(app: AppHandle, name: String, value: String) -> CmdResult<()> {
    let Some(signals) = app.try_state::<Signals>() else {
        return Err(Failure::unexpected("signals not managed"));
    };
    let now = chrono::Local::now().timestamp_millis();
    let applied = signals
        .store
        .lock()
        .unwrap()
        .apply(&json!({ name.clone(): value }), None, now);
    match applied {
        Ok(_) => {
            notify(&app);
            Ok(())
        }
        Err(Refusal::Name(_)) => Err(Failure::new("signalNameInvalid").with("name", name)),
        Err(Refusal::TooLong(_)) => Err(Failure::new("signalValueTooLong")
            .with("name", name)
            .with("max", store::MAX_TEXT)),
        Err(Refusal::TooMany) => Err(Failure::new("signalsFull").with("max", store::MAX_SIGNALS)),
        Err(refusal) => Err(Failure::unexpected(refusal)),
    }
}

/// Erases a value, whatever its lifetime: the way out of one sent "until
/// erased" by a sender that is gone.
#[tauri::command]
pub fn erase_signal(app: AppHandle, name: String) {
    let erased = app
        .try_state::<Signals>()
        .is_some_and(|signals| signals.store.lock().unwrap().erase(&name));
    if erased {
        notify(&app);
    }
}

// ---------------------------------------------------------------- games

/// Where a game stands, as Settings shows it.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GameState {
    /// Steam does not have it here.
    NotFound,
    /// No file of Candeo's in its folder.
    Disconnected,
    /// Its file says what Candeo expects, and reception is on.
    Connected,
    /// Its file is there, but names another port or token, or reception is off:
    /// connecting again sets both right.
    Outdated,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GameView {
    pub id: &'static str,
    pub name: &'static str,
    pub state: GameState,
    /// What its launch options must carry for it to read the file at all.
    pub launch_option: Option<&'static str>,
}

fn games(config: &SignalsConfig) -> Vec<GameView> {
    gsi::Game::ALL
        .into_iter()
        .map(|game| {
            let state = match gsi::folder(game) {
                None => GameState::NotFound,
                Some(folder) => {
                    let expected = gsi::file_text(game, config.port, &config.game_token);
                    match gsi::file_state(&folder, &expected) {
                        (false, _) => GameState::Disconnected,
                        (true, true) if config.enabled && !config.game_token.is_empty() => {
                            GameState::Connected
                        }
                        (true, _) => GameState::Outdated,
                    }
                }
            };
            GameView {
                id: game.id(),
                name: game.name(),
                state,
                launch_option: game.launch_option(),
            }
        })
        .collect()
}

/// Writes again the file of every game that has one: after a new port, what it
/// names must follow.
fn rewrite_games(config: &SignalsConfig) {
    if config.game_token.is_empty() {
        return;
    }
    for game in gsi::Game::ALL {
        let Some(folder) = gsi::folder(game) else {
            continue;
        };
        let file = folder.join(gsi::FILE);
        if file.exists() {
            let text = gsi::file_text(game, config.port, &config.game_token);
            if let Err(e) = std::fs::write(&file, text) {
                tracing::warn!(game = game.id(), "game file not written again: {e}");
            }
        }
    }
}

fn known_game(id: &str) -> CmdResult<gsi::Game> {
    gsi::Game::from_id(id).ok_or_else(|| Failure::unexpected(format!("no game {id:?}")))
}

#[tauri::command]
pub fn list_games(app: AppHandle) -> CmdResult<Vec<GameView>> {
    let config = crate::storage::store(&app)?.read_settings()?.signals;
    Ok(games(&config))
}

/// Writes the game's file into its folder, pointing at this computer, and turns
/// reception on if it was off: what connecting a game asks for (§4). The game
/// reads its file at launch.
#[tauri::command]
pub fn connect_game(app: AppHandle, game: String) -> CmdResult<Vec<GameView>> {
    let game = known_game(&game)?;
    let folder =
        gsi::folder(game).ok_or_else(|| Failure::new("gameNotFound").with("game", game.name()))?;
    let store = crate::storage::store(&app)?;
    let mut settings = store.read_settings()?;
    let config = &mut settings.signals;
    if config.token.is_empty() {
        config.token = new_token()?;
    }
    if config.game_token.is_empty() {
        config.game_token = new_token()?;
    }
    let turned_on = !config.enabled;
    config.enabled = true;
    let text = gsi::file_text(game, config.port, &config.game_token);
    gsi::write(&folder, &text).map_err(|e| {
        Failure::unexpected(format!(
            "cannot write the file in {}'s folder: {e}",
            game.name()
        ))
    })?;
    store.write_settings(&settings)?;
    tracing::info!(game = game.id(), turned_on, "game connected");
    reconcile(&app);
    Ok(games(&settings.signals))
}

/// Removes the game's file: it stops posting at its next launch.
#[tauri::command]
pub fn disconnect_game(app: AppHandle, game: String) -> CmdResult<Vec<GameView>> {
    let game = known_game(&game)?;
    if let Some(folder) = gsi::folder(game) {
        let file = folder.join(gsi::FILE);
        if file.exists() {
            std::fs::remove_file(&file).map_err(|e| {
                Failure::unexpected(format!(
                    "cannot remove the file from {}'s folder: {e}",
                    game.name()
                ))
            })?;
        }
    }
    tracing::info!(game = game.id(), "game disconnected");
    let config = crate::storage::store(&app)?.read_settings()?.signals;
    Ok(games(&config))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Written on both sides of the IPC, like `tray::STATE_CHANGED`: renamed on
    /// one side only, the Settings list would stop following what arrives, with
    /// no error anywhere.
    #[test]
    fn the_event_has_the_same_name_on_both_sides() {
        let ts = include_str!("../../../src/api/candeo.ts");
        assert!(
            ts.contains(CHANGED),
            "\"{CHANGED}\" not found in src/api/candeo.ts"
        );
    }

    /// A settings file written before signals existed reads as the API off, and
    /// one never turned on keeps no `signals` key.
    #[test]
    fn the_api_is_off_until_turned_on() {
        let config: SignalsConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config, SignalsConfig::default());
        assert!(!config.enabled);
        assert_eq!(config.port, DEFAULT_PORT);
        assert!(config.is_default());
    }

    #[test]
    fn the_log_names_no_address_but_loopback() {
        let lan: SocketAddr = "192.0.2.23:7317".parse().unwrap();
        let v6: SocketAddr = "[2001:db8::23]:7317".parse().unwrap();
        assert_eq!(logged(&lan), "a network interface, port 7317");
        assert!(!logged(&v6).contains("2001"));
        assert_eq!(logged(&"127.0.0.1:7317".parse().unwrap()), "127.0.0.1:7317");
    }

    #[test]
    fn a_token_is_32_random_bytes_in_hex() {
        let (a, b) = (new_token().unwrap(), new_token().unwrap());
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
