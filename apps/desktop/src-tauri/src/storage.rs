//! Stockage des effets et des réglages.
//!
//! Deux emplacements distincts, décrits dans
//! [`docs/design/effects-runtime.md`](../../../../docs/design/effects-runtime.md) §3 :
//!
//! ```text
//! app_data_dir()/effects/<id>/     source.ts · effect.js · manifest.json · swatch.json
//! app_config_dir()/settings.json   préférences · appareils · effet appliqué · réglages
//! ```
//!
//! L'effet est du **contenu**, le choix de l'effet actif est de la
//! **configuration**. Sous Windows les deux dossiers se confondent, sous Linux
//! non — d'où le passage par l'API de Tauri plutôt que par une constante.
//!
//! # La forme du fichier : les préférences d'un côté, les appareils de l'autre
//!
//! `settings.json` porte deux choses qui ne se rangent pas ensemble : ce qui vaut
//! pour l'application entière ([`Preferences`]) et ce qui est **indexé par
//! appareil** (`devices`, `activeEffects`, `effectParams`). Les mélanger à la
//! racine, c'est ce qui a produit les trois vestiges mono-appareil qu'on retire
//! ici : `activeEffect`, `device` et `brightness` décrivaient **un** effet, **un**
//! appareil et **un** niveau, alors que le moteur fait tourner un effet par
//! appareil depuis l'issue #26. Ce n'était pas la mauvaise valeur, c'était la
//! mauvaise **forme** — et la réveiller telle quelle aurait donné un fichier qui
//! décrit mal la réalité.
//!
//! La règle qui en découle vaut pour tout ce qu'on ajoutera : **une préférence
//! globale va dans `preferences`, tout ce qui dépend d'un clavier va dans une
//! liste indexée.** La langue, le jour où elle arrivera, n'a donc rien à
//! arbitrer.
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
use tauri::{AppHandle, Manager, State};

use crate::builtins;
use crate::journal::LogLevel;
use crate::runtime::swatch::{self, Swatch};
use crate::{AppState, CmdResult, DeviceRef};

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

/// Luminosité d'un clavier qu'on vient de brancher : pleine.
///
/// C'est le défaut le moins surprenant, et c'est aussi la valeur que le fichier
/// **n'écrit pas** — voir [`DeviceRecord::brightness`].
pub const BRIGHTNESS_DEFAUT: u8 = 255;

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
    /// Luminosité retenue pour **cet** appareil.
    ///
    /// # Pourquoi ici plutôt qu'à la racine
    ///
    /// Elle l'est déjà partout ailleurs : `Keyboard::set_brightness` est une
    /// commande de l'appareil (`0x0f`/`0x04`), distincte de l'effet en cours, et
    /// `set_brightness(device, level)` prend un [`DeviceRef`] depuis le premier
    /// jour. Le scalaire global de `Settings` était le seul endroit qui disait le
    /// contraire — et deux claviers n'ont aucune raison de partager un niveau.
    ///
    /// # `None` veut dire « le défaut », pas « éteint »
    ///
    /// Le fichier ne porte alors rien du tout : écrire [`BRIGHTNESS_DEFAUT`] par
    /// appareil simplement branché le ferait grossir d'entrées qui ne décident de
    /// rien. Même économie que `devices` et `effectParams` — une entrée n'existe
    /// que si quelqu'un a bougé quelque chose. C'est aussi pourquoi une entrée
    /// redevenue `detected` **sans** luminosité disparaît : voir
    /// [`Self::inerte`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<u8>,
}

impl DeviceRecord {
    /// Vrai si cette entrée ne retient plus aucune décision.
    ///
    /// `detected` sans luminosité dit exactement ce que dit l'**absence**
    /// d'entrée. La garder n'apprendrait rien à qui relit ses réglages, et
    /// ferait grossir le fichier d'une ligne par appareil effleuré une fois.
    fn inerte(&self) -> bool {
        self.state == DeviceState::Detected && self.brightness.is_none()
    }

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

/// L'effet **appliqué** sur un appareil, celui qui pilote ses LED.
///
/// # Une liste, pas un scalaire
///
/// Le champ qui précédait — `activeEffect: Option<String>` — décrivait **un**
/// effet actif, alors que le moteur en fait tourner un par appareil depuis
/// l'issue #26. Aucune valeur ne pouvait rendre ce champ juste : c'est sa forme
/// qui était fausse. Une entrée par appareil, absente tant que rien n'a été
/// appliqué, est la seule qui décrive ce que le moteur fait réellement.
///
/// # Ce n'est pas ce qu'on regarde
///
/// **L'aperçu n'écrit jamais ici.** Prévisualiser un effet ne le retient pas :
/// c'est « Appliquer » qui décide, et c'est le geste qui envoie au clavier. Voir
/// [`crate::runtime::start_preview`], qui n'a aucun accès au disque.
///
/// # La clé est le [`DeviceRef`], sans numéro de série
///
/// Même raison que [`EffectParamsRecord`] : les boucles du moteur sont indexées
/// par VID/PID, deux exemplaires du même modèle en partagent une, et les
/// distinguer ici promettrait une séparation que le moteur ne tient pas.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveEffectRecord {
    pub vid: u16,
    pub pid: u16,
    /// Identifiant de l'effet appliqué.
    pub effect: String,
}

/// Réglages d'un effet, retenus pour **un** appareil.
///
/// # Pourquoi l'appareil et l'effet ensemble
///
/// « La vague, mais plus lente » se règle sur un clavier donné : le même effet
/// n'a aucune raison de tourner à la même vitesse sur deux appareils, et deux
/// effets du même appareil n'ont pas les mêmes paramètres. La clé est donc la
/// paire, et changer d'effet puis revenir retrouve ses réglages.
///
/// # Sans le numéro de série, contrairement à [`DeviceRecord`]
///
/// Délibéré : toutes les commandes du moteur visent un [`DeviceRef`], c'est-à-dire
/// un VID et un PID. Deux exemplaires du même modèle partagent déjà leur boucle
/// de rendu — les distinguer *ici* promettrait une séparation que le reste de
/// l'application ne tient pas, et le réglage semblerait perdu une fois sur deux.
/// L'adoption, elle, décide d'ouvrir un appareil précis : elle a besoin de la
/// série, et c'est pourquoi elle la porte.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EffectParamsRecord {
    pub vid: u16,
    pub pid: u16,
    /// Identifiant de l'effet réglé.
    pub effect: String,
    /// Les valeurs, telles que l'interface les envoie au moteur.
    ///
    /// JSON brut, comme [`Manifest::params`] : leur forme est celle de
    /// `ParamValue` côté TypeScript — un nombre, une chaîne, un booléen ou une
    /// couleur `{r,g,b}` — et le Rust ne les interprète pas. Les typer ici
    /// créerait une seconde source de vérité, qui divergerait au premier type
    /// de paramètre ajouté.
    pub values: serde_json::Map<String, serde_json::Value>,
}

