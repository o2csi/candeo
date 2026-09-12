//! Stockage des effets et des réglages.
//!
//! Deux emplacements distincts, décrits dans
//! [`docs/design/effects-runtime.md`](../../../../docs/design/effects-runtime.md) §3 :
//!
//! ```text
//! app_data_dir()/effects/<id>/     source.ts · effect.js · manifest.json · swatch.json
//! app_config_dir()/settings.json   effet actif, luminosité, appareils adoptés
//! ```
//!
//! L'effet est du **contenu**, le choix de l'effet actif est de la
//! **configuration**. Sous Windows les deux dossiers se confondent, sous Linux
//! non — d'où le passage par l'API de Tauri plutôt que par une constante.
//!
//! Toute la manipulation de fichiers vit dans [`Store`], qui reçoit ses chemins
//! de base en argument ; les commandes Tauri ne font que les résoudre. C'est ce
//! qui permet de tout tester dans un dossier temporaire, sans application.
//!
//! # Les effets intégrés font partie de la bibliothèque
//!
//! Ils n'ont pas de dossier — ils sont compilés dans le binaire, voir
//! [`crate::builtins`] — mais l'appelant n'a pas à le savoir : lister, lire le
//! JavaScript ou la source les trouve comme les autres.
//!
//! **En cas d'homonymie, l'intégré l'emporte**, et l'homonymie est de toute
//! façon refusée à l'installation. Le sens de la priorité n'est pas arbitraire :
//! une entrée marquée `builtin` dans la galerie doit exécuter le code livré, et
//! rien d'autre. L'inverse laisserait un effet utilisateur se glisser sous un
//! nom connu, avec le manifeste de l'intégré affiché à l'écran et un autre code
//! exécuté — c'est exactement ce qu'on refuse. La réservation à l'installation
//! rend la situation impossible ; la priorité à la lecture est la seconde
//! barrière, pour un dossier arrivé par un autre chemin (copie manuelle,
//! bibliothèque héritée d'une version où l'identifiant était libre).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::builtins;
use crate::runtime::swatch::{self, Swatch};
use crate::CmdResult;

/// Version de l'API d'effets fournie par cette version de l'application.
///
/// Un manifeste déclare la version contre laquelle l'effet a été écrit : c'est
/// ce qui permettra de refuser proprement un effet écrit contre une API
/// disparue, plutôt que de le laisser échouer à la première image.
pub const EFFECTS_API_VERSION: u32 = 1;

/// Longueur maximale d'un identifiant d'effet, donc d'un nom de dossier.
const MAX_ID_LEN: usize = 64;

const SOURCE_FILE: &str = "source.ts";
const JS_FILE: &str = "effect.js";
const MANIFEST_FILE: &str = "manifest.json";
/// Repère de couleurs, à côté du manifeste. Voir [`crate::runtime::swatch`].
const SWATCH_FILE: &str = "swatch.json";

/// Noms réservés par Windows : un dossier ainsi nommé est refusé par le
/// système, dans n'importe quel répertoire.
const RESERVED_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

// ---------------------------------------------------------------- types exposés

/// Manifeste d'un effet, écrit tel quel dans `manifest.json`.
///
/// camelCase comme les autres types exposés : le manifeste vient de l'éditeur
/// et y retourne, et `params` contient déjà du JSON écrit côté TypeScript. Un
/// seul champ en snake_case au milieu ne se verrait qu'à l'exécution.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Paramètres déclarés, tels que l'interface les présentera.
    ///
    /// Conservés en JSON brut : leur forme est celle de `ParamSpec` côté
    /// TypeScript, elle évolue avec l'éditeur, et le Rust ne les interprète
    /// pas. Les typer ici créerait une seconde source de vérité sans emploi.
    #[serde(default)]
    pub params: serde_json::Map<String, serde_json::Value>,
    /// Version de l'API d'effets utilisée à l'écriture.
    pub api_version: u32,
}

/// Nature d'un effet : écrit par l'utilisateur, ou compilé dans le binaire.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EffectKind {
    /// Fourni avec l'application, sans dossier sur disque.
    Builtin,
    /// Installé par l'utilisateur, sous `effects/<id>/`.
    User,
}

/// Entrée de la bibliothèque : le manifeste, plus ce qui n'en fait pas partie.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EffectEntry {
    pub id: String,
    pub kind: EffectKind,
    /// Repère de couleurs, **prélevé en exécutant l'effet**.
    ///
    /// Il n'est pas dans le manifeste, et ce n'est pas un détail de rangement :
    /// le manifeste est ce que l'auteur déclare, le repère est ce que l'effet
    /// fait. Les confondre rouvrirait la porte à un repère écrit à la main,
    /// donc à un repère qui ment.
    ///
    /// Porté par l'entrée pour que la liste suffise à l'afficher : une vignette
    /// qui demanderait un second appel par effet ferait autant d'allers-retours
    /// que la bibliothèque compte d'entrées.
    ///
    /// Vide quand il n'a pas pu être calculé — voir [`crate::runtime::swatch`].
    /// L'interface retombe alors sur une pastille neutre.
    pub swatch: Swatch,
    #[serde(flatten)]
    pub manifest: Manifest,
}

/// Périphérique retenu par l'utilisateur.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceSelection {
    pub vid: u16,
    pub pid: u16,
}

/// Décision prise pour un appareil, une fois, et retenue.
///
/// Le défaut est [`Detected`](DeviceState::Detected) : **un appareil jamais vu
/// n'est pas piloté**. Écrire sur un périphérique USB qu'on comprend mal n'est
/// pas anodin, et à l'échelle d'un catalogue qui grandit — claviers, souris,
/// mémoire, ventilateurs — adopter par défaut est la façon de casser le
/// matériel de quelqu'un.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DeviceState {
    /// Listé, mais **pas** ouvert. C'est l'état de tout appareil sur lequel
    /// personne ne s'est encore prononcé.
    #[default]
    Detected,
    /// Ouvert automatiquement au démarrage, sans rien demander.
    Adopted,
    /// Laissé tranquille, et il le reste.
    Ignored,
}

