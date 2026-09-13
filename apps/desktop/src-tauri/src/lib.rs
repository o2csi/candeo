//! Liaison entre l'interface et le matériel.
//!
//! Les types exposés au front sont définis ici plutôt que dans les crates :
//! `candeo-protocol` et `candeo-device` restent ainsi sans dépendance à serde
//! ni à Tauri, et donc réutilisables et testables hors application.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use candeo_device::{Inspection, Keyboard, Layout, DEATHSTALKER_V2_PRO};
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
mod tray;

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
    /// Micrologiciel contre lequel le gabarit a été relevé, `v1.5`.
    ///
    /// Connu sans rien ouvrir : c'est une donnée du gabarit. L'afficher même
    /// appareil fermé dit contre quoi le code a été établi, ce qui est la
    /// première question devant un comportement inexpliqué.
    pub surveyed_firmware: String,
    /// Micrologiciel **lu** à l'ouverture.
    ///
    /// `None` quand l'appareil n'est pas ouvert — rien n'a été demandé, et
    /// garder la version d'une ouverture précédente la ferait attribuer à
    /// l'exemplaire branché depuis — ou quand la lecture a échoué, ce que
    /// `warnings` dit alors.
    pub firmware: Option<String>,
    /// Ce que l'inspection à l'ouverture a trouvé qui mérite d'être vu.
    ///
    /// **Vide veut dire « rien à signaler », pas « compatible ».** L'octet d'état
    /// de l'appareil confirme qu'une commande existe, jamais que ses arguments
    /// sont bons. Aucun de ces avertissements ne bloque quoi que ce soit.
    pub warnings: Vec<String>,
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

    /// Ce que cet appareil a dit de lui-même à l'ouverture — **s'il est ouvert**.
    ///
    /// Une copie, prise sous le seul verrou de la poignée et rendue aussitôt :
    /// l'inspection a été faite une fois, à l'ouverture, et la relire ici ne
    /// coûte aucun échange USB. C'est ce qui permet à la liste des appareils et
    /// au diagnostic de l'afficher sans jamais toucher à la boucle de rendu.
    pub(crate) fn inspection(&self, device: DeviceRef) -> Option<Inspection> {
        let handle = self.opened(device)?;
        let guard = handle.lock().unwrap();
        guard.as_ref().map(|kb| kb.inspection().clone())
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
///
/// ⚠️ **Le descripteur USB du DeathStalker n'en porte aucun**, sur aucune
/// interface. La série qui apparie vraiment vient du protocole, à l'ouverture :
/// voir [`serie_connue`].
pub(crate) fn plugged(api: &hidapi::HidApi, layout: &Layout) -> Option<Option<String>> {
    api.device_list()
        .find(|d| layout.is_lighting_interface(d.vendor_id(), d.product_id(), d.interface_number()))
        .map(|d| {
            d.serial_number()
                .map(str::to_owned)
                .filter(|s| !s.is_empty())
        })
}

/// La série de l'exemplaire : **celle du protocole d'abord**, celle du
/// descripteur USB à défaut.
///
/// Le protocole (`0x00`/`0x82`) est la seule source qui en donne une sur ce
/// matériel, mais il ne se lit qu'appareil ouvert — et un appareil ignoré ou
/// détecté ne s'ouvre pas pour qu'on lui pose la question. Le descripteur reste
/// donc le repli, et [`storage::DeviceRecord::matches`] tolère qu'il soit muet.
pub(crate) fn serie_connue(inspection: Option<&Inspection>, usb: Option<String>) -> Option<String> {
    inspection.and_then(|i| i.serial.clone().ok()).or(usb)
}

/// Consigne une ouverture, et ce que l'inspection y a trouvé.
///
/// Une ligne `info` pour l'ouverture elle-même — le micrologiciel y figure, c'est
/// le premier champ qu'on cherchera — puis une ligne `warn` par avertissement.
/// **À l'ouverture et à elle seule** : c'est une transition, et l'inspection
/// n'est jamais refaite.
///
/// L'empreinte de la série, jamais la série : elle identifie un exemplaire
/// précis, et un journal finit collé dans un rapport de bogue.
fn consigner_l_ouverture(device: DeviceRef, keyboard: &Keyboard, usb: Option<String>, quoi: &str) {
    let inspection = keyboard.inspection();
    let serie = serie_connue(Some(inspection), usb);
    tracing::info!(
        appareil = %device,
        serie = journal::empreinte_de(serie.as_deref()),
        micrologiciel = %inspection
            .firmware
            .as_ref()
            .map_or_else(|e| format!("non lu ({e})"), ToString::to_string),
        "{quoi}"
    );
    for avertissement in inspection.warnings(keyboard.layout()) {
        tracing::warn!(appareil = %device, "{avertissement}");
    }
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
///
/// # La série se vérifie **après** l'ouverture
///
/// Le descripteur USB ne porte pas de série sur ce matériel : avant d'ouvrir,
/// la décision ne peut s'apparier que sur le VID et le PID, et n'importe quel
/// exemplaire du modèle passe. `serial_of` rend celle que l'appareil a donnée
/// par le protocole ; si elle désigne un exemplaire que la décision ne couvre
/// pas, la poignée est **relâchée** et la tentative rend compte de pourquoi.
/// Sans quoi brancher le clavier d'un collègue — même modèle — le ferait
/// piloter au nom d'une décision prise pour un autre.
fn open_adopted<K>(
    layouts: &[&'static Layout],
    settings: &Settings,
    present: impl Fn(&Layout) -> Option<Option<String>>,
    mut open: impl FnMut(&'static Layout) -> Result<K, String>,
    serial_of: impl Fn(&K) -> Option<String>,
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
            Ok(k) => match autre_exemplaire(settings, layout, serial_of(&k).as_deref()) {
                None => {
                    opened.push((*layout, k));
                    None
                }
                // `k` tombe ici, et la poignée se referme avec lui.
                Some(refus) => Some(refus),
            },
            Err(e) => Some(e),
        };
        outcomes.push(OpenOutcome {
            device: DeviceRef::of(layout),
            error,
        });
    }

    (opened, outcomes)
}

/// `Some(raison)` si l'exemplaire ouvert n'est pas celui que la décision désigne.
///
/// Sans série lue, rien ne se conclut : un appareil qui ne répond pas aux
/// lectures reste apparié comme avant, sur son VID et son PID. Refuser pour une
/// question restée sans réponse éteindrait l'éclairage de quelqu'un qui n'a
/// qu'un exemplaire.
fn autre_exemplaire(settings: &Settings, layout: &Layout, serie: Option<&str>) -> Option<String> {
    let serie = serie?;
    (settings.device_state(layout.vid, layout.pid, Some(serie)) != DeviceState::Adopted).then(
        || {
            format!(
                "l'exemplaire branché (série {}) n'est pas celui qui a été piloté : il reste \
                 fermé. « Piloter » l'adopte à son tour.",
                journal::empreinte(serie)
            )
        },
    )
}

/// Applique les décisions retenues, au démarrage de l'application.
///
/// Ne renvoie rien et ne peut pas échouer : des réglages illisibles ou un HID
/// absent ne doivent pas empêcher la fenêtre de s'ouvrir — c'est elle qui
/// permettrait de corriger la situation.
fn apply_adoptions(app: &AppHandle, state: &AppState) {
    let (store, settings) = match storage::store(app).and_then(|s| {
        let settings = s.read_settings()?;
        Ok((s, settings))
    }) {
        Ok(lus) => lus,
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
        |kb| kb.inspection().serial.clone().ok(),
    );

    // Ce que les ouvertures apprennent, à retenir une fois la boucle finie.
    let mut appris = settings.clone();

    // Les poignées d'abord, les échecs ensuite : deux tables, jamais verrouillées
    // ensemble. Rien d'autre n'a encore pu ouvrir quoi que ce soit — l'état vient
    // d'être construit et n'est pas encore confié au gestionnaire.
    for (layout, keyboard) in ouverts {
        let device = DeviceRef::of(layout);
        // Le second `plugged` ne réénumère rien — il relit la liste que `api`
        // tient déjà — et il évite de faire porter la série par [`OpenOutcome`],
        // qui rend compte d'une tentative et n'a pas à décrire l'appareil.
        let usb = plugged(&api, layout).flatten();
        let serie = serie_connue(Some(keyboard.inspection()), usb.clone());
        consigner_l_ouverture(device, &keyboard, usb, "appareil adopté ouvert");
        // Une décision prise sans série **se complète** dès qu'on la connaît :
        // c'est la règle de [`storage::Settings::set_device_state`]. Sans elle,
        // une adoption antérieure à la lecture par protocole resterait appariée
        // sur le seul modèle, et le premier exemplaire venu passerait toujours.
        appris.set_device_state(
            layout.vid,
            layout.pid,
            serie.as_deref(),
            DeviceState::Adopted,
        );
        // **Avant** de déposer la poignée : la luminosité se réapplique sur le
        // clavier qu'on vient d'ouvrir, et on la tient encore en main.
        reappliquer_la_luminosite(&keyboard, &settings, layout, serie.as_deref());
        state.set_open(device, Some(keyboard));
    }

    // Seulement si quelque chose a été appris : réécrire à chaque démarrage un
    // fichier inchangé ne servirait qu'à multiplier les occasions de le
    // tronquer.
    if appris != settings {
        match store.write_settings(&appris) {
            Ok(()) => tracing::info!("série lue par le protocole, retenue pour l'appariement"),
            // `warn` : l'appareil est ouvert et fonctionne. On perd seulement la
            // distinction entre deux exemplaires, jusqu'au démarrage suivant.
            Err(e) => tracing::warn!("série lue mais non retenue : {e}"),
        }
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

/// Repose sur le clavier la luminosité qu'on avait retenue pour lui.
///
/// **Un niveau retenu qui ne se réapplique pas au branchement ne sert à rien** :
/// c'est toute la raison de le retenir. Le protocole relevé sait écrire la
/// luminosité, pas la relire — sans ce geste, un clavier rebranché repart à ce
/// que son micrologiciel a gardé, et le réglage de `settings.json` décrit un
/// état que rien ne produit.
///
/// Rien n'est réécrit quand c'est le défaut : le clavier y est déjà, et une
/// écriture HID de plus au démarrage de chaque appareil n'achèterait rien.
///
/// Rien ne remonte : un refus d'écriture ne doit pas empêcher l'adoption
/// d'aboutir — l'appareil est ouvert, l'effet pourra tourner, et c'est
/// l'essentiel. Il est consigné, parce qu'un clavier plus sombre que demandé
/// sans un mot nulle part est exactement le genre d'écart qui coûte une session.
fn reappliquer_la_luminosite(
    keyboard: &Keyboard,
    settings: &Settings,
    layout: &Layout,
    serial: Option<&str>,
) {
    let niveau = settings.brightness(layout.vid, layout.pid, serial);
    if niveau == storage::BRIGHTNESS_DEFAUT {
        return;
    }
    match keyboard.set_brightness(niveau) {
        Ok(()) => tracing::info!(
            appareil = %DeviceRef::of(layout),
            niveau,
            "luminosité retenue réappliquée"
        ),
        Err(e) => tracing::warn!(
            appareil = %DeviceRef::of(layout),
            niveau,
            "luminosité retenue non réappliquée : {e}"
        ),
    }
}

// ---------------------------------------------------------------- commandes

/// Liste les gabarits connus : branchés ou non, et surtout **dans quel état**.
#[tauri::command]
fn list_devices(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Vec<DeviceInfo>> {
    let settings = storage::store(&app)?.read_settings()?;
    let api = hid()?;
    // Les relevés sont faits l'un après l'autre, chacun rendant son verrou avant
    // le suivant. C'est la règle documentée sur [`AppState`], et elle vient d'un
    // interblocage réel : cette commande prenait le clavier puis les échecs,
    // `ignore_device` l'inverse, et les deux se bloquaient l'une l'autre.
    let failures = state.failures.lock().unwrap().clone();

    Ok(LAYOUTS
        .iter()
        .map(|l| {
            let device = DeviceRef::of(l);
            let branche = plugged(&api, l);
            let present = branche.is_some();
            // Relue sur la poignée, sans échange USB : `None` dit fermé.
            let inspection = state.inspection(device);
            let serial = serie_connue(inspection.as_ref(), branche.flatten());
            DeviceInfo {
                name: l.name.to_string(),
                vid: l.vid,
                pid: l.pid,
                present,
                state: settings.device_state(l.vid, l.pid, serial.as_deref()),
                open: inspection.is_some(),
                error: failures.get(&device).cloned(),
                surveyed_firmware: l.surveyed_firmware.to_string(),
                firmware: inspection
                    .as_ref()
                    .and_then(|i| i.firmware.as_ref().ok())
                    .map(ToString::to_string),
                warnings: inspection.map(|i| i.warnings(l)).unwrap_or_default(),
            }
        })
        .collect())
}

/// Retient « piloté » pour cet appareil, et l'ouvre s'il est là.
///
/// La décision est écrite **quelle que soit l'issue de l'ouverture** : c'est une
/// décision, pas le compte rendu d'une tentative. Le prochain démarrage la
/// rejouera, ce qui est précisément ce qu'on veut d'un clavier qu'un
/// concentrateur n'a pas encore fini d'énumérer.
///
/// # L'ouverture précède l'écriture, et c'est pour la série
///
/// Le descripteur USB n'en porte aucune : seule l'ouverture la lit, par le
/// protocole. Écrire la décision d'abord la retiendrait sans série — donc pour
/// **tout** exemplaire du modèle — et adopter un second clavier identique
/// retomberait sur l'entrée du premier au lieu d'en créer une. Si l'écriture
/// échoue ensuite, la poignée tombe avec la fonction : rien n'est ouvert sans
/// décision pour le justifier.
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
    let ouverture = branche.is_some().then(|| Keyboard::open(&api, layout));
    let usb = branche.flatten();
    let serial = serie_connue(
        ouverture
            .as_ref()
            .and_then(|o| o.as_ref().ok())
            .map(Keyboard::inspection),
        usb.clone(),
    );

    let store = storage::store(&app)?;
    let mut settings = store.read_settings()?;
    settings.set_device_state(vid, pid, serial.as_deref(), DeviceState::Adopted);
    store.write_settings(&settings)?;

    let Some(ouverture) = ouverture else {
        return Ok(None);
    };
    match ouverture {
        Ok(kb) => {
            consigner_l_ouverture(device, &kb, usb, "appareil piloté");
            // Comme au démarrage : ce qu'on avait retenu pour ce clavier reprend
            // effet à l'instant où on l'ouvre, pas au lancement suivant.
            reappliquer_la_luminosite(&kb, &settings, layout, serial.as_deref());
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
///
/// Celle du protocole, quand l'appareil est ouvert, est relue **avant** de le
/// refermer : c'est la seule qui distingue l'exemplaire, et elle disparaît avec
/// la poignée.
#[tauri::command]
fn ignore_device(app: AppHandle, state: State<'_, AppState>, vid: u16, pid: u16) -> CmdResult<()> {
    let device = DeviceRef { vid, pid };
    let layout = find_layout(device)?;
    let serial = serie_connue(
        state.inspection(device).as_ref(),
        hid().ok().and_then(|api| plugged(&api, layout)).flatten(),
    );

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
///
/// Sans appelant depuis un écran, comme [`disconnect`] et pour la même raison :
/// l'interface ne propose aujourd'hui qu'adopter ou ignorer, c'est-à-dire les
/// deux gestes qui **décident**. Celui qui n'engage à rien n'a pas encore son
/// bouton — et c'est la paire qu'il faudra câbler ensemble, pas l'une des deux.
#[tauri::command]
fn connect(state: State<'_, AppState>, vid: u16, pid: u16) -> CmdResult<LayoutInfo> {
    let device = DeviceRef { vid, pid };
    let layout = find_layout(device)?;

    let api = hid()?;
    let kb = Keyboard::open(&api, layout).map_err(|e| e.to_string())?;
    consigner_l_ouverture(
        device,
        &kb,
        plugged(&api, layout).flatten(),
        "appareil ouvert, sans décision",
    );
    state.set_open(device, Some(kb));
    state.failures.lock().unwrap().remove(&device);
    Ok(LayoutInfo::from(layout))
}

/// Referme **un** appareil. Les autres ne sont pas touchés.
///
/// Aucun écran ne l'appelle aujourd'hui — `useDevice` l'enveloppe, personne ne
/// déstructure l'enveloppe. Elle reste parce qu'elle est le seul geste qui
/// **referme sans décider** : `ignore_device` écrit dans `settings.json` et vaut
/// pour les fois suivantes, celle-ci relâche la poignée et rien d'autre. Ce qui
/// manque est un bouton, pas une commande.
#[tauri::command]
fn disconnect(state: State<'_, AppState>, device: DeviceRef) {
    state.set_open(device, None);
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

/// Écrit la luminosité **sur le clavier**, et rien d'autre.
///
/// Ne touche pas à `settings.json` : c'est [`remember_brightness`]. Les deux sont
/// séparées comme le sont `set_effect_params` et `remember_effect_params`, et
/// pour la même raison — elles n'ont ni la même cadence ni la même destination.
/// Un curseur qu'on glisse produit des dizaines d'écritures HID par seconde, et
/// une seule écriture disque, quand il s'arrête.
#[tauri::command]
fn set_brightness(state: State<'_, AppState>, device: DeviceRef, level: u8) -> CmdResult<()> {
    with_keyboard(&state, device, |kb| {
        kb.set_brightness(level).map_err(|e| e.to_string())
    })
}

/// Retient la luminosité de cet appareil, sans toucher au clavier.
///
/// Le pendant disque de [`set_brightness`]. Comme `remember_effect_params`, la
/// lecture, la modification et l'écriture se font ici d'un seul tenant : renvoyer
/// tout le fichier depuis la fenêtre écraserait une adoption décidée entre-temps.
///
/// La série est relevée **si elle se donne**, comme le fait [`ignore_device`] :
/// elle fait atterrir le niveau sur l'entrée du bon exemplaire quand il y en a
/// deux du même modèle, et son absence ne bloque rien —
/// [`storage::DeviceRecord::matches`] retrouve l'entrée sans elle.
///
/// Ramener le curseur au maximum **retire** l'entrée plutôt que d'écrire 255 :
/// voir [`storage::Settings::set_brightness`].
#[tauri::command]
fn remember_brightness(
    app: AppHandle,
    state: State<'_, AppState>,
    device: DeviceRef,
    level: u8,
) -> CmdResult<()> {
    let layout = find_layout(device)?;
    let serial = serie_connue(
        state.inspection(device).as_ref(),
        hid().ok().and_then(|api| plugged(&api, layout)).flatten(),
    );

    let store = storage::store(&app)?;
    let mut settings = store.read_settings()?;
    // Rien de neuf : on ne réécrit pas le fichier. Un curseur qu'on déplace puis
    // qu'on ramène repasse par ici.
    if !settings.set_brightness(device.vid, device.pid, serial.as_deref(), level) {
        return Ok(());
    }
    store.write_settings(&settings)
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
///
/// # Exposée sans appelant, et délibérément
///
/// La fenêtre n'envoie pas d'images : c'est la boucle de [`crate::runtime`] qui
/// les produit et les écrit, et l'aperçu les **reçoit** par canal au lieu de les
/// pousser. Aucun écran n'appellera donc celle-ci tant que l'architecture reste
/// celle-là.
///
/// Elle reste le seul chemin qui met une image précise sur le clavier **sans
/// moteur d'effets** — ce qui a servi à établir que l'image doit couvrir 132
/// cases et non 106, et qui resservira le jour où un appareil répondra
/// autrement. Même raison que les `pub` de `candeo-protocol` : ce qui a permis
/// le relevé reste en état de le refaire.
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
///
/// Sans appelant elle aussi, et gardée pour une raison nommée : l'écriture
/// partielle est **la** piste si la cadence doit remonter au-dessus de 30 —
/// n'envoyer que les rangées qui changent, au lieu des six à chaque image. Le
/// commentaire de `runtime::FPS` la désigne comme telle. C'est par ici que cette
/// piste se vérifie sur le matériel avant d'être écrite dans la boucle ; la
/// retirer reviendrait à retirer l'outil de mesure avant la mesure.
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

            // **Après `manage`**, et l'ordre est contraignant : le menu se bâtit
            // sur l'état réel du moteur, qu'il va chercher par le gestionnaire.
            // Après l'adoption aussi, pour que le premier menu montre les
            // appareils déjà ouverts plutôt qu'une liste vide.
            tray::installer(app.handle());
            Ok(())
        })
        // La croix **replie**, elle ne quitte pas — tant qu'il y a une icône de
        // zone de notification pour rendre l'application atteignable. Voir
        // [`tray`] pour le pourquoi, et pour ce qui arrive quand l'icône manque.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Seulement la fenêtre principale, alors qu'il n'y en a qu'une :
                // le jour où une seconde apparaîtra — une boîte de dialogue, un
                // écran détaché —, la refermer la masquerait au lieu de la
                // détruire, et elle réapparaîtrait telle quelle au rendez-vous
                // suivant. La panne se chercherait loin d'ici.
                if window.label() == single_instance::MAIN_WINDOW && tray::installee() {
                    api.prevent_close();
                    // **L'aperçu s'arrête ici, et l'effet appliqué non.** C'est
                    // toute la différence entre les deux : l'un est ce que le
                    // clavier fait — il survit à la fenêtre, c'est la promesse de
                    // [`tray`] —, l'autre est ce qu'on regarde, et il n'y a plus
                    // personne pour regarder.
                    //
                    // Ici plutôt que dans la fenêtre : replier ne détruit pas la
                    // vue web, donc ni `onBeforeUnmount` ni `pagehide` ne
                    // passent. Un contexte QuickJS et un fil resteraient
                    // entretenus pour un écran masqué, indéfiniment.
                    window.state::<AppState>().engine.stop_preview();
                    // Masquer, et non détruire : la vue web garde son état, et
                    // rouvrir est instantané. C'est aussi ce qui laisse la
                    // fenêtre entendre `candeo://etat-change` pendant qu'elle est
                    // repliée, donc revenir déjà à jour.
                    if let Err(e) = window.hide() {
                        tracing::warn!("fenêtre non repliée : {e}");
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_devices,
            adopt_device,
            ignore_device,
            connect,
            disconnect,
            get_layout,
            get_default_layout,
            set_brightness,
            remember_brightness,
            set_effect,
            present,
            write_row,
            runtime::start_effect,
            runtime::stop_effect,
            runtime::set_effect_params,
            runtime::set_output_to_keyboard,
            runtime::subscribe_frames,
            runtime::unsubscribe_frames,
            runtime::start_preview,
            runtime::stop_preview,
            runtime::set_preview_params,
            runtime::subscribe_preview_frames,
            runtime::unsubscribe_preview_frames,
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
        .build(tauri::generate_context!())
        .expect("erreur au lancement de l'application")
        // `build` puis `run`, et non `run` seul : c'est la seule façon d'obtenir
        // le gestionnaire d'événements de l'application — donc d'empêcher
        // `ExitRequested`.
        //
        // **C'est ici que « un effet tourne fenêtre fermée » devient vrai.** Sans
        // ce gestionnaire, rien n'empêche la sortie : sous Windows comme sous
        // Linux le processus s'arrête avec sa dernière fenêtre, et le fil de
        // rendu part avec lui. Trois documents affirmaient le contraire, et c'est
        // l'argument qui avait fait écarter l'exécution des effets dans le
        // WebView — la conception était juste, l'implémentation s'arrêtait avant
        // la fin.
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested { code, api, .. } = event {
                // `None` désigne une sortie demandée par l'utilisateur — la
                // dernière fenêtre qui se ferme — et `Some` une sortie demandée
                // par le code, c'est-à-dire notre propre « Quitter ». La
                // distinction est tout le mécanisme : empêcher sans la faire
                // rendrait candeo impossible à quitter, y compris par son seul
                // article prévu pour ça.
                //
                // Et seulement tant qu'il y a une icône : sans elle, plus rien ne
                // commanderait l'application ni ne la terminerait. Voir
                // [`tray::installee`].
                if code.is_none() && tray::installee() {
                    api.prevent_exit();
                }
            }
        });
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
        surveyed_firmware: candeo_protocol::Firmware { major: 1, minor: 0 },
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
        surveyed_firmware: candeo_protocol::Firmware { major: 1, minor: 0 },
        rows: 1,
        cols: 1,
        matrix: &[0],
        keys: &[],
    };

    /// Série propre à chaque gabarit, comme une énumération réelle.
    fn branches(l: &Layout) -> Option<Option<String>> {
        Some(Some(format!("S{:04x}", l.pid)))
    }

    /// Un appareil ouvert qui ne répond pas aux lectures : aucune série par le
    /// protocole, l'appariement reste celui de l'énumération.
    fn muet(_: &&'static str) -> Option<String> {
        None
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

        let (opened, comptes) = open_adopted(
            &[&PREMIER, &SECOND],
            &pilotes(),
            branches,
            |l| {
                tentatives.push(l.pid);
                if l.pid == PREMIER.pid {
                    Err("accès refusé par le système".into())
                } else {
                    Ok(l.name)
                }
            },
            muet,
        );

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
            muet,
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
            muet,
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
        let (opened, comptes) = open_adopted(
            &[&PREMIER, &SECOND],
            &pilotes(),
            branches,
            |l| {
                tentatives.push(l.pid);
                Ok(l.name)
            },
            muet,
        );

        assert_eq!(tentatives, vec![PREMIER.pid, SECOND.pid]);
        assert_eq!(ouverts(&opened), vec!["Premier", "Second"]);
        assert_eq!(comptes.len(), 2);
        assert!(comptes.iter().all(|c| c.error.is_none()));
    }

    // -------------------------------------------------------- série par protocole

    /// L'énumération du DeathStalker : branché, **sans série**.
    fn branche_muet(_: &Layout) -> Option<Option<String>> {
        Some(None)
    }

    /// Une décision prise pour un exemplaire — ou pour tout le modèle, sans série.
    fn pilote_pour(serie: Option<&str>) -> Settings {
        let mut settings = Settings::default();
        settings.set_device_state(PREMIER.vid, PREMIER.pid, serie, DeviceState::Adopted);
        settings
    }

    /// **Ce que la série par protocole change.** Le descripteur USB muet laisse
    /// passer n'importe quel exemplaire du modèle ; la série lue à l'ouverture
    /// dit que ce n'est pas le bon, et la poignée est relâchée.
    #[test]
    fn un_autre_exemplaire_du_modele_n_est_pas_pilote() {
        let (opened, comptes) = open_adopted(
            &[&PREMIER],
            &pilote_pour(Some("XY01")),
            branche_muet,
            |l| Ok(l.name),
            |_| Some("XY02".to_string()),
        );

        assert!(opened.is_empty(), "l'exemplaire voisin a été piloté");
        assert_eq!(comptes.len(), 1);
        let raison = comptes[0].error.as_deref().expect("aucune raison donnée");
        assert!(raison.contains("n'est pas celui"), "{raison}");
        // La raison s'affiche et part au journal : la série n'y figure pas,
        // son empreinte si.
        assert!(!raison.contains("XY02"), "la série a fuité : {raison}");
        assert!(raison.contains(&journal::empreinte("XY02")), "{raison}");
    }

    #[test]
    fn l_exemplaire_adopte_est_reconnu_a_sa_serie() {
        let (opened, comptes) = open_adopted(
            &[&PREMIER],
            &pilote_pour(Some("XY01")),
            branche_muet,
            |l| Ok(l.name),
            |_| Some("XY01".to_string()),
        );

        assert_eq!(ouverts(&opened), vec!["Premier"]);
        assert!(comptes[0].error.is_none());
    }

    /// Une adoption antérieure à la lecture par protocole ne porte pas de série :
    /// elle ne doit pas refuser l'exemplaire qu'on a — elle l'apprendra.
    #[test]
    fn une_adoption_sans_serie_accepte_l_exemplaire_branche() {
        let (opened, _) = open_adopted(
            &[&PREMIER],
            &pilote_pour(None),
            branche_muet,
            |l| Ok(l.name),
            |_| Some("XY02".to_string()),
        );
        assert_eq!(ouverts(&opened), vec!["Premier"]);
    }

    /// Une série qu'on n'a pas pu lire ne conclut rien : refuser pour une
    /// question sans réponse éteindrait l'éclairage de qui n'a qu'un clavier.
    #[test]
    fn sans_serie_lue_rien_ne_se_conclut() {
        let (opened, _) = open_adopted(
            &[&PREMIER],
            &pilote_pour(Some("XY01")),
            branche_muet,
            |l| Ok(l.name),
            muet,
        );
        assert_eq!(ouverts(&opened), vec!["Premier"]);
    }

    /// Le protocole l'emporte sur le descripteur, et le descripteur reste le
    /// repli d'un appareil fermé.
    #[test]
    fn la_serie_du_protocole_passe_avant_celle_du_descripteur() {
        let inspection = Inspection {
            firmware: Err("non lue".into()),
            serial: Ok("XY01".into()),
            checks: Vec::new(),
        };
        assert_eq!(
            serie_connue(Some(&inspection), Some("USB".into())).as_deref(),
            Some("XY01")
        );
        assert_eq!(
            serie_connue(None, Some("USB".into())).as_deref(),
            Some("USB")
        );
        let muette = Inspection {
            serial: Err("illisible".into()),
            ..inspection
        };
        assert_eq!(serie_connue(Some(&muette), None), None);
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
