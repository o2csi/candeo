//! L'icône de zone de notification : piloter candeo **sans la fenêtre**.
//!
//! # Ce module ne livre pas un raccourci, il livre une promesse
//!
//! L'architecture pose depuis le début qu'**un effet tourne fenêtre fermée** :
//! le moteur vit dans un fil indépendant ([`crate::runtime`]), et c'est
//! l'argument qui a fait écarter l'exécution des effets dans le WebView. La
//! conception était juste ; l'implémentation s'arrêtait avant la fin. Rien
//! n'empêchait `RunEvent::ExitRequested`, donc sous Windows comme sous Linux le
//! processus s'arrêtait avec sa dernière fenêtre — et avec lui le fil de rendu.
//! Fermer la fenêtre éteignait l'effet.
//!
//! **C'est ce module qui rend la phrase vraie**, et les deux moitiés ne se
//! défont pas l'une sans l'autre : une icône sans interception laisserait
//! l'application mourir quand même, une interception sans icône laisserait un
//! processus vivant que plus rien ne commande — et que plus rien ne quitte.
//! D'où [`installee`], et les deux interceptions de [`crate::run`] qui la
//! consultent : **tant qu'il n'y a pas d'icône, la croix reste une sortie**.
//!
//! # Ce que fait la croix de la fenêtre
//!
//! Elle **replie**, elle ne quitte pas. C'était le choix à faire, et c'est celui
//! qui suit de tout ce qui précède : fermer pour arrêter l'effet reviendrait à
//! annuler la promesse à l'endroit même où on vient de la tenir.
//!
//! Le prix est réel — l'application n'a plus de sortie évidente — et il est payé
//! deux fois : « Quitter candeo » est **la seule** sortie franche, isolée en bas
//! du menu par son propre séparateur, et la fenêtre le dit en toutes lettres
//! (voir `App.vue`). Une application qu'on ne sait pas quitter est une
//! application qu'on désinstalle.
//!
//! # Ce que quitter ne fait pas : éteindre le clavier
//!
//! Voir [`quitter`]. C'est un choix, et il est motivé sur place.
//!
//! # Le menu est une vue, jamais une source
//!
//! Il est rebâti à partir de l'état **réel** — `settings.json`, la bibliothèque,
//! [`crate::runtime::Engine::status`] — au survol de l'icône, et après chaque
//! action. Mais rien ne garantit qu'il soit à jour au moment du clic : un
//! clavier peut être débranché menu ouvert, un effet supprimé depuis la fenêtre,
//! une boucle s'arrêter d'elle-même après trente échecs. **Chaque action relit
//! donc l'état au moment où elle s'exécute** plutôt que de croire l'article sur
//! lequel on vient de cliquer ; ce qui échoue part au journal, seul endroit
//! visible en `release`.
//!
//! # Ce qui n'est pas garanti sous Linux
//!
//! `TrayIconEvent` n'y est pas émis du tout — l'icône s'affiche et son menu
//! s'ouvre, mais aucun survol ne se signale. Le menu y est donc rafraîchi par
//! les actions seules. C'est une limite de la pile GTK/AppIndicator, pas un
//! oubli ; la parade serait de reconstruire le menu sur minuterie, c'est-à-dire
//! d'énumérer l'USB et de lire le disque en boucle pour un menu que personne ne
//! regarde.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use candeo_protocol::Effect;
use tauri::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, Wry};

use candeo_device::Layout;

use crate::runtime::DeviceEngineStatus;
use crate::storage::{self, DeviceState, EffectEntry};
use crate::{journal, single_instance, AppState, CmdResult, DeviceRef};

/// Étiquette de l'icône, pour la retrouver et lui refaire son menu.
const ICONE: &str = "candeo";

/// Ce que la fenêtre doit apprendre quand l'état a changé **sans elle**.
///
/// La fenêtre interroge déjà le moteur toutes les secondes, mais elle ne relit
/// ni la liste des appareils ni `settings.json` : elle les avait lus une fois,
/// au montage, parce qu'elle était jusqu'ici la seule à les écrire. Elle ne l'est
/// plus, et elle survit désormais à sa propre fermeture — repliée, son
/// instantané peut vieillir des jours.
///
/// Écrit ici **et** dans `src/api/candeo.ts` ; le test en fin de module confronte
/// les deux, faute de quoi renommer l'événement compilerait sans un mot et
/// donnerait une fenêtre qui ne se resynchronise plus jamais.
pub(crate) const ETAT_CHANGE: &str = "candeo://etat-change";