/// Ce que `settings.json` retient d'un appareil : son identité, et la décision.
///
/// # L'identité, c'est VID / PID / numéro de série
///
/// **Ni la variante, ni le micrologiciel.** Le même clavier s'est déclaré
/// `v1.4 / Unkown Variant` puis `v1.5 / Quartz` pendant le relevé du protocole :
/// une liaison qui apparie sur ces champs se rompt à la mise à jour, et
/// l'appareil adopté redevient un inconnu du jour au lendemain.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRecord {
    pub vid: u16,
    pub pid: u16,
    /// Numéro de série, quand le système en déclare un.
    ///
    /// Absent du fichier plutôt qu'à `null` : la majorité des entrées n'en
    /// auront pas, et une clé vide répétée n'apprend rien à qui relit ses
    /// réglages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    pub state: DeviceState,
}

impl DeviceRecord {
    /// Vrai si cette entrée désigne l'appareil énuméré.
    ///
    /// Le VID et le PID doivent correspondre ; la série n'est comparée que si
    /// **les deux côtés** en portent une. Ce n'est pas du laxisme, c'est le
    /// seul arbitrage qui tienne dans les deux sens :
    ///
    /// - la série départage deux exemplaires du même modèle — sans elle, adopter
    ///   l'un adopterait l'autre ;
    /// - mais une énumération muette — hidraw sans règle udev, un concentrateur
    ///   qui ne relaie rien — ne doit pas désapparier un appareil déjà adopté,
    ///   sans quoi la décision serait à reprendre à chaque branchement.
    pub fn matches(&self, vid: u16, pid: u16, serial: Option<&str>) -> bool {
        if self.vid != vid || self.pid != pid {
            return false;
        }
        match (self.serial.as_deref(), serial) {
            (Some(mien), Some(sien)) => mien == sien,
            _ => true,
        }
    }
}

/// Réglages persistants.
///
/// `#[serde(default)]` sur la structure entière : un `settings.json` écrit par
/// une version antérieure, à qui il manque un champ ajouté depuis, se relit
/// sans erreur au lieu de rendre l'application muette au démarrage.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Identifiant de l'effet à reprendre au démarrage.
    pub active_effect: Option<String>,
    pub brightness: u8,
    /// Périphérique choisi, quand il y en a plusieurs de connus.
    pub device: Option<DeviceSelection>,
    /// Décisions prises appareil par appareil.
    ///
    /// Ne contient que celles qui **diffèrent du défaut** : un appareil absent
    /// de cette liste est `detected`, ce qui est exactement l'état d'un appareil
    /// jamais rencontré. Le fichier ne grossit donc pas d'une entrée à chaque
    /// périphérique branché une fois.
    pub devices: Vec<DeviceRecord>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            active_effect: None,
            // Pleine luminosité : c'est l'état d'un clavier qu'on vient de
            // brancher, donc le défaut le moins surprenant.
            brightness: 255,
            device: None,
            devices: Vec::new(),
        }
    }
}

impl Settings {
    /// Rang de l'entrée décrivant cet appareil, s'il y en a une.
    ///
    /// L'identité exacte d'abord — série comprise, `None` comprise —, puis la
    /// règle tolérante de [`DeviceRecord::matches`]. L'ordre compte : une entrée
    /// sans série ne doit pas décider à la place de celle qui en porte une,
    /// sinon deux exemplaires du même modèle se confondraient dès qu'un seul
    /// d'entre eux aurait été adopté sans série.
    fn position(&self, vid: u16, pid: u16, serial: Option<&str>) -> Option<usize> {
        self.devices
            .iter()
            .position(|r| r.vid == vid && r.pid == pid && r.serial.as_deref() == serial)
            .or_else(|| {
                self.devices
                    .iter()
                    .position(|r| r.matches(vid, pid, serial))
            })
    }

    /// Décision retenue pour cet appareil, ou [`DeviceState::Detected`].
    pub fn device_state(&self, vid: u16, pid: u16, serial: Option<&str>) -> DeviceState {
        self.position(vid, pid, serial)
            .map(|i| self.devices[i].state)
            .unwrap_or_default()
    }

    /// Retient une décision pour cet appareil.
    pub fn set_device_state(
        &mut self,
        vid: u16,
        pid: u16,
        serial: Option<&str>,
        state: DeviceState,
    ) {
        match self.position(vid, pid, serial) {
            Some(i) => {
                let record = &mut self.devices[i];
                record.state = state;
                // La série se complète si on vient de l'apprendre, mais ne
                // s'efface jamais : une énumération muette ne doit pas faire
                // perdre à l'entrée ce qui la distingue de l'exemplaire voisin.
                if record.serial.is_none() {
                    record.serial = serial.map(str::to_owned);
                }
            }
            None => self.devices.push(DeviceRecord {
                vid,
                pid,
                serial: serial.map(str::to_owned),
                state,
            }),
        }
    }
}

// ---------------------------------------------------------------- identifiants

/// Vrai si `id` est un nom de périphérique réservé par Windows.
fn is_reserved(id: &str) -> bool {
    RESERVED_NAMES.contains(&id)
}

/// Vérifie qu'un identifiant peut servir de nom de dossier sans risque.
///
/// Le contrôle est une **liste blanche** : `a-z`, `0-9` et le tiret. Tout le
/// reste est refusé, ce qui écarte d'un coup `..`, les séparateurs de chemin,
/// les deux-points d'un lecteur Windows et les caractères de contrôle — sans
/// dépendre d'une liste noire qu'on oublierait de compléter.
pub(crate) fn validate_id(id: &str) -> CmdResult<()> {
    if id.is_empty() {
        return Err("identifiant d'effet vide".into());
    }
    if id.len() > MAX_ID_LEN {
        return Err(format!(
            "identifiant d'effet trop long : {} caractères, {MAX_ID_LEN} au plus",
            id.len()
        ));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(format!(
            "identifiant d'effet invalide : « {id} » — seuls les caractères a-z, 0-9 et le tiret sont acceptés"
        ));
    }
    if id.starts_with('-') || id.ends_with('-') {
        return Err(format!(
            "identifiant d'effet invalide : « {id} » — il ne peut ni commencer ni finir par un tiret"
        ));
    }
    if is_reserved(id) {
        return Err(format!(
            "« {id} » est un nom réservé par Windows, il ne peut pas servir de dossier"
        ));
    }
    Ok(())
}

