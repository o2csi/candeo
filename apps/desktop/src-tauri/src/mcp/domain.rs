//! The reads, in Candeo: the functions the window's commands call, on the main
//! thread (`docs/design/mcp-server.md` §2.8).
//!
//! Synchronous commands run on the main thread, and `write_settings` keeps a
//! fixed temporary name because of it (`storage.rs`): a tool running on a tokio
//! thread would break that. Reads go the same way, since some of them write
//! back what they learn, and one rule is simpler than a list of exceptions.

use tauri::{AppHandle, Manager};

use super::server::{Host, Reads};
use super::views::{
    self, AutomationsView, CheckView, DevicesView, EffectSourceView, EffectView, EffectsView,
    LayoutView, PresetsView, SignalsView,
};
use crate::failure::Failure;
use crate::language::Language;
use crate::storage::{self, EffectKey};
use crate::{AppState, DeviceRef, LayoutInfo};

/// Candeo, as the server's host: each read is sent to the main thread, and its
/// answer back.
pub(crate) struct AppHost(pub AppHandle);

impl Host for AppHost {
    type Reads = AppReads;

    async fn with<T, F>(&self, read: F) -> Result<T, Failure>
    where
        T: Send + 'static,
        F: FnOnce(&Self::Reads) -> Result<T, Failure> + Send + 'static,
    {
        let (answer, answered) = tokio::sync::oneshot::channel();
        let app = self.0.clone();
        self.0
            .run_on_main_thread(move || {
                let _ = answer.send(read(&AppReads(app)));
            })
            .map_err(|e| Failure::unexpected(format!("main thread unreachable: {e}")))?;
        answered
            .await
            .map_err(|_| Failure::unexpected("the main thread dropped a read"))?
    }
}

/// The reads, on the main thread.
pub(crate) struct AppReads(AppHandle);

impl AppReads {
    fn settings(&self) -> Result<storage::Settings, Failure> {
        storage::store(&self.0)?.read_settings()
    }

    fn engine(&self) -> Vec<crate::runtime::DeviceEngineStatus> {
        crate::runtime::engine_status(self.0.clone(), self.0.state::<AppState>()).devices
    }
}

impl Reads for AppReads {
    fn devices(&self) -> Result<DevicesView, Failure> {
        // English: the model reads it, as an effect's author reads TypeScript's
        // diagnostics (§4.1).
        let devices = crate::device_infos(&self.0, &self.0.state::<AppState>(), Language::En)?;
        Ok(views::devices_view(
            &devices,
            &self.engine(),
            &self.settings()?,
        ))
    }

    fn layout(&self, device: Option<DeviceRef>) -> Result<LayoutView, Failure> {
        let layout = match device {
            None => LayoutInfo::from(crate::default_layout()),
            Some(device) => crate::get_layout(self.0.state::<AppState>(), device)?,
        };
        Ok(layout.into())
    }

    fn effects(&self) -> Result<EffectsView, Failure> {
        let effects = storage::store(&self.0)?.list_effects()?;
        Ok(EffectsView {
            effects: effects.into_iter().map(EffectView::from).collect(),
        })
    }

    fn effect(&self, id: &str) -> Result<EffectSourceView, Failure> {
        let key = EffectKey::parse(id)?;
        let store = storage::store(&self.0)?;
        let source = store.effect_source(&key)?;
        let entry = store
            .list_effects()?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or_else(|| Failure::new("effectNotFound").with("name", id))?;
        Ok(EffectSourceView {
            effect: entry.into(),
            source,
        })
    }

    fn presets(&self, effect: Option<&str>) -> Result<PresetsView, Failure> {
        Ok(views::presets_view(&self.settings()?, effect))
    }

    fn automations(&self) -> Result<AutomationsView, Failure> {
        Ok(views::automations_view(&self.settings()?, &self.engine()))
    }

    fn check_automation(&self, rule: &serde_json::Value) -> Result<CheckView, Failure> {
        let known: Vec<DeviceRef> = crate::layouts().iter().map(|l| DeviceRef::of(l)).collect();
        Ok(views::check_view(rule, &known, &chrono::Local::now()))
    }

    fn signals(&self) -> Result<SignalsView, Failure> {
        let receiving = self.settings()?.signals.enabled;
        let held = crate::signals::list_signals(self.0.clone());
        let now = chrono::Local::now().timestamp_millis();
        Ok(views::signals_view(receiving, &held, now))
    }
}
