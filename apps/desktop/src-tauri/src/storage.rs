//! Stockage des effets et des réglages.
//!
//! Deux emplacements distincts, décrits dans
//! [`docs/design/effects-runtime.md`](../../../../docs/design/effects-runtime.md) §3 :
//!
//! ```text
//! app_data_dir()/effects/<id>/     source.ts · effect.js · manifest.json
//! app_config_dir()/settings.json   effet actif, luminosité, périphérique choisi
//! ```
//!
//! L'effet est du **contenu**, le choix de l'effet actif est de la
//! **configuration**. Sous Windows les deux dossiers se confondent, sous Linux
//! non — d'où le passage par l'API de Tauri plutôt que par une constante.
//!
//! Toute la manipulation de fichiers vit dans [`Store`], qui reçoit ses chemins
//! de base en argument ; les commandes Tauri ne font que les résoudre. C'est ce
//! qui permet de tout tester dans un dossier temporaire, sans application.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

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
    #[serde(flatten)]
    pub manifest: Manifest,
}

/// Périphérique retenu par l'utilisateur.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceSelection {
    pub vid: u16,
    pub pid: u16,
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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            active_effect: None,
            // Pleine luminosité : c'est l'état d'un clavier qu'on vient de
            // brancher, donc le défaut le moins surprenant.
            brightness: 255,
            device: None,
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
fn validate_id(id: &str) -> CmdResult<()> {
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
        let dir = self.effects_dir.join(&id);
        create_dir(&dir)?;

        let json = serde_json::to_string_pretty(manifest)
            .map_err(|e| format!("manifeste non sérialisable : {e}"))?;
        write(&dir.join(SOURCE_FILE), source_ts)?;
        write(&dir.join(JS_FILE), js)?;
        write(&dir.join(MANIFEST_FILE), &json)?;

        Ok(id)
    }

    /// Le JavaScript exécutable d'un effet installé.
    ///
    /// C'est ce que le moteur charge, et la raison pour laquelle le `.js` est
    /// écrit sur disque à l'installation : le lire ne demande ni l'éditeur, ni
    /// la fenêtre.
    pub fn read_effect_js(&self, id: &str) -> CmdResult<String> {
        validate_id(id)?;
        let path = self.effects_dir.join(id).join(JS_FILE);
        fs::read_to_string(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => format!("aucun effet nommé « {id} »"),
            _ => format!("lecture de {} impossible : {e}", path.display()),
        })
    }

    /// La source TypeScript d'un effet installé, pour la rouvrir dans l'éditeur.
    pub fn read_effect_source(&self, id: &str) -> CmdResult<String> {
        validate_id(id)?;
        let path = self.effects_dir.join(id).join(SOURCE_FILE);
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
            let Ok(raw) = fs::read_to_string(entry.path().join(MANIFEST_FILE)) else {
                continue;
            };
            let Ok(manifest) = serde_json::from_str::<Manifest>(&raw) else {
                continue;
            };
            installed.push(EffectEntry {
                id,
                kind: EffectKind::User,
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
    pub fn delete_effect(&self, id: &str) -> CmdResult<()> {
        validate_id(id)?;
        let dir = self.effects_dir.join(id);
        if !dir.is_dir() {
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

/// Effets compilés dans le binaire.
///
/// Aucun pour l'instant : la liste existe pour que l'interface et
/// [`EffectKind`] n'aient pas à changer le jour où on en livrera.
fn builtin_effects() -> Vec<EffectEntry> {
    Vec::new()
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

/// Rend la source TypeScript d'un effet, pour la rouvrir dans l'éditeur.
///
/// C'est la contrepartie d'`install_effect` : sans elle, un effet installé ne
/// serait plus modifiable — c'est précisément pourquoi le `.ts` est écrit sur
/// disque à côté du `.js`.
#[tauri::command]
pub fn read_effect_source(app: AppHandle, id: String) -> CmdResult<String> {
    store(&app)?.read_effect_source(&id)
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

        let effects = store.list_effects().unwrap();
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].id, id);
        assert_eq!(effects[0].kind, EffectKind::User);
        assert_eq!(effects[0].manifest, manifest);
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

        let effects = store.list_effects().unwrap();
        assert_eq!(effects.len(), 1);
    }

    #[test]
    fn la_bibliotheque_est_vide_avant_toute_installation() {
        let (_tmp, store) = store_temporaire();
        assert!(store.list_effects().unwrap().is_empty());
    }

    #[test]
    fn suppression_retire_le_dossier() {
        let (tmp, store) = store_temporaire();
        let id = store.install_effect("", "", &manifeste("Onde")).unwrap();

        store.delete_effect(&id).unwrap();
        assert!(!tmp.path().join("data").join("effects").join(&id).exists());
        assert!(store.list_effects().unwrap().is_empty());

        let err = store.delete_effect(&id).unwrap_err();
        assert!(err.contains("aucun effet installé"), "message : {err}");
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
}