/// Dérive un identifiant sûr à partir d'un nom saisi par l'utilisateur.
///
/// On ne fait jamais confiance au nom : on ne le valide pas, on le **remplace**
/// par ce qu'il a de représentable. Le résultat satisfait toujours
/// [`validate_id`] — c'est ce que vérifie le test `des_noms_hostiles_donnent_un_identifiant_sur`.
///
/// Deux effets portant le même nom obtiennent le même identifiant, donc le
/// second écrase le premier : réenregistrer un effet depuis l'éditeur le met à
/// jour au lieu d'en accumuler des copies.
pub fn derive_id(name: &str) -> String {
    let mut id = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            id.push(ch.to_ascii_lowercase());
        } else if !id.ends_with('-') {
            id.push('-');
        }
    }
    let id: String = id.trim_matches('-').chars().take(MAX_ID_LEN).collect();
    let id = id.trim_end_matches('-');

    if id.is_empty() {
        return "effet".into();
    }
    if is_reserved(id) {
        return format!("{id}-effet");
    }
    id.to_string()
}

// ---------------------------------------------------------------- stockage

/// Accès disque aux effets et aux réglages.
///
/// Les chemins de base arrivent de l'extérieur : rien ici ne connaît Tauri, ce
/// qui rend l'ensemble testable dans un dossier temporaire.
pub struct Store {
    effects_dir: PathBuf,
    settings_file: PathBuf,
}

impl Store {
    /// `data_dir` porte le contenu, `config_dir` la configuration.
    pub fn new(data_dir: &Path, config_dir: &Path) -> Self {
        Self {
            effects_dir: data_dir.join("effects"),
            settings_file: config_dir.join("settings.json"),
        }
    }

    /// Écrit `effects/<id>/` et renvoie l'identifiant retenu.
    ///
    /// Les trois fichiers sont écrits ensemble : `source.ts` pour rouvrir
    /// l'effet dans l'éditeur, `effect.js` pour l'exécuter, `manifest.json`
    /// pour le décrire. Le `.js` n'est pas un cache régénérable — le
    /// transpileur vit dans Monaco, donc le reconstruire exigerait d'ouvrir la
    /// fenêtre, alors qu'un effet doit pouvoir démarrer sans interface.
    pub fn install_effect(
        &self,
        source_ts: &str,
        js: &str,
        manifest: &Manifest,
    ) -> CmdResult<String> {
        if manifest.name.trim().is_empty() {
            return Err("l'effet doit avoir un nom".into());
        }
        if manifest.api_version == 0 {
            return Err("le manifeste ne déclare pas la version de l'API d'effets".into());
        }
        if manifest.api_version > EFFECTS_API_VERSION {
            return Err(format!(
                "effet écrit pour la version {} de l'API d'effets ; cette version de candeo n'en connaît que la {EFFECTS_API_VERSION}",
                manifest.api_version
            ));
        }

        let id = derive_id(&manifest.name);
        // Les identifiants intégrés sont réservés. Accepter l'homonymie
        // obligerait à arbitrer ensuite, à chaque lecture, entre deux effets
        // portant le même identifiant — et la bibliothèque en montrerait deux
        // sous la même clé. Le refus est immédiat et se dit en une phrase.
        if builtins::find(&id).is_some() {
            return Err(format!(
                "« {id} » est l'identifiant d'un effet intégré ; donnez un autre nom au vôtre"
            ));
        }

        let dir = self.effects_dir.join(&id);
        create_dir(&dir)?;

        let json = serde_json::to_string_pretty(manifest)
            .map_err(|e| format!("manifeste non sérialisable : {e}"))?;
        write(&dir.join(SOURCE_FILE), source_ts)?;
        write(&dir.join(JS_FILE), js)?;
        write(&dir.join(MANIFEST_FILE), &json)?;

        // Le repère est prélevé **ici**, une fois, et non à chaque affichage de
        // la liste : c'est une vignette qui ne bouge pas tant que l'effet ne
        // bouge pas. Réenregistrer un effet repasse par ce point, donc le
        // recalcule — un effet devenu bleu ne garde pas sa vignette rouge.
        write_swatch(&dir, js);

        Ok(id)
    }

    /// Le JavaScript exécutable d'un effet, **intégré ou installé**.
    ///
    /// C'est ce que le moteur charge. Les intégrés sont consultés d'abord :
    /// voir la priorité justifiée en tête de module.
    ///
    /// Pour un effet utilisateur, c'est aussi la raison pour laquelle le `.js`
    /// est écrit sur disque à l'installation — le lire ne demande ni l'éditeur,
    /// ni la fenêtre.
    pub fn effect_js(&self, id: &str) -> CmdResult<String> {
        if let Some(b) = builtins::find(id) {
            return Ok(b.js.to_string());
        }
        self.read_file(id, JS_FILE)
    }

    /// La source d'un effet, pour la rouvrir dans l'éditeur.
    ///
    /// Un effet intégré n'a pas de `.ts` : son JavaScript **est** sa source. Le
    /// rendre lisible depuis l'éditeur est tout l'intérêt de le livrer — on
    /// part d'un effet qui marche, on le modifie, on l'enregistre sous un autre
    /// nom (l'identifiant intégré, lui, est réservé).
    pub fn effect_source(&self, id: &str) -> CmdResult<String> {
        if let Some(b) = builtins::find(id) {
            return Ok(b.js.to_string());
        }
        self.read_file(id, SOURCE_FILE)
    }

