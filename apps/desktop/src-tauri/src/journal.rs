//! Le journal : où partent les enregistrements, et comment les atteindre.
//!
//! Avant ce module, rien n'était lisible en `release` : huit `eprintln!` dans le
//! Rust, trois `console.` dans la fenêtre, et un binaire compilé
//! `windows_subsystem = "windows"` — donc sans console pour les recevoir. Trois
//! pannes déjà identifiées tombaient dans ce vide : l'échec d'ouverture d'un
//! appareil au démarrage, la dégradation silencieuse de l'instance unique sous
//! Linux (#45), et l'avertissement « sans bloquer » sur une version de
//! micrologiciel inattendue (#35).
//!
//! # Quatre couches, dont une qu'on oublie toujours
//!
//! 1. une **façade** — les macros de `tracing`, appelées partout, qui ignorent où
//!    partent les enregistrements ;
//! 2. un **collecteur** qui filtre par niveau et met en forme ;
//! 3. des **destinations** : un fichier tournant, et la sortie standard en
//!    développement ;
//! 4. **de quoi atteindre le fichier depuis l'application** — [`open_log_dir`] et
//!    [`diagnostic`]. Un journal que personne ne sait trouver ne sert à rien.
//!
//! # `tracing` plutôt que `tauri-plugin-log`
//!
//! Le plugin est première partie et réglerait l'essentiel en quelques lignes. Ce
//! qu'il ne donne pas, c'est le contexte : **il y a une boucle de rendu par
//! appareil**, et « écriture refusée » ne sert à rien sans savoir laquelle. Un
//! *span* ouvert par boucle — voir [`crate::runtime`] — attache l'appareil et
//! l'effet à tout ce qui s'y journalise, **sans trimballer un identifiant
//! d'appareil dans chaque appel**. C'est exactement la forme de nos pannes, et
//! c'est ce qui garde un journal lisible quand deux appareils tournent ensemble.
//!
//! Le prix est une crate de plus et un filtre à configurer.
//!
//! # Deux règles qui ne se négocient pas
//!
//! **Journaliser les transitions, jamais les occurrences.** À 30 images par
//! seconde, une écriture qui échoue produirait trente lignes par seconde et
//! enterrerait la seule qui compte. Le moteur a déjà le bon modèle — l'erreur est
//! posée puis effacée au rétablissement, et l'arrêt vient après un nombre fixe
//! d'échecs consécutifs ; [`bascule`] est ce qui le transpose au journal.
//!
//! **Le numéro de série ne fuit pas.** Le protocole en donne un et il identifie
//! un exemplaire précis ; un journal collé dans un rapport de bogue ne doit pas
//! le divulguer. Voir [`empreinte`].

use std::fmt;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt as fmt_layer, reload, EnvFilter, Registry};

use crate::{AppState, CmdResult, DeviceRef};

/// La variable d'environnement qui l'emporte sur tout le reste.
///
/// `CANDEO_LOG` et non `RUST_LOG` : cette dernière est partagée par tout
/// l'outillage Rust, et quelqu'un qui l'a posée pour lire ce que raconte `cargo`
/// changerait au passage, et sans le vouloir, le journal de l'application.
pub const VARIABLE: &str = "CANDEO_LOG";

/// Le niveau quand rien ne le dit : le cycle de vie, et rien de plus.
const DEFAUT: LogLevel = LogLevel::Info;

/// `candeo.2026-09-12.log`, dans le dossier des journaux.
const PREFIXE: &str = "candeo";
const SUFFIXE: &str = "log";

/// Nombre de fichiers gardés, rotation quotidienne comprise : une semaine.
///
/// Une application d'éclairage tourne des jours, et sans plafond le dossier ne
/// ferait que grossir. Une semaine couvre « ça a commencé lundi » — la distance
/// utile d'un rapport de bogue — sans conserver un historique que personne ne
/// relira.
///
/// ⚠️ **Le plafond porte sur le nombre de fichiers, pas sur leur taille.** Un
/// niveau élevé porte du par-image : la journée en cours peut grossir beaucoup
/// avant que la rotation ne tranche. Ce qui borne la taille, c'est le niveau —
/// d'où [`JournalStatus::verbose`], et la mention que l'interface en fait.
const MAX_FICHIERS: usize = 7;

/// Cible des enregistrements venus de la fenêtre.
///
/// Une cible fixe, et l'origine en champ : `tracing` exige une cible constante à
/// la compilation, et de toute façon « tout ce qui vient du WebView » est ce
/// qu'on veut pouvoir filtrer d'un seul mot.
const CIBLE_WEBVIEW: &str = "candeo_webview";

/// Les crates dont le niveau suit celui qu'on règle.
///
/// Les autres — Tauri, `hidapi`, WebView — restent à `warn` : monter le journal à
/// `debug` pour suivre une boucle de rendu ne doit pas noyer le fichier sous la
/// trace d'une bibliothèque tierce, qui est précisément ce qu'on ne cherche pas.
const NOTRES: &[&str] = &[
    "candeo_desktop_lib",
    "candeo_device",
    "candeo_protocol",
    CIBLE_WEBVIEW,
];

// ---------------------------------------------------------------- niveaux

/// Les niveaux, tels qu'ils veulent dire quelque chose **ici**.
///
/// | Niveau | Ce que ça veut dire |
/// |---|---|
/// | `error` | l'éclairage de l'utilisateur est cassé |
/// | `warn` | dégradé mais fonctionnel — micrologiciel inattendu (#35), exclusion d'instance inopérante (#45) |
/// | `info` | cycle de vie : appareil adopté, effet démarré, effet arrêté |
/// | `debug` / `trace` | par image, **éteint par défaut** |
///
/// L'ordre de déclaration est celui de la verbosité croissante, et il est
/// utilisé : [`directives`] en tire le niveau accordé aux crates tierces.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    /// Le nom que comprend `EnvFilter`, et celui qu'on lit dans le fichier.
    fn nom(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }

    /// Le niveau nommé par cette chaîne, s'il y en a un.
    ///
    /// Insensible à la casse et aux espaces : c'est saisi à la main dans un
    /// terminal, souvent en majuscules par habitude des autres outils.
    fn depuis(nom: &str) -> Option<Self> {
        match nom.trim().to_ascii_lowercase().as_str() {
            "error" => Some(Self::Error),
            "warn" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            "trace" => Some(Self::Trace),
            _ => None,
        }
    }

    /// Vrai si ce niveau porte du par-image.
    fn verbeux(self) -> bool {
        self >= Self::Debug
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.nom())
    }
}

