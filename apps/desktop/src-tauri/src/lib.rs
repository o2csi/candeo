//! Liaison entre l'interface et le matériel.
//!
//! Les types exposés au front sont définis ici plutôt que dans les crates :
//! `candeo-protocol` et `candeo-device` restent ainsi sans dépendance à serde
//! ni à Tauri, et donc réutilisables et testables hors application.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use candeo_device::{Keyboard, Layout, DEATHSTALKER_V2_PRO};
use candeo_protocol::{Effect, Rgb};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use storage::{DeviceState, Settings};

mod builtins;
mod runtime;
mod storage;

/// Gabarits connus. Un seul pour l'instant.
const LAYOUTS: &[&Layout] = &[&DEATHSTALKER_V2_PRO];

// ---------------------------------------------------------------- types exposés

// Les champs partent en camelCase : c'est la convention du côté qui les lit.
// Laisser filtrer le nommage Rust jusque dans l'interface serait une fuite
// d'abstraction, et elle ne se verrait qu'à l'exécution.

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub name: String,
    pub vid: u16,
    pub pid: u16,
    /// Vrai si le périphérique est effectivement branché.
    pub present: bool,
    /// Décision prise pour cet appareil, relue dans `settings.json`.
    ///
    /// `present` dit ce que voit le système, `state` ce que l'utilisateur a
    /// décidé : les deux sont indépendants. Un appareil adopté peut être
    /// débranché, un appareil branché peut être ignoré.
    pub state: DeviceState,
    /// Vrai si c'est **cet** appareil qui est ouvert en ce moment.
    pub open: bool,
    /// Dernier échec d'ouverture **de cet appareil**.
    ///
    /// Chacun porte le sien : une ouverture qui échoue ne doit ni empêcher les
    /// autres de fonctionner, ni leur faire porter son message.
    pub error: Option<String>,
}

/// Une touche, telle que le simulateur doit la dessiner.
///
/// Deux systèmes de coordonnées cohabitent, et ils ne disent pas la même chose :
///
/// - `row` / `col` situent la LED dans la matrice, donc son rang dans une image ;
/// - `x` / `y` / `w` / `h` donnent le rectangle physique, en unités de pas de
///   clavier (1 u = une touche alphabétique), origine en haut à gauche.
///
/// Le second ne se déduit pas du premier : le périphérique ne déclare aucune
/// dimension, la géométrie est une transcription manuelle de la disposition ISO
/// pleine taille. Voir `candeo_device::Key`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyInfo {
    pub index: u16,
    pub row: u8,
    pub col: u8,
    /// Nom gravé, variante French (ISO).
    pub name: &'static str,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
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
        // Parcours ligne par ligne de la matrice, pour que `keys` sorte dans
        // l'ordre des index. La table de `candeo-device` couvre toutes les
        // positions allumées — invariant tenu par son test
        // `every_lit_position_has_a_key`.
        let mut keys = Vec::with_capacity(l.lit_count());
        for row in 0..l.rows {
            for col in 0..l.cols {
                let Some(index) = l.at(row, col) else {
                    continue;
                };
                let Some(k) = l.key(index) else { continue };
                keys.push(KeyInfo {
                    index,
                    row,
                    col,
                    name: k.name,
                    x: k.x,
                    y: k.y,
                    w: k.w,
                    h: k.h,
                });
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
    /// `Arc` parce que le fil de rendu du moteur écrit sur le même clavier, et
    /// qu'il survit à la fenêtre : il ne peut donc rien emprunter à l'état
    /// d'une commande.
    pub(crate) keyboard: Arc<Mutex<Option<Keyboard>>>,
    pub(crate) engine: runtime::Engine,
    /// Dernier échec d'ouverture, **par appareil**, indexé sur VID/PID.
    ///
    /// Une table plutôt qu'un champ unique : c'est ce qui fait qu'un appareil
    /// en échec n'en entraîne aucun autre. Un message global obligerait à
    /// choisir lequel afficher, et le suivant effacerait le précédent.
    pub(crate) failures: Mutex<HashMap<(u16, u16), String>>,
}

/// Gabarit utilisé quand aucun périphérique n'est connecté.
///
/// Sert au moteur et au simulateur : écrire un effet ne doit pas exiger de
/// posséder le clavier.
pub(crate) fn default_layout() -> &'static Layout {
    LAYOUTS[0]
}