/// Vrai quand l'icône est bel et bien posée.
///
/// **Ce n'est pas une commodité : c'est ce qui empêche de rendre candeo
/// impossible à quitter.** Si la pose échoue — pas de zone de notification, pas
/// d'icône dans le paquet, un environnement de bureau sans plateau — il ne reste
/// que la fenêtre pour commander l'application. Empêcher alors la sortie, ou
/// replier la fenêtre sur sa croix, laisserait un processus qu'aucun geste
/// ordinaire ne termine.
static INSTALLEE: AtomicBool = AtomicBool::new(false);

/// Dernier échec de construction du menu, pour n'en journaliser que la bascule.
///
/// Le menu se refait **à chaque survol de l'icône**. Un `settings.json`
/// illisible ou une énumération USB en panne produiraient une ligne par passage
/// de souris : c'est la même inondation que le par-image, à une autre cadence, et
/// c'est la même règle qui la ferme. Voir [`journal::bascule`].
static DERNIER_ECHEC: Mutex<Option<String>> = Mutex::new(None);

/// Vrai si l'icône est là, donc si l'application survit à ses fenêtres.
pub(crate) fn installee() -> bool {
    INSTALLEE.load(Ordering::Relaxed)
}

/// Annonce à la fenêtre que l'état a changé sans elle.
///
/// Générique parce que [`single_instance`] l'est : c'est lui qui ramène la
/// fenêtre, et une fenêtre qui revient après avoir été repliée est exactement le
/// cas où son instantané est le plus vieux.
///
/// L'échec est avalé : personne n'écoute quand aucune fenêtre n'est ouverte, et
/// c'est le cas nominal de ce module.
pub(crate) fn signaler<R: Runtime>(app: &AppHandle<R>) {
    let _ = app.emit(ETAT_CHANGE, ());
}

// ---------------------------------------------------------------- articles

/// Ce qu'un article de menu déclenche.
///
/// Un type, et non une chaîne comparée à la main dans le gestionnaire : muda ne
/// transporte qu'un identifiant textuel, et c'est le seul endroit du code où une
/// faute de frappe ne se verrait ni à la compilation, ni à l'exécution — l'article
/// ne ferait simplement rien. Le passage par [`Action::identifiant`] et
/// [`Action::depuis`] ramène cette liaison sous le compilateur, et le test
/// d'aller-retour la vérifie sans qu'il faille cliquer.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Action {
    /// Ramener la fenêtre sous les yeux.
    Ouvrir,
    /// **La seule sortie franche.**
    Quitter,
    /// Lancer cet effet sur cet appareil.
    Effet { device: DeviceRef, effet: String },
    /// Basculer la sortie clavier de cet appareil.
    Sortie { device: DeviceRef },
    /// `Effect::Off` du micrologiciel sur cet appareil.
    Eteindre { device: DeviceRef },
}

const OUVRIR: &str = "ouvrir";
const QUITTER: &str = "quitter";
const EFFET: &str = "effet";
const SORTIE: &str = "sortie";
const ETEINDRE: &str = "eteindre";

/// Le séparateur des champs d'un identifiant.
///
/// `:` peut le rester sans échappement : un identifiant d'effet passe par
/// [`storage::validate_id`], qui n'accepte que `a-z`, `0-9` et le tiret — il ne
/// peut donc jamais en contenir un. Le test
/// `l_alphabet_des_effets_exclut_le_separateur` tient cette dépendance, sans quoi
/// élargir un jour l'alphabet des identifiants casserait le menu en silence.
const SEP: char = ':';

/// L'appareil, écrit pour être relu — quatre chiffres hexadécimaux par champ.
///
/// Pas le `Display` de [`DeviceRef`] : celui-ci est fait pour être lu par un
/// humain dans un journal (`0x1532:0x0292`), et ce qu'on écrit ici doit
/// simplement se reparser sans ambiguïté.
fn cle(device: DeviceRef) -> String {
    format!("{:04x}{SEP}{:04x}", device.vid, device.pid)
}

fn appareil(vid: &str, pid: &str) -> Option<DeviceRef> {
    Some(DeviceRef {
        vid: u16::from_str_radix(vid, 16).ok()?,
        pid: u16::from_str_radix(pid, 16).ok()?,
    })
}

