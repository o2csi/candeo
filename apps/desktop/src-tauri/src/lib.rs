//! Liaison entre l'interface et le matériel.
//!
//! Les types exposés au front sont définis ici plutôt que dans les crates :
//! `candeo-protocol` et `candeo-device` restent ainsi sans dépendance à serde
//! ni à Tauri, et donc réutilisables et testables hors application.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use candeo_device::{Keyboard, Layout, DEATHSTALKER_V2_PRO};
use candeo_protocol::{Effect, Rgb};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use storage::{DeviceState, Settings};

mod builtins;
mod journal;
mod runtime;
mod single_instance;
/// Sondes matérielles, toutes `#[ignore]` — voir le module.
#[cfg(test)]
mod sonde;
mod storage;

/// Gabarits connus. Un seul pour l'instant.
///
/// Visible dans la crate : le diagnostic de [`journal`] énumère les mêmes
/// appareils que [`list_devices`], et les recopier là-bas en ferait une seconde
/// liste qui divergerait au premier gabarit ajouté.
pub(crate) const LAYOUTS: &[&Layout] = &[&DEATHSTALKER_V2_PRO];

// ---------------------------------------------------------------- types exposés

// Les champs partent en camelCase : c'est la convention du côté qui les lit.
// Laisser filtrer le nommage Rust jusque dans l'interface serait une fuite
// d'abstraction, et elle ne se verrait qu'à l'exécution.

/// Désigne un appareil, et rien d'autre.
///
/// VID et PID, comme l'adoption les identifie (issue #25) : c'est la clé de la
/// table des appareils ouverts, de celle des échecs d'ouverture et de celle des
/// boucles de rendu. Le numéro de série départage deux exemplaires du même
/// modèle dans `settings.json`, mais il ne peut pas servir de clé ici — une
/// énumération muette (hidraw sans règle udev) n'en déclare aucun, et l'appareil
/// deviendrait indésignable.
///
/// Un type plutôt que deux entiers baladés côte à côte : il apparaît en argument
/// de commande **et** dans l'état que rend le moteur, et les inverser ne se
/// verrait qu'à l'exécution.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRef {
    pub vid: u16,
    pub pid: u16,
}

impl DeviceRef {
    pub(crate) fn of(layout: &Layout) -> Self {
        Self {
            vid: layout.vid,
            pid: layout.pid,
        }
    }
}

/// Tel qu'il apparaît dans un message destiné à être lu.
impl std::fmt::Display for DeviceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#06x}:{:#06x}", self.vid, self.pid)
    }
}

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

/// Tout ce que l'application tient ouvert, **appareil par appareil**.
///
/// # Ordre de prise des verrous
///
/// Un interblocage a déjà été attrapé ici : `list_devices` prenait le verrou du
/// clavier puis celui des échecs, `ignore_device` l'inverse. Avec une table
/// d'appareils et une boucle de rendu par appareil, la règle est donc explicite
/// et tenue :
///
/// > **Aucun code ne tient deux de ces verrous en même temps.** Une table est
/// > verrouillée le temps d'y lire ou d'y poser un `Arc`, jamais le temps d'une
/// > écriture HID, d'un démarrage de boucle ni d'une attente de fin.
///
/// C'est ce que font [`AppState::handle`] et [`AppState::open_handles`] : elles
/// clonent sous le verrou de la table et rendent celui-ci avant d'interroger
/// quoi que ce soit. Si deux verrous devenaient inévitables, l'ordre est celui
/// de la déclaration ci-dessous — `devices`, puis `engine`, puis `failures`,
/// puis la poignée d'un appareil, puis l'état partagé d'une boucle. Le fil de
/// rendu, lui, ne connaît que les deux derniers : il n'a aucun moyen de prendre
/// un verrou de l'application, donc aucun moyen d'en bloquer une commande.
#[derive(Default)]
pub struct AppState {
    /// Les appareils, **un par poignée**.
    ///
    /// Une entrée apparaît dès qu'un appareil est visé et n'est plus retirée ;
    /// c'est son contenu qui dit s'il est ouvert. Refermer met l'`Option` à
    /// `None` sans toucher à l'`Arc` : la boucle qui en tient une copie s'en
    /// aperçoit à l'image suivante et cesse d'écrire, au lieu de continuer sur
    /// une poignée que plus personne ne regarde.
    pub(crate) devices: Mutex<HashMap<DeviceRef, runtime::Handle>>,
    /// Les boucles de rendu, une par appareil. Voir [`runtime`].
    pub(crate) engine: runtime::Engine,
    /// Dernier échec d'ouverture, **par appareil**.
    ///
    /// Une table plutôt qu'un champ unique : c'est ce qui fait qu'un appareil
    /// en échec n'en entraîne aucun autre. Un message global obligerait à
    /// choisir lequel afficher, et le suivant effacerait le précédent.
    pub(crate) failures: Mutex<HashMap<DeviceRef, String>>,
}