/// Les directives données au filtre pour ce niveau.
///
/// Deux parts, et la seconde est la raison d'écrire cette fonction : nos crates
/// au niveau demandé, **tout le reste au moins aussi silencieux**. Un `trace`
/// global rendrait le fichier illisible sans rien apprendre sur candeo.
///
/// Le `min` couvre le seul cas où la règle s'inverse : demander `error`, c'est
/// demander le silence, et laisser les tiers à `warn` serait alors plus bavard
/// que ce qu'on a demandé pour soi.
fn directives(niveau: LogLevel) -> String {
    let tiers = niveau.min(LogLevel::Warn);
    let mut out = String::from(tiers.nom());
    for cible in NOTRES {
        out.push(',');
        out.push_str(cible);
        out.push('=');
        out.push_str(niveau.nom());
    }
    out
}

/// Ce que la règle de priorité a décidé.
#[derive(Debug, PartialEq, Eq)]
struct Resolution {
    /// Ce qu'on donne au filtre.
    directives: String,
    /// Le niveau, quand un seul mot le résume. `None` quand la variable
    /// d'environnement porte une directive plus fine, qu'aucun niveau ne nomme.
    niveau: Option<LogLevel>,
    /// Vrai si la variable d'environnement a tranché.
    impose: bool,
}

/// **La règle de priorité : environnement > réglage retenu > défaut.**
///
/// L'environnement l'emporte toujours, et ce n'est pas une préférence de goût :
/// c'est ce qui permet de diagnostiquer une application qui ne va pas assez loin
/// pour lire ses réglages — un dossier de configuration introuvable, un
/// `settings.json` illisible.
///
/// `CANDEO_LOG` accepte les deux formes : un niveau seul (`debug`), qui est le
/// geste courant, ou une directive complète
/// (`candeo_desktop_lib::runtime=trace,warn`) pour viser un module précis. Dans
/// le second cas aucun niveau ne résume ce qui est demandé, et l'interface le
/// dit plutôt que d'en inventer un.
///
/// Une variable **vide** vaut une variable absente : `CANDEO_LOG=` est ce
/// qu'écrit un shell qui l'a « effacée », et l'entendre comme une directive vide
/// couperait tout le journal sans que personne ne l'ait demandé.
///
/// Fonction pure, et c'est délibéré : la règle se vérifie sans collecteur, sans
/// disque et sans application.
fn resoudre(env: Option<&str>, reglage: Option<LogLevel>) -> Resolution {
    match env.map(str::trim).filter(|v| !v.is_empty()) {
        Some(brut) => match LogLevel::depuis(brut) {
            Some(niveau) => Resolution {
                directives: directives(niveau),
                niveau: Some(niveau),
                impose: true,
            },
            None => Resolution {
                directives: brut.to_string(),
                niveau: None,
                impose: true,
            },
        },
        None => {
            let niveau = reglage.unwrap_or(DEFAUT);
            Resolution {
                directives: directives(niveau),
                niveau: Some(niveau),
                impose: false,
            }
        }
    }
}

// ---------------------------------------------------------------- collecteur

/// Ce que l'initialisation laisse derrière elle, et que les commandes relisent.
struct Collecteur {
    /// La poignée de rechargement du filtre.
    ///
    /// **C'est l'exigence qui structure tout le reste** : le défaut qu'on cherche
    /// peut ne pas survivre au redémarrage. Un clavier qui décroche après deux
    /// heures, un effet qui dérive lentement — dire « relancez en mode détaillé »
    /// revient à demander de reproduire ce qu'on vient d'observer, et souvent on
    /// ne peut pas. Elle est posée **dès l'initialisation** : la rajouter ensuite
    /// supposerait de reprendre celle-ci de bout en bout.
    filtre: reload::Handle<EnvFilter, Registry>,
    /// Vrai si [`VARIABLE`] a tranché pour cette exécution.
    impose: bool,
    /// Le dossier des journaux. `None` quand il n'a pas pu être résolu — le
    /// journal tourne alors sans fichier, ce qui vaut mieux que pas de journal.
    dossier: Option<PathBuf>,
}

/// Une seule fois par processus, et ensuite en lecture seule.
static COLLECTEUR: OnceLock<Collecteur> = OnceLock::new();

/// Démarre le journal. **Ne peut pas échouer.**
///
/// Appelée en tête du `setup` de l'application, c'est-à-dire au premier instant
/// où `app_log_dir()` existe — et surtout **avant que le magasin ne soit
/// résolu**. L'ordre n'est pas cosmétique : un échec de résolution du dossier de
/// configuration est exactement le genre de chose qu'on veut voir, et il
/// arriverait avant qu'il n'y ait de quoi l'écrire.
///
/// Le réglage retenu, lui, n'est pas encore lu : on démarre au défaut, puis
/// [`relire_le_reglage`] ajuste. Jamais l'inverse.
///
/// Rien ne remonte, et deux pannes sont absorbées ici : un dossier de journaux
/// introuvable — le journal tourne alors sans fichier — et un collecteur déjà
/// installé, ce qui n'arrive qu'à un second appel. Ni l'une ni l'autre ne doit
/// empêcher la fenêtre de s'ouvrir : c'est elle qui permettrait de corriger la
/// situation.
pub fn init(app: &AppHandle) {
    let resolution = resoudre(std::env::var(VARIABLE).ok().as_deref(), None);

    // Une directive illisible ne doit pas priver du journal celui qui la
    // corrigera : on retombe sur le défaut, et on le dit — une fois le
    // collecteur en place, puisqu'avant il n'y a personne pour l'entendre.
    let (filtre, directive_refusee) = match EnvFilter::try_new(&resolution.directives) {
        Ok(f) => (f, None),
        Err(e) => (
            EnvFilter::new(directives(DEFAUT)),
            Some(format!("{} : {e}", resolution.directives)),
        ),
    };
    let (couche_filtre, poignee) = reload::Layer::new(filtre);

    let (dossier, fichier, echec_fichier) = match ouvrir_le_fichier(app) {
        Ok((dossier, appender)) => (Some(dossier), Some(appender), None),
        Err(e) => (None, None, Some(e)),
    };

    // `with_ansi(false)` sur le fichier : les séquences de couleur ne veulent
    // rien dire dans un fichier, et elles rendent illisible ce qu'on colle dans
    // un rapport de bogue.
    //
    // Écriture directe, sans fil d'écriture intercalé : on ne journalise que des
    // transitions, le volume est donc négligeable — et un journal dont les
    // dernières lignes sont perdues dans la panne qu'on traque ne vaut rien.
    let couche_fichier = fichier.map(|appender| {
        fmt_layer::layer()
            .with_ansi(false)
            .with_target(true)
            .with_writer(appender)
    });

    // En développement seulement : en `release` le binaire est compilé
    // `windows_subsystem = "windows"`, il n'y a aucune console pour recevoir
    // quoi que ce soit.
    let couche_console = cfg!(debug_assertions).then(fmt_layer::layer);

    // `try_init` plutôt que `init` : installer un collecteur alors qu'il y en a
    // déjà un est une erreur de programmation, pas une raison de refuser de
    // démarrer l'application.
    let installe = tracing_subscriber::registry()
        .with(couche_filtre)
        .with(couche_fichier)
        .with(couche_console)
        .try_init()
        .is_ok();
    if !installe {
        return;
    }

    let _ = COLLECTEUR.set(Collecteur {
        filtre: poignee,
        impose: resolution.impose,
        dossier: dossier.clone(),
    });

    tracing::info!(
        version = app.package_info().version.to_string(),
        systeme = std::env::consts::OS,
        architecture = std::env::consts::ARCH,
        journal = dossier.as_ref().map(|d| d.display().to_string()),
        "candeo démarre"
    );
    if let Some(e) = echec_fichier {
        tracing::error!("aucun journal sur disque, la sortie standard seule reste : {e}");
    }
    if let Some(e) = directive_refusee {
        tracing::warn!("{VARIABLE} illisible, niveau par défaut appliqué — {e}");
    }
    if let Some(niveau) = resolution.niveau {
        prevenir_si_verbeux(niveau);
    }
}