    fn read_file(&self, id: &str, file: &str) -> CmdResult<String> {
        validate_id(id)?;
        let path = self.effects_dir.join(id).join(file);
        fs::read_to_string(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => format!("aucun effet nommé « {id} »"),
            _ => format!("lecture de {} impossible : {e}", path.display()),
        })
    }

    /// Bibliothèque complète : effets intégrés **et** effets utilisateur.
    ///
    /// Les intégrés sont compilés dans le binaire et n'ont pas de dossier ;
    /// ils apparaissent malgré tout, distingués par [`EffectKind`], pour que
    /// l'interface n'ait qu'une seule liste à afficher.
    pub fn list_effects(&self) -> CmdResult<Vec<EffectEntry>> {
        let mut effects = builtin_effects();

        // Dossier absent : premier lancement, aucun effet installé. Ce n'est
        // pas une erreur.
        let entries = match fs::read_dir(&self.effects_dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(effects),
            Err(e) => {
                return Err(format!(
                    "lecture de {} impossible : {e}",
                    self.effects_dir.display()
                ))
            }
        };

        let mut installed = Vec::new();
        for entry in entries {
            let entry =
                entry.map_err(|e| format!("lecture de la bibliothèque interrompue : {e}"))?;
            let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            // Un dossier abîmé ou étranger est ignoré plutôt que de faire
            // échouer toute la liste : une bibliothèque de vingt effets ne doit
            // pas disparaître parce que l'un d'eux a un manifeste illisible.
            if validate_id(&id).is_err() {
                continue;
            }
            // Un dossier qui usurpe l'identifiant d'un intégré est écarté : la
            // liste est indexée par identifiant, elle ne peut pas en contenir
            // deux, et c'est l'intégré qui démarrerait de toute façon. L'y
            // laisser afficherait un effet qui ne s'exécutera jamais. Il reste
            // supprimable — `delete_effect` ne consulte que le disque.
            if builtins::find(&id).is_some() {
                continue;
            }
            let Ok(raw) = fs::read_to_string(entry.path().join(MANIFEST_FILE)) else {
                continue;
            };
            let Ok(manifest) = serde_json::from_str::<Manifest>(&raw) else {
                continue;
            };
            installed.push(EffectEntry {
                id,
                kind: EffectKind::User,
                swatch: read_swatch(&entry.path()),
                manifest,
            });
        }

        // Ordre stable : le système de fichiers n'en garantit aucun, et une
        // galerie qui se réordonne à chaque ouverture est illisible.
        installed.sort_by(|a, b| a.id.cmp(&b.id));
        effects.append(&mut installed);
        Ok(effects)
    }

    /// Supprime `effects/<id>/`.
    ///
    /// Un effet intégré n'a pas de dossier, donc rien à supprimer — mais le
    /// dossier est vérifié **avant** son cas : c'est ce qui laisse retirer un
    /// dossier qui usurperait un identifiant intégré, invisible dans la liste
    /// et inexécutable, mais bien présent sur disque.
    pub fn delete_effect(&self, id: &str) -> CmdResult<()> {
        validate_id(id)?;
        let dir = self.effects_dir.join(id);
        if !dir.is_dir() {
            if builtins::find(id).is_some() {
                return Err(format!(
                    "« {id} » est un effet intégré : il est livré avec l'application et ne peut pas être supprimé"
                ));
            }
            return Err(format!("aucun effet installé sous l'identifiant « {id} »"));
        }
        fs::remove_dir_all(&dir)
            .map_err(|e| format!("suppression de {} impossible : {e}", dir.display()))
    }

    /// Lit `settings.json`, ou renvoie les valeurs par défaut s'il n'existe pas.
    pub fn read_settings(&self) -> CmdResult<Settings> {
        let raw = match fs::read_to_string(&self.settings_file) {
            Ok(raw) => raw,
            // Premier lancement : pas de fichier, donc les défauts. Une erreur
            // ici obligerait l'interface à traiter le cas nominal comme un
            // incident.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
            Err(e) => {
                return Err(format!(
                    "lecture de {} impossible : {e}",
                    self.settings_file.display()
                ))
            }
        };
        serde_json::from_str(&raw).map_err(|e| {
            format!(
                "réglages illisibles dans {} : {e}",
                self.settings_file.display()
            )
        })
    }

    /// Écrit `settings.json`.
    ///
    /// Passage par un fichier temporaire puis renommage : une coupure en cours
    /// d'écriture laisserait sinon des réglages tronqués, donc une application
    /// qui ne démarre plus.
    pub fn write_settings(&self, settings: &Settings) -> CmdResult<()> {
        let Some(parent) = self.settings_file.parent() else {
            return Err("chemin de réglages sans dossier parent".into());
        };
        create_dir(parent)?;

        let json = serde_json::to_string_pretty(settings)
            .map_err(|e| format!("réglages non sérialisables : {e}"))?;
        let tmp = self.settings_file.with_extension("json.tmp");
        write(&tmp, &json)?;
        fs::rename(&tmp, &self.settings_file).map_err(|e| {
            format!(
                "écriture de {} impossible : {e}",
                self.settings_file.display()
            )
        })
    }
}

/// Effets compilés dans le binaire, sous la forme qu'attend la galerie.
///
/// Le manifeste est reconstruit à chaque appel plutôt que gardé : quatre petits
/// objets JSON, contre une initialisation paresseuse et son verrou. Un JSON de
/// paramètres invalide donnerait ici un manifeste sans paramètres, ce que le
/// test `les_parametres_integres_sont_du_json_valide` interdit — mieux vaut un
/// test qui échoue qu'une panique au démarrage de l'application.
fn builtin_effects() -> Vec<EffectEntry> {
    builtins::ALL
        .iter()
        .zip(builtins::swatches())
        .map(|(b, swatch)| EffectEntry {
            id: b.id.to_string(),
            kind: EffectKind::Builtin,
            // Les intégrés n'ont pas de dossier : leur repère vit en mémoire,
            // calculé une fois par exécution. Le pourquoi est dans
            // [`builtins::swatches`].
            swatch: swatch.clone(),
            manifest: Manifest {
                name: b.name.to_string(),
                description: b.description.to_string(),
                params: serde_json::from_str(b.params).unwrap_or_default(),
                api_version: EFFECTS_API_VERSION,
            },
        })
        .collect()
}

/// Échantillonne le repère de l'effet et l'écrit à côté de son manifeste — ou
/// efface celui qui s'y trouvait.
///
/// **Rien ne remonte, pas même une erreur.** Un repère est un agrément : il ne
/// doit jamais empêcher l'installation d'un effet par ailleurs valide. Un effet
/// qui lève, ne charge pas ou boucle pendant l'échantillonnage s'installe donc
/// normalement, simplement sans vignette.
///
/// L'effacement compte autant que l'écriture : un effet modifié qui ne
/// s'échantillonne plus garderait sinon l'ancien fichier et afficherait les
/// couleurs d'une version qui n'existe plus.
fn write_swatch(dir: &Path, js: &str) {
    // Le gabarit par défaut, jamais celui du clavier branché : un repère qui
    // dépendrait du matériel présent à l'installation ne serait comparable ni
    // d'un effet à l'autre, ni d'une machine à l'autre.
    let swatch = swatch::sample(js, crate::default_layout());
    let path = dir.join(SWATCH_FILE);

    if swatch.is_empty() {
        let _ = fs::remove_file(&path);
        return;
    }
    if let Ok(json) = serde_json::to_string(&swatch) {
        let _ = fs::write(&path, json);
    }
}

