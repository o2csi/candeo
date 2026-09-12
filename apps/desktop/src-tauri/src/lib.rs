//! Liaison entre l'interface et le matériel.
//!
//! Les types exposés au front sont définis ici plutôt que dans les crates :
//! `candeo-protocol` et `candeo-device` restent ainsi sans dépendance à serde
//! ni à Tauri, et donc réutilisables et testables hors application.

use std::sync::Mutex;

use candeo_device::{Keyboard, Layout, DEATHSTALKER_V2_PRO};
use candeo_protocol::{Effect, Rgb};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

/// Gabarits connus. Un seul pour l'instant.
const LAYOUTS: &[&Layout] = &[&DEATHSTALKER_V2_PRO];

// ---------------------------------------------------------------- types exposés

#[derive(Serialize)]
pub struct DeviceInfo {
    pub name: String,
    pub vid: u16,
    pub pid: u16,
    /// Vrai si le périphérique est effectivement branché.
    pub present: bool,
}

#[derive(Serialize)]
pub struct KeyInfo {
    pub index: u16,
    pub row: u8,
    pub col: u8,
}

#[derive(Serialize)]
pub struct LayoutInfo {
    pub name: String,
    pub rows: u8,
    pub cols: u8,
    /// Taille d'une image : **toutes** les cases, trous compris.
    pub frame_len: usize,
    /// Uniquement les cases portant une LED.
    pub keys: Vec<KeyInfo>,
}

impl From<&'static Layout> for LayoutInfo {
    fn from(l: &'static Layout) -> Self {
        let mut keys = Vec::new();
        for row in 0..l.rows {
            for col in 0..l.cols {
                if let Some(index) = l.at(row, col) {
                    keys.push(KeyInfo { index, row, col });
                }
            }
        }
        Self {
            name: l.name.to_string(),
            rows: l.rows,
            cols: l.cols,
            frame_len: l.led_count(),
            keys,
        }
    }
}

/// Effet, tel que nommé côté interface.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EffectDto {
    Off,
    SpectrumCycle,
    Wave { direction: u8, speed: u8 },
    Custom,
}

impl From<EffectDto> for Effect {
    fn from(e: EffectDto) -> Self {
        match e {
            EffectDto::Off => Effect::Off,
            EffectDto::SpectrumCycle => Effect::SpectrumCycle,
            EffectDto::Wave { direction, speed } => Effect::Wave { direction, speed },
            EffectDto::Custom => Effect::Custom,
        }
    }
}

// ---------------------------------------------------------------- état

#[derive(Default)]
pub struct AppState {
    keyboard: Mutex<Option<Keyboard>>,
}

/// Les erreurs remontent au front sous forme de chaîne : l'interface les
/// affiche telles quelles, elles doivent donc rester lisibles.
type CmdResult<T> = Result<T, String>;

fn hid() -> CmdResult<hidapi::HidApi> {
    hidapi::HidApi::new().map_err(|e| format!("initialisation HID impossible : {e}"))
}

// ---------------------------------------------------------------- commandes

/// Liste les gabarits connus et indique lesquels sont branchés.
#[tauri::command]
fn list_devices() -> CmdResult<Vec<DeviceInfo>> {
    let api = hid()?;
    Ok(LAYOUTS
        .iter()
        .map(|l| DeviceInfo {
            name: l.name.to_string(),
            vid: l.vid,
            pid: l.pid,
            present: api.device_list().any(|d| {
                d.vendor_id() == l.vid
                    && d.product_id() == l.pid
                    && d.interface_number() == l.interface as i32
            }),
        })
        .collect())
}

#[tauri::command]
fn connect(state: State<'_, AppState>, vid: u16, pid: u16) -> CmdResult<LayoutInfo> {
    let layout = LAYOUTS
        .iter()
        .copied()
        .find(|l| l.vid == vid && l.pid == pid)
        .ok_or_else(|| format!("aucun gabarit connu pour {vid:#06x}:{pid:#06x}"))?;

    let api = hid()?;
    let kb = Keyboard::open(&api, layout).map_err(|e| e.to_string())?;
    *state.keyboard.lock().unwrap() = Some(kb);
    Ok(LayoutInfo::from(layout))
}

#[tauri::command]
fn disconnect(state: State<'_, AppState>) {
    *state.keyboard.lock().unwrap() = None;
}

#[tauri::command]
fn is_connected(state: State<'_, AppState>) -> bool {
    state.keyboard.lock().unwrap().is_some()
}

/// Gabarit du périphérique connecté.
#[tauri::command]
fn get_layout(state: State<'_, AppState>) -> CmdResult<LayoutInfo> {
    let guard = state.keyboard.lock().unwrap();
    let kb = guard.as_ref().ok_or("aucun périphérique connecté")?;
    Ok(LayoutInfo::from(kb.layout()))
}

#[tauri::command]
fn set_brightness(state: State<'_, AppState>, level: u8) -> CmdResult<()> {
    let guard = state.keyboard.lock().unwrap();
    let kb = guard.as_ref().ok_or("aucun périphérique connecté")?;
    kb.set_brightness(level).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_effect(state: State<'_, AppState>, effect: EffectDto) -> CmdResult<()> {
    let guard = state.keyboard.lock().unwrap();
    let kb = guard.as_ref().ok_or("aucun périphérique connecté")?;
    kb.set_effect(effect.into()).map_err(|e| e.to_string())
}

/// Pousse une image complète.
///
/// `frame` est une suite plate de triplets RGB et doit couvrir **toutes** les
/// cases de la matrice. En envoyer moins laisse les dernières rangées figées
/// sur leur valeur précédente — c'est le piège classique de ce matériel.
#[tauri::command]
fn present(state: State<'_, AppState>, frame: Vec<u8>) -> CmdResult<()> {
    let guard = state.keyboard.lock().unwrap();
    let kb = guard.as_ref().ok_or("aucun périphérique connecté")?;
    let expected = kb.layout().led_count();

    if frame.len() != expected * 3 {
        return Err(format!(
            "image de {} octets, {} attendus ({expected} cases × 3)",
            frame.len(),
            expected * 3
        ));
    }
    let colors: Vec<Rgb> = frame
        .chunks_exact(3)
        .map(|c| Rgb::new(c[0], c[1], c[2]))
        .collect();
    kb.present(&colors).map_err(|e| e.to_string())
}

/// Écrit un segment de rangée, sans toucher au reste.
#[tauri::command]
fn write_row(state: State<'_, AppState>, row: u8, col_start: u8, colors: Vec<u8>) -> CmdResult<()> {
    let guard = state.keyboard.lock().unwrap();
    let kb = guard.as_ref().ok_or("aucun périphérique connecté")?;
    if colors.len() % 3 != 0 || colors.is_empty() {
        return Err("les couleurs doivent former des triplets RGB non vides".into());
    }
    let c: Vec<Rgb> = colors
        .chunks_exact(3)
        .map(|x| Rgb::new(x[0], x[1], x[2]))
        .collect();
    kb.write_row(row, col_start, &c).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- point d'entrée

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(AppState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_devices,
            connect,
            disconnect,
            is_connected,
            get_layout,
            set_brightness,
            set_effect,
            present,
            write_row,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application");
}