/// Le fichier tournant, et le dossier qui le porte.
///
/// **Par `app_log_dir()`, jamais un chemin en dur** : sous Windows données et
/// configuration se confondent, sous Linux non — et les journaux ne sont ni
/// l'une ni l'autre.
fn ouvrir_le_fichier(
    app: &AppHandle,
) -> Result<(PathBuf, tracing_appender::rolling::RollingFileAppender), String> {
    let dossier = app
        .path()
        .app_log_dir()
        .map_err(|e| format!("dossier des journaux introuvable : {e}"))?;
    let appender = tourner_dans(&dossier)?;
    Ok((dossier, appender))
}

/// Le fichier tournant d'un dossier donné.
///
/// Séparé de [`ouvrir_le_fichier`] pour la seule raison qui vaille : c'est la
/// part qu'un test peut exercer. Une configuration de rotation refusée ne se
/// verrait sinon qu'au lancement de l'application, sous la forme d'un journal qui
/// n'écrit nulle part.
fn tourner_dans(
    dossier: &std::path::Path,
) -> Result<tracing_appender::rolling::RollingFileAppender, String> {
    // Créé ici plutôt qu'à la première écriture : un dossier qu'on ne peut pas
    // créer se dit maintenant, pas à la première panne qu'on voulait consigner.
    std::fs::create_dir_all(dossier)
        .map_err(|e| format!("création de {} impossible : {e}", dossier.display()))?;

    tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix(PREFIXE)
        .filename_suffix(SUFFIXE)
        .max_log_files(MAX_FICHIERS)
        .build(dossier)
        .map_err(|e| format!("journal non ouvert dans {} : {e}", dossier.display()))
}

/// Relit le niveau retenu dans `settings.json` et l'applique.
///
/// Appelée juste après [`init`], et c'est tout l'ordre d'amorçage : on démarre au
/// défaut, **puis** on ajuste. Le magasin est donc résolu ici pour la seconde
/// fois du démarrage — l'adoption le résout aussi — et c'est le prix assumé de
/// ne pas faire dépendre le journal de ce qui l'utilise.
///
/// Ne fait rien quand [`VARIABLE`] a tranché : la règle de priorité vaut aussi au
/// démarrage, sans quoi le réglage retenu écraserait ce qu'on vient de demander
/// en ligne de commande.
pub fn relire_le_reglage(app: &AppHandle) {
    if COLLECTEUR.get().is_some_and(|c| c.impose) {
        return;
    }
    match crate::storage::store(app).and_then(|s| s.read_settings()) {
        // Le journal est déjà en place : c'est précisément pour ce message-là
        // qu'il devait démarrer avant le magasin.
        Err(e) => tracing::error!("niveau de journal non relu, le défaut s'applique : {e}"),
        Ok(settings) => {
            if let Some(niveau) = settings.preferences.log_level {
                appliquer(niveau);
            }
        }
    }
}

/// Ramène le niveau au défaut, sans redémarrer.
///
/// Appelée par la remise à zéro de la configuration : le réglage vient d'être
/// effacé du fichier, et un écran qui afficherait encore « détaillé » mentirait.
/// Sans effet quand [`VARIABLE`] a tranché — la priorité ne se suspend pas pour
/// une remise à zéro.
pub(crate) fn revenir_au_defaut() {
    if COLLECTEUR.get().is_some_and(|c| c.impose) {
        return;
    }
    appliquer(DEFAUT);
}

/// Remplace le filtre du collecteur **déjà en place**.
///
/// C'est le changement à chaud : aucun redémarrage, aucun fichier rouvert, et les
/// spans ouverts — donc les boucles de rendu en cours — gardent leur contexte.
fn appliquer(niveau: LogLevel) {
    let Some(collecteur) = COLLECTEUR.get() else {
        return;
    };
    match EnvFilter::try_new(directives(niveau)) {
        Ok(filtre) => match collecteur.filtre.reload(filtre) {
            Ok(()) => {
                tracing::info!(niveau = %niveau, "niveau de journal changé");
                prevenir_si_verbeux(niveau);
            }
            Err(e) => tracing::error!("niveau de journal inchangé : {e}"),
        },
        // Les directives sont construites à partir d'un niveau connu : ce
        // chemin n'est atteignable qu'en se trompant dans [`directives`].
        Err(e) => tracing::error!("directives de journal refusées : {e}"),
    }
}