/// Ce qui vaut pour l'application entière, et pour aucun appareil en
/// particulier.
///
/// Un objet à part plutôt que des champs à la racine : c'est le rangement qui
/// empêche la confusion dont ce module vient de sortir. Tout ce qui dépend d'un
/// clavier vit dans une liste indexée ; ce qui n'en dépend pas vit ici, et la
/// langue — quand elle arrivera — n'aura rien à arbitrer.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Preferences {
    /// Niveau du journal, quand quelqu'un l'a changé depuis l'application.
    ///
    /// `None` — donc absent du fichier — veut dire « le défaut », et non « pas de
    /// journal » : écrire le défaut ferait croire à une décision là où il n'y en
    /// a pas eu, et figerait au passage un choix que la prochaine version
    /// pourrait vouloir revoir.
    ///
    /// **Il survit au redémarrage**, et c'est un arbitrage : le retour
    /// automatique au défaut protégerait du disque plein, la persistance sert
    /// celui qui traque un défaut **au démarrage** — l'adoption des appareils en
    /// est un — à qui l'on ne peut pas demander de remonter le niveau après coup.
    /// Le prix est payé par la mention qu'en fait l'interface. Voir
    /// [`crate::journal`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_level: Option<LogLevel>,
}

/// Réglages persistants.
///
/// `#[serde(default)]` sur la structure entière : un `settings.json` écrit par
/// une version antérieure, à qui il manque un champ ajouté depuis, se relit
/// sans erreur au lieu de rendre l'application muette au démarrage.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Ce qui ne dépend d'aucun appareil. Voir [`Preferences`].
    pub preferences: Preferences,
    /// Décisions prises appareil par appareil, luminosité comprise.
    ///
    /// Ne contient que celles qui **diffèrent du défaut** : un appareil absent
    /// de cette liste est `detected` et à pleine luminosité, ce qui est
    /// exactement l'état d'un appareil jamais rencontré. Le fichier ne grossit
    /// donc pas d'une entrée à chaque périphérique branché une fois.
    pub devices: Vec<DeviceRecord>,
    /// L'effet **appliqué** sur chaque appareil. Voir [`ActiveEffectRecord`].
    ///
    /// Même économie que le reste : pas d'entrée tant que rien n'a été appliqué,
    /// et l'entrée part quand l'effet s'arrête ou qu'il est supprimé.
    pub active_effects: Vec<ActiveEffectRecord>,
    /// Réglages d'effet retenus, par appareil et par effet.
    ///
    /// Même économie que `devices` : une entrée n'existe que si quelqu'un a
    /// **déplacé** un curseur. Rétablir les valeurs déclarées la retire, plutôt
    /// que d'écrire une copie des défauts que la prochaine version de l'effet
    /// contredirait.
    pub effect_params: Vec<EffectParamsRecord>,
    /// Le niveau du journal tel qu'une version antérieure l'écrivait, **à la
    /// racine**.
    ///
    /// Lu, jamais réécrit (`skip_serializing`) : [`Store::read_settings`] le
    /// verse dans [`Preferences`], et il disparaît du fichier à la première
    /// écriture. Sans cette passerelle, déplacer `logLevel` aurait ramené au
    /// défaut le niveau de celui qui était **en train** de chercher une panne —
    /// c'est-à-dire au pire moment, puisque c'est le seul où ce réglage sert.
    ///
    /// L'alternative écartée : ne rien faire et l'assumer. Elle coûtait douze
    /// lignes de moins et une session de diagnostic perdue. À retirer quand plus
    /// aucun `settings.json` antérieur à la v2.1 ne circule.
    #[serde(default, rename = "logLevel", skip_serializing)]
    log_level_herite: Option<LogLevel>,
}