impl Action {
    fn identifiant(&self) -> String {
        match self {
            Self::Ouvrir => OUVRIR.to_owned(),
            Self::Quitter => QUITTER.to_owned(),
            Self::Effet { device, effet } => format!("{EFFET}{SEP}{}{SEP}{effet}", cle(*device)),
            Self::Sortie { device } => format!("{SORTIE}{SEP}{}", cle(*device)),
            Self::Eteindre { device } => format!("{ETEINDRE}{SEP}{}", cle(*device)),
        }
    }

    /// L'action que désigne cet identifiant, s'il en désigne une.
    ///
    /// `None` plutôt qu'une panique : le gestionnaire est **global** — il reçoit
    /// les événements de tous les menus de l'application —, et un article venu
    /// d'ailleurs n'est pas une erreur de programmation.
    fn depuis(id: &str) -> Option<Self> {
        match id {
            OUVRIR => return Some(Self::Ouvrir),
            QUITTER => return Some(Self::Quitter),
            _ => {}
        }

        let (verbe, reste) = id.split_once(SEP)?;
        let (vid, reste) = reste.split_once(SEP)?;
        match verbe {
            EFFET => {
                let (pid, effet) = reste.split_once(SEP)?;
                Some(Self::Effet {
                    device: appareil(vid, pid)?,
                    effet: effet.to_owned(),
                })
            }
            SORTIE => Some(Self::Sortie {
                device: appareil(vid, reste)?,
            }),
            ETEINDRE => Some(Self::Eteindre {
                device: appareil(vid, reste)?,
            }),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------- le menu

/// Un appareil piloté, tel que le menu doit le présenter.
struct Pilote {
    layout: &'static Layout,
    device: DeviceRef,
    /// Vrai si le périphérique est effectivement branché.
    ///
    /// Distinct de « piloté », comme partout ailleurs : un appareil adopté peut
    /// être débranché, et il garde alors sa place dans le menu. Le retirer ferait
    /// disparaître de la liste un clavier que l'utilisateur a décidé de piloter,
    /// pour la seule raison qu'il est momentanément ailleurs.
    branche: bool,
}

/// Les appareils pilotés, branchés ou non.
///
/// L'énumération USB peut échouer — c'est le cas d'un HID indisponible — et ce
/// n'est pas une raison de vider le menu : on retombe alors sur « aucune série
/// connue », ce que [`storage::DeviceRecord::matches`] tolère précisément pour
/// cette situation.
fn pilotes(settings: &storage::Settings) -> Vec<Pilote> {
    let api = crate::hid().ok();
    crate::LAYOUTS
        .iter()
        .copied()
        .filter_map(|layout| {
            let branche = api.as_ref().and_then(|api| crate::plugged(api, layout));
            let serie = branche.as_ref().and_then(|s| s.as_deref());
            if settings.device_state(layout.vid, layout.pid, serie) != DeviceState::Adopted {
                return None;
            }
            Some(Pilote {
                layout,
                device: DeviceRef::of(layout),
                branche: branche.is_some(),
            })
        })
        .collect()
}

/// Les sous-menus des appareils pilotés, bâtis sur l'état **courant**.
///
/// Isolé du reste du menu parce que c'est la seule part qui dépende du disque et
/// de l'USB : voir [`menu`], qui traite son échec comme une dégradation et non
/// comme un refus.
fn appareils(app: &AppHandle) -> CmdResult<Vec<Submenu<Wry>>> {
    let store = storage::store(app)?;
    let settings = store.read_settings()?;
    let bibliotheque = store.list_effects()?;
    // L'état réel du moteur, et non un souvenir : c'est la même source que la
    // commande `engine_status` que lit la fenêtre.
    //
    // **Les appareils seuls, jamais l'aperçu.** Ce menu décrit ce que font les
    // claviers ; cocher ici un effet qu'on est seulement en train de regarder
    // dans la fenêtre serait le mensonge que l'issue #63 refuse. Rien à filtrer —
    // `device_status` ne peut pas rendre l'aperçu.
    let moteur = app.state::<AppState>().engine.device_status();

    pilotes(&settings)
        .iter()
        .map(|pilote| sous_menu(app, pilote, &bibliotheque, &moteur))
        .collect()
}

/// Le menu, et ce qui a manqué pour le bâtir en entier.
///
/// Le second membre est `Some` quand le menu est **dégradé** : l'icône est là,
/// « Ouvrir la fenêtre » et « Quitter candeo » aussi, mais la liste des appareils
/// a manqué. C'est délibérément une dégradation et non une erreur — les deux
/// articles qui restent sont ceux qui ne dépendent de rien, et ce sont eux dont
/// on a le plus besoin quand quelque chose ne va pas. Refuser de poser l'icône
/// pour un `settings.json` illisible rendrait en prime la croix à nouveau
/// mortelle pour les effets, le temps d'une session entière.
///
/// Et le dire **dans le menu** n'est pas un pis-aller : en `release` le binaire
/// est compilé sans console, et l'icône est précisément l'endroit où un échec
/// peut se voir sans en ouvrir une.
fn menu(app: &AppHandle) -> CmdResult<(Menu<Wry>, Option<String>)> {
    let mut articles: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();

    let degrade = match appareils(app) {
        Ok(sous_menus) => {
            if sous_menus.is_empty() {
                // Une section vide se lirait comme une panne de l'icône. Nommer
                // l'absence, et dire où se prend la décision, coûte une ligne.
                articles.push(Box::new(muet(
                    app,
                    "Aucun appareil piloté — ouvrez la fenêtre pour en adopter un",
                )?));
            }
            for appareil in sous_menus {
                articles.push(Box::new(appareil));
            }
            None
        }
        Err(e) => {
            articles.push(Box::new(muet(
                app,
                &format!("Appareils indisponibles — {e}"),
            )?));
            Some(e)
        }
    };

    articles.push(Box::new(separateur(app)?));
    articles.push(Box::new(article(
        app,
        &Action::Ouvrir,
        "Ouvrir la fenêtre",
        true,
    )?));
    // Son propre séparateur, et il n'est pas décoratif : fermer la fenêtre ne
    // quitte plus, donc cet article est la seule sortie de l'application. Le
    // noyer dans la liste au-dessus reviendrait à la cacher.
    articles.push(Box::new(separateur(app)?));
    articles.push(Box::new(article(
        app,
        &Action::Quitter,
        "Quitter candeo",
        true,
    )?));

    let refs: Vec<&dyn IsMenuItem<Wry>> = articles.iter().map(AsRef::as_ref).collect();
    let assemble = Menu::with_items(app, &refs).map_err(|e| format!("menu non assemblé : {e}"))?;
    Ok((assemble, degrade))
}

/// Le sous-menu d'un appareil : son effet, sa sortie, son extinction.
fn sous_menu(
    app: &AppHandle,
    pilote: &Pilote,
    bibliotheque: &[EffectEntry],
    moteur: &[DeviceEngineStatus],
) -> CmdResult<Submenu<Wry>> {
    let etat = moteur
        .iter()
        .find(|s| s.device == pilote.device)
        .map(|s| &s.status);

    // `effect_id` survit à l'arrêt d'une boucle qui s'est coupée elle-même après
    // trente échecs : sans le filtre, le menu cocherait un effet que plus rien
    // ne fait tourner.
    let en_cours = etat
        .filter(|s| s.running)
        .and_then(|s| s.effect_id.as_deref());

    let mut articles: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();
    for entree in bibliotheque {
        articles.push(Box::new(coche(
            app,
            &Action::Effet {
                device: pilote.device,
                effet: entree.id.clone(),
            },
            &entree.manifest.name,
            true,
            en_cours == Some(entree.id.as_str()),
        )?));
    }

    articles.push(Box::new(separateur(app)?));
    articles.push(Box::new(coche(
        app,
        &Action::Sortie {
            device: pilote.device,
        },
        "Envoyer au clavier",
        // La bascule vit dans la boucle : sans boucle, elle n'a rien à basculer.
        en_cours.is_some(),
        etat.is_some_and(|s| s.to_keyboard),
    )?));
    articles.push(Box::new(article(
        app,
        &Action::Eteindre {
            device: pilote.device,
        },
        "Éteindre",
        // Grisé quand l'appareil n'est pas là — mais ce n'est qu'un confort
        // d'affichage : ce qui protège vraiment, c'est que [`eteindre`] relit
        // l'état au moment du clic. Voir l'en-tête du module.
        pilote.branche,
    )?));

    let refs: Vec<&dyn IsMenuItem<Wry>> = articles.iter().map(AsRef::as_ref).collect();
    let titre = if pilote.branche {
        pilote.layout.name.to_owned()
    } else {
        format!("{} — débranché", pilote.layout.name)
    };

    Submenu::with_items(app, titre, true, &refs)
        .map_err(|e| format!("sous-menu de {} non assemblé : {e}", pilote.device))
}

fn article(app: &AppHandle, action: &Action, texte: &str, actif: bool) -> CmdResult<MenuItem<Wry>> {
    MenuItem::with_id(app, action.identifiant(), texte, actif, None::<&str>)
        .map_err(|e| format!("article « {texte} » non créé : {e}"))
}

fn coche(
    app: &AppHandle,
    action: &Action,
    texte: &str,
    actif: bool,
    cochee: bool,
) -> CmdResult<CheckMenuItem<Wry>> {
    CheckMenuItem::with_id(
        app,
        action.identifiant(),
        texte,
        actif,
        cochee,
        None::<&str>,
    )
    .map_err(|e| format!("bascule « {texte} » non créée : {e}"))
}

/// Un article qui ne fait rien : il informe, et il est grisé pour le dire.
///
/// Sans identifiant, donc sans [`Action`] : cliquer dessus est impossible, et
/// lui en donner un laisserait croire le contraire.
fn muet(app: &AppHandle, texte: &str) -> CmdResult<MenuItem<Wry>> {
    MenuItem::new(app, texte, false, None::<&str>)
        .map_err(|e| format!("article « {texte} » non créé : {e}"))
}

fn separateur(app: &AppHandle) -> CmdResult<PredefinedMenuItem<Wry>> {
    PredefinedMenuItem::separator(app).map_err(|e| format!("séparateur non créé : {e}"))
}

// ---------------------------------------------------------------- les actions

/// Exécute ce que l'article demande, en relisant l'état au passage.
fn agir(app: &AppHandle, action: Action) {
    match action {
        // Ces deux-là ne touchent pas au moteur, et ne passent donc pas par
        // [`rendre_compte`] : [`reveler`] prévient déjà la fenêtre qui revient,
        // et [`quitter`] emporte le processus — refaire un menu ou prévenir une
        // fenêtre qu'on est en train de détruire n'ajouterait qu'une ligne
        // d'échec à chaque sortie. `app.exit` **rend la main** : ce qui suit un
        // appel à [`quitter`] s'exécute bel et bien.
        Action::Ouvrir => reveler(app),
        Action::Quitter => quitter(app),
        Action::Effet { device, effet } => {
            demarrer(app, device, &effet);
            rendre_compte(app);
        }
        Action::Sortie { device } => {
            basculer(app, device);
            rendre_compte(app);
        }
        Action::Eteindre { device } => {
            eteindre(app, device);
            rendre_compte(app);
        }
    }
}

/// Ce qui vient d'être démenti par une action : le menu, et la fenêtre.
///
/// Refaire le menu **maintenant** est ce qui tient « le menu reflète l'état
/// réel » là où aucun survol n'est signalé, c'est-à-dire sous Linux.
fn rendre_compte(app: &AppHandle) {
    rafraichir(app);
    signaler(app);
}

fn reveler(app: &AppHandle) {
    // Le chemin de l'instance unique, et non un second : il sait montrer une
    // fenêtre masquée comme en rouvrir une depuis sa déclaration, et il cherche
    // sur l'étiquette seule — une fenêtre `create: false` reste donc celle qu'il
    // faut ouvrir. En écrire un autre ici en ferait deux à maintenir d'accord.
    if let Err(e) = single_instance::reveal(app) {
        tracing::warn!("fenêtre non ramenée depuis la zone de notification : {e}");
    }
}

/// **La seule sortie franche.**
///
/// # L'éclairage est laissé tel quel, et c'est un choix
///
/// Quitter n'éteint pas le clavier. Un effet micrologiciel survit de toute façon
/// à l'extinction du logiciel — c'est le micrologiciel qui l'exécute — et un
/// clavier qui s'éteindrait en quittant surprendrait davantage qu'un clavier qui
/// reste comme on l'a laissé. « Éteindre » est dans le menu, à un clic, pour qui
/// veut le noir.
///
/// La conséquence est assumée : un effet de la boucle hôte laisse le clavier sur
/// sa **dernière image**, figée, et une image figée ressemble à un effet qui
/// tourne encore. C'est le prix du choix inverse de [`crate::release_devices`],
/// qui éteint parce qu'elle, elle repart d'un état connu.
///
/// Les boucles sont tout de même arrêtées, et **attendues** : une écriture HID
/// coupée en plein transfert par la fin du processus laisserait l'appareil sur
/// une trame partielle. L'éclairage, lui, ne bouge pas — arrêter une boucle
/// n'écrit rien de plus.
fn quitter(app: &AppHandle) {
    app.state::<AppState>().engine.stop_all();
    tracing::info!("candeo s'arrête, demandé depuis la zone de notification");
    // Un code, donc `ExitRequested { code: Some(_) }` : c'est ce qui distingue
    // cette sortie-ci de celle que provoque la fermeture de la dernière fenêtre,
    // et donc ce qui la laisse passer. Voir [`crate::run`].
    app.exit(0);
}

fn demarrer(app: &AppHandle, device: DeviceRef, effet: &str) {
    let params = match parametres(app, device, effet) {
        Ok(params) => params,
        Err(e) => {
            tracing::error!(appareil = %device, effet, "effet non lancé depuis la zone de notification : {e}");
            return;
        }
    };

    // La commande de la fenêtre, et non une copie : lancer un effet depuis le
    // menu doit faire exactement ce que fait la galerie — même résolution de la
    // bibliothèque, même gabarit, même poignée partagée avec la boucle. Une
    // seconde écriture de ce chemin divergerait au premier changement.
    if let Err(e) = crate::runtime::start_effect(
        app.clone(),
        app.state(),
        device,
        effet.to_owned(),
        serde_json::Value::Object(params),
    ) {
        // Le moteur a déjà pu nommer la cause sous son propre *span* ; ce qui
        // manquerait sans cette ligne, c'est **d'où venait la demande** — et
        // l'échec le plus probable ici, un effet supprimé depuis que le menu a
        // été bâti, est refusé avant que le moteur n'en sache rien. Une ligne
        // par clic n'inonde personne : la règle des transitions vise le
        // par-image, pas un geste humain.
        tracing::error!(appareil = %device, effet, "effet non lancé depuis la zone de notification : {e}");
    }
}

/// Les valeurs sur lesquelles lancer cet effet, relues maintenant.
fn parametres(
    app: &AppHandle,
    device: DeviceRef,
    effet: &str,
) -> CmdResult<serde_json::Map<String, serde_json::Value>> {
    let store = storage::store(app)?;
    let entree = store
        .list_effects()?
        .into_iter()
        .find(|e| e.id == effet)
        .ok_or_else(|| format!("aucun effet nommé « {effet} »"))?;
    let settings = store.read_settings()?;

    Ok(storage::starting_params(
        &entree.manifest,
        settings.effect_params(device.vid, device.pid, effet),
    ))
}

/// Bascule la sortie clavier, **d'après l'état du moteur**.
///
/// Pas d'après la case du menu : muda la retourne toute seule au clic, et elle
/// datait de la dernière construction. S'y fier ferait rétablir une sortie qu'on
/// venait de couper depuis la fenêtre.
fn basculer(app: &AppHandle, device: DeviceRef) {
    let state = app.state::<AppState>();
    let Some(etat) = state
        .engine
        .device_status()
        .into_iter()
        .find(|s| s.device == device)
    else {
        tracing::warn!(appareil = %device, "sortie non basculée : aucune boucle sur cet appareil");
        return;
    };

    crate::runtime::set_output_to_keyboard(app.state(), device, !etat.status.to_keyboard);
}

/// `Effect::Off` du micrologiciel : coût nul, et ça survit à la fermeture.
///
/// L'ordre est celui de [`crate::release_devices`], et il n'est pas indifférent :
/// la boucle s'arrête **et son arrêt est attendu**, sinon l'image suivante
/// rallumerait ce qu'on vient d'éteindre.
fn eteindre(app: &AppHandle, device: DeviceRef) {
    let state = app.state::<AppState>();
    state.engine.stop(device);
    // Plus rien ne tourne sur cet appareil, et le fichier doit le dire : laisser
    // l'identifiant en place ferait décrire par `settings.json` un effet que
    // personne n'a plus demandé. C'est le même geste que `stop_effect` fait
    // depuis la fenêtre.
    crate::runtime::retenir_l_effet_actif(app, device, None);

    if let Err(e) = crate::with_keyboard(&state, device, |kb| {
        kb.set_effect(Effect::Off).map_err(|e| e.to_string())
    }) {
        // Le cas prévu : l'appareil a été débranché — ou ignoré depuis la
        // fenêtre — pendant que le menu était ouvert. L'article était grisé au
        // moment où le menu a été bâti ; il ne l'était plus au moment du clic.
        tracing::warn!(appareil = %device, "extinction refusée depuis la zone de notification : {e}");
    }
}

// ---------------------------------------------------------------- la pose

/// Pose l'icône. **Ne peut pas échouer**, au sens où rien ne remonte.
///
/// Un échec ici ne doit pas empêcher l'application de démarrer : il la ramène à
/// ce qu'elle était avant cette issue — une fenêtre, et la croix pour la quitter.
/// C'est [`installee`] qui porte cette bascule, et [`crate::run`] qui la lit.
///
/// Restent les échecs qui n'en sont pas : un `settings.json` illisible ou une
/// énumération USB en panne posent l'icône quand même, avec un menu qui le dit.
/// Voir [`menu`].
pub(crate) fn installer(app: &AppHandle) {
    match poser(app) {
        Ok(()) => {
            INSTALLEE.store(true, Ordering::Relaxed);
            tracing::info!(
                "icône de zone de notification posée, la fenêtre n'est plus la seule commande"
            );
        }
        // `error` : sans icône, fermer la fenêtre arrête les effets — c'est
        // exactement la panne que cette issue referme, et elle redevient
        // silencieuse si personne ne la dit.
        Err(e) => tracing::error!(
            "aucune icône de zone de notification, fermer la fenêtre arrêtera les effets : {e}"
        ),
    }
}

fn poser(app: &AppHandle) -> CmdResult<()> {
    // L'icône de l'application, pas une seconde image à tenir à jour : c'est
    // celle que le paquet embarque déjà, et celle que l'utilisateur reconnaît.
    let icone = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| "aucune icône d'application dans le paquet".to_string())?;

    // Le menu de départ, dégradation comprise ; ce qui a manqué est consigné par
    // le même chemin que les suivants, pour n'en garder que le début.
    let (depart, degrade) = menu(app)?;
    consigner(degrade.as_deref());

    TrayIconBuilder::with_id(ICONE)
        .icon(icone)
        .tooltip("candeo")
        .menu(&depart)
        // Le clic gauche ouvre la fenêtre, le clic droit ouvre le menu : c'est la
        // convention de la zone de notification, et elle met « Ouvrir la
        // fenêtre » à un seul clic. Sous Linux aucun clic n'est signalé, seul le
        // menu du clic droit répond — d'où l'article, qui reste le chemin sûr.
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|icone, evenement| match evenement {
            // Le survol précède le clic droit : c'est le dernier instant où l'on
            // peut refaire le menu avant qu'il ne s'affiche.
            TrayIconEvent::Enter { .. } => rafraichir(icone.app_handle()),
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } => reveler(icone.app_handle()),
            _ => {}
        })
        .on_menu_event(|app, evenement: MenuEvent| {
            // Le gestionnaire est global : il voit passer les articles de tous
            // les menus. Ce qui ne nous concerne pas est ignoré, pas refusé.
            if let Some(action) = Action::depuis(evenement.id.as_ref()) {
                agir(app, action);
            }
        })
        .build(app)
        // La poignée est déposée : le gestionnaire de l'application en garde une,
        // c'est elle qui maintient l'icône en vie, et [`rafraichir`] la retrouve
        // par son étiquette.
        .map(|_| ())
        .map_err(|e| format!("icône non posée : {e}"))
}