/// Écrit dans le fichier lui-même qu'un niveau élevé est actif.
///
/// L'interface le dit déjà — c'est [`JournalStatus::verbose`] — mais le journal
/// se lit ailleurs et plus tard, souvent par quelqu'un d'autre : un fichier de
/// plusieurs gigaoctets doit porter sa propre explication, plutôt que de laisser
/// chercher ce qui s'est emballé.
fn prevenir_si_verbeux(niveau: LogLevel) {
    if niveau.verbeux() {
        tracing::warn!(
            "niveau « {niveau} » : le journal porte du par-image et grossit vite. \
             La rotation plafonne le nombre de fichiers, pas la taille de celui du jour — \
             revenir à « info » une fois le relevé fini."
        );
    }
}

// ---------------------------------------------------------------- transitions

/// Ce qu'un changement d'état d'erreur donne à journaliser.
///
/// **Le cas qui compte est [`Rien`](Bascule::Rien)**, et il couvre deux
/// situations que rien ne rapproche à part leur conclusion :
///
/// - tout va bien et allait déjà bien — c'est l'immense majorité des images ;
/// - **la panne dure, et la raison a changé.** Ce n'est pas un nouvel incident :
///   un message qui porte un compteur, une position ou un horodatage varierait à
///   chaque image, et journaliser ce changement rouvrirait exactement la
///   inondation qu'on cherche à éviter — trente lignes par seconde, dont aucune
///   n'apprend rien de plus que la première.
///
/// La première raison, elle, est consignée : c'est celle qui nomme la panne.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Bascule {
    /// La panne commence. Une ligne, avec sa raison.
    Commence,
    /// Elle est finie. Une ligne, sans quoi l'interface et le journal
    /// afficheraient une erreur périmée indéfiniment.
    Retabli,
    /// Rien à dire.
    Rien,
}

/// La bascule entre deux états d'erreur successifs.
pub(crate) fn bascule(avant: Option<&str>, apres: Option<&str>) -> Bascule {
    match (avant, apres) {
        (None, Some(_)) => Bascule::Commence,
        (Some(_), None) => Bascule::Retabli,
        _ => Bascule::Rien,
    }
}

// ---------------------------------------------------------------- série

const FNV_DEPART: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PREMIER: u64 = 0x0000_0100_0000_01b3;

/// Une empreinte stable du numéro de série, **qui ne le divulgue pas**.
///
/// Le protocole donne un numéro de série (`0x00`/`0x82`) et il identifie un
/// exemplaire précis — le descripteur USB, lui, n'en porte aucun. Un journal
/// qu'on colle dans un rapport de bogue ne doit pas le révéler ; mais l'effacer
/// purement et simplement rendrait indiscernables deux claviers du même modèle,
/// ce qui est justement la situation où le journal sert le plus.
///
/// FNV-1a écrit ici plutôt qu'un hacheur de la bibliothèque standard :
/// `DefaultHasher` ne promet pas de rendre la même valeur d'une version de Rust à
/// l'autre, et une empreinte qui change à la recompilation ne permettrait plus de
/// rapprocher deux journaux du même appareil.
///
/// Ce n'est pas une protection cryptographique et ça n'a pas à l'être : on
/// empêche une divulgation accidentelle, pas une attaque.
pub(crate) fn empreinte(serial: &str) -> String {
    let mut h = FNV_DEPART;
    for b in serial.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(FNV_PREMIER);
    }
    format!("{h:016x}")
}

/// L'empreinte d'une série qui n'existe peut-être pas.
///
/// « Aucune » n'est pas un cas dégénéré : c'est ce que rend hidraw sous Linux
/// quand la règle udev n'accorde pas la lecture des attributs, et le distinguer
/// d'une série présente est ce qui évite de chercher une panne d'appareil là où
/// il n'y a qu'une permission manquante.
pub(crate) fn empreinte_de(serial: Option<&str>) -> String {
    serial.map_or_else(|| "aucune".to_string(), empreinte)
}

// ---------------------------------------------------------------- commandes

/// L'état du journal, tel que l'interface le montre.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalStatus {
    /// Le niveau appliqué. `None` quand [`VARIABLE`] porte une directive qu'aucun
    /// niveau ne résume — on ne prétend pas la nommer.
    pub level: Option<LogLevel>,
    /// Le niveau retenu dans `settings.json`.
    ///
    /// Distinct de `level` : c'est celui que l'interface propose de changer, et
    /// il reste modifiable même quand la variable d'environnement l'emporte pour
    /// cette exécution-ci.
    pub setting: Option<LogLevel>,
    /// Vrai si [`VARIABLE`] impose le niveau. L'interface le dit plutôt que de
    /// laisser croire qu'un réglage sans effet a été pris en compte.
    pub forced_by_env: bool,
    /// Le dossier des journaux, `None` s'il n'a pas pu être résolu.
    pub dir: Option<String>,
    /// Vrai si le niveau actif porte du **par-image**.
    ///
    /// ⚠️ C'est ce qui rend visible qu'un niveau élevé est actif. Laissé en place
    /// et oublié, `trace` remplit le disque en silence — et la rotation plafonne
    /// le nombre de fichiers, pas la taille de celui du jour.
    ///
    /// Relevé sur le filtre réellement installé, et non déduit de `level` : c'est
    /// la seule façon de répondre juste quand la directive vient de
    /// l'environnement et ne se résume à aucun niveau.
    pub verbose: bool,
}

fn etat(setting: Option<LogLevel>) -> JournalStatus {
    let collecteur = COLLECTEUR.get();
    JournalStatus {
        level: niveau_actif(),
        setting,
        forced_by_env: collecteur.is_some_and(|c| c.impose),
        dir: collecteur
            .and_then(|c| c.dossier.as_ref())
            .map(|d| d.display().to_string()),
        verbose: tracing::level_filters::LevelFilter::current() >= tracing::Level::DEBUG,
    }
}

/// Le niveau le plus verbeux que le filtre laisse passer, s'il en nomme un.
fn niveau_actif() -> Option<LogLevel> {
    tracing::level_filters::LevelFilter::current()
        .into_level()
        .and_then(|l| LogLevel::depuis(l.as_str()))
}

/// L'état du journal : niveau appliqué, niveau retenu, dossier.
#[tauri::command]
pub fn get_journal(app: AppHandle) -> CmdResult<JournalStatus> {
    Ok(etat(
        crate::storage::store(&app)?
            .read_settings()?
            .preferences
            .log_level,
    ))
}