impl Settings {
    /// Verse dans [`Preferences`] ce qu'un fichier antérieur portait à la racine.
    ///
    /// Ce qui est déjà rangé l'emporte : un fichier écrit par cette version a
    /// raison contre une clé héritée qu'un éditeur de texte y aurait laissée.
    fn absorber_l_heritage(&mut self) {
        if let Some(niveau) = self.log_level_herite.take() {
            self.preferences.log_level.get_or_insert(niveau);
        }
    }

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
                brightness: None,
            }),
        }
        self.elaguer();
    }

    /// La luminosité retenue pour cet appareil, ou [`BRIGHTNESS_DEFAUT`].
    pub fn brightness(&self, vid: u16, pid: u16, serial: Option<&str>) -> u8 {
        self.position(vid, pid, serial)
            .and_then(|i| self.devices[i].brightness)
            .unwrap_or(BRIGHTNESS_DEFAUT)
    }

    /// Retient une luminosité. [`BRIGHTNESS_DEFAUT`] **oublie** l'entrée.
    ///
    /// Le parallèle de « rétablir les valeurs déclarées » pour les réglages
    /// d'effet : remonter le curseur à fond ne doit pas écrire 255 dans le
    /// fichier, il doit y retirer la ligne. Un appareil dont c'était la seule
    /// décision disparaît alors complètement — voir [`DeviceRecord::inerte`].
    ///
    /// Rend vrai si quelque chose a changé, pour qu'on ne repasse pas par le
    /// fichier temporaire et son renommage quand il n'y a rien à y écrire.
    pub fn set_brightness(&mut self, vid: u16, pid: u16, serial: Option<&str>, level: u8) -> bool {
        let retenu = (level != BRIGHTNESS_DEFAUT).then_some(level);
        match self.position(vid, pid, serial) {
            Some(i) => {
                if self.devices[i].brightness == retenu {
                    return false;
                }
                self.devices[i].brightness = retenu;
                // Même règle que [`Self::set_device_state`] : la série se
                // complète si on vient de l'apprendre, elle ne s'efface jamais.
                if self.devices[i].serial.is_none() {
                    self.devices[i].serial = serial.map(str::to_owned);
                }
            }
            None => {
                // Le défaut, sur un appareil dont on ne retient rien : il n'y a
                // aucune entrée à créer pour n'y rien mettre.
                let Some(level) = retenu else { return false };
                self.devices.push(DeviceRecord {
                    vid,
                    pid,
                    serial: serial.map(str::to_owned),
                    state: DeviceState::default(),
                    brightness: Some(level),
                });
            }
        }
        self.elaguer();
        true
    }

    /// Retire les entrées d'appareil qui ne retiennent plus rien.
    fn elaguer(&mut self) {
        self.devices.retain(|r| !r.inerte());
    }

    /// L'effet appliqué sur cet appareil, s'il y en a un.
    pub fn active_effect(&self, vid: u16, pid: u16) -> Option<&str> {
        self.active_effects
            .iter()
            .find(|r| r.vid == vid && r.pid == pid)
            .map(|r| r.effect.as_str())
    }

    /// Retient l'effet appliqué, ou l'oublie avec `None`.
    ///
    /// Rend vrai si quelque chose a changé : relancer deux fois le même effet sur
    /// le même clavier — ce que fait un double-clic — ne doit pas réécrire le
    /// fichier.
    pub fn set_active_effect(&mut self, vid: u16, pid: u16, effect: Option<&str>) -> bool {
        let position = self
            .active_effects
            .iter()
            .position(|r| r.vid == vid && r.pid == pid);

        match (position, effect) {
            (Some(i), None) => {
                self.active_effects.remove(i);
                true
            }
            (Some(i), Some(e)) if self.active_effects[i].effect == e => false,
            (Some(i), Some(e)) => {
                self.active_effects[i].effect = e.to_owned();
                true
            }
            (None, None) => false,
            (None, Some(e)) => {
                self.active_effects.push(ActiveEffectRecord {
                    vid,
                    pid,
                    effect: e.to_owned(),
                });
                true
            }
        }
    }

    /// Les réglages retenus pour cet effet sur cet appareil, s'il y en a.
    pub fn effect_params(
        &self,
        vid: u16,
        pid: u16,
        effect: &str,
    ) -> Option<&serde_json::Map<String, serde_json::Value>> {
        self.effect_params
            .iter()
            .find(|r| r.vid == vid && r.pid == pid && r.effect == effect)
            .map(|r| &r.values)
    }

    /// Retient des réglages. Une table **vide** efface l'entrée.
    ///
    /// C'est ce qui fait de « rétablir les valeurs déclarées » un oubli et non
    /// une copie : l'effet repart alors de son manifeste, y compris si une
    /// version ultérieure en change les défauts.
    pub fn set_effect_params(
        &mut self,
        vid: u16,
        pid: u16,
        effect: &str,
        values: serde_json::Map<String, serde_json::Value>,
    ) {
        let position = self
            .effect_params
            .iter()
            .position(|r| r.vid == vid && r.pid == pid && r.effect == effect);

        match (position, values.is_empty()) {
            (Some(i), true) => {
                self.effect_params.remove(i);
            }
            (Some(i), false) => self.effect_params[i].values = values,
            (None, true) => {}
            (None, false) => self.effect_params.push(EffectParamsRecord {
                vid,
                pid,
                effect: effect.to_owned(),
                values,
            }),
        }
    }

    /// Oublie un effet **partout** : ses réglages, et son application.
    ///
    /// Appelée quand l'effet est supprimé. Sans cela ses réglages resteraient
    /// dans `settings.json` pour un identifiant que plus rien ne désigne, et le
    /// fichier ne ferait que grossir.
    ///
    /// # Pourquoi l'application part avec, et pas seulement les réglages
    ///
    /// C'est le piège que l'issue #48 avait relevé et que #64 tranche ici :
    /// `delete_effect` laissait un **identifiant pendant**. Tant que le champ
    /// était mort, la question ne se posait pas ; maintenant qu'`activeEffects`
    /// est écrit, elle se pose, et les deux réponses possibles n'ont pas le même
    /// prix :
    ///
    /// - **purger à la suppression** — retenu : le fichier ne contient jamais un
    ///   identifiant que la bibliothèque ne connaît pas, et cet invariant se
    ///   vérifie sans faire tourner quoi que ce soit ;
    /// - se replier en silence au démarrage — écarté *comme seule mesure* : un
    ///   silence au lancement est exactement le genre de panne qui coûte une
    ///   session, et l'identifiant survivrait à autant de démarrages qu'on veut.
    ///
    /// Le repli reste nécessaire en **seconde** barrière — un dossier d'effet
    /// retiré à la main ne passe pas par ici — mais il n'est plus le seul.
    ///
    /// Rend vrai si quelque chose a été retiré, pour qu'on ne réécrive pas le
    /// fichier quand il n'y a rien à y changer.
    pub fn forget_effect(&mut self, effect: &str) -> bool {
        let avant = self.effect_params.len() + self.active_effects.len();
        self.effect_params.retain(|r| r.effect != effect);
        self.active_effects.retain(|r| r.effect != effect);
        self.effect_params.len() + self.active_effects.len() != avant
    }
}