impl AppState {
    /// La poignée de cet appareil, créée fermée si elle n'existait pas.
    ///
    /// Le verrou de la table n'est tenu que le temps du clonage : ce qu'on fera
    /// ensuite de la poignée — une écriture HID, une boucle qui démarre — ne
    /// doit retenir aucune commande visant un autre appareil.
    pub(crate) fn handle(&self, device: DeviceRef) -> runtime::Handle {
        Arc::clone(self.devices.lock().unwrap().entry(device).or_default())
    }

    /// La poignée de cet appareil, **sans en créer une**.
    ///
    /// Les commandes qui exigent un appareil ouvert passent par ici : viser un
    /// appareil inconnu doit être un refus, pas une ligne de plus dans la table.
    fn opened(&self, device: DeviceRef) -> Option<runtime::Handle> {
        self.devices.lock().unwrap().get(&device).map(Arc::clone)
    }

    /// Ouvre — ou referme, avec `None` — cet appareil.
    fn set_open(&self, device: DeviceRef, keyboard: Option<Keyboard>) {
        *self.handle(device).lock().unwrap() = keyboard;
    }

    /// Les appareils réellement ouverts en ce moment.
    ///
    /// Deux temps, et jamais les deux verrous ensemble : on copie les poignées
    /// sous le verrou de la table, on le rend, puis on les interroge. Les
    /// interroger sur place bloquerait toute la table pendant l'écriture HID
    /// d'une boucle.
    fn open_handles(&self) -> HashSet<DeviceRef> {
        let handles: Vec<(DeviceRef, runtime::Handle)> = self
            .devices
            .lock()
            .unwrap()
            .iter()
            .map(|(d, h)| (*d, Arc::clone(h)))
            .collect();

        handles
            .into_iter()
            .filter(|(_, h)| h.lock().unwrap().is_some())
            .map(|(d, _)| d)
            .collect()
    }
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

pub(crate) fn hid() -> CmdResult<hidapi::HidApi> {
    hidapi::HidApi::new().map_err(|e| format!("initialisation HID impossible : {e}"))
}

fn find_layout(device: DeviceRef) -> CmdResult<&'static Layout> {
    LAYOUTS
        .iter()
        .copied()
        .find(|l| DeviceRef::of(l) == device)
        .ok_or_else(|| format!("aucun gabarit connu pour {device}"))
}

/// Numéro de série de l'exemplaire branché — s'il y en a un de branché.
///
/// Les deux niveaux disent deux choses différentes et ne se confondent pas :
/// `None` veut dire **débranché**, `Some(None)` **branché sans série déclarée**.
/// Le second n'est pas un cas dégénéré — c'est ce que rend hidraw sous Linux
/// quand la règle udev n'accorde pas la lecture des attributs.
pub(crate) fn plugged(api: &hidapi::HidApi, layout: &Layout) -> Option<Option<String>> {
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
    pub device: DeviceRef,
    pub error: Option<String>,
}