/// Refait le menu à partir de l'état courant.
///
/// Le prix est celui d'une commande ordinaire : une lecture de `settings.json`,
/// une lecture de la bibliothèque, et une énumération USB. C'est exactement ce
/// que coûte `list_devices` à chaque ouverture de l'écran des périphériques, et
/// c'est payé ici au même titre — sur un geste de l'utilisateur, jamais sur une
/// minuterie. Un menu qui se reconstruirait en fond énumérerait l'USB
/// indéfiniment pour un menu que personne ne regarde.
pub(crate) fn rafraichir(app: &AppHandle) {
    let Some(icone) = app.tray_by_id(ICONE) else {
        return;
    };

    let echec = match menu(app) {
        // Un menu dégradé est bel et bien posé : il porte de quoi ouvrir la
        // fenêtre et de quoi quitter, et il **dit** ce qui a manqué. Ce que le
        // journal en retient, c'est le début de la panne, pas un survol sur deux.
        Ok((menu, degrade)) => icone
            .set_menu(Some(menu))
            .map_err(|e| format!("menu non remplacé, l'ancien reste affiché : {e}"))
            .err()
            .or(degrade),
        Err(e) => Some(e),
    };
    consigner(echec.as_deref());
}

/// Journalise le **début** d'un échec de menu, et son rétablissement. Rien
/// d'autre — voir [`DERNIER_ECHEC`].
fn consigner(echec: Option<&str>) {
    let mut dernier = DERNIER_ECHEC.lock().unwrap();
    let bascule = journal::bascule(dernier.as_deref(), echec);
    *dernier = echec.map(str::to_owned);
    drop(dernier);

    match bascule {
        journal::Bascule::Commence => tracing::error!(
            "menu de la zone de notification incomplet : {}",
            echec.unwrap_or_default()
        ),
        journal::Bascule::Retabli => tracing::info!("menu de la zone de notification rétabli"),
        journal::Bascule::Rien => {}
    }
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    const APPAREIL: DeviceRef = DeviceRef {
        vid: 0x1532,
        pid: 0x0292,
    };
    /// Deux appareils, parce que tout ce qui est « par appareil » n'a de sens
    /// qu'à partir de deux — et que confondre deux identifiants ferait agir le
    /// menu sur le mauvais clavier.
    const AUTRE: DeviceRef = DeviceRef {
        vid: 0x1532,
        pid: 0x0001,
    };

    fn toutes() -> Vec<Action> {
        vec![
            Action::Ouvrir,
            Action::Quitter,
            Action::Effet {
                device: APPAREIL,
                effet: "onde-circulaire".into(),
            },
            Action::Effet {
                device: AUTRE,
                effet: "a".into(),
            },
            Action::Sortie { device: APPAREIL },
            Action::Eteindre { device: AUTRE },
        ]
    }

    /// **Le seul lien entre un article et ce qu'il fait est une chaîne.** muda ne
    /// transporte rien d'autre : une écriture et une lecture qui divergeraient
    /// donneraient un article qui ne fait rien, sans erreur ni à la compilation
    /// ni à l'exécution.
    #[test]
    fn chaque_action_se_relit_telle_qu_elle_s_ecrit() {
        for action in toutes() {
            let id = action.identifiant();
            assert_eq!(
                Action::depuis(&id).as_ref(),
                Some(&action),
                "identifiant « {id} »"
            );
        }
    }

    /// Deux appareils ne doivent pas se confondre : le menu agirait sur le
    /// mauvais clavier, et rien ne le signalerait.
    #[test]
    fn deux_appareils_donnent_deux_identifiants() {
        assert_ne!(
            Action::Eteindre { device: APPAREIL }.identifiant(),
            Action::Eteindre { device: AUTRE }.identifiant()
        );
        assert_ne!(cle(APPAREIL), cle(AUTRE));
    }

    /// Le gestionnaire est global : il reçoit les articles de tous les menus de
    /// l'application. Ce qui ne vient pas d'ici doit être ignoré, pas interprété
    /// de travers.
    #[test]
    fn un_identifiant_etranger_ne_declenche_rien() {
        for id in [
            "",
            "ouvrir-vraiment",
            "effet",
            "effet:1532",
            "effet:1532:0292",
            "effet:zzzz:0292:onde",
            "sortie:1532",
            "sortie:1532:0292:en-trop",
            "eteindre:1532:029x",
            "3", // un identifiant que muda a numéroté lui-même
        ] {
            assert_eq!(Action::depuis(id), None, "« {id} » a été interprété");
        }
    }

    /// **Ce qui rend `:` utilisable sans échappement.** Le jour où l'alphabet des
    /// identifiants d'effet s'élargirait, le menu se mettrait à viser le mauvais
    /// effet — ou aucun — sans que rien ne le dise.
    #[test]
    fn l_alphabet_des_effets_exclut_le_separateur() {
        assert!(storage::validate_id(&format!("a{SEP}b")).is_err());

        let effet = "a".repeat(64);
        let action = Action::Effet {
            device: APPAREIL,
            effet: effet.clone(),
        };
        storage::validate_id(&effet).expect("identifiant refusé");
        assert_eq!(Action::depuis(&action.identifiant()), Some(action));
    }

    /// Le nom de l'événement est écrit des deux côtés de l'IPC, et rien ne relie
    /// les deux à la compilation : le renommer d'un seul côté donnerait une
    /// fenêtre qui ne se resynchronise plus, sans une erreur nulle part. Même
    /// garde que pour l'étiquette de la fenêtre, voir [`single_instance`].
    #[test]
    fn l_evenement_porte_le_meme_nom_des_deux_cotes() {
        let ts = include_str!("../../src/api/candeo.ts");
        assert!(
            ts.contains(ETAT_CHANGE),
            "« {ETAT_CHANGE} » est introuvable dans src/api/candeo.ts"
        );
    }
}
