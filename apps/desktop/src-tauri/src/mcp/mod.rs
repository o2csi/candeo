//! The MCP server (#359): assistants on this computer read Candeo's devices,
//! layouts, effects, presets, rules and the installed effects API
//! (`docs/design/mcp-server.md`, phase 1, #384). Nothing it offers changes
//! anything yet.
//!
//! - [`pipe`]: the local pipe only the signed-in user can open.
//! - [`relay`]: `candeo --mcp`, the bridge an assistant starts.
//! - [`server`]: the tools and resources, over rmcp; [`views`], what they
//!   return; [`domain`], the reads on the main thread.
//! - This module ties them to the application: the pipe follows the setting,
//!   off by default, and the connections are counted for Settings.
//!
//! # The log
//!
//! Connections and disconnections at `info`, reads at `debug`, each naming the
//! connection by its number and the assistant by the name it gives: never the
//! pipe's name or path, an argument, a source or a value.

pub(crate) mod domain;
pub(crate) mod pipe;
pub mod relay;
pub(crate) mod server;
pub(crate) mod views;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use rmcp::transport::async_rw::AsyncRwTransport;
use rmcp::ServiceExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;

use crate::CmdResult;
use pipe::{BindError, Listener, Stream, MAX_CONNECTIONS};
use server::{Host, Server, MAX_MESSAGE};

/// Emitted when MCP is turned on or off and when an assistant connects,
/// names itself or leaves: Settings follows without polling.
pub const CHANGED: &str = "candeo://mcp-changed";

/// MCP as `settings.json` keeps it, absent until someone turns it on.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct McpConfig {
    /// Assistants may connect. Off by default (#359).
    pub enabled: bool,
    /// What this version does not know, written back unchanged (#373).
    #[serde(flatten)]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

impl McpConfig {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

// ---------------------------------------------------------------- connections

/// The connections open now, by number, with the name each assistant gave.
#[derive(Default)]
pub(crate) struct Connections {
    next: AtomicU64,
    live: Mutex<BTreeMap<u64, Option<String>>>,
    /// Tells the window, once the application is there.
    changed: OnceLock<Box<dyn Fn() + Send + Sync>>,
}

impl Connections {
    fn changed(&self) {
        if let Some(changed) = self.changed.get() {
            changed();
        }
    }

    /// A new connection, and its number.
    pub(crate) fn open(&self) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        self.live.lock().unwrap().insert(id, None);
        tracing::info!(connection = id, "assistant connected");
        self.changed();
        id
    }

    /// The name an assistant gives: a label it chooses, never an authorisation.
    /// Logged when it first gives one, or another.
    pub(crate) fn name(&self, id: u64, name: &str) {
        if name.is_empty() {
            return;
        }
        let renamed = {
            let mut live = self.live.lock().unwrap();
            match live.get_mut(&id) {
                Some(known) if known.as_deref() != Some(name) => {
                    *known = Some(name.to_owned());
                    true
                }
                _ => false,
            }
        };
        if renamed {
            tracing::info!(connection = id, assistant = name, "assistant named itself");
            self.changed();
        }
    }

    pub(crate) fn close(&self, id: u64) {
        let name = self.live.lock().unwrap().remove(&id).flatten();
        tracing::info!(
            connection = id,
            assistant = name.as_deref().unwrap_or(""),
            "assistant disconnected"
        );
        self.changed();
    }

    /// The names of the assistants connected, in the order they came; empty for
    /// one that gave none yet.
    pub(crate) fn names(&self) -> Vec<String> {
        self.live
            .lock()
            .unwrap()
            .values()
            .map(|n| n.clone().unwrap_or_default())
            .collect()
    }
}

// ---------------------------------------------------------------- the listener

/// Where listening stands, as Settings shows it.
#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Listening {
    /// MCP is off: no pipe.
    #[default]
    Off,
    /// The pipe is open; assistants can connect.
    On,
    /// Something else holds the pipe's name: nothing listens.
    Taken,
    /// The pipe could not be created; the log says why.
    Failed,
}

#[derive(Default)]
pub struct Mcp {
    connections: Arc<Connections>,
    /// What stops the listener and every connection, while it listens.
    running: Mutex<Option<CancellationToken>>,
    listening: Mutex<Listening>,
}

/// Starts following the setting: listens now if MCP is on.
pub(crate) fn start(app: &AppHandle) {
    let Some(mcp) = app.try_state::<Mcp>() else {
        return;
    };
    let emitting = app.clone();
    let _ = mcp.connections.changed.set(Box::new(move || {
        let _ = emitting.emit(CHANGED, ());
    }));
    reconcile(app);
}