/// Change le niveau **sans redémarrer**, et le retient.
///
/// # Il survit au redémarrage, et c'est un choix
///
/// Le retour automatique au défaut protégerait du disque plein ; la persistance
/// sert celui qui traque un défaut **au démarrage**. Un défaut qui ne se produit
/// qu'au lancement existe — l'adoption des appareils en est un — et lui demander
/// de remonter le niveau après coup revient à lui demander l'impossible. Donc on
/// persiste, et on le dit : [`JournalStatus::verbose`] est là pour ça.
///
/// # Quand la variable d'environnement l'emporte
///
/// Le réglage est écrit, mais **le filtre n'est pas touché** : la priorité vaut
/// pendant toute l'exécution, pas seulement au démarrage. Le réglage vaudra au
/// prochain lancement sans la variable, et `forcedByEnv` dit à l'interface de
/// l'annoncer.
#[tauri::command]
pub fn set_log_level(app: AppHandle, level: LogLevel) -> CmdResult<JournalStatus> {
    let store = crate::storage::store(&app)?;
    let mut settings = store.read_settings()?;
    if settings.preferences.log_level != Some(level) {
        settings.preferences.log_level = Some(level);
        store.write_settings(&settings)?;
    }

    if !COLLECTEUR.get().is_some_and(|c| c.impose) {
        appliquer(level);
    }
    Ok(etat(settings.preferences.log_level))
}

/// Ouvre le dossier des journaux dans le gestionnaire de fichiers du système.
///
/// **Un journal que personne ne sait trouver ne sert à rien**, et le chemin
/// dépend du système : le donner à lire ne suffit pas, il faut y emmener.
#[tauri::command]
pub fn open_log_dir(app: AppHandle) -> CmdResult<()> {
    let dossier = COLLECTEUR
        .get()
        .and_then(|c| c.dossier.clone())
        .ok_or_else(|| {
            "aucun dossier de journaux : le journal n'écrit pas sur disque".to_string()
        })?;

    app.opener()
        .open_path(dossier.display().to_string(), None::<&str>)
        .map_err(|e| format!("ouverture de {} impossible : {e}", dossier.display()))
}

/// Le diagnostic, prêt à être collé dans un rapport de bogue.
///
/// **Ça vaut mieux que n'importe quel fouillage de journal** : tout ce qu'on
/// redemande systématiquement — version, système, appareils, état du moteur —
/// tient en vingt lignes, sans qu'il faille expliquer où chercher.
///
/// Le numéro de série n'y figure pas : son [`empreinte`] suffit à distinguer deux
/// exemplaires, et c'est tout ce qu'on demande à un rapport de bogue.
#[tauri::command]
pub fn diagnostic(app: AppHandle, state: State<'_, AppState>) -> CmdResult<String> {
    let mut out = String::new();
    let ligne = |out: &mut String, cle: &str, valeur: &str| {
        out.push_str(cle);
        out.push_str(" : ");
        out.push_str(valeur);
        out.push('\n');
    };

    ligne(&mut out, "candeo", &app.package_info().version.to_string());
    ligne(
        &mut out,
        "système",
        &format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
    );
    // Les réglages et HID sont relevés séparément, et aucun des deux n'est
    // déballé : ne pas pouvoir énumérer l'USB ou relire `settings.json` est
    // exactement ce qu'un diagnostic doit **dire**, pas ce qui doit
    // l'interrompre.
    let settings = crate::storage::store(&app).and_then(|s| s.read_settings());
    let api = crate::hid();

    let journal = etat(settings.as_ref().ok().and_then(|s| s.preferences.log_level));
    ligne(
        &mut out,
        "journal",
        &format!(
            "niveau {} ({}){}",
            // Le niveau appliqué, pas celui retenu : c'est lui qui explique ce
            // que le fichier contient — ou ne contient pas.
            journal
                .level
                .map_or_else(|| "directive".to_string(), |l| l.to_string()),
            journal.dir.as_deref().unwrap_or("aucun fichier"),
            if journal.forced_by_env {
                format!(", imposé par {VARIABLE}")
            } else {
                String::new()
            }
        ),
    );

    // Ce qui décide si fermer la fenêtre arrête les effets. Sans cette ligne, un
    // rapport disant « mon effet s'arrête quand je ferme » et un autre disant le
    // contraire seraient indiscernables — la pose de l'icône peut échouer, et
    // c'est alors la croix qui redevient une sortie. Voir [`crate::tray`].
    ligne(
        &mut out,
        "zone de notification",
        if crate::tray::installee() {
            "posée — fermer la fenêtre replie, « Quitter candeo » quitte"
        } else {
            "absente — fermer la fenêtre arrête les effets"
        },
    );

    out.push_str("\nAppareils\n");
    for layout in crate::LAYOUTS {
        let device = DeviceRef::of(layout);
        let branche = api
            .as_ref()
            .ok()
            .and_then(|api| crate::plugged(api, layout));
        // Relue sur la poignée : le diagnostic ne refait aucun échange avec
        // l'appareil, il dit ce que l'ouverture a obtenu.
        let inspection = state.inspection(device);
        let serial = crate::serie_connue(inspection.as_ref(), branche.clone().flatten());
        let etat_retenu = settings
            .as_ref()
            .ok()
            .map(|s| decision(s.device_state(layout.vid, layout.pid, serial.as_deref())));

        ligne(
            &mut out,
            &format!("  {} {}", layout.name, device),
            &format!(
                "{} · {} · série {} · gabarit {}×{} ({} cases, {} touches)",
                match branche {
                    Some(_) => "branché",
                    None => "débranché",
                },
                etat_retenu.unwrap_or("état inconnu"),
                empreinte_de(serial.as_deref()),
                layout.rows,
                layout.cols,
                layout.led_count(),
                layout.lit_count(),
            ),
        );

        // Ce que le **fichier** retient pour cet appareil, en face de ce que le
        // moteur en fait plus bas. Les deux doivent concorder ; quand ils
        // divergent — un effet appliqué qui ne tourne pas, une luminosité retenue
        // qu'aucune adoption n'a réappliquée — c'est précisément la ligne qui le
        // montre, et elle ne coûte rien à celui qui lit.
        if let Ok(s) = &settings {
            ligne(
                &mut out,
                "    retenu",
                &format!(
                    "effet {} · luminosité {}",
                    s.active_effect(layout.vid, layout.pid).unwrap_or("aucun"),
                    match s.brightness(layout.vid, layout.pid, serial.as_deref()) {
                        crate::storage::BRIGHTNESS_DEFAUT => "pleine (défaut)".to_string(),
                        n => n.to_string(),
                    }
                ),
            );
        }

        // **Le premier champ qu'on demandera** devant un comportement
        // inexpliqué : la version lue, en face de celle du relevé. Fermé, on dit
        // qu'elle n'a pas été lue plutôt que de répéter celle d'une ouverture
        // passée — l'exemplaire branché depuis n'est peut-être plus le même.
        ligne(
            &mut out,
            "    micrologiciel",
            &format!(
                "{} · gabarit relevé sur {}",
                match &inspection {
                    None => "non lu, appareil fermé".to_string(),
                    Some(i) => i
                        .firmware
                        .as_ref()
                        .map_or_else(|e| format!("non lu ({e})"), ToString::to_string),
                },
                layout.surveyed_firmware
            ),
        );
        if let Some(i) = &inspection {
            ligne(
                &mut out,
                "    commandes",
                &i.checks
                    .iter()
                    .map(|c| format!("{} {}", c.name, verdict(&c.verdict)))
                    .collect::<Vec<_>>()
                    .join(" · "),
            );
            if let Err(e) = &i.serial {
                ligne(
                    &mut out,
                    "    série par le protocole",
                    &format!("non lue ({e})"),
                );
            }
            for avertissement in i.warnings(layout) {
                ligne(&mut out, "    avertissement", &avertissement);
            }
        }
    }
    if let Err(e) = &api {
        ligne(&mut out, "  énumération USB", e);
    }
    if let Err(e) = &settings {
        ligne(&mut out, "  réglages", e);
    }

    out.push_str("\nMoteur\n");
    let rapport = state.engine.report();
    let moteur = rapport.devices;
    if moteur.is_empty() {
        out.push_str("  aucun appareil visé depuis le démarrage\n");
    }
    for s in moteur {
        ligne(
            &mut out,
            &format!("  {}", s.device),
            &format!(
                "{} · effet {} · sortie {} · atteint {}{}{}",
                if s.status.running {
                    "en cours"
                } else {
                    "arrêté"
                },
                s.status.effect_id.as_deref().unwrap_or("aucun"),
                if s.status.to_keyboard {
                    "ouverte"
                } else {
                    "coupée"
                },
                s.status.reaching_keyboard,
                s.status
                    .error
                    .map_or(String::new(), |e| format!(" · erreur d'effet : {e}")),
                s.status
                    .device_error
                    .map_or(String::new(), |e| format!(" · erreur d'écriture : {e}")),
            ),
        );
    }

    // **Sur sa propre ligne, et dite pour ce qu'elle est.** Un aperçu n'écrit sur
    // aucun clavier : le confondre avec ce qui précède ferait chercher côté
    // matériel une panne qui n'y est pas — et son absence de la liste ci-dessus
    // se lirait comme un oubli si rien ne la nommait ici.
    ligne(
        &mut out,
        "  aperçu",
        &match rapport.preview {
            None => "aucun — rien n'est prévisualisé".to_string(),
            Some(p) => format!(
                "{} · effet {} · gabarit emprunté {} · aucune sortie clavier{}",
                if p.running { "en cours" } else { "arrêté" },
                p.effect_id.as_deref().unwrap_or("aucun"),
                p.layout_of,
                p.error
                    .map_or(String::new(), |e| format!(" · erreur d'effet : {e}")),
            ),
        },
    );

    Ok(out)
}