/// Ouvre **tous** les appareils pilotés et présents, un par un.
///
/// C'est la boucle de démarrage, écrite sans Tauri ni HID — la présence et
/// l'ouverture arrivent en argument — pour que son invariant soit vérifiable
/// par un test ordinaire : *l'échec d'un appareil n'en entraîne aucun autre*.
/// Chaque tentative produit sa propre ligne de compte-rendu, et une erreur
/// n'interrompt pas la boucle.
///
/// Tous, et non plus un seul : `AppState` porte désormais une table d'appareils
/// ouverts (issue #26). Le second appareil piloté n'est donc plus laissé fermé
/// faute de place pour lui.
fn open_adopted<K>(
    layouts: &[&'static Layout],
    settings: &Settings,
    present: impl Fn(&Layout) -> Option<Option<String>>,
    mut open: impl FnMut(&'static Layout) -> Result<K, String>,
) -> (Vec<(&'static Layout, K)>, Vec<OpenOutcome>) {
    let mut opened = Vec::new();
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
        let error = match open(layout) {
            Ok(k) => {
                opened.push((*layout, k));
                None
            }
            Err(e) => Some(e),
        };
        outcomes.push(OpenOutcome {
            device: DeviceRef::of(layout),
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
            tracing::error!("adoption abandonnée, aucun appareil ouvert : {e}");
            return;
        }
    };
    let api = match hid() {
        Ok(api) => api,
        Err(e) => {
            tracing::error!("adoption abandonnée, aucun appareil ouvert : {e}");
            return;
        }
    };

    let (ouverts, outcomes) = open_adopted(
        LAYOUTS,
        &settings,
        |l| plugged(&api, l),
        |l| Keyboard::open(&api, l).map_err(|e| e.to_string()),
    );

    // Les poignées d'abord, les échecs ensuite : deux tables, jamais verrouillées
    // ensemble. Rien d'autre n'a encore pu ouvrir quoi que ce soit — l'état vient
    // d'être construit et n'est pas encore confié au gestionnaire.
    for (layout, keyboard) in ouverts {
        let device = DeviceRef::of(layout);
        // L'empreinte plutôt que la série : elle distingue deux exemplaires du
        // même modèle sans divulguer lequel. Voir [`journal::empreinte`]. Le
        // second `plugged` ne réénumère rien — il relit la liste que `api` tient
        // déjà — et il évite de faire porter la série par [`OpenOutcome`], qui
        // rend compte d'une tentative et n'a pas à décrire l'appareil.
        let serie = journal::empreinte_de(plugged(&api, layout).flatten().as_deref());
        tracing::info!(appareil = %device, serie, "appareil adopté ouvert");
        state.set_open(device, Some(keyboard));
    }

    let mut failures = state.failures.lock().unwrap();
    for outcome in outcomes {
        match outcome.error {
            Some(e) => {
                // `error` et non `warn` : un appareil adopté qui ne s'ouvre pas,
                // c'est l'éclairage de l'utilisateur qui ne s'allumera pas.
                tracing::error!(appareil = %outcome.device, "appareil adopté non ouvert : {e}");
                failures.insert(outcome.device, e);
            }
            None => {
                failures.remove(&outcome.device);
            }
        }
    }
}

// ---------------------------------------------------------------- commandes

/// Liste les gabarits connus : branchés ou non, et surtout **dans quel état**.
#[tauri::command]
fn list_devices(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Vec<DeviceInfo>> {
    let settings = storage::store(&app)?.read_settings()?;
    let api = hid()?;
    // Les deux relevés sont faits l'un après l'autre, chacun rendant son verrou
    // avant le suivant. C'est la règle documentée sur [`AppState`], et elle vient
    // d'un interblocage réel : cette commande prenait le clavier puis les échecs,
    // `ignore_device` l'inverse, et les deux se bloquaient l'une l'autre.
    let ouverts = state.open_handles();
    let failures = state.failures.lock().unwrap().clone();

    Ok(LAYOUTS
        .iter()
        .map(|l| {
            let device = DeviceRef::of(l);
            let branche = plugged(&api, l);
            let present = branche.is_some();
            let serial = branche.flatten();
            DeviceInfo {
                name: l.name.to_string(),
                vid: l.vid,
                pid: l.pid,
                present,
                state: settings.device_state(l.vid, l.pid, serial.as_deref()),
                open: ouverts.contains(&device),
                error: failures.get(&device).cloned(),
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
    let device = DeviceRef { vid, pid };
    let layout = find_layout(device)?;
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
            tracing::info!(
                appareil = %device,
                serie = journal::empreinte_de(serial.as_deref()),
                "appareil piloté"
            );
            // Les autres appareils ouverts le restent : adopter celui-ci n'est
            // pas un choix à la place des autres.
            state.set_open(device, Some(kb));
            state.failures.lock().unwrap().remove(&device);
            Ok(Some(LayoutInfo::from(layout)))
        }
        Err(e) => {
            let message = e.to_string();
            tracing::error!(appareil = %device, "appareil non ouvert : {message}");
            state
                .failures
                .lock()
                .unwrap()
                .insert(device, message.clone());
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
    let device = DeviceRef { vid, pid };
    let layout = find_layout(device)?;
    let serial = hid().ok().and_then(|api| plugged(&api, layout)).flatten();

    let store = storage::store(&app)?;
    let mut settings = store.read_settings()?;
    settings.set_device_state(vid, pid, serial.as_deref(), DeviceState::Ignored);
    store.write_settings(&settings)?;

    tracing::info!(appareil = %device, "appareil ignoré et refermé");

    // « Laissé tranquille » : on ne garde pas ouvert ce qu'on s'engage à ne plus
    // toucher. La poignée est vidée, pas retirée : la boucle qui l'alimentait en
    // tient une copie, elle s'en aperçoit à l'image suivante et cesse d'écrire —
    // sans que les autres appareils soient touchés.
    state.set_open(device, None);
    state.failures.lock().unwrap().remove(&device);
    Ok(())
}

/// Ramène **tous** les appareils à un état connu : rien ne tourne, rien n'est
/// allumé, rien n'est ouvert.
///
/// Le pendant de [`apply_adoptions`], et ce qui permet à
/// [`storage::reset_settings`] de tenir sa promesse : après elle, plus aucune
/// boucle ni aucune poignée ne dépend de décisions que le fichier ne porte plus.
///
/// Trois temps, et l'ordre n'est pas indifférent :
///
/// 1. les boucles s'arrêtent, et l'arrêt est **attendu** — l'image suivante
///    rallumerait ce qu'on est sur le point d'éteindre ;
/// 2. le rétroéclairage s'éteint. Arrêter une boucle laisse le clavier sur sa
///    dernière image, et une image figée ressemble à un effet qui tourne encore ;
///    `Effect::Off` laisse l'appareil dans un état qui se lit, pour un coût nul —
///    c'est le micrologiciel qui l'exécute ;
/// 3. les poignées sont vidées, comme le fait [`ignore_device`] : on ne garde pas
///    ouvert un appareil que plus aucune décision ne désigne.
///
/// Les échecs d'ouverture partent avec : ils rendaient compte de tentatives
/// faites pour des adoptions qui n'existent plus, et les garder afficherait une
/// erreur rouge sur un appareil dont personne n'a plus rien demandé.
///
/// Rien ne remonte, et c'est délibéré : un clavier qui refuse de s'éteindre —
/// débranché entre-temps, accès perdu — ne doit pas empêcher la remise à zéro.
/// L'extinction est un agrément, pas le geste.
pub(crate) fn release_devices(state: &AppState) {
    state.engine.stop_all();

    for device in state.open_handles() {
        let _ = with_keyboard(state, device, |kb| {
            kb.set_effect(Effect::Off).map_err(|e| e.to_string())
        });
        state.set_open(device, None);
    }

    state.failures.lock().unwrap().clear();
}

/// Ouverture ponctuelle, sans rien décider.
///
/// Distincte d'[`adopt_device`] : elle ne touche pas à `settings.json`, donc
/// elle ne survit pas au redémarrage. C'est ce qu'on veut pour essayer un
/// appareil sans s'engager.
#[tauri::command]
fn connect(state: State<'_, AppState>, vid: u16, pid: u16) -> CmdResult<LayoutInfo> {
    let device = DeviceRef { vid, pid };
    let layout = find_layout(device)?;

    let api = hid()?;
    let kb = Keyboard::open(&api, layout).map_err(|e| e.to_string())?;
    state.set_open(device, Some(kb));
    state.failures.lock().unwrap().remove(&device);
    Ok(LayoutInfo::from(layout))
}

/// Referme **un** appareil. Les autres ne sont pas touchés.
#[tauri::command]
fn disconnect(state: State<'_, AppState>, device: DeviceRef) {
    state.set_open(device, None);
}

#[tauri::command]
fn is_connected(state: State<'_, AppState>, device: DeviceRef) -> bool {
    state
        .opened(device)
        .is_some_and(|h| h.lock().unwrap().is_some())
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

/// Agit sur la poignée d'un appareil, **si** il est ouvert.
///
/// Un seul verrou est tenu, celui de la poignée, et jamais celui d'une table :
/// une écriture HID prend quelques millisecondes et ne doit retenir aucune
/// commande visant un autre appareil.
fn with_keyboard<T>(
    state: &AppState,
    device: DeviceRef,
    f: impl FnOnce(&Keyboard) -> CmdResult<T>,
) -> CmdResult<T> {
    let handle = state
        .opened(device)
        .ok_or_else(|| format!("aucun appareil ouvert pour {device}"))?;
    let guard = handle.lock().unwrap();
    let kb = guard
        .as_ref()
        .ok_or_else(|| format!("aucun appareil ouvert pour {device}"))?;
    f(kb)
}

/// Gabarit d'un appareil ouvert.
#[tauri::command]
fn get_layout(state: State<'_, AppState>, device: DeviceRef) -> CmdResult<LayoutInfo> {
    with_keyboard(&state, device, |kb| Ok(LayoutInfo::from(kb.layout())))
}

#[tauri::command]
fn set_brightness(state: State<'_, AppState>, device: DeviceRef, level: u8) -> CmdResult<()> {
    with_keyboard(&state, device, |kb| {
        kb.set_brightness(level).map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn set_effect(state: State<'_, AppState>, device: DeviceRef, effect: EffectDto) -> CmdResult<()> {
    with_keyboard(&state, device, |kb| {
        kb.set_effect(effect.into()).map_err(|e| e.to_string())
    })
}

/// Pousse une image complète.
///
/// `frame` est une suite plate de triplets RGB et doit couvrir **toutes** les
/// cases de la matrice. En envoyer moins laisse les dernières rangées figées
/// sur leur valeur précédente — c'est le piège classique de ce matériel.
#[tauri::command]
fn present(state: State<'_, AppState>, device: DeviceRef, frame: Vec<u8>) -> CmdResult<()> {
    with_keyboard(&state, device, |kb| {
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
    })
}

/// Écrit un segment de rangée, sans toucher au reste.
#[tauri::command]
fn write_row(
    state: State<'_, AppState>,
    device: DeviceRef,
    row: u8,
    col_start: u8,
    colors: Vec<u8>,
) -> CmdResult<()> {
    with_keyboard(&state, device, |kb| {
        if colors.len() % 3 != 0 || colors.is_empty() {
            return Err("les couleurs doivent former des triplets RGB non vides".into());
        }
        let c: Vec<Rgb> = colors
            .chunks_exact(3)
            .map(|x| Rgb::new(x[0], x[1], x[2]))
            .collect();
        kb.write_row(row, col_start, &c).map_err(|e| e.to_string())
    })
}

// ---------------------------------------------------------------- point d'entrée

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // En tête, et l'ordre n'est pas décoratif : c'est là que le second
        // processus s'arrête, et il doit le faire avant d'avoir touché au
        // clavier. Voir [`single_instance`].
        .plugin(single_instance::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // **En tout premier**, et avant que le magasin ne soit résolu : un
            // échec de résolution du dossier de configuration est exactement ce
            // qu'on veut voir, et il arriverait sinon avant qu'il n'y ait de quoi
            // l'écrire. C'est aussi le premier instant où `app_log_dir()` existe
            // — tout ce qui précède, l'enregistrement des plugins, ne journalise
            // pas. Le niveau retenu, lui, est relu juste après : on démarre au
            // défaut puis on ajuste, jamais l'inverse.
            journal::init(app.handle());
            journal::relire_le_reglage(app.handle());

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
            storage::reset_settings,
            storage::remember_effect_params,
            journal::get_journal,
            journal::set_log_level,
            journal::open_log_dir,
            journal::diagnostic,
            journal::log_from_webview,
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

    /// Les gabarits réellement ouverts, par leur nom — la valeur que rend le
    /// faux « ouvrir » des tests.
    fn ouverts(opened: &[(&'static Layout, &'static str)]) -> Vec<&'static str> {
        opened.iter().map(|(_, name)| *name).collect()
    }

    /// **Le cœur de l'adoption.** Le premier appareil refuse de s'ouvrir ; le
    /// second doit s'ouvrir quand même, et le message d'échec rester attaché à
    /// celui qui a échoué.
    #[test]
    fn un_appareil_en_echec_n_en_bloque_aucun_autre() {
        let mut tentatives = Vec::new();

        let (opened, comptes) = open_adopted(&[&PREMIER, &SECOND], &pilotes(), branches, |l| {
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
        assert_eq!(ouverts(&opened), vec!["Second"]);

        assert_eq!(comptes.len(), 2);
        assert_eq!(
            comptes[0],
            OpenOutcome {
                device: DeviceRef::of(&PREMIER),
                error: Some("accès refusé par le système".into()),
            }
        );
        assert_eq!(
            comptes[1],
            OpenOutcome {
                device: DeviceRef::of(&SECOND),
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
        let (opened, comptes) = open_adopted(
            &[&PREMIER, &SECOND], // PREMIER n'a jamais été vu : détecté
            &settings,
            branches,
            |l| {
                tentatives += 1;
                Ok(l.name)
            },
        );

        assert_eq!(tentatives, 0, "un appareil non piloté a été ouvert");
        assert!(opened.is_empty());
        assert!(comptes.is_empty());
    }

    /// Adopté mais débranché : rien à ouvrir, et surtout rien à signaler — ce
    /// n'est pas un échec, l'appareil est simplement ailleurs.
    #[test]
    fn un_appareil_pilote_mais_debranche_ne_produit_aucune_erreur() {
        let (opened, comptes) = open_adopted(
            &[&PREMIER, &SECOND],
            &pilotes(),
            |_| None,
            |l| Ok(l.name) as Result<&'static str, String>,
        );

        assert!(opened.is_empty());
        assert!(comptes.is_empty());
    }

    /// Ce que l'issue #26 change : `AppState` porte une **table** d'appareils, le
    /// second piloté n'est donc plus laissé fermé faute de place. Chacun aura sa
    /// poignée, sa boucle et son effet.
    #[test]
    fn tous_les_appareils_pilotes_sont_ouverts() {
        let mut tentatives = Vec::new();
        let (opened, comptes) = open_adopted(&[&PREMIER, &SECOND], &pilotes(), branches, |l| {
            tentatives.push(l.pid);
            Ok(l.name)
        });

        assert_eq!(tentatives, vec![PREMIER.pid, SECOND.pid]);
        assert_eq!(ouverts(&opened), vec!["Premier", "Second"]);
        assert_eq!(comptes.len(), 2);
        assert!(comptes.iter().all(|c| c.error.is_none()));
    }

    /// La table est indexée sur VID/PID : deux appareils du même fabricant ne
    /// doivent pas se confondre, sans quoi ouvrir le second refermerait le
    /// premier.
    #[test]
    fn deux_appareils_du_meme_fabricant_ont_des_cles_distinctes() {
        let state = AppState::default();
        let premier = DeviceRef::of(&PREMIER);
        let second = DeviceRef::of(&SECOND);

        assert_ne!(premier, second);
        assert!(state.open_handles().is_empty());

        // Sans matériel on ne peut pas poser de `Keyboard` : on vérifie ce qui
        // ne dépend que de la table — la poignée d'un appareil est bien la
        // sienne, et viser un appareil jamais ouvert n'en fabrique aucune.
        assert!(state.opened(premier).is_none());
        let handle = state.handle(premier);
        assert!(state.opened(premier).is_some());
        assert!(
            state.opened(second).is_none(),
            "viser un appareil en a ouvert un autre"
        );
        assert!(Arc::ptr_eq(&handle, &state.handle(premier)));
    }
}