/// Le repère d'un effet installé, vide à défaut.
///
/// Aucun recalcul ici : lister la bibliothèque doit rester une lecture de
/// disque. Échantillonner à l'affichage ferait dépendre l'ouverture de la
/// galerie du comportement de tous les effets installés — et un repère ne change
/// pas entre deux affichages.
///
/// Un effet installé par une version antérieure n'a donc pas de repère tant
/// qu'il n'est pas réenregistré. C'est le prix de cette règle, et il est payé
/// par une pastille neutre, pas par une erreur.
fn read_swatch(dir: &Path) -> Swatch {
    fs::read_to_string(dir.join(SWATCH_FILE))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn create_dir(path: &Path) -> CmdResult<()> {
    fs::create_dir_all(path).map_err(|e| format!("création de {} impossible : {e}", path.display()))
}

fn write(path: &Path, contents: &str) -> CmdResult<()> {
    fs::write(path, contents)
        .map_err(|e| format!("écriture de {} impossible : {e}", path.display()))
}

// ---------------------------------------------------------------- commandes

/// Résout les emplacements du système. Aucun chemin n'est écrit en dur : sous
/// Windows les deux appels renvoient le même dossier, sous Linux non.
pub(crate) fn store(app: &AppHandle) -> CmdResult<Store> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("dossier de données introuvable : {e}"))?;
    let config = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("dossier de configuration introuvable : {e}"))?;
    Ok(Store::new(&data, &config))
}

/// Installe un effet et renvoie son identifiant.
#[tauri::command]
pub fn install_effect(
    app: AppHandle,
    source_ts: String,
    js: String,
    manifest: Manifest,
) -> CmdResult<String> {
    store(&app)?.install_effect(&source_ts, &js, &manifest)
}

/// Effets intégrés et installés, avec leur nature.
#[tauri::command]
pub fn list_effects(app: AppHandle) -> CmdResult<Vec<EffectEntry>> {
    store(&app)?.list_effects()
}

#[tauri::command]
pub fn delete_effect(app: AppHandle, id: String) -> CmdResult<()> {
    store(&app)?.delete_effect(&id)
}