/// La décision d'adoption, dans la langue du rapport de bogue.
///
/// Une table plutôt qu'un `{:?}` mis en minuscules : le diagnostic est lu par un
/// humain, et les noms de variantes Rust n'ont aucune raison d'y apparaître. Le
/// compilateur réclamera cette ligne le jour où un quatrième état existera.
fn decision(state: crate::storage::DeviceState) -> &'static str {
    match state {
        crate::storage::DeviceState::Detected => "détecté",
        crate::storage::DeviceState::Adopted => "piloté",
        crate::storage::DeviceState::Ignored => "ignoré",
    }
}

/// Le verdict d'une commande, dans la langue du rapport de bogue.
///
/// « connue » et non « comprise » : l'octet d'état confirme que le couple
/// classe / commande existe, jamais que ses arguments sont bons. Un diagnostic
/// qui dirait « compatible » enverrait chercher ailleurs une panne d'argument.
fn verdict(v: &candeo_device::Verdict) -> String {
    use candeo_device::Verdict;
    match v {
        Verdict::Understood => "connue (0x02), relue à l'identique".to_string(),
        Verdict::Unsupported => "inconnue (0x05), plus envoyée".to_string(),
        Verdict::ReadBackDiffers { wrote, read } => {
            format!("acceptée, mais relue {read} après réécriture de {wrote}")
        }
        Verdict::Unverified(raison) => format!("non vérifiée ({raison})"),
    }
}

/// Le niveau d'un enregistrement venu de la fenêtre.
///
/// Un type dédié plutôt que [`LogLevel`] : la fenêtre n'a rien à dire au-delà de
/// `debug` — le par-image du moteur ne passe pas par elle — et un niveau qu'on ne
/// sait pas produire n'a pas à être acceptable en argument.
#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "camelCase")]
pub enum WebviewLevel {
    Error,
    Warn,
    Info,
    Debug,
}