/// Les erreurs remontent au front sous forme de chaîne : l'interface les
/// affiche telles quelles, elles doivent donc rester lisibles.
type CmdResult<T> = Result<T, String>;

fn hid() -> CmdResult<hidapi::HidApi> {
    hidapi::HidApi::new().map_err(|e| format!("initialisation HID impossible : {e}"))
}

fn find_layout(vid: u16, pid: u16) -> CmdResult<&'static Layout> {
    LAYOUTS
        .iter()
        .copied()
        .find(|l| l.vid == vid && l.pid == pid)
        .ok_or_else(|| format!("aucun gabarit connu pour {vid:#06x}:{pid:#06x}"))
}

/// Numéro de série de l'exemplaire branché — s'il y en a un de branché.
///
/// Les deux niveaux disent deux choses différentes et ne se confondent pas :
/// `None` veut dire **débranché**, `Some(None)` **branché sans série déclarée**.
/// Le second n'est pas un cas dégénéré — c'est ce que rend hidraw sous Linux
/// quand la règle udev n'accorde pas la lecture des attributs.
fn plugged(api: &hidapi::HidApi, layout: &Layout) -> Option<Option<String>> {
    api.device_list()
        .find(|d| {
            d.vendor_id() == layout.vid
                && d.product_id() == layout.pid
                && d.interface_number() == layout.interface as i32
        })
        .map(|d| {
            d.serial_number()
                .map(str::to_owned)
                .filter(|s| !s.is_empty())
        })
}

// ---------------------------------------------------------------- adoption

/// Ce qu'a donné la tentative d'ouverture d'**un** appareil.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct OpenOutcome {
    pub vid: u16,
    pub pid: u16,
    pub error: Option<String>,
}