/// Les valeurs sur lesquelles un effet doit démarrer : ce qu'il **déclare**,
/// recouvert par ce qu'on a **retenu** pour cet appareil.
///
/// # Pourquoi ce calcul existe en Rust
///
/// La fenêtre le fait déjà, en deux morceaux — `startingParams` pour les défauts
/// du manifeste, `merge` pour le recouvrement. Mais l'icône de zone de
/// notification lance un effet **sans fenêtre** : elle ne peut rien emprunter au
/// TypeScript, et lancer un effet avec un objet de paramètres vide ne donnerait
/// pas le même éclairage que le même clic fait depuis la galerie. Voir
/// [`crate::tray`].
///
/// Les deux écritures de la règle doivent donc rester d'accord. Ce qu'elles
/// disent, et c'est la seule chose à retenir : **bornée aux paramètres
/// déclarés**. Un réglage retenu pour un paramètre que l'effet n'a plus disparaît
/// de lui-même, au lieu de voyager indéfiniment vers une boucle qui ne le lit
/// plus — et un paramètre déclaré sans valeur retenue prend son défaut, jamais
/// rien.
///
/// Un manifeste dont un paramètre ne déclare pas de `default` est laissé de
/// côté : `params` est du JSON brut que le Rust n'interprète pas (voir
/// [`Manifest::params`]), et inventer une valeur pour une sorte de paramètre
/// qu'on ne connaît pas serait pire que de laisser l'effet appliquer la sienne.
pub(crate) fn starting_params(
    manifest: &Manifest,
    retenus: Option<&serde_json::Map<String, serde_json::Value>>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut out = serde_json::Map::new();
    for (id, spec) in &manifest.params {
        if let Some(defaut) = spec.get("default") {
            out.insert(id.clone(), defaut.clone());
        }
    }
    if let Some(retenus) = retenus {
        for (id, valeur) in retenus {
            // Seulement ce que le manifeste déclare encore : c'est le même
            // bornage que côté fenêtre, et c'est lui qui fait disparaître un
            // réglage devenu orphelin.
            if manifest.params.contains_key(id) {
                out.insert(id.clone(), valeur.clone());
            }
        }
    }
    out
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

    /// Dit si cet effet peut être supprimé, **sans rien supprimer**.
    ///
    /// Le dossier est consulté **avant** le cas des intégrés : c'est ce qui laisse
    /// retirer un dossier qui usurperait un identifiant intégré, invisible dans la
    /// liste et inexécutable, mais bien présent sur disque. L'ordre est l'inverse
    /// de celui de la résolution à l'exécution ([`Self::effect_js`]), qui consulte
    /// les intégrés d'abord pour qu'un effet livré ne puisse pas être usurpé. Les
    /// deux asymétries servent le même but et ne doivent pas être alignées.
    ///
    /// Séparé de [`Self::delete_effect`] parce que la commande arrête les boucles
    /// **entre** le refus et l'effacement : refuser après coup ferait payer à un
    /// effet intégré qui tourne le prix d'un arrêt qu'on ne lui devait pas.
    pub fn check_deletable(&self, id: &str) -> CmdResult<()> {
        validate_id(id)?;
        if self.effects_dir.join(id).is_dir() {
            return Ok(());
        }
        if builtins::find(id).is_some() {
            return Err(format!(
                "« {id} » est un effet intégré : il est livré avec l'application et ne peut pas être supprimé"
            ));
        }
        Err(format!("aucun effet installé sous l'identifiant « {id} »"))
    }

    /// Supprime `effects/<id>/`.
    pub fn delete_effect(&self, id: &str) -> CmdResult<()> {
        // Revérifié plutôt que supposé : entre le refus de la commande et cet
        // appel, le dossier a pu disparaître — et c'est ici qu'on le dit.
        self.check_deletable(id)?;
        let dir = self.effects_dir.join(id);
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
        let mut settings: Settings = serde_json::from_str(&raw).map_err(|e| {
            format!(
                "réglages illisibles dans {} : {e}",
                self.settings_file.display()
            )
        })?;
        // Ici et nulle part ailleurs : c'est le seul chemin par lequel un fichier
        // entre dans l'application, donc le seul endroit où une clé héritée peut
        // être traduite une fois pour toutes.
        settings.absorber_l_heritage();
        Ok(settings)
    }

    /// Écrit `settings.json`.
    ///
    /// Passage par un fichier temporaire puis renommage : une coupure en cours
    /// d'écriture laisserait sinon des réglages tronqués, donc une application
    /// qui ne démarre plus.
    ///
    /// Un seul nom de temporaire, et il peut le rester : les commandes Tauri
    /// **synchrones** s'exécutent sur le fil principal, donc deux séquences
    /// lire-modifier-écrire ne s'entrelacent pas. Un nom unique par écriture a
    /// été essayé puis retiré — il défendait contre un entrelacement que rien ne
    /// produit, et laissait un fichier derrière lui à chaque échec, là où un nom
    /// fixe est simplement réécrit à la tentative suivante.
    ///
    /// ⚠️ **Ce raisonnement vaut à l'intérieur d'un processus, pas entre deux.**
    /// Deux candeo écriraient dans le *même* `settings.json.tmp`, et l'un
    /// renommerait ce que l'autre est en train d'écrire : le renommage resterait
    /// atomique, mais ce qu'il publierait ne le serait plus — des réglages
    /// tronqués, ou ceux du voisin. Ce qui tient ce nom fixe, c'est donc
    /// [`crate::single_instance`], et les deux décisions ne se défont pas l'une
    /// sans l'autre : rendre candeo multi-instance obligerait à reprendre ce nom,
    /// et le reprendre sans cela n'achèterait rien.
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

    /// Réécrit `settings.json` avec les valeurs par défaut.
    ///
    /// **Ne touche à aucun effet**, et ne saurait pas le faire : elle n'écrit que
    /// dans `settings_file`. C'est la distinction que porte tout ce module — un
    /// effet est du contenu, le choix de l'effet actif est de la configuration —
    /// et c'est ici qu'elle protège quelque chose : celui qui veut seulement
    /// désadopter un clavier ne doit pas perdre du code écrit à la main.
    ///
    /// Le fichier est réécrit plutôt qu'effacé. Les deux se relisent pareil —
    /// [`Self::read_settings`] rend les défauts quand il n'y a pas de fichier —
    /// mais un fichier qui disparaît ressemble à un dégât, là où un fichier remis
    /// à plat se lit et se compare.
    pub fn reset_settings(&self) -> CmdResult<()> {
        self.write_settings(&Settings::default())
    }
}

/// Effets compilés dans le binaire, sous la forme qu'attend la galerie.
///
/// Le manifeste est reconstruit à chaque appel plutôt que gardé : cinq petits
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