/// Makes the pipe match the setting: opens it when MCP is on, closes it and
/// every connection when it is off.
pub(crate) fn reconcile(app: &AppHandle) {
    let Some(mcp) = app.try_state::<Mcp>() else {
        return;
    };
    let enabled = match crate::storage::store(app).and_then(|s| s.read_settings()) {
        Ok(settings) => settings.mcp.enabled,
        Err(e) => {
            tracing::debug!("MCP left as it is, settings not read: {e}");
            return;
        }
    };
    let mut running = mcp.running.lock().unwrap();
    match (enabled, running.is_some()) {
        (true, false) => {
            // The pipe is created on the runtime it is served on.
            let bound = tauri::async_runtime::block_on(async { Listener::bind() });
            let listening = match bound {
                Ok(listener) => {
                    let stop = CancellationToken::new();
                    let host = Arc::new(domain::AppHost(app.clone()));
                    tauri::async_runtime::spawn(listen(
                        listener,
                        host,
                        Arc::clone(&mcp.connections),
                        stop.clone(),
                    ));
                    *running = Some(stop);
                    tracing::info!("MCP listening");
                    Listening::On
                }
                Err(BindError::Taken) => {
                    tracing::warn!("MCP not listening: another program holds its pipe");
                    Listening::Taken
                }
                Err(BindError::Failed(e)) => {
                    tracing::warn!("MCP not listening: {}", e.kind());
                    Listening::Failed
                }
            };
            *mcp.listening.lock().unwrap() = listening;
        }
        (false, true) => {
            if let Some(stop) = running.take() {
                // Closes the pipe and every connection: their bridges exit.
                stop.cancel();
            }
            tracing::info!("MCP stopped listening");
            *mcp.listening.lock().unwrap() = Listening::Off;
        }
        (false, false) => *mcp.listening.lock().unwrap() = Listening::Off,
        (true, true) => {}
    }
    drop(running);
    let _ = app.emit(CHANGED, ());
}

/// Accepts connections until stopped, four served at once (§2.5).
async fn listen<H: Host>(
    mut listener: Listener,
    host: Arc<H>,
    connections: Arc<Connections>,
    stop: CancellationToken,
) {
    let slots = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));
    loop {
        let stream = tokio::select! {
            () = stop.cancelled() => break,
            accepted = listener.accept() => match accepted {
                Ok(stream) => stream,
                Err(e) => {
                    tracing::warn!("MCP connection not accepted: {}", e.kind());
                    // Not a busy loop on a pipe that keeps failing.
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    continue;
                }
            },
        };
        // A fifth assistant is connected, and waits here for a slot.
        let slot = tokio::select! {
            () = stop.cancelled() => break,
            slot = Arc::clone(&slots).acquire_owned() => match slot {
                Ok(slot) => slot,
                Err(_) => break,
            },
        };
        if let Err(e) = listener.rearm() {
            tracing::warn!("MCP pipe not prepared for the next assistant: {}", e.kind());
        }
        let id = connections.open();
        let server = Server::new(Arc::clone(&host), Arc::clone(&connections), id);
        let connections = Arc::clone(&connections);
        let stop = stop.child_token();
        tauri::async_runtime::spawn(async move {
            serve(stream, server, stop).await;
            connections.close(id);
            drop(slot);
        });
    }
}

/// One connection, its own rmcp service, until it closes or MCP stops.
pub(crate) async fn serve<H: Host>(
    stream: impl Stream,
    server: Server<H>,
    stop: CancellationToken,
) {
    let (read, write) = tokio::io::split(stream);
    let transport = AsyncRwTransport::new_server(read, write).with_max_line_length(MAX_MESSAGE);
    match server.serve_with_ct(transport, stop).await {
        Ok(running) => {
            let _ = running.waiting().await;
        }
        Err(e) => tracing::debug!("MCP connection ended before its first request: {e}"),
    }
}

// ---------------------------------------------------------------- the window

/// MCP as Settings shows it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub enabled: bool,
    pub listening: Listening,
    /// The assistants connected, by the names they gave; empty for one that
    /// gave none yet.
    pub assistants: Vec<String>,
    /// What an assistant starts, with `--mcp`: a path that survives updates
    /// (§2.6). `None` when this process cannot say where it is.
    pub command: Option<String>,
}

fn status(app: &AppHandle, enabled: bool) -> McpStatus {
    let (listening, assistants) = match app.try_state::<Mcp>() {
        Some(mcp) => (*mcp.listening.lock().unwrap(), mcp.connections.names()),
        None => (Listening::Off, Vec::new()),
    };
    McpStatus {
        enabled,
        listening,
        assistants,
        command: command().map(|path| path.display().to_string()),
    }
}