/// Ouvre les appareils **pilotés et présents**, un par un.
///
/// C'est la boucle de démarrage, écrite sans Tauri ni HID — la présence et
/// l'ouverture arrivent en argument — pour que son invariant soit vérifiable
/// par un test ordinaire : *l'échec d'un appareil n'en entraîne aucun autre*.
/// Chaque tentative produit sa propre ligne de compte-rendu, et une erreur
/// n'interrompt pas la boucle.
///
/// Ne renvoie qu'un seul appareil ouvert : `AppState` n'en porte qu'un pour
/// l'instant (issue #26). Les pilotés suivants sont donc laissés fermés plutôt
/// qu'ouverts puis relâchés aussitôt — toucher un appareil qu'on ne pilotera
/// pas serait exactement ce que l'adoption sert à éviter.
fn open_adopted<K>(
    layouts: &[&'static Layout],
    settings: &Settings,
    present: impl Fn(&Layout) -> Option<Option<String>>,
    mut open: impl FnMut(&'static Layout) -> Result<K, String>,
) -> (Option<K>, Vec<OpenOutcome>) {
    let mut opened = None;
    let mut outcomes = Vec::new();

    for layout in layouts {
        // Débranché : rien à ouvrir, et rien à signaler non plus.
        let Some(serial) = present(layout) else {
            continue;
        };
        if settings.device_state(layout.vid, layout.pid, serial.as_deref()) != DeviceState::Adopted
        {
            continue;
        }
        if opened.is_some() {
            continue;
        }
        let error = match open(layout) {
            Ok(k) => {
                opened = Some(k);
                None
            }
            Err(e) => Some(e),
        };
        outcomes.push(OpenOutcome {
            vid: layout.vid,
            pid: layout.pid,
            error,
        });
    }

    (opened, outcomes)
}

/// Applique les décisions retenues, au démarrage de l'application.
///
/// Ne renvoie rien et ne peut pas échouer : des réglages illisibles ou un HID
/// absent ne doivent pas empêcher la fenêtre de s'ouvrir — c'est elle qui
/// permettrait de corriger la situation.
fn apply_adoptions(app: &AppHandle, state: &AppState) {
    let settings = match storage::store(app).and_then(|s| s.read_settings()) {
        Ok(settings) => settings,
        Err(e) => {
            eprintln!("adoption : aucun appareil ouvert, {e}");
            return;
        }
    };
    let api = match hid() {
        Ok(api) => api,
        Err(e) => {
            eprintln!("adoption : aucun appareil ouvert, {e}");
            return;
        }
    };

    let (keyboard, outcomes) = open_adopted(
        LAYOUTS,
        &settings,
        |l| plugged(&api, l),
        |l| Keyboard::open(&api, l).map_err(|e| e.to_string()),
    );

    let mut failures = state.failures.lock().unwrap();
    for outcome in outcomes {
        match outcome.error {
            Some(e) => {
                eprintln!(
                    "adoption : {:#06x}:{:#06x} non ouvert — {e}",
                    outcome.vid, outcome.pid
                );
                failures.insert((outcome.vid, outcome.pid), e);
            }
            None => {
                failures.remove(&(outcome.vid, outcome.pid));
            }
        }
    }
    // Rien d'autre n'a encore pu ouvrir quoi que ce soit : l'état vient d'être
    // construit et n'est pas encore confié au gestionnaire.
    *state.keyboard.lock().unwrap() = keyboard;
}

// ---------------------------------------------------------------- commandes

/// Liste les gabarits connus : branchés ou non, et surtout **dans quel état**.
#[tauri::command]
fn list_devices(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Vec<DeviceInfo>> {
    let settings = storage::store(&app)?.read_settings()?;
    let api = hid()?;
    // Les deux verrous ne sont jamais tenus ensemble, et l'ordre n'est pas
    // indifférent : `ignore_device` prend le clavier puis les échecs. Les
    // prendre ici dans l'ordre inverse, tous les deux à la fois, suffirait à
    // bloquer les deux commandes l'une contre l'autre.
    let ouvert = state
        .keyboard
        .lock()
        .unwrap()
        .as_ref()
        .map(|kb| (kb.layout().vid, kb.layout().pid));
    let failures = state.failures.lock().unwrap();

    Ok(LAYOUTS
        .iter()
        .map(|l| {
            let branche = plugged(&api, l);
            let present = branche.is_some();
            let serial = branche.flatten();
            DeviceInfo {
                name: l.name.to_string(),
                vid: l.vid,
                pid: l.pid,
                present,
                state: settings.device_state(l.vid, l.pid, serial.as_deref()),
                open: ouvert == Some((l.vid, l.pid)),
                error: failures.get(&(l.vid, l.pid)).cloned(),
            }
        })
        .collect())
}

/// Retient « piloté » pour cet appareil, et l'ouvre s'il est là.
///
/// La décision est écrite **avant** l'ouverture, et elle tient même si
/// celle-ci échoue : c'est une décision, pas le compte rendu d'une tentative.
/// Le prochain démarrage la rejouera, ce qui est précisément ce qu'on veut
/// d'un clavier qu'un concentrateur n'a pas encore fini d'énumérer.
///
/// Rend le gabarit quand l'appareil a été ouvert, `None` quand il est adopté
/// mais débranché — ce n'est pas une erreur, il sera ouvert au branchement
/// suivant.
#[tauri::command]
fn adopt_device(
    app: AppHandle,
    state: State<'_, AppState>,
    vid: u16,
    pid: u16,
) -> CmdResult<Option<LayoutInfo>> {
    let layout = find_layout(vid, pid)?;
    let api = hid()?;
    let branche = plugged(&api, layout);
    let present = branche.is_some();
    let serial = branche.flatten();

    let store = storage::store(&app)?;
    let mut settings = store.read_settings()?;
    settings.set_device_state(vid, pid, serial.as_deref(), DeviceState::Adopted);
    store.write_settings(&settings)?;

    if !present {
        return Ok(None);
    }
    match Keyboard::open(&api, layout) {
        Ok(kb) => {
            *state.keyboard.lock().unwrap() = Some(kb);
            state.failures.lock().unwrap().remove(&(vid, pid));
            Ok(Some(LayoutInfo::from(layout)))
        }
        Err(e) => {
            let message = e.to_string();
            state
                .failures
                .lock()
                .unwrap()
                .insert((vid, pid), message.clone());
            Err(message)
        }
    }
}

/// Retient « ignoré », et referme l'appareil s'il était ouvert.
///
/// Ne passe pas par HID : ignorer un appareil doit rester possible quand c'est
/// justement l'accès HID qui pose problème. La série n'est donc relevée que si
/// elle se donne — [`storage::DeviceRecord::matches`] retrouve l'entrée sans.
#[tauri::command]
fn ignore_device(app: AppHandle, state: State<'_, AppState>, vid: u16, pid: u16) -> CmdResult<()> {
    let layout = find_layout(vid, pid)?;
    let serial = hid().ok().and_then(|api| plugged(&api, layout)).flatten();

    let store = storage::store(&app)?;
    let mut settings = store.read_settings()?;
    settings.set_device_state(vid, pid, serial.as_deref(), DeviceState::Ignored);
    store.write_settings(&settings)?;

    // « Laissé tranquille » : on ne garde pas ouvert ce qu'on s'engage à ne
    // plus toucher. Le bloc rend le verrou avant de prendre celui des échecs —
    // les tenir tous les deux ici, quand `list_devices` les prend dans l'autre
    // ordre, suffirait à bloquer les deux commandes l'une contre l'autre.
    {
        let mut guard = state.keyboard.lock().unwrap();
        if guard
            .as_ref()
            .is_some_and(|kb| kb.layout().vid == vid && kb.layout().pid == pid)
        {
            *guard = None;
        }
    }
    state.failures.lock().unwrap().remove(&(vid, pid));
    Ok(())
}

/// Ouverture ponctuelle, sans rien décider.
///
/// Distincte d'[`adopt_device`] : elle ne touche pas à `settings.json`, donc
/// elle ne survit pas au redémarrage. C'est ce qu'on veut pour essayer un
/// appareil sans s'engager.
#[tauri::command]
fn connect(state: State<'_, AppState>, vid: u16, pid: u16) -> CmdResult<LayoutInfo> {
    let layout = find_layout(vid, pid)?;

    let api = hid()?;
    let kb = Keyboard::open(&api, layout).map_err(|e| e.to_string())?;
    *state.keyboard.lock().unwrap() = Some(kb);
    state.failures.lock().unwrap().remove(&(vid, pid));
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

/// Gabarit servant de repli quand rien n'est connecté.
///
/// `get_layout` refuse hors connexion, et c'est justement le cas qu'il faut
/// servir : on dessine le clavier et on écrit un effet **avant** d'avoir
/// branché quoi que ce soit, ou sans posséder le clavier.
///
/// Sans cette commande, l'interface n'aurait d'autre choix que de recopier la
/// géométrie — une seconde source de vérité qui divergerait en silence.
#[tauri::command]
fn get_default_layout() -> LayoutInfo {
    LayoutInfo::from(default_layout())
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
            let state = AppState::default();
            // Avant `manage` : l'état n'est plus accessible ensuite qu'à
            // travers le gestionnaire, et l'adoption n'a besoin que de lui.
            apply_adoptions(app.handle(), &state);
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_devices,
            adopt_device,
            ignore_device,
            connect,
            disconnect,
            is_connected,
            get_layout,
            get_default_layout,
            set_brightness,
            set_effect,
            present,
            write_row,
            runtime::start_effect,
            runtime::stop_effect,
            runtime::set_effect_params,
            runtime::set_output_to_keyboard,
            runtime::subscribe_frames,
            runtime::unsubscribe_frames,
            runtime::engine_status,
            storage::install_effect,
            storage::list_effects,
            storage::delete_effect,
            storage::read_effect_source,
            storage::get_settings,
            storage::set_settings,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application");
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    /// Deux gabarits inventés : le seul gabarit réel est unique, et l'invariant
    /// à vérifier — un appareil n'en entraîne aucun autre — n'a de sens qu'à
    /// partir de deux. Ils ne servent qu'à être identifiés, d'où la matrice
    /// minimale.
    static PREMIER: Layout = Layout {
        name: "Premier",
        vid: 0x1532,
        pid: 0x1111,
        interface: 3,
        rows: 1,
        cols: 1,
        matrix: &[0],
        keys: &[],
    };
    static SECOND: Layout = Layout {
        name: "Second",
        vid: 0x1532,
        pid: 0x2222,
        interface: 3,
        rows: 1,
        cols: 1,
        matrix: &[0],
        keys: &[],
    };

    /// Série propre à chaque gabarit, comme une énumération réelle.
    fn branches(l: &Layout) -> Option<Option<String>> {
        Some(Some(format!("S{:04x}", l.pid)))
    }

    fn pilotes() -> Settings {
        let mut settings = Settings::default();
        for l in [&PREMIER, &SECOND] {
            settings.set_device_state(
                l.vid,
                l.pid,
                branches(l).flatten().as_deref(),
                DeviceState::Adopted,
            );
        }
        settings
    }

    /// **Le cœur de l'adoption.** Le premier appareil refuse de s'ouvrir ; le
    /// second doit s'ouvrir quand même, et le message d'échec rester attaché à
    /// celui qui a échoué.
    #[test]
    fn un_appareil_en_echec_n_en_bloque_aucun_autre() {
        let mut tentatives = Vec::new();

        let (ouvert, comptes) = open_adopted(&[&PREMIER, &SECOND], &pilotes(), branches, |l| {
            tentatives.push(l.pid);
            if l.pid == PREMIER.pid {
                Err("accès refusé par le système".into())
            } else {
                Ok(l.name)
            }
        });

        assert_eq!(
            tentatives,
            vec![PREMIER.pid, SECOND.pid],
            "la boucle s'est arrêtée au premier échec"
        );
        assert_eq!(ouvert, Some("Second"));

        assert_eq!(comptes.len(), 2);
        assert_eq!(
            comptes[0],
            OpenOutcome {
                vid: PREMIER.vid,
                pid: PREMIER.pid,
                error: Some("accès refusé par le système".into()),
            }
        );
        assert_eq!(
            comptes[1],
            OpenOutcome {
                vid: SECOND.vid,
                pid: SECOND.pid,
                error: None,
            },
            "l'échec du premier a débordé sur le second"
        );
    }

    /// Ni « détecté » ni « ignoré » ne doivent produire la moindre ouverture :
    /// c'est toute la raison d'être des trois états.
    #[test]
    fn seul_un_appareil_pilote_est_ouvert() {
        let mut settings = Settings::default();
        settings.set_device_state(
            SECOND.vid,
            SECOND.pid,
            branches(&SECOND).flatten().as_deref(),
            DeviceState::Ignored,
        );

        let mut tentatives = 0;
        let (ouvert, comptes) = open_adopted(
            &[&PREMIER, &SECOND], // PREMIER n'a jamais été vu : détecté
            &settings,
            branches,
            |l| {
                tentatives += 1;
                Ok(l.name)
            },
        );

        assert_eq!(tentatives, 0, "un appareil non piloté a été ouvert");
        assert_eq!(ouvert, None);
        assert!(comptes.is_empty());
    }

    /// Adopté mais débranché : rien à ouvrir, et surtout rien à signaler — ce
    /// n'est pas un échec, l'appareil est simplement ailleurs.
    #[test]
    fn un_appareil_pilote_mais_debranche_ne_produit_aucune_erreur() {
        let (ouvert, comptes) = open_adopted(
            &[&PREMIER, &SECOND],
            &pilotes(),
            |_| None,
            |l| Ok(l.name) as Result<&'static str, String>,
        );

        assert_eq!(ouvert, None);
        assert!(comptes.is_empty());
    }

    /// `AppState` ne porte qu'un clavier (issue #26) : le second appareil
    /// piloté est laissé fermé, pas ouvert puis relâché. Ouvrir un appareil
    /// qu'on ne pilotera pas est exactement ce que l'adoption sert à éviter.
    #[test]
    fn le_second_appareil_pilote_n_est_pas_touche_pour_rien() {
        let mut tentatives = Vec::new();
        let (ouvert, comptes) = open_adopted(&[&PREMIER, &SECOND], &pilotes(), branches, |l| {
            tentatives.push(l.pid);
            Ok(l.name)
        });

        assert_eq!(tentatives, vec![PREMIER.pid]);
        assert_eq!(ouvert, Some("Premier"));
        assert_eq!(comptes.len(), 1);
    }
}