/// Rend la source d'un effet, pour la rouvrir dans l'éditeur.
///
/// C'est la contrepartie d'`install_effect` : sans elle, un effet installé ne
/// serait plus modifiable — c'est précisément pourquoi le `.ts` est écrit sur
/// disque à côté du `.js`. Un effet intégré rend son JavaScript, qui est sa
/// source.
#[tauri::command]
pub fn read_effect_source(app: AppHandle, id: String) -> CmdResult<String> {
    store(&app)?.effect_source(&id)
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> CmdResult<Settings> {
    store(&app)?.read_settings()
}

#[tauri::command]
pub fn set_settings(app: AppHandle, settings: Settings) -> CmdResult<()> {
    store(&app)?.write_settings(&settings)
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    /// Deux dossiers distincts, comme sous Linux : un test qui les
    /// confondrait laisserait passer une confusion données / configuration.
    fn store_temporaire() -> (tempfile::TempDir, Store) {
        let tmp = tempfile::tempdir().expect("dossier temporaire");
        let store = Store::new(&tmp.path().join("data"), &tmp.path().join("config"));
        (tmp, store)
    }

    /// La part installée de la bibliothèque. Les intégrés y sont toujours
    /// présents : les tests d'installation parlent des autres.
    fn installes(store: &Store) -> Vec<EffectEntry> {
        store
            .list_effects()
            .unwrap()
            .into_iter()
            .filter(|e| e.kind == EffectKind::User)
            .collect()
    }

    /// Écrit un dossier d'effet à la main, sans passer par `install_effect`.
    /// C'est la seule façon d'obtenir un identifiant réservé sur disque — et
    /// donc de vérifier ce qui se passe alors.
    fn poser_un_dossier(tmp: &tempfile::TempDir, id: &str, js: &str) {
        let dir = tmp.path().join("data").join("effects").join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(JS_FILE), js).unwrap();
        fs::write(dir.join(SOURCE_FILE), js).unwrap();
        fs::write(
            dir.join(MANIFEST_FILE),
            serde_json::to_string(&manifeste("Usurpateur")).unwrap(),
        )
        .unwrap();
    }

    fn manifeste(name: &str) -> Manifest {
        Manifest {
            name: name.into(),
            description: "Une onde de teinte se propage depuis le centre".into(),
            params: serde_json::json!({
                "speed": { "kind": "number", "label": "Vitesse", "min": 0, "max": 400, "default": 120 }
            })
            .as_object()
            .unwrap()
            .clone(),
            api_version: EFFECTS_API_VERSION,
        }
    }

    #[test]
    fn installation_puis_lecture_font_un_aller_retour() {
        let (tmp, store) = store_temporaire();
        let manifest = manifeste("Onde circulaire");

        let id = store
            .install_effect(
                "export const x: number = 1",
                "export const x = 1",
                &manifest,
            )
            .unwrap();
        assert_eq!(id, "onde-circulaire");

        let dir = tmp.path().join("data").join("effects").join(&id);
        assert_eq!(
            fs::read_to_string(dir.join("source.ts")).unwrap(),
            "export const x: number = 1"
        );
        assert_eq!(
            fs::read_to_string(dir.join("effect.js")).unwrap(),
            "export const x = 1",
            "le .js est un livrable, pas un cache : il doit être sur disque"
        );

        let effects = installes(&store);
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].id, id);
        assert_eq!(effects[0].manifest, manifest);
        assert_eq!(store.effect_js(&id).unwrap(), "export const x = 1");
    }

    #[test]
    fn reinstaller_le_meme_nom_met_a_jour_au_lieu_de_dupliquer() {
        let (_tmp, store) = store_temporaire();
        store
            .install_effect("v1", "v1", &manifeste("Onde"))
            .unwrap();
        store
            .install_effect("v2", "v2", &manifeste("Onde"))
            .unwrap();

        assert_eq!(installes(&store).len(), 1);
    }

    #[test]
    fn la_bibliotheque_ne_contient_que_les_integres_avant_installation() {
        let (_tmp, store) = store_temporaire();
        let effects = store.list_effects().unwrap();

        assert_eq!(effects.len(), builtins::ALL.len());
        assert!(effects.iter().all(|e| e.kind == EffectKind::Builtin));
        // La galerie n'est jamais vide au premier lancement : c'est tout
        // l'objet des effets livrés.
        assert!(!effects.is_empty());

        for (entry, b) in effects.iter().zip(&builtins::ALL) {
            assert_eq!(entry.id, b.id);
            assert_eq!(entry.manifest.name, b.name);
            assert_eq!(entry.manifest.api_version, EFFECTS_API_VERSION);
            assert!(
                !entry.manifest.params.is_empty(),
                "« {} » : paramètres perdus à la lecture du JSON",
                b.id
            );
            // Les intégrés n'ont pas de dossier, mais ils ont un repère : il est
            // calculé en mémoire, à la première lecture de la bibliothèque.
            assert!(
                !entry.swatch.is_empty(),
                "« {} » : aucun repère de couleurs",
                b.id
            );
        }
    }

    // ------------------------------------------------------------ repère

    /// Un effet d'une seule couleur : son repère est cette couleur, quatre fois.
    fn effet_uni(hex: &str) -> String {
        format!(
            "export default {{ name: 'Uni', render({{ layout, frame }}) {{ \
             for (const key of layout.keys) frame.set(key, {{ r: 0x{}, g: 0x{}, b: 0x{} }}) }} }}",
            &hex[0..2],
            &hex[2..4],
            &hex[4..6]
        )
    }

    fn swatch_sur_disque(tmp: &tempfile::TempDir, id: &str) -> Option<String> {
        let path = tmp
            .path()
            .join("data")
            .join("effects")
            .join(id)
            .join(SWATCH_FILE);
        fs::read_to_string(path).ok()
    }

    /// Le repère est écrit à l'installation, à côté du manifeste, et la liste le
    /// rend sans second appel.
    #[test]
    fn l_installation_preleve_le_repere_sur_l_effet() {
        let (tmp, store) = store_temporaire();
        let id = store
            .install_effect("", &effet_uni("00ff00"), &manifeste("Uni"))
            .unwrap();

        assert_eq!(
            swatch_sur_disque(&tmp, &id).as_deref(),
            Some(r##"["#00ff00","#00ff00","#00ff00","#00ff00"]"##),
            "le repère doit être rangé à côté du manifeste"
        );
        assert_eq!(installes(&store)[0].swatch, vec!["#00ff00"; 4]);
    }

    /// Réenregistrer un effet modifié refait son repère : c'est toute la raison
    /// de l'échantillonner plutôt que de le déclarer. Un effet devenu rouge ne
    /// peut pas garder sa vignette verte.
    #[test]
    fn reenregistrer_un_effet_refait_son_repere() {
        let (_tmp, store) = store_temporaire();
        store
            .install_effect("", &effet_uni("00ff00"), &manifeste("Uni"))
            .unwrap();
        store
            .install_effect("", &effet_uni("ff0000"), &manifeste("Uni"))
            .unwrap();

        assert_eq!(installes(&store)[0].swatch, vec!["#ff0000"; 4]);
    }

    /// **Un repère qu'on n'arrive pas à calculer n'empêche pas l'installation.**
    /// C'est du code utilisateur : il a le droit d'être cassé, et l'effet doit
    /// tout de même se ranger — sans quoi on ne pourrait même plus le rouvrir
    /// dans l'éditeur pour le réparer.
    #[test]
    fn un_effet_qui_leve_s_installe_quand_meme_sans_repere() {
        let (tmp, store) = store_temporaire();
        let js = "export default { name: 'Cassé', render() { throw new Error('boum') } }";

        let id = store
            .install_effect("source", js, &manifeste("Cassé"))
            .unwrap();

        assert_eq!(store.effect_js(&id).unwrap(), js, "l'effet doit être écrit");
        assert_eq!(swatch_sur_disque(&tmp, &id), None);
        assert!(installes(&store)[0].swatch.is_empty());
    }

    /// Et le repère précédent est **effacé**, pas conservé : afficher les
    /// couleurs d'une version qui n'existe plus serait pire que n'en afficher
    /// aucune.
    #[test]
    fn un_effet_devenu_casse_perd_son_repere() {
        let (tmp, store) = store_temporaire();
        let id = store
            .install_effect("", &effet_uni("00ff00"), &manifeste("Uni"))
            .unwrap();
        assert!(swatch_sur_disque(&tmp, &id).is_some());

        store
            .install_effect(
                "",
                "export default { render() { throw 1 } }",
                &manifeste("Uni"),
            )
            .unwrap();

        assert_eq!(swatch_sur_disque(&tmp, &id), None);
        assert!(installes(&store)[0].swatch.is_empty());
    }

    #[test]
    fn suppression_retire_le_dossier() {
        let (tmp, store) = store_temporaire();
        let id = store.install_effect("", "", &manifeste("Onde")).unwrap();

        store.delete_effect(&id).unwrap();
        assert!(!tmp.path().join("data").join("effects").join(&id).exists());
        assert!(installes(&store).is_empty());

        let err = store.delete_effect(&id).unwrap_err();
        assert!(err.contains("aucun effet installé"), "message : {err}");
    }

    // ------------------------------------------------------------ intégrés

    /// Le moteur demande le JavaScript par identifiant : les intégrés doivent
    /// donc se résoudre sans dossier, sinon ils ne démarreraient jamais.
    #[test]
    fn un_effet_integre_se_lit_sans_dossier() {
        let (_tmp, store) = store_temporaire();

        for b in &builtins::ALL {
            assert_eq!(store.effect_js(b.id).unwrap(), b.js);
            // La source aussi : un effet livré est là pour être lu et modifié,
            // et son JavaScript *est* sa source.
            assert_eq!(store.effect_source(b.id).unwrap(), b.js);
        }
    }

    /// L'usurpation, dans les deux sens : par l'installation, puis par un
    /// dossier posé à la main.
    #[test]
    fn un_effet_utilisateur_ne_peut_pas_usurper_un_identifiant_integre() {
        let (tmp, store) = store_temporaire();

        for integre in &builtins::ALL {
            // Un nom qui dérive exactement vers l'identifiant visé : c'est ce
            // que taperait quelqu'un qui a lu la galerie.
            let err = store
                .install_effect("", "", &manifeste(integre.id))
                .unwrap_err();
            assert!(err.contains("effet intégré"), "message : {err}");
            assert!(
                !tmp.path()
                    .join("data")
                    .join("effects")
                    .join(integre.id)
                    .exists(),
                "« {} » : le refus est arrivé après l'écriture",
                integre.id
            );

            // Le dossier posé à la main ne prend pas la main non plus : c'est
            // toujours le code livré qui s'exécute, et la galerie n'affiche
            // qu'une entrée sous cet identifiant — celle de l'intégré.
            poser_un_dossier(&tmp, integre.id, "export default { render() {} }");
            assert_eq!(store.effect_js(integre.id).unwrap(), integre.js);
            assert_eq!(store.effect_source(integre.id).unwrap(), integre.js);

            let entrees: Vec<_> = store
                .list_effects()
                .unwrap()
                .into_iter()
                .filter(|e| e.id == integre.id)
                .collect();
            assert_eq!(
                entrees.len(),
                1,
                "« {} » : deux fois dans la liste",
                integre.id
            );
            assert_eq!(entrees[0].kind, EffectKind::Builtin);
            assert_eq!(entrees[0].manifest.name, integre.name);
        }
    }

    /// Un intégré ne se supprime pas — mais un dossier qui en usurpe
    /// l'identifiant, si : sans quoi il resterait sur disque, invisible et
    /// inamovible.
    #[test]
    fn un_effet_integre_ne_se_supprime_pas_mais_son_usurpateur_oui() {
        let (tmp, store) = store_temporaire();
        let id = builtins::ALL[0].id;

        let err = store.delete_effect(id).unwrap_err();
        assert!(err.contains("effet intégré"), "message : {err}");

        poser_un_dossier(&tmp, id, "export default { render() {} }");
        store.delete_effect(id).unwrap();
        assert!(!tmp.path().join("data").join("effects").join(id).exists());
    }

    #[test]
    fn les_identifiants_dangereux_sont_refuses() {
        let (tmp, store) = store_temporaire();
        // Un dossier voisin de `effects/`, qu'aucune remontée ne doit atteindre.
        let voisin = tmp.path().join("data").join("secrets");
        fs::create_dir_all(&voisin).unwrap();
        let trop_long = "x".repeat(MAX_ID_LEN + 1);

        for id in [
            "",
            "..",
            "../secrets",
            "..\\secrets",
            "effects/../../secrets",
            "a/b",
            "a\\b",
            "C:\\Windows",
            "/etc/passwd",
            "con",
            "nul",
            "com1",
            "LPT1",
            "Onde",       // majuscules : hors liste blanche
            "onde effet", // espace
            "onde.js",    // point
            "-onde",
            "onde-",
            trop_long.as_str(),
        ] {
            let Err(err) = store.delete_effect(id) else {
                panic!("« {id} » aurait dû être refusé");
            };
            // Le refus doit venir de la validation, pas du disque : si le
            // message parle d'effet introuvable, c'est que l'identifiant a été
            // pris pour un chemin acceptable.
            assert!(
                !err.contains("aucun effet") && !err.contains("suppression"),
                "« {id} » a atteint le disque : {err}"
            );
        }
        assert!(voisin.is_dir(), "un dossier voisin a été touché");
    }

    #[test]
    fn des_noms_hostiles_donnent_un_identifiant_sur() {
        let trop_long = "a".repeat(200);

        for name in [
            "../../etc/passwd",
            "..",
            "  ",
            "CON",
            "NUL",
            "Onde / Vague : v2",
            "🙂🙂🙂",
            "Ondulation",
            trop_long.as_str(),
        ] {
            let id = derive_id(name);
            validate_id(&id).unwrap_or_else(|e| panic!("« {name} » → « {id} » : {e}"));
        }
        assert_eq!(derive_id("Onde / Vague : v2"), "onde-vague-v2");
        assert_eq!(derive_id("🙂🙂🙂"), "effet");
        assert_eq!(derive_id("CON"), "con-effet");
    }

    #[test]
    fn un_effet_ecrit_pour_une_api_future_est_refuse() {
        let (_tmp, store) = store_temporaire();
        let mut manifest = manifeste("Onde");
        manifest.api_version = EFFECTS_API_VERSION + 1;

        let err = store.install_effect("", "", &manifest).unwrap_err();
        assert!(err.contains("API d'effets"), "message : {err}");

        manifest.api_version = 0;
        assert!(store.install_effect("", "", &manifest).is_err());
    }

    #[test]
    fn un_effet_sans_nom_est_refuse() {
        let (_tmp, store) = store_temporaire();
        let err = store.install_effect("", "", &manifeste("   ")).unwrap_err();
        assert!(err.contains("nom"), "message : {err}");
    }

    #[test]
    fn sans_fichier_les_reglages_valent_le_defaut() {
        let (_tmp, store) = store_temporaire();
        assert_eq!(store.read_settings().unwrap(), Settings::default());
    }

    #[test]
    fn les_reglages_font_un_aller_retour() {
        let (tmp, store) = store_temporaire();
        let settings = Settings {
            active_effect: Some("onde".into()),
            brightness: 128,
            device: Some(DeviceSelection {
                vid: 0x1532,
                pid: 0x0292,
            }),
            devices: vec![DeviceRecord {
                vid: 0x1532,
                pid: 0x0292,
                serial: Some("XY01".into()),
                state: DeviceState::Adopted,
            }],
        };

        store.write_settings(&settings).unwrap();
        assert_eq!(store.read_settings().unwrap(), settings);
        assert!(
            tmp.path().join("config").join("settings.json").is_file(),
            "les réglages vont dans le dossier de configuration, pas dans celui des données"
        );
    }

    #[test]
    fn un_reglage_absent_du_fichier_reprend_sa_valeur_par_defaut() {
        let (tmp, store) = store_temporaire();
        let config = tmp.path().join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("settings.json"), r#"{"brightness":10}"#).unwrap();

        let settings = store.read_settings().unwrap();
        assert_eq!(settings.brightness, 10);
        assert_eq!(settings.active_effect, None);
    }

    #[test]
    fn des_reglages_illisibles_donnent_un_message_lisible() {
        let (tmp, store) = store_temporaire();
        let config = tmp.path().join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("settings.json"), "{ ceci n'est pas du JSON").unwrap();

        let err = store.read_settings().unwrap_err();
        assert!(err.contains("réglages illisibles"), "message : {err}");
    }

    // ------------------------------------------------------- adoption

    const VID: u16 = 0x1532;
    const PID: u16 = 0x0292;

    /// Le défaut, et c'est le cœur de la décision : brancher n'est pas adopter.
    #[test]
    fn un_appareil_jamais_vu_est_detecte_pas_pilote() {
        let settings = Settings::default();
        assert_eq!(
            settings.device_state(VID, PID, Some("XY01")),
            DeviceState::Detected
        );
        assert!(settings.devices.is_empty());
    }

    #[test]
    fn une_decision_se_retient_puis_se_change() {
        let (_tmp, store) = store_temporaire();
        let mut settings = Settings::default();

        settings.set_device_state(VID, PID, Some("XY01"), DeviceState::Adopted);
        store.write_settings(&settings).unwrap();
        assert_eq!(
            store
                .read_settings()
                .unwrap()
                .device_state(VID, PID, Some("XY01")),
            DeviceState::Adopted
        );

        settings.set_device_state(VID, PID, Some("XY01"), DeviceState::Ignored);
        store.write_settings(&settings).unwrap();
        let relu = store.read_settings().unwrap();
        assert_eq!(
            relu.device_state(VID, PID, Some("XY01")),
            DeviceState::Ignored
        );
        // Changer d'avis modifie l'entrée, il n'en empile pas une seconde :
        // sinon la plus ancienne finirait par répondre à la place de la bonne.
        assert_eq!(relu.devices.len(), 1);
    }

    /// La série est l'identité, et elle sert à ça : deux claviers identiques,
    /// une seule décision. Sans elle, adopter l'un adopterait l'autre.
    #[test]
    fn deux_exemplaires_du_meme_modele_se_distinguent_par_la_serie() {
        let mut settings = Settings::default();
        settings.set_device_state(VID, PID, Some("XY01"), DeviceState::Adopted);

        assert_eq!(
            settings.device_state(VID, PID, Some("XY01")),
            DeviceState::Adopted
        );
        assert_eq!(
            settings.device_state(VID, PID, Some("XY02")),
            DeviceState::Detected,
            "le second exemplaire a hérité de la décision prise pour le premier"
        );

        settings.set_device_state(VID, PID, Some("XY02"), DeviceState::Ignored);
        assert_eq!(settings.devices.len(), 2);
        assert_eq!(
            settings.device_state(VID, PID, Some("XY01")),
            DeviceState::Adopted
        );
    }

    /// L'autre sens : une énumération qui ne déclare pas de série — hidraw sans
    /// règle udev — retrouve quand même l'appareil adopté. Reprendre la décision
    /// à chaque branchement serait exactement la cérémonie qu'on supprime.
    #[test]
    fn une_enumeration_muette_retrouve_l_appareil_adopte() {
        let mut settings = Settings::default();
        settings.set_device_state(VID, PID, Some("XY01"), DeviceState::Adopted);

        assert_eq!(settings.device_state(VID, PID, None), DeviceState::Adopted);

        // Et la série ne s'efface pas au passage, sans quoi le second
        // exemplaire deviendrait indiscernable du premier.
        settings.set_device_state(VID, PID, None, DeviceState::Adopted);
        assert_eq!(settings.devices[0].serial.as_deref(), Some("XY01"));
    }

    /// Une décision prise sans série se complète dès qu'on l'apprend, plutôt
    /// que de laisser une entrée large à côté d'une entrée précise.
    #[test]
    fn la_serie_complete_une_entree_qui_n_en_avait_pas() {
        let mut settings = Settings::default();
        settings.set_device_state(VID, PID, None, DeviceState::Adopted);
        settings.set_device_state(VID, PID, Some("XY01"), DeviceState::Adopted);

        assert_eq!(settings.devices.len(), 1);
        assert_eq!(settings.devices[0].serial.as_deref(), Some("XY01"));
    }

    /// Le fichier d'une version antérieure ne connaît pas `devices`. Il doit
    /// se relire, et surtout **ne rien perdre** quand on le réécrit.
    #[test]
    fn un_fichier_anterieur_se_relit_et_garde_ses_reglages() {
        let (tmp, store) = store_temporaire();
        let config = tmp.path().join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("settings.json"),
            r#"{"activeEffect":"onde-radiale","brightness":90}"#,
        )
        .unwrap();

        let mut settings = store.read_settings().unwrap();
        assert!(settings.devices.is_empty());
        settings.set_device_state(VID, PID, None, DeviceState::Adopted);
        store.write_settings(&settings).unwrap();

        let relu = store.read_settings().unwrap();
        assert_eq!(relu.active_effect.as_deref(), Some("onde-radiale"));
        assert_eq!(relu.brightness, 90);
        assert_eq!(relu.device_state(VID, PID, None), DeviceState::Adopted);
    }

    /// Les champs partent en camelCase, comme tous les DTO, et une entrée sans
    /// série n'écrit pas de clé vide.
    #[test]
    fn les_appareils_se_serialisent_en_camel_case() {
        let mut settings = Settings::default();
        settings.set_device_state(VID, PID, None, DeviceState::Adopted);
        let json = serde_json::to_string(&settings).unwrap();

        assert!(json.contains(r#""devices":[{"vid":5426,"pid":658,"state":"adopted"}]"#));
        assert!(json.contains(r#""activeEffect""#));
        assert!(!json.contains("serial"), "clé vide écrite : {json}");
    }
}