/// The path assistants are given. Inside the MSIX, the execution alias: the
/// package's own folder carries its version, and changes with every update.
/// In an AppImage, the image's own file rather than where it is mounted.
/// Shown in Settings, on this computer; never logged.
pub(crate) fn command() -> Option<PathBuf> {
    #[cfg(windows)]
    if crate::msix::packaged() {
        return alias(std::env::var_os("LOCALAPPDATA").map(PathBuf::from));
    }
    #[cfg(target_os = "linux")]
    if let Some(image) = std::env::var_os("APPIMAGE").map(PathBuf::from) {
        if image.is_absolute() {
            return Some(image);
        }
    }
    std::env::current_exe().ok()
}

/// Where Windows puts the execution alias the MSIX declares
/// (`packaging/windows/AppxManifest.xml`).
#[cfg_attr(not(windows), allow(dead_code))]
fn alias(local_app_data: Option<PathBuf>) -> Option<PathBuf> {
    local_app_data
        .filter(|path| path.is_absolute())
        .map(|path| path.join("Microsoft").join("WindowsApps").join(ALIAS))
}

/// The alias's name, as the manifest declares it.
const ALIAS: &str = "candeo.exe";

#[tauri::command]
pub fn get_mcp(app: AppHandle) -> CmdResult<McpStatus> {
    let enabled = crate::storage::store(&app)?.read_settings()?.mcp.enabled;
    Ok(status(&app, enabled))
}