/// Supprime un effet, **et tout ce que `settings.json` retenait de lui** : ses
/// réglages, et son application sur les appareils.
///
/// Les deux vont ensemble : laisser les réglages derrière ferait grossir
/// `settings.json` d'entrées désignant un identifiant que plus rien ne nomme, et
/// un effet réinstallé plus tard sous le même nom hériterait en silence des
/// réglages de son homonyme disparu. Laisser l'**application** derrière laisserait
/// en plus un identifiant pendant, que le jour où l'on reprendra l'effet au
/// démarrage on essaierait de lancer — voir [`Settings::forget_effect`], où ce
/// choix est arbitré.
///
/// L'oubli vient **après** la suppression : si celle-ci échoue, l'effet est
/// toujours là et ses réglages doivent l'être aussi.
///
/// # Trois temps, dans cet ordre
///
/// 1. **le refus**, avant tout le reste : un effet intégré ou un identifiant qui
///    ne désigne rien s'entend dire non sans que rien n'ait été arrêté ;
/// 2. **l'arrêt des boucles**, sur tous les appareils où l'effet tourne, et
///    avant l'effacement : le moteur exécute un `effect.js` lu au démarrage et
///    gardé en mémoire, il continuerait donc sans erreur visible sur un dossier
///    disparu ;
/// 3. **l'effacement**, puis l'oubli des réglages.
///
/// L'arrêt côté Rust plutôt que dans la fenêtre : c'est le seul endroit qui le
/// garantisse quel que soit l'appelant, et l'invariant — aucune boucle ne fait
/// tourner un effet supprimé — ne tient que s'il tient partout.
#[tauri::command]
pub fn delete_effect(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let store = store(&app)?;
    store.check_deletable(&id)?;
    state.engine.stop_everywhere(&id);
    store.delete_effect(&id)?;

    let mut settings = store.read_settings()?;
    if settings.forget_effect(&id) {
        store.write_settings(&settings)?;
    }
    Ok(())
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

/// Remet la configuration au défaut, et repose les appareils.
///
/// # Ce que ça ne fait pas
///
/// **Aucun effet n'est touché.** Les effets écrits vivent dans
/// `app_data_dir()/effects/`, la configuration dans `settings.json` : deux
/// emplacements, deux gestes. Retirer un effet est une autre commande,
/// [`delete_effect`], une par effet — confondre les deux ferait perdre du code
/// écrit à la main à qui voulait seulement désadopter un clavier.
///
/// Ce n'est pas non plus un endroit où libérer des ressources côté effets :
/// chaque boucle porte son `Runtime` et son `Context` QuickJS, et les deux sont
/// détruits avec elle — tout le tas JavaScript part avec.
///
/// # L'ordre
///
/// Les appareils sont reposés **avant** l'écriture : remettre la table des
/// appareils à zéro pendant qu'un effet tourne laisserait des boucles que plus
/// aucune décision ne désigne. Voir [`crate::release_devices`] pour le détail de
/// ce que « reposer » veut dire.
///
/// Le magasin est résolu en premier, avant même l'arrêt : un dossier de
/// configuration introuvable doit se dire sans avoir rien éteint.
///
/// Le niveau du journal repart au défaut avec le reste, **et tout de suite** : il
/// vient d'être effacé du fichier, le laisser appliqué jusqu'au prochain
/// lancement ferait mentir l'écran qui l'affiche.
#[tauri::command]
pub fn reset_settings(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let store = store(&app)?;
    crate::release_devices(&state);
    store.reset_settings()?;
    crate::journal::revenir_au_defaut();
    Ok(())
}

/// Retient les réglages d'un effet pour un appareil, sans toucher au reste.
///
/// Une commande dédiée plutôt qu'un `set_settings` depuis l'interface : la
/// lecture, la modification et l'écriture se font ici, d'un seul tenant.
///
/// Ce n'est pas une précaution contre un entrelacement — les commandes
/// synchrones s'exécutent sur le fil principal, elles ne se chevauchent pas.
/// C'est une précaution contre une **copie périmée** : la fenêtre lit les
/// réglages une fois, au montage de l'écran, et un `set_settings` posté au
/// premier mouvement de curseur renverrait cet instantané tel quel, effaçant
/// tout ce qui aurait été décidé depuis. Ce n'est pas un cas d'école — le Rust
/// écrit `settings.json` à chaque `adopt_device`, et adopter un appareil est
/// justement ce qu'on fait entre deux réglages.
///
/// Elle ne change **rien** à l'effet en cours : ajuster à chaud, c'est
/// [`crate::runtime::set_effect_params`]. Les deux sont séparées parce qu'elles
/// n'ont ni la même cadence ni la même destination — des dizaines d'appels par
/// seconde vers la boucle de rendu, un seul vers le disque quand le curseur
/// s'arrête.
#[tauri::command]
pub fn remember_effect_params(
    app: AppHandle,
    device: DeviceRef,
    effect: String,
    params: serde_json::Map<String, serde_json::Value>,
) -> CmdResult<()> {
    validate_id(&effect)?;
    let store = store(&app)?;
    let mut settings = store.read_settings()?;

    // Rien de neuf : on ne réécrit pas le fichier. Un curseur qu'on déplace puis
    // qu'on ramène repasse par ici, et l'aller-retour entre deux effets aussi —
    // une écriture disque par passage n'apprendrait rien à personne.
    let retenus = settings.effect_params(device.vid, device.pid, &effect);
    if retenus == Some(&params) || (retenus.is_none() && params.is_empty()) {
        return Ok(());
    }

    settings.set_effect_params(device.vid, device.pid, &effect, params);
    store.write_settings(&settings)
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

    /// Le refus doit s'obtenir **sans rien supprimer**.
    ///
    /// C'est ce qui permet à la commande d'arrêter les boucles entre le refus et
    /// l'effacement : un effet intégré qui tourne s'entend dire non sans avoir
    /// payé l'arrêt de sa boucle au passage.
    #[test]
    fn le_refus_de_suppression_s_obtient_sans_rien_supprimer() {
        let (tmp, store) = store_temporaire();
        let id = store.install_effect("", "", &manifeste("Onde")).unwrap();

        store.check_deletable(&id).unwrap();
        assert!(
            tmp.path().join("data").join("effects").join(&id).is_dir(),
            "la vérification a emporté le dossier"
        );

        let err = store.check_deletable(builtins::ALL[0].id).unwrap_err();
        assert!(err.contains("effet intégré"), "message : {err}");

        let err = store.check_deletable("jamais-installe").unwrap_err();
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
        // Le disque d'abord, y compris pour le refus préalable de la commande :
        // s'il consultait les intégrés en premier, l'usurpateur serait refusé
        // avant même d'arriver à la suppression.
        store.check_deletable(id).unwrap();
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
            preferences: Preferences {
                log_level: Some(LogLevel::Debug),
            },
            devices: vec![DeviceRecord {
                vid: 0x1532,
                pid: 0x0292,
                serial: Some("XY01".into()),
                state: DeviceState::Adopted,
                brightness: Some(128),
            }],
            active_effects: vec![ActiveEffectRecord {
                vid: 0x1532,
                pid: 0x0292,
                effect: "onde".into(),
            }],
            effect_params: vec![EffectParamsRecord {
                vid: 0x1532,
                pid: 0x0292,
                effect: "respiration".into(),
                values: valeurs(&[("period", serde_json::json!(12.5))]),
            }],
            log_level_herite: None,
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
        fs::write(
            config.join("settings.json"),
            r#"{"devices":[{"vid":5426,"pid":658,"state":"adopted","brightness":10}]}"#,
        )
        .unwrap();

        let settings = store.read_settings().unwrap();
        assert_eq!(settings.brightness(VID, PID, None), 10);
        assert_eq!(settings.active_effect(VID, PID), None);
        assert_eq!(settings.preferences, Preferences::default());
    }

    /// **Les trois vestiges mono-appareil ont disparu du fichier.** Les garder
    /// aurait produit un `settings.json` qui décrit un effet actif, un appareil
    /// choisi et un niveau de luminosité là où le moteur en fait tourner un par
    /// appareil depuis l'issue #26.
    #[test]
    fn le_fichier_ne_porte_plus_de_champ_mono_appareil() {
        let (_tmp, store) = store_temporaire();
        let mut settings = Settings::default();
        settings.set_device_state(VID, PID, Some("XY01"), DeviceState::Adopted);
        settings.set_active_effect(VID, PID, Some("onde"));
        settings.set_brightness(VID, PID, Some("XY01"), 40);
        store.write_settings(&settings).unwrap();

        let json = serde_json::to_string(&settings).unwrap();
        for mort in [r#""activeEffect""#, r#""device":"#, r#""brightness":40,"#] {
            assert!(!json.contains(mort), "« {mort} » subsiste : {json}");
        }
        // Ce qui les remplace est bien là, et indexé par appareil.
        assert!(json.contains(r#""activeEffects":[{"vid":5426,"pid":658,"effect":"onde"}]"#));
        assert!(json.contains(r#""brightness":40"#));
    }

    /// La luminosité est une décision **de l'appareil** : deux claviers ne
    /// partagent pas un niveau, et c'est tout l'objet du déplacement.
    #[test]
    fn la_luminosite_est_retenue_appareil_par_appareil() {
        let mut settings = Settings::default();
        settings.set_device_state(VID, PID, Some("XY01"), DeviceState::Adopted);
        settings.set_device_state(VID, PID + 1, Some("ZZ02"), DeviceState::Adopted);

        assert!(settings.set_brightness(VID, PID, Some("XY01"), 40));
        assert_eq!(settings.brightness(VID, PID, Some("XY01")), 40);
        assert_eq!(
            settings.brightness(VID, PID + 1, Some("ZZ02")),
            BRIGHTNESS_DEFAUT,
            "le niveau du premier a débordé sur le second"
        );

        // Rien de neuf : pas de réécriture du fichier pour la même valeur.
        assert!(!settings.set_brightness(VID, PID, Some("XY01"), 40));
    }

    /// Le défaut ne s'écrit pas, et **retirer** est ce que fait le retour au
    /// maximum : le pendant de « rétablir les valeurs déclarées » côté effets.
    #[test]
    fn la_luminosite_par_defaut_ne_laisse_aucune_entree() {
        let mut settings = Settings::default();

        // Sur un appareil dont on ne retient rien : aucune entrée n'est créée.
        assert!(!settings.set_brightness(VID, PID, None, BRIGHTNESS_DEFAUT));
        assert!(settings.devices.is_empty());

        // Réglée puis ramenée au maximum : l'entrée naît puis disparaît, parce
        // qu'elle ne retenait que ça.
        assert!(settings.set_brightness(VID, PID, None, 40));
        assert_eq!(settings.devices.len(), 1);
        assert!(settings.set_brightness(VID, PID, None, BRIGHTNESS_DEFAUT));
        assert!(
            settings.devices.is_empty(),
            "une entrée qui ne décide plus rien est restée : {:?}",
            settings.devices
        );

        // Mais une décision d'adoption, elle, retient l'entrée.
        settings.set_device_state(VID, PID, None, DeviceState::Ignored);
        settings.set_brightness(VID, PID, None, 40);
        settings.set_brightness(VID, PID, None, BRIGHTNESS_DEFAUT);
        assert_eq!(settings.devices.len(), 1);
        assert_eq!(settings.device_state(VID, PID, None), DeviceState::Ignored);
    }

    /// L'effet appliqué est une liste indexée, pas un scalaire : deux claviers
    /// portent deux effets, et c'est exactement ce que le moteur fait.
    #[test]
    fn l_effet_applique_est_retenu_par_appareil() {
        let mut settings = Settings::default();

        assert!(settings.set_active_effect(VID, PID, Some("onde")));
        assert!(settings.set_active_effect(VID, PID + 1, Some("respiration")));
        assert_eq!(settings.active_effect(VID, PID), Some("onde"));
        assert_eq!(settings.active_effect(VID, PID + 1), Some("respiration"));

        // Relancer le même effet ne réécrit pas le fichier : c'est un double-clic.
        assert!(!settings.set_active_effect(VID, PID, Some("onde")));
        // Changer d'effet remplace l'entrée, il n'en empile pas une seconde.
        assert!(settings.set_active_effect(VID, PID, Some("balayage")));
        assert_eq!(settings.active_effects.len(), 2);

        // Arrêter oublie, plutôt que de laisser un identifiant qui ne décrit rien.
        assert!(settings.set_active_effect(VID, PID, None));
        assert_eq!(settings.active_effect(VID, PID), None);
        assert_eq!(settings.active_effects.len(), 1);
        assert!(!settings.set_active_effect(VID, PID, None));
    }

    /// Le niveau du journal **survit au redémarrage** — c'est l'arbitrage retenu
    /// pour qui traque un défaut au démarrage — mais tant que personne ne l'a
    /// changé, il n'apparaît pas dans le fichier : écrire le défaut ferait croire
    /// à une décision là où il n'y en a pas eu.
    #[test]
    fn le_niveau_de_journal_se_retient_et_ne_s_ecrit_que_choisi() {
        let (tmp, store) = store_temporaire();
        let fichier = tmp.path().join("config").join("settings.json");

        store.write_settings(&Settings::default()).unwrap();
        let ecrit = fs::read_to_string(&fichier).unwrap();
        assert!(
            !ecrit.contains("logLevel"),
            "le défaut a été écrit : {ecrit}"
        );

        let settings = Settings {
            preferences: Preferences {
                log_level: Some(LogLevel::Trace),
            },
            ..Settings::default()
        };
        store.write_settings(&settings).unwrap();
        assert!(fs::read_to_string(&fichier)
            .unwrap()
            .contains(r#""logLevel": "trace""#));
        assert_eq!(
            store.read_settings().unwrap().preferences.log_level,
            Some(LogLevel::Trace)
        );
    }

    /// **La passerelle de la v2.1.** `logLevel` a quitté la racine pour
    /// [`Preferences`] ; un fichier antérieur doit y arriver quand même, sans
    /// quoi le niveau retomberait au défaut sous celui qui était justement en
    /// train de chercher une panne.
    #[test]
    fn un_niveau_de_journal_ecrit_a_la_racine_est_recupere() {
        let (tmp, store) = store_temporaire();
        let config = tmp.path().join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("settings.json"), r#"{"logLevel":"debug"}"#).unwrap();

        let settings = store.read_settings().unwrap();
        assert_eq!(settings.preferences.log_level, Some(LogLevel::Debug));

        // Et il ne repart pas à la racine : la passerelle traduit une fois.
        store.write_settings(&settings).unwrap();
        let ecrit = fs::read_to_string(config.join("settings.json")).unwrap();
        assert!(ecrit.contains(r#""preferences""#), "écrit : {ecrit}");
        assert_eq!(
            ecrit.matches(r#""logLevel""#).count(),
            1,
            "le niveau est écrit deux fois : {ecrit}"
        );
    }

    /// Ce qui est déjà rangé l'emporte sur la clé héritée : un fichier écrit par
    /// cette version a raison contre une racine qu'un éditeur de texte y aurait
    /// laissée.
    #[test]
    fn les_preferences_l_emportent_sur_la_cle_heritee() {
        let (tmp, store) = store_temporaire();
        let config = tmp.path().join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("settings.json"),
            r#"{"logLevel":"debug","preferences":{"logLevel":"error"}}"#,
        )
        .unwrap();

        assert_eq!(
            store.read_settings().unwrap().preferences.log_level,
            Some(LogLevel::Error)
        );
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

    /// **La distinction que tout ce module tient**, vérifiée là où elle coûte le
    /// plus cher à perdre : remettre la configuration au défaut ne vide pas la
    /// bibliothèque. Qui veut seulement désadopter un clavier ne doit pas y
    /// laisser du code écrit à la main.
    #[test]
    fn la_remise_a_zero_oublie_la_configuration_et_garde_les_effets() {
        let (tmp, store) = store_temporaire();
        let id = store
            .install_effect("la source", "le js", &manifeste("Onde"))
            .unwrap();

        let mut settings = Settings::default();
        settings.set_device_state(VID, PID, Some("XY01"), DeviceState::Adopted);
        settings.set_brightness(VID, PID, Some("XY01"), 12);
        settings.set_active_effect(VID, PID, Some(&id));
        settings.set_effect_params(VID, PID, &id, valeurs(&[("speed", serde_json::json!(3))]));
        store.write_settings(&settings).unwrap();

        store.reset_settings().unwrap();

        assert_eq!(store.read_settings().unwrap(), Settings::default());
        assert!(
            tmp.path().join("config").join("settings.json").is_file(),
            "le fichier a disparu au lieu d'être remis à plat"
        );

        // Et la bibliothèque est intacte, source comprise : c'est elle qu'on ne
        // peut pas réinstaller.
        assert_eq!(installes(&store).len(), 1);
        assert_eq!(store.effect_source(&id).unwrap(), "la source");
        assert_eq!(store.effect_js(&id).unwrap(), "le js");
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

    /// Le fichier d'une version antérieure ne connaît ni `devices` ni la forme
    /// actuelle. Il doit se relire sans erreur, et la réécriture ne doit pas
    /// perdre ce qu'on vient d'y décider.
    ///
    /// Ce qui **ne survit pas**, et c'est le sujet de l'issue #64 : les trois
    /// champs mono-appareil. `activeEffect` et `device` n'étaient lus ni écrits
    /// par personne, et `brightness` à la racine décrivait un niveau partagé que
    /// deux claviers n'ont aucune raison d'avoir. Les récupérer aurait demandé de
    /// choisir *quel* appareil ils désignaient — question sans réponse.
    #[test]
    fn un_fichier_anterieur_se_relit_et_garde_ses_reglages() {
        let (tmp, store) = store_temporaire();
        let config = tmp.path().join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("settings.json"),
            r#"{"activeEffect":"onde-radiale","brightness":90,"device":{"vid":5426,"pid":658}}"#,
        )
        .unwrap();

        let mut settings = store.read_settings().unwrap();
        assert!(settings.devices.is_empty());
        assert!(settings.active_effects.is_empty());
        settings.set_device_state(VID, PID, None, DeviceState::Adopted);
        store.write_settings(&settings).unwrap();

        let relu = store.read_settings().unwrap();
        assert_eq!(relu.device_state(VID, PID, None), DeviceState::Adopted);
        assert_eq!(relu.brightness(VID, PID, None), BRIGHTNESS_DEFAUT);
    }

    /// Les champs partent en camelCase, comme tous les DTO, et une entrée sans
    /// série ni luminosité n'écrit pas de clé vide.
    #[test]
    fn les_appareils_se_serialisent_en_camel_case() {
        let mut settings = Settings::default();
        settings.set_device_state(VID, PID, None, DeviceState::Adopted);
        let json = serde_json::to_string(&settings).unwrap();

        assert!(json.contains(r#""devices":[{"vid":5426,"pid":658,"state":"adopted"}]"#));
        assert!(json.contains(r#""activeEffects":[]"#));
        assert!(!json.contains("serial"), "clé vide écrite : {json}");
        assert!(!json.contains("brightness"), "défaut écrit : {json}");
    }

    // ------------------------------------------------- réglages d'effet

    /// Une table de valeurs, écrite comme l'interface l'envoie.
    fn valeurs(paires: &[(&str, serde_json::Value)]) -> serde_json::Map<String, serde_json::Value> {
        paires
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect()
    }

    /// Le cœur de l'issue #28 : changer d'effet puis revenir ne perd rien, et
    /// deux appareils ne se marchent pas dessus.
    #[test]
    fn les_reglages_sont_retenus_par_appareil_et_par_effet() {
        let mut settings = Settings::default();
        let lent = valeurs(&[("speed", serde_json::json!(0.5))]);
        let rapide = valeurs(&[("speed", serde_json::json!(9.0))]);

        settings.set_effect_params(VID, PID, "balayage", lent.clone());
        settings.set_effect_params(VID, PID, "respiration", rapide.clone());
        // Même effet, autre appareil : une entrée de plus, pas un écrasement.
        settings.set_effect_params(VID, PID + 1, "balayage", rapide.clone());

        assert_eq!(settings.effect_params(VID, PID, "balayage"), Some(&lent));
        assert_eq!(
            settings.effect_params(VID, PID, "respiration"),
            Some(&rapide)
        );
        assert_eq!(
            settings.effect_params(VID, PID + 1, "balayage"),
            Some(&rapide)
        );
        assert_eq!(settings.effect_params(VID, PID, "onde-radiale"), None);
    }

    /// Bouger le même curseur cent fois n'écrit pas cent entrées : c'est
    /// exactement ce que produit un glissement de souris.
    #[test]
    fn regler_deux_fois_le_meme_effet_remplace_l_entree() {
        let mut settings = Settings::default();
        for i in 0..5 {
            settings.set_effect_params(VID, PID, "balayage", valeurs(&[("speed", i.into())]));
        }

        assert_eq!(settings.effect_params.len(), 1);
        assert_eq!(
            settings.effect_params(VID, PID, "balayage"),
            Some(&valeurs(&[("speed", serde_json::json!(4))]))
        );
    }

    /// Rétablir les valeurs déclarées **oublie**, au lieu d'en écrire une copie :
    /// l'effet repart de son manifeste, y compris si une version ultérieure en
    /// change les défauts.
    #[test]
    fn retablir_les_valeurs_declarees_retire_l_entree() {
        let mut settings = Settings::default();
        settings.set_effect_params(VID, PID, "balayage", valeurs(&[("speed", 3.into())]));
        settings.set_effect_params(VID, PID, "balayage", serde_json::Map::new());

        assert!(settings.effect_params.is_empty());
        assert_eq!(settings.effect_params(VID, PID, "balayage"), None);

        // Et oublier ce qui n'a jamais été réglé ne crée pas d'entrée vide.
        settings.set_effect_params(VID, PID, "onde-radiale", serde_json::Map::new());
        assert!(settings.effect_params.is_empty());
    }

    /// Le fichier d'une version antérieure ne connaît pas `effectParams`. Il se
    /// relit — c'est ce que `#[serde(default)]` sur la structure garantit — et
    /// la réécriture ne perd ni l'adoption ni la luminosité.
    #[test]
    fn un_fichier_sans_reglages_d_effet_se_relit_et_les_accueille() {
        let (tmp, store) = store_temporaire();
        let config = tmp.path().join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(
            config.join("settings.json"),
            r#"{"devices":[{"vid":5426,"pid":658,"state":"adopted","brightness":90}]}"#,
        )
        .unwrap();

        let mut settings = store.read_settings().unwrap();
        assert!(settings.effect_params.is_empty());

        let reglages = valeurs(&[
            ("speed", serde_json::json!(0.5)),
            ("color", serde_json::json!({ "r": 0, "g": 180, "b": 255 })),
        ]);
        settings.set_effect_params(VID, PID, "balayage", reglages.clone());
        store.write_settings(&settings).unwrap();

        let relu = store.read_settings().unwrap();
        assert_eq!(relu.effect_params(VID, PID, "balayage"), Some(&reglages));
        assert_eq!(relu.brightness(VID, PID, None), 90);
        assert_eq!(relu.device_state(VID, PID, None), DeviceState::Adopted);
    }

    /// Les quatre sortes de `ParamSpec` survivent au disque telles quelles : le
    /// Rust ne les interprète pas, il ne doit pas non plus les abîmer. Une
    /// couleur est un objet `{r,g,b}`, pas une chaîne.
    #[test]
    fn les_quatre_sortes_de_valeurs_font_un_aller_retour() {
        let (_tmp, store) = store_temporaire();
        let reglages = valeurs(&[
            ("speed", serde_json::json!(0.5)),
            ("bounce", serde_json::json!(true)),
            ("axis", serde_json::json!("vertical")),
            ("color", serde_json::json!({ "r": 255, "g": 96, "b": 0 })),
        ]);

        let mut settings = Settings::default();
        settings.set_effect_params(VID, PID, "balayage", reglages.clone());
        store.write_settings(&settings).unwrap();

        assert_eq!(
            store
                .read_settings()
                .unwrap()
                .effect_params(VID, PID, "balayage"),
            Some(&reglages)
        );
    }

    /// Supprimer un effet emporte ses réglages, sur tous les appareils, et
    /// n'emporte que les siens. Sans quoi `settings.json` garderait des entrées
    /// pour un identifiant que plus rien ne désigne — et un effet réinstallé
    /// plus tard sous le même nom hériterait des réglages de son homonyme.
    #[test]
    fn oublier_un_effet_retire_ses_reglages_partout() {
        let mut settings = Settings::default();
        settings.set_effect_params(VID, PID, "balayage", valeurs(&[("speed", 3.into())]));
        settings.set_effect_params(VID, PID + 1, "balayage", valeurs(&[("speed", 9.into())]));
        settings.set_effect_params(VID, PID, "respiration", valeurs(&[("period", 12.into())]));

        assert!(settings.forget_effect("balayage"));
        assert_eq!(settings.effect_params.len(), 1);
        assert_eq!(settings.effect_params(VID, PID, "balayage"), None);
        assert_eq!(settings.effect_params(VID, PID + 1, "balayage"), None);
        assert!(settings.effect_params(VID, PID, "respiration").is_some());

        // Rien à retirer : le fichier n'a aucune raison d'être réécrit.
        assert!(!settings.forget_effect("balayage"));
        assert!(!settings.forget_effect("jamais-regle"));
    }

    /// **Le piège de l'issue #48, tranché.** Supprimer l'effet appliqué doit
    /// purger son identifiant, sinon `settings.json` désignerait comme appliqué
    /// un effet que la bibliothèque ne connaît plus — et le jour où l'on
    /// reprendra l'effet au démarrage, on tenterait de lancer un effet absent.
    #[test]
    fn oublier_un_effet_purge_aussi_son_application() {
        let mut settings = Settings::default();
        settings.set_active_effect(VID, PID, Some("a-supprimer"));
        settings.set_active_effect(VID, PID + 1, Some("a-supprimer"));
        settings.set_active_effect(VID, PID + 2, Some("epargne"));

        assert!(settings.forget_effect("a-supprimer"));
        assert_eq!(settings.active_effect(VID, PID), None);
        assert_eq!(settings.active_effect(VID, PID + 1), None);
        assert_eq!(
            settings.active_effect(VID, PID + 2),
            Some("epargne"),
            "la suppression a emporté l'effet d'un autre appareil"
        );
    }

    /// Les réglages d'effet partent en camelCase comme le reste des DTO.
    #[test]
    fn les_reglages_d_effet_se_serialisent_en_camel_case() {
        let mut settings = Settings::default();
        settings.set_effect_params(VID, PID, "balayage", valeurs(&[("speed", 3.into())]));
        let json = serde_json::to_string(&settings).unwrap();

        assert!(
            json.contains(
                r#""effectParams":[{"vid":5426,"pid":658,"effect":"balayage","values":{"speed":3}}]"#
            ),
            "sérialisation : {json}"
        );
    }

    // ------------------------------------------------- valeurs de départ

    /// Un manifeste déclarant trois paramètres, dont un sans `default`.
    fn declare() -> Manifest {
        Manifest {
            name: "Balayage".into(),
            description: String::new(),
            params: serde_json::json!({
                "speed":  { "kind": "number",  "label": "Vitesse", "default": 120 },
                "bounce": { "kind": "boolean", "label": "Rebond",  "default": false },
                "muet":   { "kind": "number",  "label": "Sans défaut" }
            })
            .as_object()
            .unwrap()
            .clone(),
            api_version: EFFECTS_API_VERSION,
        }
    }

    /// **Le cas nominal de l'icône de zone de notification** : lancer un effet
    /// sans fenêtre doit donner le même éclairage que le lancer depuis la
    /// galerie — donc les défauts du manifeste, recouverts par ce qu'on a retenu.
    #[test]
    fn les_valeurs_de_depart_partent_du_manifeste_et_sont_recouvertes() {
        let retenus = valeurs(&[("speed", serde_json::json!(40))]);
        let depart = starting_params(&declare(), Some(&retenus));

        assert_eq!(depart.get("speed"), Some(&serde_json::json!(40)));
        assert_eq!(depart.get("bounce"), Some(&serde_json::json!(false)));
    }

    /// Sans rien de retenu, ce que l'effet déclare, et rien de plus : un
    /// paramètre sans `default` est laissé à l'effet plutôt que deviné.
    #[test]
    fn un_parametre_sans_defaut_n_est_pas_invente() {
        let depart = starting_params(&declare(), None);

        assert_eq!(depart.len(), 2, "valeurs de départ : {depart:?}");
        assert!(!depart.contains_key("muet"));
    }

    /// **Bornée aux paramètres déclarés**, comme côté fenêtre : un réglage
    /// retenu pour un paramètre que l'effet n'a plus disparaît de lui-même, au
    /// lieu de voyager vers une boucle qui ne le lit plus.
    #[test]
    fn un_reglage_orphelin_ne_part_pas_vers_la_boucle() {
        let retenus = valeurs(&[
            ("speed", serde_json::json!(40)),
            ("disparu", serde_json::json!(7)),
        ]);
        let depart = starting_params(&declare(), Some(&retenus));

        assert!(!depart.contains_key("disparu"));
        assert_eq!(depart.get("speed"), Some(&serde_json::json!(40)));
    }
}