/// Consigne un enregistrement venu de la fenêtre.
///
/// Sans elle, `app.config.errorHandler` et les erreurs de compilation d'effet
/// partaient dans une console que personne n'ouvre en `release`. Elles arrivent
/// désormais dans le **même fichier** que le reste : une panne se lit d'un bout à
/// l'autre, et l'ordre entre ce qu'a vu la fenêtre et ce qu'a vu le moteur est
/// celui du fichier.
///
/// `source` nomme d'où ça vient — un composant, un module — en champ plutôt qu'en
/// cible : `tracing` exige une cible constante à la compilation, et de toute
/// façon « ce qui vient du WebView » est ce qu'on veut pouvoir filtrer d'un mot.
///
/// Ne rend rien et ne peut pas échouer : journaliser ne doit jamais devenir une
/// seconde panne à traiter dans le gestionnaire d'erreurs.
#[tauri::command]
pub fn log_from_webview(level: WebviewLevel, source: String, message: String) {
    match level {
        WebviewLevel::Error => tracing::error!(target: CIBLE_WEBVIEW, source, "{message}"),
        WebviewLevel::Warn => tracing::warn!(target: CIBLE_WEBVIEW, source, "{message}"),
        WebviewLevel::Info => tracing::info!(target: CIBLE_WEBVIEW, source, "{message}"),
        WebviewLevel::Debug => tracing::debug!(target: CIBLE_WEBVIEW, source, "{message}"),
    }
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------- priorité

    /// **La règle, dans l'ordre.** Rien : le défaut. Un réglage : le réglage.
    /// L'environnement : l'environnement, quoi qu'il y ait d'autre.
    #[test]
    fn l_environnement_l_emporte_puis_le_reglage_puis_le_defaut() {
        assert_eq!(resoudre(None, None).niveau, Some(DEFAUT));
        assert!(!resoudre(None, None).impose);

        assert_eq!(
            resoudre(None, Some(LogLevel::Debug)).niveau,
            Some(LogLevel::Debug)
        );
        assert!(!resoudre(None, Some(LogLevel::Debug)).impose);

        let impose = resoudre(Some("trace"), Some(LogLevel::Debug));
        assert_eq!(impose.niveau, Some(LogLevel::Trace));
        assert!(impose.impose, "le réglage a eu le dernier mot");
    }

    /// Saisi à la main dans un terminal : majuscules et espaces se pardonnent.
    #[test]
    fn le_niveau_de_l_environnement_se_lit_quelle_que_soit_sa_casse() {
        assert_eq!(resoudre(Some("  WARN "), None).niveau, Some(LogLevel::Warn));
    }

    /// `CANDEO_LOG=` est ce qu'écrit un shell qui l'a « effacée ». L'entendre
    /// comme une directive vide couperait tout le journal sans qu'on l'ait
    /// demandé.
    #[test]
    fn une_variable_vide_vaut_une_variable_absente() {
        let r = resoudre(Some("   "), Some(LogLevel::Warn));
        assert_eq!(r.niveau, Some(LogLevel::Warn));
        assert!(!r.impose);
    }

    /// Une directive fine passe telle quelle, et **aucun niveau ne la résume** :
    /// l'interface doit pouvoir le dire plutôt qu'en inventer un.
    #[test]
    fn une_directive_complete_passe_sans_etre_nommee() {
        let r = resoudre(Some("candeo_desktop_lib::runtime=trace,warn"), None);
        assert_eq!(r.directives, "candeo_desktop_lib::runtime=trace,warn");
        assert_eq!(r.niveau, None);
        assert!(r.impose);
    }

    // -------------------------------------------------------- directives

    /// Monter notre niveau ne doit pas monter celui des bibliothèques tierces :
    /// c'est ce qui garde le fichier lisible quand on cherche une boucle de
    /// rendu.
    #[test]
    fn seules_nos_crates_suivent_le_niveau_demande() {
        let d = directives(LogLevel::Trace);
        assert!(d.starts_with("warn,"), "les tiers ne sont pas bridés : {d}");
        for cible in NOTRES {
            assert!(
                d.contains(&format!("{cible}=trace")),
                "{cible} absente de {d}"
            );
        }
        // Et ce qu'on écrit doit être acceptable par le filtre, sans quoi la
        // panne n'apparaîtrait qu'au lancement de l'application.
        EnvFilter::try_new(&d).expect("directives refusées par le filtre");
    }

    /// Demander `error`, c'est demander le silence : laisser les tiers à `warn`
    /// rendrait le journal plus bavard que ce qu'on a demandé pour soi.
    #[test]
    fn demander_le_silence_fait_taire_les_tiers_aussi() {
        let d = directives(LogLevel::Error);
        assert!(d.starts_with("error,"), "{d}");
    }

    #[test]
    fn chaque_niveau_produit_des_directives_valides() {
        for niveau in [
            LogLevel::Error,
            LogLevel::Warn,
            LogLevel::Info,
            LogLevel::Debug,
            LogLevel::Trace,
        ] {
            EnvFilter::try_new(directives(niveau)).unwrap_or_else(|e| panic!("« {niveau} » : {e}"));
            assert_eq!(LogLevel::depuis(niveau.nom()), Some(niveau));
        }
    }

    /// Le défaut n'est pas verbeux, et `debug` l'est : c'est cette frontière que
    /// l'interface annonce.
    #[test]
    fn le_par_image_commence_a_debug() {
        assert!(!DEFAUT.verbeux());
        assert!(!LogLevel::Warn.verbeux());
        assert!(LogLevel::Debug.verbeux());
        assert!(LogLevel::Trace.verbeux());
    }

    // -------------------------------------------------------- fichier

    /// Le fichier tournant s'ouvre, s'écrit, et porte le nom qu'on attend.
    ///
    /// La configuration de rotation est refusée à la construction quand elle est
    /// incohérente — un préfixe vide, un plafond nul — et ce refus ne se verrait
    /// autrement qu'au lancement de l'application, sous la forme d'un journal
    /// silencieux. Le dossier n'existe pas encore au départ : c'est le cas du
    /// premier lancement, et il ne doit pas être une panne.
    #[test]
    fn le_fichier_tournant_s_ouvre_et_porte_le_nom_attendu() {
        use std::io::Write;

        let tmp = tempfile::tempdir().expect("dossier temporaire");
        let dossier = tmp.path().join("logs");
        assert!(!dossier.exists(), "le test ne vérifierait plus la création");

        let mut appender = tourner_dans(&dossier).expect("journal non ouvert");
        appender.write_all(b"une ligne\n").expect("écriture");
        appender.flush().expect("vidange");

        let fichiers: Vec<String> = std::fs::read_dir(&dossier)
            .expect("dossier des journaux")
            .map(|e| {
                e.expect("entrée")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();

        assert_eq!(fichiers.len(), 1, "fichiers : {fichiers:?}");
        // `candeo.AAAA-MM-JJ.log` : le préfixe rend le fichier reconnaissable
        // dans un dossier qu'on ouvre depuis l'application, la date le rend
        // triable, et le suffixe le rend ouvrable d'un double-clic.
        let nom = &fichiers[0];
        assert!(nom.starts_with(&format!("{PREFIXE}.")), "nom : {nom}");
        assert!(nom.ends_with(&format!(".{SUFFIXE}")), "nom : {nom}");
        assert!(std::fs::read_to_string(dossier.join(nom))
            .expect("lecture")
            .contains("une ligne"));
    }

    // -------------------------------------------------------- recharge

    /// Un cahier qui garde ce que le collecteur écrit, pour pouvoir l'y relire.
    #[derive(Clone, Default)]
    struct Cahier(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl Cahier {
        fn texte(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
        }
    }

    impl std::io::Write for Cahier {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl fmt_layer::MakeWriter<'_> for Cahier {
        type Writer = Self;
        fn make_writer(&self) -> Self {
            self.clone()
        }
    }

    /// **Le niveau se change à chaud, sans redémarrer.**
    ///
    /// C'est l'exigence qui structure tout le module, et elle ne se vérifie pas
    /// en lisant le code : la recharge dépend de la place du filtre dans la pile
    /// de couches et de la reconstruction du cache d'intérêt de `tracing`. Une
    /// pile mal montée compilerait, et le réglage n'aurait simplement aucun
    /// effet — la panne la plus difficile à remarquer, puisqu'elle ne se voit
    /// que le jour où l'on en a besoin.
    ///
    /// Le collecteur est monté ici comme [`init`] le monte, mais posé sur le fil
    /// du test — `set_default` plutôt que `try_init` : un collecteur global ne
    /// s'installe qu'une fois par processus, et les autres tests n'ont pas à
    /// dépendre de celui-là.
    #[test]
    fn le_niveau_se_change_a_chaud_sans_redemarrer() {
        let cahier = Cahier::default();
        let (couche, poignee) = reload::Layer::new(EnvFilter::new(directives(LogLevel::Info)));
        let abonne = tracing_subscriber::registry().with(couche).with(
            fmt_layer::layer()
                .with_ansi(false)
                .with_writer(cahier.clone()),
        );
        let _garde = tracing::subscriber::set_default(abonne);

        tracing::debug!("clé-avant");
        tracing::info!("clé-pendant");
        assert!(
            !cahier.texte().contains("clé-avant"),
            "le par-image passe alors que le niveau est « info » : {}",
            cahier.texte()
        );
        assert!(cahier.texte().contains("clé-pendant"));

        poignee
            .reload(EnvFilter::new(directives(LogLevel::Debug)))
            .expect("recharge refusée");

        tracing::debug!("clé-après");
        assert!(
            cahier.texte().contains("clé-après"),
            "le niveau n'a pas changé sans redémarrage : {}",
            cahier.texte()
        );

        // Et dans l'autre sens : on doit pouvoir refermer le robinet sans
        // relancer non plus, sinon un relevé oublié remplirait le disque jusqu'à
        // la prochaine fermeture de l'application.
        poignee
            .reload(EnvFilter::new(directives(LogLevel::Info)))
            .expect("recharge refusée");
        tracing::debug!("clé-refermé");
        assert!(
            !cahier.texte().contains("clé-refermé"),
            "le niveau ne redescend pas : {}",
            cahier.texte()
        );
    }

    // -------------------------------------------------------- transitions

    /// **Les transitions, jamais les occurrences.** À 30 images par seconde, une
    /// écriture qui échoue produirait trente lignes par seconde.
    #[test]
    fn seul_un_changement_d_etat_se_journalise() {
        assert_eq!(bascule(None, Some("refusée")), Bascule::Commence);
        assert_eq!(bascule(Some("refusée"), None), Bascule::Retabli);
        assert_eq!(bascule(None, None), Bascule::Rien);
        assert_eq!(bascule(Some("refusée"), Some("refusée")), Bascule::Rien);
    }

    /// La panne dure et la raison change : ce n'est pas un nouvel incident. Un
    /// message qui porte un compteur varierait à chaque image, et le journaliser
    /// rouvrirait l'inondation qu'on vient de fermer.
    #[test]
    fn une_raison_qui_change_pendant_la_panne_ne_dit_rien_de_neuf() {
        assert_eq!(
            bascule(Some("image 1 refusée"), Some("image 2 refusée")),
            Bascule::Rien
        );
    }

    // -------------------------------------------------------- série

    /// Ce qu'un journal collé dans un rapport de bogue ne doit **jamais**
    /// contenir. La série choisie ne s'écrit pas en hexadécimal, faute de quoi le
    /// test pourrait passer par accident.
    #[test]
    fn l_empreinte_ne_divulgue_pas_le_numero_de_serie() {
        let serie = "XYZW-KLM-9921";
        let e = empreinte(serie);
        assert!(!e.contains(serie), "la série est dans l'empreinte : {e}");
        for morceau in ["XYZW", "KLM", "9921"] {
            assert!(!e.contains(morceau), "« {morceau} » a fuité dans {e}");
        }
    }

    /// Stable d'un appel à l'autre — sinon deux journaux du même appareil ne se
    /// rapprocheraient pas — et distincte d'un exemplaire à l'autre, sinon elle
    /// ne servirait à rien.
    #[test]
    fn l_empreinte_est_stable_et_distingue_deux_exemplaires() {
        assert_eq!(empreinte("XY01"), empreinte("XY01"));
        assert_ne!(empreinte("XY01"), empreinte("XY02"));
        assert_eq!(empreinte("XY01").len(), 16);
    }

    /// « Branché sans série déclarée » n'est pas un cas dégénéré : c'est hidraw
    /// sans règle udev, et le confondre avec une série ferait chercher une panne
    /// d'appareil là où il n'y a qu'une permission manquante.
    #[test]
    fn une_enumeration_muette_se_dit_autrement_qu_une_empreinte() {
        assert_eq!(empreinte_de(None), "aucune");
        assert_eq!(empreinte_de(Some("XY01")), empreinte("XY01"));
    }

    // -------------------------------------------------------- micrologiciel

    /// L'octet d'état ne valide aucun argument : le diagnostic ne doit jamais
    /// laisser lire « compatible » ni « compris » là où l'appareil a seulement
    /// dit qu'il connaissait la commande.
    #[test]
    fn le_diagnostic_ne_survend_pas_une_commande_connue() {
        use candeo_device::Verdict;
        let texte = verdict(&Verdict::Understood);
        assert!(texte.contains("connue"), "{texte}");
        for mot in ["compatible", "compris"] {
            assert!(!texte.contains(mot), "« {mot} » dans « {texte} »");
        }
        assert!(verdict(&Verdict::Unsupported).contains("plus envoyée"));
    }

    // -------------------------------------------------------- sérialisation

    /// Les niveaux traversent l'IPC : leur écriture est celle de l'interface et
    /// celle de `settings.json`, et elle ne doit pas bouger.
    #[test]
    fn les_niveaux_se_serialisent_comme_ils_s_ecrivent() {
        assert_eq!(serde_json::to_string(&LogLevel::Warn).unwrap(), r#""warn""#);
        assert_eq!(
            serde_json::from_str::<LogLevel>(r#""trace""#).unwrap(),
            LogLevel::Trace
        );
    }
}