/// Turns MCP on or off. Off is the revocation: the pipe closes, and every
/// assistant connected is dropped.
#[tauri::command]
pub fn set_mcp(app: AppHandle, enabled: bool) -> CmdResult<McpStatus> {
    let store = crate::storage::store(&app)?;
    let mut settings = store.read_settings()?;
    if settings.mcp.enabled != enabled {
        settings.mcp.enabled = enabled;
        store.write_settings(&settings)?;
        tracing::info!(enabled, "MCP set");
    }
    reconcile(&app);
    Ok(status(&app, enabled))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Written on both sides of the IPC: renamed on one side only, Settings
    /// would stop following connections, with no error anywhere.
    #[test]
    fn the_event_has_the_same_name_on_both_sides() {
        let ts = include_str!("../../../src/api/candeo.ts");
        assert!(
            ts.contains(CHANGED),
            "\"{CHANGED}\" not found in src/api/candeo.ts"
        );
    }

    /// A settings file written before MCP existed reads as MCP off, and one
    /// never turned on keeps no `mcp` key. What a later version adds survives.
    #[test]
    fn mcp_is_off_until_turned_on() {
        let config: McpConfig = serde_json::from_str("{}").unwrap();
        assert!(!config.enabled);
        assert!(config.is_default());
        let settings: crate::storage::Settings = serde_json::from_str("{}").unwrap();
        assert!(!settings.mcp.enabled);
        assert!(serde_json::to_value(&settings)
            .unwrap()
            .get("mcp")
            .is_none());

        let later: McpConfig =
            serde_json::from_str(r#"{ "enabled": true, "confirmInCandeo": true }"#).unwrap();
        assert_eq!(
            serde_json::to_value(&later).unwrap(),
            serde_json::json!({ "enabled": true, "confirmInCandeo": true })
        );
    }

    #[test]
    fn the_msix_alias_is_in_windows_apps() {
        let local = std::env::temp_dir();
        assert_eq!(
            alias(Some(local.clone())),
            Some(
                local
                    .join("Microsoft")
                    .join("WindowsApps")
                    .join("candeo.exe")
            )
        );
        assert_eq!(alias(None), None);
        assert_eq!(alias(Some(PathBuf::from("relative"))), None);
    }

    /// The package declares the alias assistants are given: without it, the
    /// path Settings shows would start nothing.
    #[test]
    fn the_package_declares_the_alias() {
        let manifest = include_str!("../../../../../packaging/windows/AppxManifest.xml");
        assert!(manifest.contains("windows.appExecutionAlias"));
        assert!(manifest.contains(&format!("Alias=\"{ALIAS}\"")));
    }

    #[test]
    fn connections_are_counted_and_named() {
        let connections = Connections::default();
        let first = connections.open();
        let second = connections.open();
        assert_ne!(first, second);
        connections.name(first, "Claude Code");
        connections.name(first, "");
        assert_eq!(connections.names(), ["Claude Code", ""]);
        connections.close(first);
        assert_eq!(connections.names(), [""]);
        // A late name for a connection gone changes nothing.
        connections.name(first, "Cursor");
        assert_eq!(connections.names(), [""]);
    }

    /// A host none of these tests reads from: listing tools reads nothing.
    #[cfg(windows)]
    struct Nothing;

    #[cfg(windows)]
    impl server::Reads for Nothing {
        fn devices(&self) -> Result<views::DevicesView, crate::failure::Failure> {
            Err(crate::failure::Failure::unexpected("unused"))
        }
        fn layout(
            &self,
            _: Option<crate::DeviceRef>,
        ) -> Result<views::LayoutView, crate::failure::Failure> {
            Err(crate::failure::Failure::unexpected("unused"))
        }
        fn effects(&self) -> Result<views::EffectsView, crate::failure::Failure> {
            Err(crate::failure::Failure::unexpected("unused"))
        }
        fn effect(&self, _: &str) -> Result<views::EffectSourceView, crate::failure::Failure> {
            Err(crate::failure::Failure::unexpected("unused"))
        }
        fn presets(&self, _: Option<&str>) -> Result<views::PresetsView, crate::failure::Failure> {
            Err(crate::failure::Failure::unexpected("unused"))
        }
        fn automations(&self) -> Result<views::AutomationsView, crate::failure::Failure> {
            Err(crate::failure::Failure::unexpected("unused"))
        }
        fn check_automation(
            &self,
            _: &serde_json::Value,
        ) -> Result<views::CheckView, crate::failure::Failure> {
            Err(crate::failure::Failure::unexpected("unused"))
        }
        fn signals(&self) -> Result<views::SignalsView, crate::failure::Failure> {
            Err(crate::failure::Failure::unexpected("unused"))
        }
    }

    #[cfg(windows)]
    impl Host for Nothing {
        type Reads = Nothing;
        async fn with<T, F>(&self, read: F) -> Result<T, crate::failure::Failure>
        where
            T: Send + 'static,
            F: FnOnce(&Self::Reads) -> Result<T, crate::failure::Failure> + Send + 'static,
        {
            read(self)
        }
    }

    #[cfg(windows)]
    fn pipe_for(test: &str) -> String {
        format!(r"\\.\pipe\candeo-mcp-test-{test}-{}", std::process::id())
    }

    /// Stopping MCP closes the pipe: what was connected sees it end, and
    /// nothing answers on its name any more.
    #[cfg(windows)]
    #[tokio::test]
    async fn turning_mcp_off_closes_every_connection_and_the_pipe() {
        use tokio::io::AsyncReadExt;
        let name = pipe_for("off");
        let listener = Listener::bind_at(&name).unwrap();
        let connections = Arc::new(Connections::default());
        let stop = CancellationToken::new();
        let listening = tokio::spawn(listen(
            listener,
            Arc::new(Nothing),
            Arc::clone(&connections),
            stop.clone(),
        ));
        let mut client = pipe::connect_to(&name).unwrap();
        // Served: counted as connected.
        for _ in 0..100 {
            if connections.names().len() == 1 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(connections.names().len(), 1);

        stop.cancel();
        listening.await.unwrap();
        let mut byte = [0u8; 1];
        let read = tokio::time::timeout(std::time::Duration::from_secs(5), client.read(&mut byte))
            .await
            .expect("the connection ends");
        assert!(matches!(read, Ok(0) | Err(_)), "{read:?}");
        assert!(matches!(
            pipe::connect_to(&name),
            Err(pipe::Unreachable::Absent)
        ));
    }

    /// MCP over the named pipe itself, as the bridge carries it: an assistant of
    /// each revision lists the tools, and is named in Settings.
    #[cfg(windows)]
    #[tokio::test]
    async fn an_assistant_speaks_mcp_over_the_pipe() {
        use rmcp::model::{ClientCapabilities, ClientConfig, Implementation, ProtocolVersion};
        use rmcp::{ClientLifecycleMode, ClientServiceExt};
        let name = pipe_for("talk");
        let listener = Listener::bind_at(&name).unwrap();
        let connections = Arc::new(Connections::default());
        let stop = CancellationToken::new();
        tokio::spawn(listen(
            listener,
            Arc::new(Nothing),
            Arc::clone(&connections),
            stop.clone(),
        ));
        for lifecycle in [
            ClientLifecycleMode::Initialize,
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
        ] {
            let pipe = pipe::connect_to(&name).unwrap();
            let assistant = ClientConfig::new(
                ClientCapabilities::default(),
                Implementation::new("Pipe test", "1.0"),
            );
            let client = assistant
                .serve_with_lifecycle(pipe, lifecycle)
                .await
                .unwrap();
            assert_eq!(client.list_all_tools().await.unwrap().len(), 9);
            assert_eq!(connections.names(), ["Pipe test"]);
            client.cancel().await.unwrap();
            for _ in 0..100 {
                if connections.names().is_empty() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            assert!(connections.names().is_empty());
        }
        stop.cancel();
    }
}
