//! Moteur d'effets : **un fil de rendu par appareil**, indépendants de la fenêtre.
//!
//! C'est le seul endroit où du code d'effet s'exécute. Le front n'en exécute
//! jamais : il envoie la source et reçoit les images. L'aperçu du simulateur
//! est donc la production, par construction — et non une ressemblance obtenue
//! en faisant tourner le même code dans un second moteur JavaScript.
//!
//! # Un appareil, un effet
//!
//! Chaque appareil porte sa boucle, donc sa cadence, ses paramètres, son état
//! d'erreur et sa sortie. Rien n'est partagé entre deux appareils : c'est ce qui
//! fait qu'un appareil en panne n'en affecte aucun autre — l'invariant de
//! l'adoption (issue #25), tenu cette fois au niveau du moteur.
//!
//! Une boucle reçoit **un gabarit** et **une sortie**, jamais « un clavier ».
//! Le jour où un gabarit couvrira plusieurs appareils, c'est [`DeviceOut`] qui
//! répartira l'image, et le code des effets ne changera pas d'une ligne.
//!
//! # Et une boucle d'aperçu, qui n'est celle d'aucun appareil
//!
//! **Prévisualiser ne doit jamais interrompre l'effet en cours sur le clavier.**
//! Le moteur étant à un effet par appareil, prévisualiser Y sur un clavier qui
//! exécute X arrêterait X : parcourir la galerie éteindrait l'éclairage en cours
//! (issue #63). La sortie est une boucle **séparée**, une seule, dont la sortie
//! matérielle est [`SansSortie`] — `DeviceOut::present` rendant `None` veut déjà
//! dire « aucun appareil ouvert, ce n'est pas un échec ».
//!
//! Elle **emprunte le gabarit** de l'appareil sélectionné, pour ressembler à ce
//! qu'on obtiendra, sans rien lui prendre d'autre : ni sa boucle, ni sa poignée,
//! ni sa ligne d'état.
//!
//! C'est pourquoi [`EngineReport`] range les deux dans **deux champs distincts**
//! plutôt que dans une liste à filtrer. Voir [`PreviewStatus`].
//!
//! # Ce qu'un effet ne peut pas faire durer
//!
//! Un `while (true)` dans `render` gèlerait son fil définitivement : le drapeau
//! `stop` est lu *entre* deux images, il ne serait donc jamais relu — et comme
//! un effet tourne fenêtre fermée, la fermer ne sauverait pas. Une allocation
//! sans fin, elle, emporterait le processus entier plutôt que le seul effet.
//!
//! Ni l'une ni l'autre n'est une question de malveillance : ce sont deux erreurs
//! de programmation ordinaires, et deux bornes suffisent à les traiter comme
//! telles — un temps de calcul par image ([`BUDGET_IMAGE`], et
//! [`BUDGET_CHARGEMENT`] pour le corps du module) et une mémoire par effet
//! ([`BUDGET_MEMOIRE`]). Le dépassement n'ouvre **aucun chemin nouveau** : il
//! emprunte celui des exceptions, que [`MAX_CONSECUTIVE_ERRORS`] transforme en
//! arrêt propre. Ce que le moteur ajoute, c'est le nom de la cause : voir
//! [`nommer_la_cause`].
//!
//! # Ordre de prise des verrous
//!
//! Trois, et l'ordre est celui de la déclaration : **table des boucles → fil d'un
//! appareil → état partagé d'une boucle**.
//!
//! La table n'est verrouillée que le temps d'y lire ou d'y poser un `Arc`, jamais
//! pendant un démarrage ni une attente de fin. Les deux suivants sont pris
//! ensemble, dans cet ordre, par [`DeviceLoop::start`] et [`DeviceLoop::stop`] —
//! c'est ce qui sérialise démarrage et arrêt d'un appareil, et le verrou attendu
//! est **le sien** : attendre la fin de l'un ne retient aucune commande visant
//! les autres.
//!
//! Le fil de rendu, lui, ne prend que le dernier : il ne connaît que son
//! [`Shared`] et sa sortie, jamais le moteur. Il n'a donc aucun moyen de retenir
//! une commande, et il ne tient jamais la poignée d'un appareil et un verrou du
//! moteur en même temps. Voir `crate::AppState`.
//!
//! Voir `docs/design/effects-runtime.md`.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use candeo_device::{Keyboard, Layout};
use candeo_protocol::Rgb;
use rquickjs::loader::{BuiltinLoader, BuiltinResolver};
use rquickjs::runtime::InterruptHandler;
use rquickjs::{CatchResultExt, Context, Function, Module, Runtime};
use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};

use crate::journal;

pub mod swatch;

/// Le module que l'hôte fournit, et que l'éditeur décrit par son `.d.ts`.
const API_JS: &str = include_str!("api.js");

/// La colle qui importe l'effet et installe la fonction de rendu.
const BOOTSTRAP_JS: &str = include_str!("bootstrap.js");

/// 30 images par seconde — **mesuré, pas supposé**.
///
/// La cadence était à 60 par analogie avec un écran. Le chronométrage sur le
/// matériel dit autre chose : une mise à jour complète coûte **7 transferts de
/// contrôle** — 6 rangées puis le passage en mode custom, voir
/// `Keyboard::present` — et les 120 mesurées donnent **13,1 ms en moyenne,
/// 14,4 ms au pire**, sans une seule écriture refusée. L'appareil accepte donc
/// jusqu'à ~76 img/s.
///
/// 60 img/s tenait donc *à peine* : l'écriture seule mangeait **78 % de la
/// période** de 16,7 ms, laissant ~3,6 ms à l'effet — moins que le budget de
/// calcul qu'on lui accorde. Autrement dit un effet **parfaitement dans les
/// clous** faisait déjà rater l'échéance, et la boucle retombait en silence à
/// une cadence qu'elle n'annonçait nulle part.
///
/// À 33,3 ms, l'écriture retombe à 39 % et il reste ~20 ms pour l'effet. Ce que
/// 60 promettait sans le tenir, 30 le tient.
///
/// ⚠️ **Le goulot est le bus, pas le JavaScript.** Deux pistes si la cadence
/// devait remonter : ne réécrire que les rangées qui changent — l'écriture
/// partielle est vérifiée sur le matériel — et cesser de réémettre la trame de
/// mode custom quand on y est déjà, qui vaut à elle seule ~1,9 ms sur les 13.
const FPS: u32 = 30;

/// Au-delà, on arrête. Un effet qui lève à chaque image ne se rétablira pas
/// tout seul, et continuer reviendrait à remplir le journal en silence.
const MAX_CONSECUTIVE_ERRORS: u32 = 30;

/// Temps accordé au calcul d'**une** image.
///
/// ## Ce que ce chiffre mesure vraiment
///
/// Pas une allocation de performance : **un détecteur de gel**, avec de la marge
/// pour l'à-coup machine. Un effet ordinaire coûte **0,23 ms** et un champ de
/// cinq mille particules avec une seconde de traînée **1,1 ms** — mesurés en
/// `release` sur ce moteur, pour un clavier de 132 LED. Aucun effet réaliste ne
/// vit entre 1 et 10 ms.
///
/// Ce que ces millisecondes achètent, c'est donc de la **préemption tolérée** :
/// l'échéance se mesure en temps réel, pas en temps de calcul, et un fil que
/// l'ordonnanceur suspend au milieu d'une image consomme son budget sans rien
/// exécuter. À 10 ms, un effet ordinaire peut se faire suspendre près de 10 ms
/// sans être accusé de geler.
///
/// ## Où est le plafond
///
/// L'écriture HID d'une image complète coûte **13,1 ms en moyenne, 14,4 ms au
/// pire** (§5 du relevé), et elle vit dans la même période de 33,3 ms :
///
/// ```text
///   budget 10 ms + écriture 14,4 ms = 24,4 ms   →  9 ms de marge
/// ```
///
/// Le seuil où l'on commencerait à **rater l'échéance en silence** est vers
/// **19 ms** de budget. On en est loin, et c'est ce qui rend 10 ms sans risque
/// là où le chiffre d'origine — la moitié d'une période de 16,7 ms — n'était
/// qu'une proportion, plus une mesure.
///
/// ## Et très en-dessous d'un gel
///
/// Un effet qui ne sort pas s'arrête au bout de [`MAX_CONSECUTIVE_ERRORS`]
/// images, soit **0,3 s** — alors qu'un budget d'une seconde par image aurait
/// fait attendre une demi-minute avant de dire ce qui ne va pas.
///
/// Dépasser une fois n'arrête rien : le compteur d'erreurs consécutives repart
/// à zéro dès la première image rendue. Il en faut trente d'affilée.
#[cfg(not(debug_assertions))]
const BUDGET_IMAGE: Duration = Duration::from_millis(10);

/// Le même budget, à la vitesse du moteur qu'on a réellement compilé.
///
/// QuickJS est du C, compilé au niveau d'optimisation du profil. Non optimisé,
/// la **même** image du **même** effet simple passait de 0,23 ms à 6,3 ms de
/// JavaScript : vingt-cinq fois plus lent, mesuré. Ce n'est pas l'effet qui
/// changeait, c'est l'interpréteur.
///
/// Un budget unique aurait donc dû choisir son camp : à 10 ms il couperait des
/// effets irréprochables dès qu'on lance l'application en développement ; à
/// 200 ms il laisserait un gel de six secondes en production.
///
/// ⚠️ **Ce facteur 25 a été mesuré avant que `[profile.dev.package."*"]` ne
/// passe les dépendances en `opt-level = 2`.** QuickJS arrive par une
/// dépendance : il est donc optimisé en débogage lui aussi, désormais, et
/// l'écart devrait avoir fondu. Cette valeur reste **un plafond, pas une
/// cible** — la garder large ne coûte rien tant que personne ne la prend pour
/// une mesure à jour. À refaire si quelqu'un veut unifier les deux budgets.
#[cfg(debug_assertions)]
const BUDGET_IMAGE: Duration = Duration::from_millis(200);

/// Temps accordé au **chargement** d'un effet, corps du module compris.
///
/// Le corps du module s'exécute une fois, avant la première image : il échappe
/// donc au budget d'image. Sans borne ici, une boucle écrite hors de `render`
/// bloquerait [`DeviceLoop::start`] pour toujours — la commande attend le
/// verdict du chargement, le verrou de l'appareil à la main, et ce clavier ne
/// démarrerait ni n'arrêterait plus rien.
///
/// Analyser et évaluer un module se compte en millisecondes ; deux secondes
/// sont trois ordres de grandeur au-dessus, et ce prix n'est payé qu'une fois.
const BUDGET_CHARGEMENT: Duration = Duration::from_secs(2);

/// Mémoire accordée au moteur JavaScript d'un effet — **un par appareil**.
///
/// Le tampon d'image ne pèse rien : `bootstrap.js` le réutilise d'une image à
/// l'autre. Mais un effet a le droit de garder un état, et c'est lui qu'il faut
/// loger. Mesuré, contexte QuickJS et modules chargés compris : 0,17 Mo pour un
/// effet sans état, 2,7 Mo pour deux mille particules gardant une seconde
/// d'images, 6 Mo pour cinq mille. Trente-deux mégaoctets laissent donc cinq
/// fois l'effet le plus démesuré qu'on sache écrire pour 132 LED, et près de
/// deux cents fois l'effet ordinaire — tout en restant négligeables devant
/// l'application, même avec un effet par clavier.
///
/// Contrairement au temps, la mémoire ne dépend pas du profil de compilation :
/// les mêmes objets occupent les mêmes octets.
///
/// Ce qu'on borne, ce n'est pas l'appétit d'un effet : c'est qu'un tableau qui
/// grandit à chaque image emporte tout le processus au lieu de lui-même.
const BUDGET_MEMOIRE: usize = 32 * 1024 * 1024;

/// Ce que la boucle partage avec le reste de l'application.
///
/// Tout est derrière `Arc` : le fil de rendu survit à la fenêtre, il ne peut
/// donc rien emprunter à l'état d'une commande.
struct Shared {
    stop: AtomicBool,
    /// Paramètres de l'effet, en JSON. Relus à chaque image : les régler ne
    /// redémarre pas la boucle.
    params: Mutex<String>,
    /// Sortie clavier. Séparée de la sortie simulateur — on doit pouvoir
    /// écrire un effet sans posséder le clavier, et le laisser tourner sans
    /// regarder l'écran.
    to_keyboard: AtomicBool,
    /// Sortie simulateur. `None` tant que personne n'écoute : rien n'est alors
    /// sérialisé.
    frames: Mutex<Option<Channel<InvokeResponseBody>>>,
    /// Dernière erreur de l'effet. Lisible même fenêtre fermée puis rouverte,
    /// ce qu'un événement ponctuel ne permettrait pas.
    error: Mutex<Option<String>>,
    /// Dernier échec d'écriture vers le clavier.
    ///
    /// Distinct de l'erreur d'effet ci-dessus : ces deux pannes n'ont ni la
    /// même cause ni le même remède, et les confondre enverrait chercher au
    /// mauvais endroit. Un effet impeccable peut très bien n'atteindre aucune
    /// LED.
    device_error: Mutex<Option<String>>,
    /// Vrai si la dernière image a réellement été écrite sur un périphérique.
    ///
    /// Sans cela, lancer un effet sans clavier connecté ne produisait **aucun
    /// signe** : le simulateur s'animait, la case « envoyer » restait cochée,
    /// et le clavier gardait son image précédente. Un silence qui se lit comme
    /// une panne du moteur.
    reaching: AtomicBool,
    /// Nom de l'effet en cours, pour que l'interface sache quoi mettre en
    /// avant après un redémarrage de la fenêtre.
    effect_id: Mutex<Option<String>>,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            stop: AtomicBool::new(false),
            params: Mutex::new("{}".to_string()),
            to_keyboard: AtomicBool::new(true),
            frames: Mutex::new(None),
            error: Mutex::new(None),
            device_error: Mutex::new(None),
            reaching: AtomicBool::new(false),
            effect_id: Mutex::new(None),
        }
    }
}

/// État du moteur pour **un** appareil, tel que l'interface le lit.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub running: bool,
    pub effect_id: Option<String>,
    /// Erreur venant du code de l'effet, déjà lisible : affichée telle quelle.
    pub error: Option<String>,
    /// Échec d'écriture vers le clavier — rien à voir avec le code de l'effet.
    pub device_error: Option<String>,
    /// Vrai si les images parviennent effectivement à un clavier.
    pub reaching_keyboard: bool,
    pub to_keyboard: bool,
}

/// L'état d'un appareil, et à qui il appartient.
///
/// `engine_status()` en rend une par appareil visé : un message global
/// obligerait à choisir lequel afficher, et le suivant effacerait le précédent —
/// exactement ce que la table des échecs d'ouverture évite déjà côté adoption.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceEngineStatus {
    pub device: DeviceRef,
    #[serde(flatten)]
    pub status: EngineStatus,
}

/// Ce que la fenêtre **regarde**, et qui n'atteint aucun clavier.
///
/// # Un type à part, et non une ligne de plus dans la liste des appareils
///
/// C'est la quatrième fois dans ce projet qu'un état qui ment coûte une session
/// de diagnostic — le clavier non adopté, l'écriture « acceptée », l'image figée
/// après arrêt automatique, et maintenant l'aperçu. Un drapeau à filtrer se
/// filtre mal : il suffit d'un appelant qui l'oublie — l'icône de zone de
/// notification, le journal, la galerie — pour annoncer comme tournant sur le
/// clavier un effet qu'on ne fait que regarder. Ici il n'y a **rien à filtrer** :
/// l'aperçu n'est pas dans la liste, et un appelant ne peut pas l'y trouver par
/// mégarde.
///
/// # Ce qu'il ne porte pas est aussi délibéré
///
/// Ni `toKeyboard`, ni `reachingKeyboard`, ni `deviceError`. Une boucle d'aperçu
/// n'a **aucune** sortie matérielle ; ces trois champs à faux ne décriraient pas
/// un aperçu, ils décriraient un effet qui n'arrive pas à écrire — c'est-à-dire
/// une panne, là où il n'y a qu'un choix.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewStatus {
    /// L'appareil dont l'aperçu **emprunte** le gabarit.
    ///
    /// Il n'est pas piloté, il n'est même pas forcément branché : c'est une
    /// géométrie, pas une destination. Le nommer permet à l'interface de dire « à
    /// quoi ça ressemblera sur ce clavier-là ».
    pub layout_of: DeviceRef,
    pub running: bool,
    pub effect_id: Option<String>,
    /// Erreur venant du code de l'effet, déjà lisible : affichée telle quelle.
    pub error: Option<String>,
}

/// Tout ce que le moteur sait, **rangé de façon à ne pas se confondre**.
///
/// Deux champs, pas une liste : `devices` décrit ce qui tourne sur le matériel,
/// `preview` ce que la fenêtre regarde. La zone de notification, le journal et la
/// galerie ne lisent que le premier — voir [`PreviewStatus`] pour le pourquoi.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineReport {
    pub devices: Vec<DeviceEngineStatus>,
    /// `None` quand rien n'est prévisualisé — ce qui est le cas dès que la
    /// fenêtre est fermée, voir [`Engine::stop_preview`].
    pub preview: Option<PreviewStatus>,
}

// ---------------------------------------------------------------- sortie

/// La sortie matérielle d'une boucle.
///
/// Un trait plutôt que le [`Keyboard`] lui-même, pour deux raisons qui comptent
/// autant l'une que l'autre :
///
/// 1. **c'est le seul endroit où une boucle touche du matériel.** Le jour où un
///    gabarit couvrira plusieurs appareils, c'est ici que l'image se répartira ;
///    ni la boucle ni le code des effets n'auront à changer ;
/// 2. **un test peut faire échouer un appareil.** Sans ce joint, « un appareil
///    en panne n'en affecte aucun autre » ne serait vérifiable qu'avec deux
///    claviers branchés, donc jamais.
pub(crate) trait DeviceOut: Send {
    /// Écrit une image.
    ///
    /// `None` quand aucun appareil n'est ouvert. Ce n'est pas un échec : on
    /// écrit un effet sans posséder le clavier, et l'interface doit pouvoir le
    /// dire autrement qu'en erreur.
    fn present(&self, colors: &[Rgb]) -> Option<Result<(), String>>;
}

/// La poignée d'un appareil, partagée entre les commandes et sa boucle.
///
/// `Arc` parce que la boucle survit à la fenêtre : elle ne peut rien emprunter
/// à l'état d'une commande. `Option` parce que refermer un appareil — ignoré,
/// débranché — ne doit pas arrêter la boucle qui l'alimentait : elle s'en
/// aperçoit à l'image suivante et le signale par `reachingKeyboard`.
pub(crate) type Handle = Arc<Mutex<Option<Keyboard>>>;

impl DeviceOut for Handle {
    fn present(&self, colors: &[Rgb]) -> Option<Result<(), String>> {
        // Le verrou de la poignée est rendu **avant** que le résultat ne soit
        // consigné : une boucle ne tient jamais la poignée et un verrou du
        // moteur en même temps.
        let guard = self.lock().unwrap();
        let kb = guard.as_ref()?;
        Some(kb.present(colors).map_err(|e| e.to_string()))
    }
}

/// La sortie de l'aperçu : **aucune**.
///
/// Rien à inventer ici — `None` veut déjà dire « aucun appareil ouvert, ce n'est
/// pas un échec », et c'est exactement ce qu'est un aperçu. La boucle alimente
/// donc son canal d'images et rien d'autre, `reachingKeyboard` reste faux, et
/// aucun octet ne part vers un clavier.
struct SansSortie;

impl DeviceOut for SansSortie {
    fn present(&self, _colors: &[Rgb]) -> Option<Result<(), String>> {
        None
    }
}

/// À qui appartient une boucle de rendu.
///
/// Un type, et non un `bool` en plus du [`DeviceRef`] : les deux cas ne se
/// journalisent ni au même niveau ni sous le même mot, et « l'appareil de
/// l'aperçu » n'existe pas — il n'y a qu'un gabarit emprunté. Le compilateur
/// tient ici une distinction que deux arguments côte à côte laisseraient
/// confondre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cible {
    /// La boucle d'un appareil : elle écrit sur le matériel.
    Appareil(DeviceRef),
    /// La boucle d'aperçu, qui emprunte le gabarit de cet appareil sans le
    /// piloter.
    Apercu(DeviceRef),
}

// ---------------------------------------------------------------- moteur

/// Les boucles en cours, une par appareil.
///
/// La table ne porte que des `Arc` : on la verrouille le temps d'une recherche,
/// jamais le temps d'un démarrage ou d'une attente. Arrêter la boucle d'un
/// appareil ne retient donc aucune commande visant les autres — sans quoi une
/// écriture HID bloquée sur l'un gèlerait l'autre, et l'invariant de l'adoption
/// ne survivrait pas au moteur.
#[derive(Default)]
pub struct Engine {
    loops: Mutex<HashMap<DeviceRef, Arc<DeviceLoop>>>,
    /// La boucle d'aperçu : **une seule**, sans sortie matérielle.
    ///
    /// Une par fenêtre, et il n'y en a qu'une — la promesse « un effet tourne
    /// fenêtre fermée » ne vaut que pour les appareils, et un aperçu que
    /// personne ne regarde est un contexte QuickJS entretenu pour rien.
    ///
    /// Elle vit **à côté** de la table, jamais dedans : une entrée de la table
    /// serait trouvée par `all()`, donc arrêtée par `stop_everywhere`, comptée par
    /// `status()`, et il aurait fallu l'exclure à chaque fois. La sortir de la
    /// table, c'est faire tenir par le type ce qu'on aurait sinon tenu par
    /// vigilance.
    preview: Arc<DeviceLoop>,
    /// Le gabarit que l'aperçu emprunte, écrit et effacé avec la boucle.
    preview_layout: Mutex<Option<DeviceRef>>,
}

/// La boucle d'**un** appareil.
///
/// Deux verrous, et l'ordre entre eux est fixe : `thread` puis `shared`.
/// `thread` sérialise démarrage et arrêt ; `shared` n'est pris que le temps de
/// cloner ou de remplacer un `Arc`, jamais pendant une attente. C'est ce qui
/// permet de lire l'état d'un appareil pendant qu'un autre démarre — et même
/// pendant que celui-ci démarre.
#[derive(Default)]
struct DeviceLoop {
    thread: Mutex<Option<JoinHandle<()>>>,
    shared: Mutex<Option<Arc<Shared>>>,
}

impl DeviceLoop {
    fn current(&self) -> Option<Arc<Shared>> {
        self.shared.lock().unwrap().clone()
    }

    /// Vrai si c'est **cet** effet que la boucle fait tourner.
    ///
    /// L'identifiant est celui qu'on a demandé à `start`, pas une propriété du
    /// code chargé : le JavaScript est lu une fois au démarrage et vit ensuite en
    /// mémoire, il n'y a donc rien à relire pour le savoir — et c'est précisément
    /// pourquoi supprimer un effet ne se remarque pas tout seul.
    fn runs(&self, effect: &str) -> bool {
        self.current()
            .is_some_and(|s| s.effect_id.lock().unwrap().as_deref() == Some(effect))
    }

    fn status(&self) -> EngineStatus {
        match self.current() {
            None => EngineStatus::default(),
            Some(s) => EngineStatus {
                running: !s.stop.load(Ordering::Relaxed),
                effect_id: s.effect_id.lock().unwrap().clone(),
                error: s.error.lock().unwrap().clone(),
                device_error: s.device_error.lock().unwrap().clone(),
                reaching_keyboard: s.reaching.load(Ordering::Relaxed),
                to_keyboard: s.to_keyboard.load(Ordering::Relaxed),
            },
        }
    }

    /// Arrête la boucle et **attend** sa fin.
    ///
    /// L'attente n'est pas un détail : sans elle, démarrer un effet juste après
    /// en avoir arrêté un laisserait deux boucles écrire sur le même appareil le
    /// temps que la première s'aperçoive qu'elle doit s'arrêter. Le raisonnement
    /// vaut par appareil, et le verrou attendu l'est aussi.
    ///
    /// `cible` ne sert qu'au journal : une boucle ne connaît pas son appareil —
    /// elle reçoit un gabarit et une sortie — et « effet arrêté » sans dire lequel
    /// ne vaudrait rien avec deux claviers branchés.
    fn stop(&self, cible: Cible) {
        let mut thread = self.thread.lock().unwrap();
        let tournait = self.shared.lock().unwrap().take().inspect(|s| {
            s.stop.store(true, Ordering::Relaxed);
        });
        if let Some(h) = thread.take() {
            let _ = h.join();
        }
        // Seulement si quelque chose tournait : arrêter un appareil au repos est
        // le geste le plus courant de tous — chaque suppression d'effet y passe —
        // et n'apprend rien à personne.
        if let Some(s) = tournait {
            let effet = s.effect_id.lock().unwrap().clone();
            match cible {
                Cible::Appareil(d) => tracing::info!(appareil = %d, effet, "effet arrêté"),
                // `debug` : voir [`DeviceLoop::start`]. Le niveau « cycle de vie »
                // décrit ce que fait le clavier, et un aperçu ne le touche pas.
                Cible::Apercu(d) => tracing::debug!(gabarit = %d, effet, "aperçu arrêté"),
            }
        }
    }

    /// Démarre un effet sur cette cible. Remplace celui qui tournait.
    fn start(
        &self,
        cible: Cible,
        effect_id: String,
        js: String,
        params: String,
        layout: &'static Layout,
        out: Box<dyn DeviceOut>,
    ) -> Result<(), String> {
        // Gardé du début à la fin : c'est ce verrou qui interdit à deux boucles
        // de se chevaucher sur cet appareil. Il n'est pris qu'ici et dans
        // [`Self::stop`], et le fil de rendu ne le connaît pas.
        let mut thread = self.thread.lock().unwrap();
        if let Some(s) = self.shared.lock().unwrap().take() {
            s.stop.store(true, Ordering::Relaxed);
        }
        if let Some(h) = thread.take() {
            let _ = h.join();
        }

        let shared = Arc::new(Shared::default());
        *shared.params.lock().unwrap() = params;
        *shared.effect_id.lock().unwrap() = Some(effect_id.clone());

        // Le contexte JavaScript est bâti **dans** le fil et n'en sort jamais :
        // les types de QuickJS ne traversent pas les fils, et les enfermer ici
        // est plus sûr que de les rendre partageables.
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let s = Arc::clone(&shared);

        let pour_le_fil = effect_id.clone();
        let handle = std::thread::Builder::new()
            .name("candeo-effect".into())
            .spawn(move || render_loop(cible, pour_le_fil, s, js, layout, out, ready_tx))
            .map_err(|e| format!("impossible de démarrer le fil de rendu : {e}"))?;

        // On attend le verdict du chargement : une erreur de syntaxe doit
        // remonter à l'appel, pas se découvrir dans un état plus tard.
        match ready_rx.recv() {
            Ok(Ok(())) => {
                match cible {
                    Cible::Appareil(d) => {
                        tracing::info!(appareil = %d, effet = %effect_id, "effet démarré")
                    }
                    // `debug` et non `info`, et ce n'est pas une timidité :
                    // « cycle de vie » est le niveau qui décrit **ce que fait le
                    // clavier**, et un aperçu ne le touche pas. Parcourir la
                    // galerie remplirait sinon le journal de lignes qui ne
                    // correspondent à rien d'allumé.
                    Cible::Apercu(d) => {
                        tracing::debug!(gabarit = %d, effet = %effect_id, "aperçu démarré")
                    }
                }
                *self.shared.lock().unwrap() = Some(shared);
                *thread = Some(handle);
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = handle.join();
                match cible {
                    // `error` : l'effet demandé ne tournera pas, donc l'éclairage
                    // n'est pas celui qu'on a demandé. L'appelant reçoit le même
                    // message — le journal sert à qui lit après coup, et à qui n'a
                    // pas la fenêtre sous les yeux.
                    Cible::Appareil(d) => {
                        tracing::error!(appareil = %d, effet = %effect_id, "effet non démarré : {e}")
                    }
                    // `warn` : rien n'est cassé sur le matériel, mais l'écran ne
                    // montrera pas ce qu'on a demandé — et c'est justement le
                    // premier endroit où un effet fraîchement écrit se casse.
                    Cible::Apercu(d) => {
                        tracing::warn!(gabarit = %d, effet = %effect_id, "aperçu non démarré : {e}")
                    }
                }
                Err(e)
            }
            Err(_) => {
                let _ = handle.join();
                let e = "le fil de rendu s'est arrêté avant d'avoir chargé l'effet".to_string();
                tracing::error!(cible = ?cible, effet = %effect_id, "{e}");
                Err(e)
            }
        }
    }
}

impl Engine {
    /// La boucle de cet appareil, créée à l'arrêt si elle n'existait pas.
    ///
    /// Seul un démarrage en crée une. L'entrée n'est ensuite jamais retirée : un
    /// appareil qui a porté un effet garde sa ligne dans `engine_status()`,
    /// arrêté plutôt qu'absent. « Cet appareil ne fait rien » et « je ne sais
    /// rien de cet appareil » ne se disent pas pareil.
    fn device_loop(&self, device: DeviceRef) -> Arc<DeviceLoop> {
        Arc::clone(self.loops.lock().unwrap().entry(device).or_default())
    }

    /// La boucle de cet appareil, **sans en créer une**.
    ///
    /// Régler ou arrêter un appareil qui n'a jamais rien lancé ne fait rien, et
    /// ne doit surtout pas lui inventer une ligne d'état.
    fn existing(&self, device: DeviceRef) -> Option<Arc<DeviceLoop>> {
        self.loops.lock().unwrap().get(&device).map(Arc::clone)
    }

    /// Toutes les boucles, table déverrouillée.
    ///
    /// La copie n'est pas un détail : agir sur une boucle demande d'attendre la
    /// fin d'un fil, ce qu'on refuse de faire le verrou de la table à la main.
    fn all(&self) -> Vec<(DeviceRef, Arc<DeviceLoop>)> {
        let mut all: Vec<_> = self
            .loops
            .lock()
            .unwrap()
            .iter()
            .map(|(d, l)| (*d, Arc::clone(l)))
            .collect();
        // Une table de hachage n'ordonne rien, et une liste qui se réordonne à
        // chaque interrogation est illisible dans l'interface.
        all.sort_by_key(|(d, _)| (d.vid, d.pid));
        all
    }

    /// État de chaque appareil visé depuis le démarrage de l'application.
    ///
    /// **L'aperçu n'y figure pas, et ne peut pas y figurer** : il ne vit pas dans
    /// la table. C'est ce que lisent l'icône de zone de notification et le
    /// diagnostic — les deux endroits qui décrivent le matériel.
    pub fn device_status(&self) -> Vec<DeviceEngineStatus> {
        self.all()
            .into_iter()
            .map(|(device, l)| DeviceEngineStatus {
                device,
                status: l.status(),
            })
            .collect()
    }

    /// L'aperçu en cours, s'il y en a un.
    ///
    /// `None` dès que la boucle est arrêtée par [`Self::stop_preview`] : un
    /// aperçu est transitoire, et « le dernier effet que vous avez regardé » n'est
    /// une information pour personne. Un aperçu qui s'est coupé **tout seul** —
    /// trente images en échec — reste en revanche visible, `running` à faux et
    /// l'erreur avec : c'est la seule façon de savoir pourquoi l'écran s'est figé.
    pub fn preview_status(&self) -> Option<PreviewStatus> {
        let layout_of = (*self.preview_layout.lock().unwrap())?;
        let s = self.preview.current()?;
        let etat = PreviewStatus {
            layout_of,
            running: !s.stop.load(Ordering::Relaxed),
            effect_id: s.effect_id.lock().unwrap().clone(),
            error: s.error.lock().unwrap().clone(),
        };
        Some(etat)
    }

    /// Tout ce que le moteur sait, appareils et aperçu **séparés**.
    pub fn report(&self) -> EngineReport {
        EngineReport {
            devices: self.device_status(),
            preview: self.preview_status(),
        }
    }

    /// L'état partagé de la boucle en cours sur cet appareil, s'il y en a une.
    fn shared(&self, device: DeviceRef) -> Option<Arc<Shared>> {
        self.existing(device).and_then(|l| l.current())
    }

    pub fn stop(&self, device: DeviceRef) {
        if let Some(l) = self.existing(device) {
            l.stop(Cible::Appareil(device));
        }
    }

    /// Arrête **toutes** les boucles, aperçu compris, et attend leur fin.
    ///
    /// Sert à la fin du processus comme à la remise à zéro de la configuration :
    /// dans les deux cas on repart d'un état connu, et laisser tourner des
    /// boucles que plus rien ne désigne serait exactement le contraire. L'aperçu
    /// en fait partie — il n'écrit sur aucun clavier, mais il entretient un
    /// contexte QuickJS et un fil.
    pub fn stop_all(&self) {
        self.stop_preview();
        for (device, l) in self.all() {
            l.stop(Cible::Appareil(device));
        }
    }

    // ------------------------------------------------------------ aperçu

    /// Démarre — ou remplace — l'aperçu, **sans toucher à aucun appareil**.
    ///
    /// `layout_of` désigne l'appareil dont on emprunte le gabarit ; il n'est ni
    /// ouvert, ni piloté, ni même nécessairement branché. La sortie est
    /// [`SansSortie`] : aucun octet ne part vers un clavier, quoi qu'il arrive.
    ///
    /// Remplacer coûte un contexte QuickJS détruit et un autre construit. Ce
    /// n'est pas gratuit, et c'est pourquoi la cadence est bornée **du côté du
    /// geste** — la fenêtre attend que la sélection se pose avant d'appeler. La
    /// borner ici aurait obligé à choisir entre faire attendre la dernière
    /// sélection et la perdre, et la fenêtre aurait dû réconcilier ce qu'elle
    /// croyait avoir demandé avec ce qui tourne.
    pub fn start_preview(
        &self,
        layout_of: DeviceRef,
        effect_id: String,
        js: String,
        params: String,
        layout: &'static Layout,
    ) -> Result<(), String> {
        // Écrit **avant** le démarrage : si celui-ci échoue, la boucle est vide
        // et `preview_status` rend `None` de toute façon — alors qu'un gabarit
        // posé après coup manquerait pendant tout le chargement.
        *self.preview_layout.lock().unwrap() = Some(layout_of);
        self.preview.start(
            Cible::Apercu(layout_of),
            effect_id,
            js,
            params,
            layout,
            Box::new(SansSortie),
        )
    }

    /// Arrête l'aperçu. Aucun effet d'appareil n'est touché.
    pub fn stop_preview(&self) {
        // Le gabarit est relevé avant l'arrêt, pour que la ligne de journal
        // nomme celui qu'on empruntait plutôt que rien.
        let emprunte = self.preview_layout.lock().unwrap().take();
        if let Some(device) = emprunte {
            self.preview.stop(Cible::Apercu(device));
        }
    }

    pub fn set_preview_params(&self, params: String) {
        if let Some(s) = self.preview.current() {
            *s.params.lock().unwrap() = params;
        }
    }

    pub fn set_preview_channel(&self, channel: Option<Channel<InvokeResponseBody>>) {
        if let Some(s) = self.preview.current() {
            *s.frames.lock().unwrap() = channel;
        }
    }

    /// Arrête cet effet **partout où il tourne**, et rend les appareils touchés.
    ///
    /// Appelée avant la suppression d'un effet : la boucle exécute un `effect.js`
    /// chargé en mémoire au démarrage, elle continuerait donc sans la moindre
    /// erreur alors que son dossier n'existe plus — un appareil piloté par un
    /// effet absent de la bibliothèque.
    ///
    /// Tous les appareils, pas seulement celui qu'on regarde : le même effet se
    /// lance sur autant de claviers qu'on veut, et en oublier un le laisserait
    /// dans cet état invisible.
    ///
    /// La ligne d'état de l'appareil ne disparaît pas, mais elle cesse de nommer
    /// l'effet — c'est ce que fait [`DeviceLoop::stop`], et c'est bien ce qu'on
    /// veut ici : l'identifiant ne désigne plus rien.
    pub fn stop_everywhere(&self, effect: &str) -> Vec<DeviceRef> {
        // **L'aperçu aussi**, et pour exactement la même raison : il exécute le
        // même `effect.js` chargé en mémoire, et le laisser tourner donnerait un
        // écran qui anime un effet absent de la bibliothèque. Il ne figure pas
        // dans la liste rendue — aucun appareil n'a été touché.
        if self.preview.runs(effect) {
            self.stop_preview();
        }

        let mut stopped = Vec::new();
        for (device, l) in self.all() {
            // Le verrou de la table est déjà rendu — `all` a copié les pointeurs.
            // Un arrêt attend la fin d'un fil, et on ne fait jamais attendre une
            // commande visant un autre appareil.
            if l.runs(effect) {
                l.stop(Cible::Appareil(device));
                stopped.push(device);
            }
        }
        stopped
    }

    pub fn set_params(&self, device: DeviceRef, params: String) {
        if let Some(s) = self.shared(device) {
            *s.params.lock().unwrap() = params;
        }
    }

    pub fn set_to_keyboard(&self, device: DeviceRef, on: bool) {
        if let Some(s) = self.shared(device) {
            s.to_keyboard.store(on, Ordering::Relaxed);
        }
    }

    pub fn set_channel(&self, device: DeviceRef, channel: Option<Channel<InvokeResponseBody>>) {
        if let Some(s) = self.shared(device) {
            *s.frames.lock().unwrap() = channel;
        }
    }

    /// Démarre un effet sur un appareil. Remplace celui qui y tournait.
    ///
    /// Les autres appareils ne sont pas touchés — ni leur boucle, ni leur
    /// cadence, ni leur état d'erreur.
    pub fn start(
        &self,
        device: DeviceRef,
        effect_id: String,
        js: String,
        params: String,
        layout: &'static Layout,
        out: Box<dyn DeviceOut>,
    ) -> Result<(), String> {
        self.device_loop(device)
            .start(Cible::Appareil(device), effect_id, js, params, layout, out)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop_all();
    }
}

/// Prépare le contexte QuickJS, puis tourne jusqu'à l'arrêt.
fn render_loop(
    cible: Cible,
    effect_id: String,
    shared: Arc<Shared>,
    js: String,
    layout: &'static Layout,
    out: Box<dyn DeviceOut>,
    ready: std::sync::mpsc::Sender<Result<(), String>>,
) {
    // **Le span, et c'est la raison d'avoir choisi `tracing`.** Il y a une boucle
    // par appareil : « écriture refusée » ne sert à rien sans savoir laquelle.
    // Ouvert ici, il porte l'appareil et l'effet jusqu'à la fin du fil, et tout ce
    // qui se journalise en dessous — y compris dans [`emit`] — les porte aussi,
    // sans qu'un seul appel n'ait à les passer.
    //
    // Deux noms, et non un champ à lire : dans un journal relu après coup,
    // « rendu » et « aperçu » doivent se distinguer d'un coup d'œil — une erreur
    // d'effet dans l'un n'a pas éteint le clavier, dans l'autre si.
    let span = match cible {
        Cible::Appareil(d) => tracing::info_span!("rendu", appareil = %d, effet = %effect_id),
        Cible::Apercu(d) => tracing::info_span!("aperçu", gabarit = %d, effet = %effect_id),
    };
    let _entree = span.enter();

    let frame_len = layout.led_count();

    // Le budget naît ici et ne sort pas du fil : le gestionnaire d'interruption
    // ne traverse aucune frontière, et l'échéance n'est écrite que par cette
    // boucle, juste avant chaque exécution de code d'effet.
    let budget = Rc::new(Budget::default());

    // Le chargement a la sienne : le corps du module tourne une fois, avant la
    // première image, donc hors de tout budget d'image.
    budget.accorder(BUDGET_CHARGEMENT);

    // `_rt` doit vivre aussi longtemps que le contexte : c'est lui qui porte
    // le résolveur de modules. Le laisser tomber ici rendrait tout `import`
    // introuvable à la première image.
    let (_rt, ctx) = match prepare_budgeted(&js, layout, &budget) {
        Ok(c) => {
            let _ = ready.send(Ok(()));
            c
        }
        Err(e) => {
            let _ = ready.send(Err(nommer_la_cause(
                e,
                &budget,
                "le chargement de l'effet",
                &format!("{} s", BUDGET_CHARGEMENT.as_secs()),
            )));
            return;
        }
    };

    let period = Duration::from_nanos(1_000_000_000 / u64::from(FPS));
    let started = Instant::now();
    let mut deadline = Instant::now();
    let mut frame_index: u32 = 0;
    let mut consecutive_errors: u32 = 0;

    while !shared.stop.load(Ordering::Relaxed) {
        let params = shared.params.lock().unwrap().clone();
        let time = started.elapsed().as_secs_f64();

        // L'échéance est renouvelée avant **chaque** image : c'est tout l'objet
        // de la cellule partagée. Le gestionnaire, lui, a été posé une fois pour
        // toutes sur le `Runtime`.
        budget.accorder(BUDGET_IMAGE);

        match render_once(&ctx, time, frame_index, &params, frame_len) {
            Ok(bytes) => {
                consecutive_errors = 0;
                // L'effet s'est rétabli : on efface, sinon l'interface
                // afficherait une erreur périmée indéfiniment.
                let avant = shared.error.lock().unwrap().take();
                if journal::bascule(avant.as_deref(), None) == journal::Bascule::Retabli {
                    tracing::info!("l'effet s'est rétabli");
                }
                emit(&shared, out.as_ref(), &bytes);
            }
            Err(e) => {
                // Un dépassement n'ouvre aucun chemin nouveau : c'est une erreur
                // d'image comme une autre, que le compteur ci-dessous finit par
                // transformer en arrêt propre.
                let e = nommer_la_cause(
                    e,
                    &budget,
                    "l'effet",
                    &format!("{} ms par image", BUDGET_IMAGE.as_millis()),
                );
                consecutive_errors += 1;
                // **Une ligne au début de la panne, pas une par image.** À 30
                // images par seconde, journaliser chaque échec produirait trente
                // lignes par seconde et enterrerait celle qui nomme la cause.
                // C'est [`journal::bascule`] qui tient la règle.
                let avant = shared.error.lock().unwrap().replace(e.clone());
                if journal::bascule(avant.as_deref(), Some(&e)) == journal::Bascule::Commence {
                    tracing::warn!("l'effet a commencé à échouer : {e}");
                }
                if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                    tracing::error!(
                        echecs = MAX_CONSECUTIVE_ERRORS,
                        "effet arrêté après {MAX_CONSECUTIVE_ERRORS} échecs consécutifs : {e}"
                    );
                    shared.stop.store(true, Ordering::Relaxed);
                    break;
                }
            }
        }

        frame_index = frame_index.wrapping_add(1);

        // Échéance absolue plutôt que `sleep(period)` : une image lente ne doit
        // pas décaler toutes les suivantes. Si on a pris du retard, on repart
        // de maintenant au lieu d'essayer de le rattraper en accéléré.
        deadline += period;
        let now = Instant::now();
        if deadline > now {
            std::thread::sleep(deadline - now);
        } else {
            deadline = now;
        }
    }
}

/// L'échéance que le gestionnaire d'interruption consulte — et ce qu'il en a
/// fait.
///
/// Une cellule partagée, et non une échéance capturée : le gestionnaire se pose
/// sur le `Runtime` **une fois**, alors que l'échéance change à **chaque image**.
/// [`prepare_bounded`], qui n'en accorde qu'une pour toute une installation,
/// peut se contenter de la capturer ; la boucle de rendu, non.
///
/// Le drapeau sert à **nommer la cause**. QuickJS lève la même
/// « InternalError: interrupted » quelle que soit la raison d'une interruption,
/// et c'est le gestionnaire — lui seul — qui sait que c'est l'échéance qui l'a
/// fait lever.
#[derive(Default)]
struct Budget {
    deadline: Cell<Option<Instant>>,
    depasse: Cell<bool>,
}

impl Budget {
    /// Accorde `duree` à l'exécution qui suit, drapeau baissé.
    fn accorder(&self, duree: Duration) {
        self.deadline.set(Some(Instant::now() + duree));
        self.depasse.set(false);
    }

    /// Le gestionnaire lui-même : `true` coupe l'exécution en cours.
    ///
    /// QuickJS l'appelle toutes les 10 000 instructions — un `Instant::now()` à
    /// cette fréquence ne se mesure pas.
    fn expire(&self) -> bool {
        match self.deadline.get() {
            Some(fin) if Instant::now() >= fin => {
                self.depasse.set(true);
                true
            }
            _ => false,
        }
    }
}

/// Contexte JavaScript prêt à rendre, **sans borne de temps**.
///
/// Réservé aux tests. Depuis que la boucle de rendu borne chaque image et que
/// l'échantillonnage borne son installation, plus aucun appelant de production
/// ne prépare un contexte qu'un `while (true)` pourrait figer — c'est tout
/// l'objet des deux budgets. Restent les tests qui ne portent pas sur les
/// bornes, et auxquels une échéance n'ajouterait qu'un aléa de machine.
///
/// Le `Runtime` est renvoyé avec le contexte, et non gardé ici : c'est lui qui
/// porte le résolveur de modules, il doit donc vivre aussi longtemps.
#[cfg(test)]
fn prepare(js: &str, layout: &'static Layout) -> Result<(Runtime, Context), String> {
    prepare_bounded(js, layout, None)
}

/// Comme [`prepare_with`], avec une échéance **unique**, capturée par valeur.
///
/// Elle couvre tout ce que l'appelant fera du contexte, du chargement à la
/// dernière image. C'est ce dont a besoin l'échantillonnage du repère —
/// quelques images, une seule borne, dans le fil d'une commande où un
/// `while (true)` empêcherait une installation d'aboutir. La boucle de rendu,
/// elle, en change à chaque image : voir [`Budget`].
///
/// L'échéance est posée **avant** toute évaluation, pour couvrir aussi le corps
/// du module.
fn prepare_bounded(
    js: &str,
    layout: &'static Layout,
    deadline: Option<Instant>,
) -> Result<(Runtime, Context), String> {
    let interrupt =
        deadline.map(|fin| -> InterruptHandler { Box::new(move || Instant::now() >= fin) });
    prepare_with(js, layout, interrupt)
}

/// Comme [`prepare_with`], mais borné **appel par appel** : le gestionnaire lit
/// un budget que l'appelant renouvelle avant chaque exécution.
fn prepare_budgeted(
    js: &str,
    layout: &'static Layout,
    budget: &Rc<Budget>,
) -> Result<(Runtime, Context), String> {
    let budget = Rc::clone(budget);
    prepare_with(js, layout, Some(Box::new(move || budget.expire())))
}

/// Le tronc commun, pour un gabarit du dépôt : il sérialise, puis délègue.
fn prepare_with(
    js: &str,
    layout: &'static Layout,
    interrupt: Option<InterruptHandler>,
) -> Result<(Runtime, Context), String> {
    prepare_with_layout(js, layout.led_count(), layout_json(layout), interrupt)
}

/// Le tronc : bornes posées, modules résolus, `__candeo_render` installé, pour
/// un gabarit **déjà sérialisé**.
///
/// Les deux bornes sont posées **avant** toute évaluation — le corps du module
/// est du code d'effet comme un autre.
///
/// Le gabarit arrive en JSON plutôt qu'en [`Layout`] pour une raison qui ne se
/// voit qu'aux tests : `candeo_device::Key` porte un rectangle **obligatoire**,
/// donc aucun gabarit de ce dépôt ne peut décrire un appareil sans géométrie
/// relevée — et c'est pourtant le cas qu'un effet qui mesure des distances
/// physiques doit refuser en le disant. L'écrire à la main est le seul moyen de
/// vérifier ce refus tant que #34 n'a pas rendu le rectangle facultatif.
fn prepare_with_layout(
    js: &str,
    frame_len: usize,
    layout_json: String,
    interrupt: Option<InterruptHandler>,
) -> Result<(Runtime, Context), String> {
    let rt = Runtime::new().map_err(|e| format!("QuickJS : {e}"))?;

    rt.set_interrupt_handler(interrupt);

    // La borne mémoire, elle, ne se renouvelle pas et vaut pour tous les
    // appelants : une allocation sans fin emporte le processus entier, qu'elle
    // survienne dans une boucle de rendu ou pendant l'installation d'un effet.
    rt.set_memory_limit(BUDGET_MEMOIRE);

    // `@candeo/effects-api` est **interne**. C'est ce qui permet d'écrire un
    // `import` normal sans bundler, sans résolution de chemins et sans
    // `node_modules`.
    let resolver = BuiltinResolver::default()
        .with_module("@candeo/effects-api")
        .with_module("effect");
    let loader = BuiltinLoader::default()
        .with_module("@candeo/effects-api", API_JS)
        .with_module("effect", js);
    rt.set_loader(resolver, loader);

    let ctx = Context::full(&rt).map_err(|e| format!("QuickJS : {e}"))?;

    ctx.with(|ctx| -> Result<(), String> {
        let g = ctx.globals();
        g.set("__candeo_frame_len", frame_len as u32)
            .map_err(js_error)?;
        g.set("__candeo_layout", layout_json).map_err(js_error)?;

        Module::evaluate(ctx.clone(), "bootstrap", BOOTSTRAP_JS)
            .catch(&ctx)
            .map_err(|e| format!("chargement de l'effet : {e}"))?
            .finish::<()>()
            .catch(&ctx)
            .map_err(|e| format!("chargement de l'effet : {e}"))?;
        Ok(())
    })?;

    Ok((rt, ctx))
}

fn render_once(
    ctx: &Context,
    time: f64,
    frame_index: u32,
    params: &str,
    frame_len: usize,
) -> Result<Vec<u8>, String> {
    ctx.with(|ctx| {
        let render: Function = ctx
            .globals()
            .get("__candeo_render")
            .map_err(|_| "la fonction de rendu a disparu du contexte".to_string())?;

        let out: Vec<u8> = render
            .call((time, frame_index, params))
            .catch(&ctx)
            .map_err(|e| format!("{e}"))?;

        if out.len() != frame_len * 3 {
            return Err(format!(
                "l'effet a rendu {} octets, {} attendus",
                out.len(),
                frame_len * 3
            ));
        }
        Ok(out)
    })
}

/// Le texte que QuickJS lève quand la limite mémoire est atteinte, et le seul
/// signe qu'il en donne (`JS_ThrowOutOfMemory`).
const OOM_QUICKJS: &str = "out of memory";

/// Traduit un échec d'exécution bornée en une cause que l'auteur de l'effet
/// peut relier à **son** code.
///
/// QuickJS ne nomme aucune des deux bornes : une interruption remonte en
/// « InternalError: interrupted », qui ne désigne rien, et un dépassement
/// mémoire en « out of memory », qui ne dit ni de qui ni de combien. Les deux
/// bornes sont posées ici ; c'est donc ici, et nulle part ailleurs, qu'on sait
/// les expliquer.
///
/// Le temps se lit sur le drapeau du budget, jamais sur le message : c'est le
/// gestionnaire qui a coupé, il est seul à le savoir de source sûre. La mémoire
/// n'a que le texte de QuickJS — d'où la comparaison, et le repli sur l'erreur
/// brute quand elle ne dit rien : mal nommer une cause serait pire que de ne pas
/// la nommer.
///
/// `sujet` distingue les deux endroits bornés — une image, un chargement. La
/// boucle qui ne se termine pas n'est pas au même endroit du fichier, et le
/// temps accordé n'est pas le même.
fn nommer_la_cause(erreur: String, budget: &Budget, sujet: &str, accorde: &str) -> String {
    if budget.depasse.get() {
        return format!(
            "{sujet} a dépassé son temps de calcul ({accorde}) : \
             une boucle qui ne se termine pas, ou un calcul trop lourd."
        );
    }
    if erreur.contains(OOM_QUICKJS) {
        return format!(
            "{sujet} a dépassé la mémoire qui lui est accordée ({} Mo) : \
             un état qui grandit à chaque image, ou une allocation démesurée.",
            BUDGET_MEMOIRE / (1024 * 1024)
        );
    }
    erreur
}

/// Les deux sorties, indépendantes : chacune peut être absente.
fn emit(shared: &Shared, out: &dyn DeviceOut, bytes: &[u8]) {
    if !shared.to_keyboard.load(Ordering::Relaxed) {
        // Sortie coupée volontairement : ce n'est pas un défaut, mais les
        // images n'atteignent aucun clavier et l'interface doit pouvoir le dire.
        shared.reaching.store(false, Ordering::Relaxed);
    } else {
        let colors: Vec<Rgb> = bytes
            .chunks_exact(3)
            .map(|c| Rgb::new(c[0], c[1], c[2]))
            .collect();
        // Un clavier débranché en cours de route n'arrête pas l'effet : le
        // simulateur continue, et la reconnexion passe par les commandes
        // existantes. Mais l'échec est **consigné**, pas avalé — le taire
        // donnait une boucle qui se dit saine pendant qu'aucun octet n'atteint
        // l'appareil. Et l'échec de celui-ci ne dit rien des autres : chaque
        // boucle écrit dans son propre état.
        match out.present(&colors) {
            // **Aucun périphérique ouvert.** Sans ce signalement, lancer un
            // effet sans clavier connecté ne produisait aucun signe : le
            // simulateur s'animait, la case « envoyer » restait cochée, et le
            // clavier gardait son image précédente. On lisait ça comme « seule
            // la première image est passée ».
            // Rien à journaliser : écrire un effet **sans posséder le clavier**
            // est un usage prévu, pas une panne. Le dire par image le noierait,
            // et le dire une fois ferait passer pour un incident ce que
            // `reachingKeyboard` rend déjà visible à l'écran.
            None => shared.reaching.store(false, Ordering::Relaxed),
            Some(Ok(())) => {
                let avant = shared.device_error.lock().unwrap().take();
                if journal::bascule(avant.as_deref(), None) == journal::Bascule::Retabli {
                    tracing::info!("l'écriture vers l'appareil est rétablie");
                }
                shared.reaching.store(true, Ordering::Relaxed);
            }
            Some(Err(e)) => {
                // Même règle que pour l'erreur d'effet : le début de la panne,
                // et rien d'autre. Un clavier débranché en cours de route
                // échouerait à chaque image jusqu'à ce qu'on le rebranche.
                let avant = shared.device_error.lock().unwrap().replace(e.clone());
                if journal::bascule(avant.as_deref(), Some(&e)) == journal::Bascule::Commence {
                    tracing::warn!("l'écriture vers l'appareil a commencé à échouer : {e}");
                }
                shared.reaching.store(false, Ordering::Relaxed);
            }
        }
    }

    // Binaire brut : sérialiser en tableau JSON d'entiers ferait passer une
    // image de 396 octets à plus de 1,5 Ko, soixante fois par seconde.
    if let Some(ch) = shared.frames.lock().unwrap().as_ref() {
        let _ = ch.send(InvokeResponseBody::Raw(bytes.to_vec()));
    }
}

/// Le gabarit tel que l'effet le voit.
///
/// Sérialisé à la main : `candeo-device` n'a pas serde, et c'est délibéré —
/// ses tests tournent sans dépendance système.
///
/// # Deux espaces, et les deux voyagent
///
/// `row`/`col` situent la LED dans la matrice ; `x`/`y`/`w`/`h` donnent le
/// rectangle du capuchon, en unités de pas de clavier — la même unité que
/// [`candeo_device::Key`], sans conversion en chemin. Un effet qui parle de
/// distance ne peut pas être juste sans le second : une case de matrice vaut une
/// case, que la touche fasse 1 u ou 6,25 u.
///
/// La géométrie est **toujours** émise ici, parce que `candeo_device::Key` la
/// porte toujours. Le jour où le rectangle deviendra facultatif (#34), ces
/// quatre champs disparaîtront pour les gabarits non dessinés, et les effets qui
/// les lisent échoueront en le disant — voir `bounds` et `center` dans `api.js`.
fn layout_json(l: &'static Layout) -> String {
    let mut keys = String::new();
    for row in 0..l.rows {
        for col in 0..l.cols {
            let Some(index) = l.at(row, col) else {
                continue;
            };
            let Some(k) = l.key(index) else { continue };
            if !keys.is_empty() {
                keys.push(',');
            }
            keys.push_str(&format!(
                r#"{{"index":{index},"row":{row},"col":{col},"label":{},"x":{},"y":{},"w":{},"h":{}}}"#,
                json_string(k.name),
                k.x,
                k.y,
                k.w,
                k.h
            ));
        }
    }
    format!(
        r#"{{"name":{},"rows":{},"cols":{},"keys":[{keys}]}}"#,
        json_string(l.name),
        l.rows,
        l.cols
    )
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn js_error(e: rquickjs::Error) -> String {
    format!("QuickJS : {e}")
}

// ---------------------------------------------------------------- commandes

use crate::{AppState, CmdResult, DeviceRef};
use tauri::{AppHandle, State};

/// Démarre un effet, intégré ou installé, **sur un appareil**.
///
/// La résolution `identifiant → JavaScript` est celle de la bibliothèque, donc
/// les intégrés d'abord : voir [`crate::storage`]. Le moteur, lui, ne fait
/// aucune différence — un effet livré est un module chargé exactement comme
/// celui qu'on vient d'écrire.
///
/// Le gabarit vient de **l'appareil visé**, ouvert ou non. C'est délibéré : on
/// doit pouvoir écrire et prévisualiser un effet **sans posséder le clavier**,
/// et le gabarit d'un appareil ne dépend pas de sa présence. Viser un appareil
/// débranché lance donc l'effet, alimente le simulateur, et `reachingKeyboard`
/// reste faux jusqu'à l'ouverture.
///
/// # C'est le geste qui **engage** le clavier
///
/// Prévisualiser est l'autre chemin, et il ne passe pas par ici :
/// [`start_preview`] n'ouvre aucune sortie matérielle et n'écrit rien sur disque.
/// « Appliquer » fait les deux — il envoie au clavier, et il **retient** l'effet
/// pour cet appareil.
#[tauri::command]
pub fn start_effect(
    app: AppHandle,
    state: State<'_, AppState>,
    device: DeviceRef,
    id: String,
    params: serde_json::Value,
) -> CmdResult<()> {
    let layout = crate::find_layout(device)?;
    let js = crate::storage::store(&app)?.effect_js(&id)?;

    let params = serde_json::to_string(&params)
        .map_err(|e| format!("paramètres non sérialisables : {e}"))?;

    // La poignée est partagée avec la boucle, pas copiée : refermer l'appareil
    // plus tard — ignoré, débranché — se voit à l'image suivante.
    let out = Box::new(state.handle(device));
    state
        .engine
        .start(device, id.clone(), js, params, layout, out)?;

    // **Après** le démarrage, jamais avant : on ne retient que ce qui tourne
    // vraiment. Un effet dont le chargement échoue ne doit pas laisser derrière
    // lui un identifiant que le fichier présente comme appliqué.
    retenir_l_effet_actif(&app, device, Some(&id));
    Ok(())
}

#[tauri::command]
pub fn stop_effect(app: AppHandle, state: State<'_, AppState>, device: DeviceRef) {
    state.engine.stop(device);
    retenir_l_effet_actif(&app, device, None);
}

/// Retient — ou oublie, avec `None` — l'effet appliqué sur cet appareil.
///
/// # Un échec est journalisé, pas remonté
///
/// L'effet tourne, le clavier est éclairé : faire échouer « Appliquer » parce que
/// le disque n'a pas pris note ferait payer à l'éclairage un incident qui ne le
/// concerne pas. Ce qu'on perd est borné et se dit en une ligne — le fichier ne
/// reprendra pas cet effet plus tard.
pub(crate) fn retenir_l_effet_actif(app: &AppHandle, device: DeviceRef, effect: Option<&str>) {
    let ecrire = || -> CmdResult<()> {
        let store = crate::storage::store(app)?;
        let mut settings = store.read_settings()?;
        // Rien de neuf : on ne repasse pas par le fichier temporaire et son
        // renommage. Relancer deux fois le même effet est un double-clic.
        if settings.set_active_effect(device.vid, device.pid, effect) {
            store.write_settings(&settings)?;
        }
        Ok(())
    };
    if let Err(e) = ecrire() {
        tracing::warn!(appareil = %device, "effet appliqué non retenu : {e}");
    }
}

// ---------------------------------------------------------------- aperçu

/// Démarre l'aperçu d'un effet, **sans toucher au clavier ni au disque**.
///
/// C'est le pendant exact de [`start_effect`], moins tout ce qui engage :
/// aucune sortie matérielle, aucune écriture dans `settings.json`, et surtout
/// **aucune boucle d'appareil arrêtée**. Sélectionner un effet dans la galerie
/// passe par ici ; l'effet qui tourne sur le clavier continue de tourner.
///
/// `device` désigne l'appareil dont on **emprunte le gabarit**. `None` retombe
/// sur le gabarit par défaut : on prévisualise sans posséder le clavier, et sans
/// en avoir adopté aucun — c'est la même raison qui fait exister
/// `get_default_layout`.
#[tauri::command]
pub fn start_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    device: Option<DeviceRef>,
    id: String,
    params: serde_json::Value,
) -> CmdResult<()> {
    let layout = match device {
        Some(d) => crate::find_layout(d)?,
        None => crate::default_layout(),
    };
    let js = crate::storage::store(&app)?.effect_js(&id)?;
    let params = serde_json::to_string(&params)
        .map_err(|e| format!("paramètres non sérialisables : {e}"))?;

    state
        .engine
        .start_preview(DeviceRef::of(layout), id, js, params, layout)
}

#[tauri::command]
pub fn stop_preview(state: State<'_, AppState>) {
    state.engine.stop_preview();
}

/// Ajuste les paramètres de l'aperçu à chaud, comme [`set_effect_params`] le
/// fait pour un appareil. La boucle relit le JSON à chaque image.
#[tauri::command]
pub fn set_preview_params(state: State<'_, AppState>, params: serde_json::Value) -> CmdResult<()> {
    let params = serde_json::to_string(&params)
        .map_err(|e| format!("paramètres non sérialisables : {e}"))?;
    state.engine.set_preview_params(params);
    Ok(())
}

/// Ouvre le flux d'images de l'aperçu vers le simulateur.
///
/// Un canal distinct de celui des appareils, et c'est ce qui permet de regarder
/// un effet pendant qu'un autre tourne sur le clavier : les deux flux existent en
/// même temps, et la fenêtre choisit lequel elle affiche.
#[tauri::command]
pub fn subscribe_preview_frames(state: State<'_, AppState>, channel: Channel<InvokeResponseBody>) {
    state.engine.set_preview_channel(Some(channel));
}

#[tauri::command]
pub fn unsubscribe_preview_frames(state: State<'_, AppState>) {
    state.engine.set_preview_channel(None);
}

/// Ajuste les paramètres à chaud. La boucle ne redémarre pas : elle relit le
/// JSON à chaque image.
#[tauri::command]
pub fn set_effect_params(
    state: State<'_, AppState>,
    device: DeviceRef,
    params: serde_json::Value,
) -> CmdResult<()> {
    let params = serde_json::to_string(&params)
        .map_err(|e| format!("paramètres non sérialisables : {e}"))?;
    state.engine.set_params(device, params);
    Ok(())
}

/// Active ou coupe la sortie clavier d'un appareil, sans toucher au simulateur.
#[tauri::command]
pub fn set_output_to_keyboard(state: State<'_, AppState>, device: DeviceRef, on: bool) {
    state.engine.set_to_keyboard(device, on);
}

/// Ouvre le flux d'images d'**un appareil** vers le simulateur.
///
/// Un canal, et non un événement global : la destination est connue, la portée
/// est explicite, et le binaire passe brut. Libérer le canal côté front, ou
/// appeler [`unsubscribe_frames`], arrête le flux **sans arrêter l'effet**, qui
/// continue d'alimenter le clavier fenêtre fermée.
///
/// Le simulateur suit l'appareil sélectionné : changer de sélection, c'est se
/// réabonner ailleurs, pas multiplexer un flux unique.
#[tauri::command]
pub fn subscribe_frames(
    state: State<'_, AppState>,
    device: DeviceRef,
    channel: Channel<InvokeResponseBody>,
) {
    state.engine.set_channel(device, Some(channel));
}

#[tauri::command]
pub fn unsubscribe_frames(state: State<'_, AppState>, device: DeviceRef) {
    state.engine.set_channel(device, None);
}

/// État du moteur : ce qui tourne **sur les appareils**, et ce qu'on **regarde**.
///
/// Interrogé plutôt que poussé : une erreur survenue fenêtre fermée doit
/// pouvoir être lue à la réouverture, ce qu'un événement ponctuel ne permet
/// pas.
///
/// `devices` porte une entrée par appareil visé depuis le démarrage — pas
/// seulement par appareil ouvert, ni par boucle en cours : « cet appareil ne fait
/// rien » et « je ne sais rien de cet appareil » ne se disent pas pareil.
///
/// `preview` est **à part**, et l'interface ne peut pas les confondre : voir
/// [`PreviewStatus`].
#[tauri::command]
pub fn engine_status(state: State<'_, AppState>) -> EngineReport {
    state.engine.report()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU32;

    use super::*;

    /// Un effet minimal, écrit comme l'utilisateur l'écrirait.
    const EFFET: &str = r#"
        import { hsv } from '@candeo/effects-api'
        export default {
          name: 'Test',
          render({ layout, time, frame, params }) {
            for (const key of layout.keys) {
              frame.set(key, hsv(time * Number(params.speed ?? 0) + key.col * 10, 1, 1))
            }
          },
        }
    "#;

    fn layout() -> &'static Layout {
        &candeo_device::DEATHSTALKER_V2_PRO
    }

    /// `unwrap_err` exigerait que `(Runtime, Context)` soit `Debug`, ce que
    /// rquickjs ne fournit pas.
    fn erreur_de_chargement(js: &str) -> String {
        match prepare(js, layout()) {
            Ok(_) => panic!("le chargement aurait dû échouer"),
            Err(e) => e,
        }
    }

    // ------------------------------------------------------------ un appareil

    /// Deux appareils inventés : le seul gabarit réel est unique, et tout ce qui
    /// est « par appareil » n'a de sens qu'à partir de deux. Ils ne servent qu'à
    /// être distingués — la géométrie, elle, reste celle du vrai gabarit, pour
    /// que les effets rendent de vraies images.
    const PREMIER: DeviceRef = DeviceRef {
        vid: 0x1532,
        pid: 0x1111,
    };
    const SECOND: DeviceRef = DeviceRef {
        vid: 0x1532,
        pid: 0x2222,
    };

    /// Au-delà, on considère que la condition attendue ne viendra pas.
    ///
    /// Généreux, et à dessein : à 30 images par seconde quelques images tiennent
    /// dans quelques dizaines de millisecondes, mais la cadence d'un coureur
    /// d'intégration continue n'est pas celle d'une machine de développement.
    /// Un test qui dort une durée choisie serait soit lent, soit capricieux.
    const PATIENCE: Duration = Duration::from_secs(5);

    /// Une sortie d'appareil pilotée depuis le test.
    ///
    /// C'est tout l'intérêt de [`DeviceOut`] : faire échouer **un** appareil sans
    /// en brancher aucun. Avec le `Keyboard` en dur, l'invariant « un appareil en
    /// panne n'en affecte aucun autre » n'aurait été vérifiable qu'avec deux
    /// claviers sur le bureau, donc jamais.
    #[derive(Default)]
    struct Sortie {
        ecrites: AtomicU32,
        en_panne: AtomicBool,
    }

    impl DeviceOut for Arc<Sortie> {
        fn present(&self, _colors: &[Rgb]) -> Option<Result<(), String>> {
            if self.en_panne.load(Ordering::Relaxed) {
                return Some(Err("écriture refusée par l'appareil".into()));
            }
            self.ecrites.fetch_add(1, Ordering::Relaxed);
            Some(Ok(()))
        }
    }

    fn demarrer(engine: &Engine, device: DeviceRef, effect_id: &str, out: Arc<Sortie>) {
        engine
            .start(
                device,
                effect_id.into(),
                EFFET.into(),
                "{}".into(),
                layout(),
                Box::new(out),
            )
            .expect("démarrage");
    }

    /// L'état d'un appareil, extrait de la liste que rend le moteur.
    fn etat(engine: &Engine, device: DeviceRef) -> EngineStatus {
        engine
            .device_status()
            .into_iter()
            .find(|s| s.device == device)
            .unwrap_or_else(|| panic!("aucun état pour {device}"))
            .status
    }

    /// Attend qu'une condition se réalise, ou échoue. Voir [`PATIENCE`].
    fn attendre(quoi: &str, pret: impl FnMut() -> bool) {
        attendre_au_plus(PATIENCE, quoi, pret);
    }

    /// Comme [`attendre`], mais avec une patience calculée sur la borne qu'on
    /// éprouve.
    ///
    /// [`PATIENCE`] est une durée fixe, choisie pour des conditions qui se
    /// réalisent en quelques images. Une borne, elle, promet une durée : l'arrêt
    /// d'un effet qui boucle demande [`MAX_CONSECUTIVE_ERRORS`] images coupées,
    /// et une image coupée dure tout le [`BUDGET_IMAGE`] — lequel n'est pas le
    /// même selon le profil de compilation. Attendre un multiple de ce qu'on
    /// éprouve garde le test juste dans les deux cas.
    fn attendre_au_plus(patience: Duration, quoi: &str, mut pret: impl FnMut() -> bool) {
        let limite = Instant::now() + patience;
        while Instant::now() < limite {
            if pret() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("{quoi} : rien en {patience:?}");
    }

    /// **L'invariant de l'adoption, au niveau du moteur.**
    ///
    /// `un_appareil_en_echec_n_en_bloque_aucun_autre` le vérifie à l'ouverture ;
    /// celui-ci le vérifie une fois les boucles lancées, là où l'appareil tombe
    /// en marche. Un appareil dont toute écriture échoue ne doit rien retirer aux
    /// autres : ni leur boucle, ni leurs images, ni leur état — et l'arrêter ne
    /// doit pas les arrêter.
    #[test]
    fn un_appareil_en_panne_n_en_affecte_aucun_autre() {
        let engine = Engine::default();
        let panne = Arc::new(Sortie::default());
        panne.en_panne.store(true, Ordering::Relaxed);
        let sain = Arc::new(Sortie::default());

        demarrer(&engine, PREMIER, "casse", Arc::clone(&panne));
        demarrer(&engine, SECOND, "sain", Arc::clone(&sain));

        attendre("le voisin sain n'écrit rien", || {
            sain.ecrites.load(Ordering::Relaxed) >= 3
        });

        let casse = etat(&engine, PREMIER);
        assert!(
            casse.running,
            "la boucle de l'appareil en panne s'est arrêtée"
        );
        assert!(
            casse.device_error.is_some(),
            "l'échec d'écriture n'a pas été consigné"
        );
        assert!(!casse.reaching_keyboard);
        assert_eq!(panne.ecrites.load(Ordering::Relaxed), 0);

        let ok = etat(&engine, SECOND);
        assert!(ok.running, "la boucle du voisin s'est arrêtée");
        assert_eq!(ok.device_error, None, "l'échec du voisin a débordé");
        assert!(ok.reaching_keyboard, "le voisin n'est plus atteint");
        assert_eq!(
            ok.error, None,
            "erreur d'effet sur le voisin : {:?}",
            ok.error
        );

        // Arrêter l'appareil en panne laisse l'autre tourner : les boucles ne
        // partagent ni fil, ni verrou, ni état.
        let avant = sain.ecrites.load(Ordering::Relaxed);
        engine.stop(PREMIER);
        attendre("le voisin s'est arrêté avec son camarade", || {
            sain.ecrites.load(Ordering::Relaxed) > avant
        });
        assert!(!etat(&engine, PREMIER).running);
        assert!(etat(&engine, SECOND).running);

        engine.stop(SECOND);
    }

    /// Chaque appareil porte son effet et sa sortie. Couper l'un ne coupe pas
    /// l'autre — sans quoi « envoyer au clavier » serait une bascule globale
    /// déguisée en réglage d'appareil.
    #[test]
    fn chaque_appareil_porte_son_effet_et_sa_sortie() {
        let engine = Engine::default();
        let a = Arc::new(Sortie::default());
        let b = Arc::new(Sortie::default());

        demarrer(&engine, PREMIER, "premier", Arc::clone(&a));
        demarrer(&engine, SECOND, "second", Arc::clone(&b));

        assert_eq!(etat(&engine, PREMIER).effect_id.as_deref(), Some("premier"));
        assert_eq!(etat(&engine, SECOND).effect_id.as_deref(), Some("second"));

        engine.set_to_keyboard(SECOND, false);
        attendre("la sortie du second reste ouverte", || {
            !etat(&engine, SECOND).reaching_keyboard
        });

        let fige = b.ecrites.load(Ordering::Relaxed);
        let avant = a.ecrites.load(Ordering::Relaxed);
        attendre("le premier n'écrit plus", || {
            a.ecrites.load(Ordering::Relaxed) > avant + 2
        });

        assert!(etat(&engine, PREMIER).to_keyboard, "la coupure a débordé");
        assert!(etat(&engine, PREMIER).reaching_keyboard);
        assert_eq!(
            b.ecrites.load(Ordering::Relaxed),
            fige,
            "la sortie coupée écrit encore"
        );

        engine.stop(PREMIER);
        engine.stop(SECOND);
    }

    /// Un appareil arrêté garde sa ligne : « cet appareil ne fait rien » et « je
    /// ne sais rien de cet appareil » ne se disent pas pareil, et l'interface
    /// doit pouvoir les distinguer.
    #[test]
    fn un_appareil_arrete_garde_sa_ligne_d_etat() {
        let engine = Engine::default();
        demarrer(&engine, PREMIER, "premier", Arc::new(Sortie::default()));
        engine.stop(PREMIER);

        let s = etat(&engine, PREMIER);
        assert!(!s.running);
        assert_eq!(s.effect_id, None);
        assert_eq!(engine.device_status().len(), 1);
    }

    /// **Ce que la suppression d'un effet doit obtenir du moteur.**
    ///
    /// Un effet supprimé peut tourner sur plusieurs appareils, et il doit
    /// s'arrêter sur tous — une boucle oubliée continuerait d'exécuter un
    /// `effect.js` chargé en mémoire, sans erreur visible, alors que son dossier
    /// n'existe plus. Les boucles qui font tourner **autre chose** ne sont pas
    /// concernées : supprimer un effet n'éteint pas les claviers des autres.
    #[test]
    fn arreter_un_effet_l_arrete_partout_et_nulle_part_ailleurs() {
        let engine = Engine::default();
        let voisin = Arc::new(Sortie::default());

        demarrer(&engine, PREMIER, "a-supprimer", Arc::new(Sortie::default()));
        demarrer(&engine, SECOND, "autre", Arc::clone(&voisin));

        let arretes = engine.stop_everywhere("a-supprimer");
        assert_eq!(arretes, vec![PREMIER]);

        let supprime = etat(&engine, PREMIER);
        assert!(!supprime.running);
        assert_eq!(
            supprime.effect_id, None,
            "la ligne d'état nomme encore un effet qui n'existe plus"
        );

        // Le voisin, lui, n'a rien vu passer : il tourne toujours, et ses images
        // continuent de partir.
        let avant = voisin.ecrites.load(Ordering::Relaxed);
        assert!(etat(&engine, SECOND).running);
        attendre("le voisin s'est arrêté avec son camarade", || {
            voisin.ecrites.load(Ordering::Relaxed) > avant
        });

        engine.stop(SECOND);
    }

    /// Le même effet sur deux appareils : les deux boucles partent.
    #[test]
    fn arreter_un_effet_couvre_tous_les_appareils_qui_le_font_tourner() {
        let engine = Engine::default();
        demarrer(&engine, PREMIER, "partout", Arc::new(Sortie::default()));
        demarrer(&engine, SECOND, "partout", Arc::new(Sortie::default()));

        assert_eq!(engine.stop_everywhere("partout"), vec![PREMIER, SECOND]);
        assert!(!etat(&engine, PREMIER).running);
        assert!(!etat(&engine, SECOND).running);
    }

    /// Un identifiant que personne ne fait tourner n'arrête rien. C'est le cas
    /// courant : on supprime un effet qu'on n'a pas appliqué.
    #[test]
    fn arreter_un_effet_qui_ne_tourne_nulle_part_ne_touche_a_rien() {
        let engine = Engine::default();
        demarrer(&engine, PREMIER, "en-cours", Arc::new(Sortie::default()));

        assert!(engine.stop_everywhere("jamais-lance").is_empty());
        assert!(etat(&engine, PREMIER).running);

        engine.stop(PREMIER);
    }

    /// Ce que la remise à zéro de la configuration attend du moteur : plus une
    /// seule boucle, quel que soit l'effet et quel que soit l'appareil — aperçu
    /// compris, qui n'écrit sur rien mais entretient un contexte QuickJS.
    #[test]
    fn tout_arreter_ne_laisse_aucune_boucle() {
        let engine = Engine::default();
        demarrer(&engine, PREMIER, "premier", Arc::new(Sortie::default()));
        demarrer(&engine, SECOND, "second", Arc::new(Sortie::default()));
        apercevoir(&engine, PREMIER, "regarde");

        engine.stop_all();

        assert!(engine.device_status().iter().all(|s| !s.status.running));
        // Les lignes restent : « cet appareil ne fait rien » et « je ne sais rien
        // de cet appareil » ne se disent pas pareil, remise à zéro ou non.
        assert_eq!(engine.device_status().len(), 2);
        assert!(engine.preview_status().is_none());
    }

    // ------------------------------------------------------------ aperçu

    fn apercevoir(engine: &Engine, layout_of: DeviceRef, effect_id: &str) {
        engine
            .start_preview(
                layout_of,
                effect_id.into(),
                EFFET.into(),
                "{}".into(),
                layout(),
            )
            .expect("démarrage de l'aperçu");
    }

    /// **Le cœur de l'issue #63.** Prévisualiser Y pendant que X tourne sur le
    /// clavier ne doit rien arrêter : parcourir la galerie éteindrait sinon
    /// l'éclairage en cours, et ça ne se verrait qu'une fois livré.
    #[test]
    fn l_apercu_n_interrompt_pas_l_effet_de_l_appareil() {
        let engine = Engine::default();
        let sortie = Arc::new(Sortie::default());
        demarrer(&engine, PREMIER, "applique", Arc::clone(&sortie));

        // Trois aperçus à la suite, comme un parcours de galerie.
        for effet in ["regarde-1", "regarde-2", "regarde-3"] {
            apercevoir(&engine, PREMIER, effet);
        }

        let appareil = etat(&engine, PREMIER);
        assert!(
            appareil.running,
            "l'aperçu a arrêté la boucle de l'appareil"
        );
        assert_eq!(appareil.effect_id.as_deref(), Some("applique"));

        // Et les images continuent de partir vers le clavier pendant l'aperçu.
        let avant = sortie.ecrites.load(Ordering::Relaxed);
        attendre("le clavier n'est plus alimenté", || {
            sortie.ecrites.load(Ordering::Relaxed) > avant + 2
        });

        engine.stop_all();
    }

    /// **Aucun octet ne part vers un clavier depuis un aperçu.** C'est ce que
    /// [`SansSortie`] garantit, et c'est la seule garantie qui compte : une
    /// sortie ouverte par inadvertance ferait de l'aperçu une application.
    #[test]
    fn l_apercu_n_ecrit_sur_aucun_appareil() {
        let engine = Engine::default();
        let sortie = Arc::new(Sortie::default());
        demarrer(&engine, PREMIER, "applique", Arc::clone(&sortie));
        engine.stop(PREMIER);

        let fige = sortie.ecrites.load(Ordering::Relaxed);
        apercevoir(&engine, PREMIER, "regarde");
        attendre("l'aperçu n'a rendu aucune image", || {
            engine
                .preview_status()
                .is_some_and(|p| p.running && p.error.is_none())
        });
        // Quelques images passent pendant que le test dort.
        std::thread::sleep(Duration::from_millis(120));

        assert_eq!(
            sortie.ecrites.load(Ordering::Relaxed),
            fige,
            "l'aperçu a écrit sur la sortie de l'appareil"
        );
        engine.stop_all();
    }

    /// L'aperçu ne se confond avec aucun appareil : il n'ajoute aucune ligne à la
    /// liste que lisent l'icône de zone de notification et le diagnostic.
    #[test]
    fn l_apercu_n_apparait_pas_dans_la_liste_des_appareils() {
        let engine = Engine::default();
        apercevoir(&engine, PREMIER, "regarde");

        assert!(
            engine.device_status().is_empty(),
            "l'aperçu s'est glissé dans la liste des appareils"
        );

        let rapport = engine.report();
        assert!(rapport.devices.is_empty());
        let apercu = rapport.preview.expect("aucun aperçu");
        assert_eq!(apercu.effect_id.as_deref(), Some("regarde"));
        assert_eq!(
            apercu.layout_of, PREMIER,
            "le gabarit emprunté n'est pas celui qu'on a demandé"
        );

        engine.stop_preview();
        assert!(
            engine.preview_status().is_none(),
            "un aperçu arrêté reste annoncé"
        );
    }

    /// Supprimer un effet arrête aussi l'**aperçu** qui le fait tourner : le
    /// JavaScript est chargé en mémoire, l'écran continuerait d'animer un effet
    /// absent de la bibliothèque. Et il n'apparaît pas dans la liste des
    /// appareils arrêtés — aucun appareil n'a été touché.
    #[test]
    fn supprimer_un_effet_arrete_aussi_son_apercu() {
        let engine = Engine::default();
        demarrer(&engine, PREMIER, "autre", Arc::new(Sortie::default()));
        apercevoir(&engine, PREMIER, "a-supprimer");

        assert!(engine.stop_everywhere("a-supprimer").is_empty());
        assert!(engine.preview_status().is_none());
        assert!(
            etat(&engine, PREMIER).running,
            "la boucle de l'appareil a été arrêtée au passage"
        );

        engine.stop_all();
    }

    /// Deux sélections à la suite : la seconde **remplace** la première, elle ne
    /// s'y ajoute pas. Il n'y a qu'une boucle d'aperçu, donc qu'un contexte
    /// QuickJS à la fois.
    #[test]
    fn changer_de_selection_remplace_l_apercu() {
        let engine = Engine::default();
        apercevoir(&engine, PREMIER, "premier-regarde");
        apercevoir(&engine, SECOND, "second-regarde");

        let apercu = engine.preview_status().expect("aucun aperçu");
        assert_eq!(apercu.effect_id.as_deref(), Some("second-regarde"));
        assert_eq!(apercu.layout_of, SECOND);
        assert!(engine.device_status().is_empty());

        engine.stop_preview();
    }

    /// BOUT EN BOUT — écrit sur le VRAI clavier. `#[ignore]` par défaut.
    ///
    /// `cargo test -p candeo-desktop bout_en_bout -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bout_en_bout_sur_le_vrai_clavier() {
        let api = hidapi::HidApi::new().expect("HID");
        let l = layout();
        let kb = match Keyboard::open(&api, l) {
            Ok(kb) => kb,
            Err(e) => panic!("ouverture impossible : {e}"),
        };
        println!("clavier ouvert : {}", l.name);

        let device = DeviceRef {
            vid: l.vid,
            pid: l.pid,
        };
        let handle: Handle = Arc::new(Mutex::new(Some(kb)));
        let engine = Engine::default();
        let js = crate::builtins::find("onde-radiale").expect("intégré").js;

        engine
            .start(
                device,
                "onde-radiale".into(),
                js.to_string(),
                "{}".into(),
                l,
                Box::new(Arc::clone(&handle)),
            )
            .expect("démarrage");
        println!("moteur démarré — 3 s d'onde radiale sur le clavier");
        std::thread::sleep(Duration::from_secs(3));

        let s = etat(&engine, device);
        println!("état : running={} erreur={:?}", s.running, s.error);
        assert!(s.running, "la boucle s'est arrêtée");
        assert!(s.error.is_none(), "erreur pendant le rendu : {:?}", s.error);
        assert!(
            s.reaching_keyboard,
            "aucune image n'atteint le clavier : {:?}",
            s.device_error
        );

        engine.stop(device);
        println!("arrêté proprement");
    }

    #[test]
    fn un_effet_rend_une_image_complete() {
        let (_rt, ctx) = prepare(EFFET, layout()).expect("chargement");
        let bytes = render_once(&ctx, 0.0, 0, "{}", layout().led_count()).expect("rendu");

        // 132 positions, pas 106 : une image couvre toute la matrice.
        assert_eq!(bytes.len(), 132 * 3);
    }

    #[test]
    fn les_positions_sans_led_restent_noires() {
        let (_rt, ctx) = prepare(EFFET, layout()).expect("chargement");
        let bytes = render_once(&ctx, 1.0, 0, "{}", layout().led_count()).expect("rendu");

        // (0, 1) est un trou de la matrice — l'effet itère `layout.keys`, il ne
        // peut donc pas l'atteindre.
        let trou = 1usize;
        assert_eq!(&bytes[trou * 3..trou * 3 + 3], &[0, 0, 0]);

        // Échap, elle, est allumée.
        assert_ne!(&bytes[0..3], &[0, 0, 0]);
    }

    #[test]
    fn une_erreur_de_syntaxe_remonte_au_chargement() {
        let err = erreur_de_chargement("ceci n'est pas du JavaScript {{{");
        assert!(
            err.contains("chargement de l'effet"),
            "message inattendu : {err}"
        );
    }

    #[test]
    fn un_module_sans_export_par_defaut_est_refuse() {
        let err = erreur_de_chargement("export const x = 1");
        assert!(
            err.contains("export par défaut"),
            "message inattendu : {err}"
        );
    }

    /// Une exception à l'exécution ne doit pas faire tomber le moteur : elle
    /// remonte en `Err`, la boucle la compte et continue.
    #[test]
    fn une_exception_a_l_execution_est_rattrapee() {
        let js = "export default { name: 'X', render() { throw new Error('boum') } }";
        let (_rt, ctx) = prepare(js, layout()).expect("chargement");
        let err = render_once(&ctx, 0.0, 0, "{}", layout().led_count()).unwrap_err();
        assert!(err.contains("boum"), "message inattendu : {err}");
    }

    /// Une couleur aberrante doit devenir un octet valide, pas faire échouer la
    /// conversion loin de sa cause.
    #[test]
    fn les_couleurs_hors_bornes_sont_bornees() {
        let js = r#"
            export default {
              name: 'X',
              render({ layout, frame }) {
                for (const key of layout.keys) frame.set(key, { r: 999, g: -5, b: NaN })
              },
            }
        "#;
        let (_rt, ctx) = prepare(js, layout()).expect("chargement");
        let bytes = render_once(&ctx, 0.0, 0, "{}", layout().led_count()).expect("rendu");
        assert_eq!(&bytes[0..3], &[255, 0, 0]);
    }

    /// `api.js` et `packages/effects-api/src/index.ts` décrivent la même API.
    /// Si un nom disparaît d'ici, l'éditeur promettrait une fonction absente.
    #[test]
    fn api_js_exports_match_the_typescript_surface() {
        let js = r#"
            import * as api from '@candeo/effects-api'
            export default {
              name: 'X',
              render({ frame }) {
                const manquants = ['rgb','hsv','mix','lerp','BLACK','defineEffect','center','bounds'].filter(n => api[n] === undefined)
                if (manquants.length) throw new Error('absents de api.js : ' + manquants.join(', '))
                frame.fill(api.BLACK)
              },
            }
        "#;
        let (_rt, ctx) = prepare(js, layout()).expect("chargement");
        render_once(&ctx, 0.0, 0, "{}", layout().led_count()).expect("rendu");
    }

    // ---------------------------------------------------------------- géométrie

    /// Le rectangle des touches arrive jusqu'à l'effet, dans l'unité du Rust.
    ///
    /// La barre d'espace est le cas qui résume le sujet : **une** case de
    /// matrice, **6,25 u** de capuchon. Sans le rectangle, un effet ne peut pas
    /// faire la différence entre elle et une touche alphabétique.
    #[test]
    fn le_gabarit_remis_a_l_effet_porte_la_geometrie() {
        let json: serde_json::Value =
            serde_json::from_str(&layout_json(layout())).expect("gabarit JSON");
        let keys = json["keys"].as_array().expect("touches");

        let espace = keys
            .iter()
            .find(|k| k["label"] == "Espace")
            .expect("la barre d'espace");
        assert_eq!(espace["col"], 6, "une seule case de matrice");
        assert_eq!(espace["w"], 6.25, "et 6,25 u de capuchon");
        assert_eq!(espace["x"], 3.75);
        assert_eq!(espace["y"], 5.5);
        assert_eq!(espace["h"], 1.0);

        assert!(
            keys.iter().all(|k| k["w"].as_f64().unwrap_or(0.0) > 0.0),
            "une touche sans largeur ne se distingue pas d'une touche sans géométrie"
        );
    }

    /// Un gabarit dont personne n'a dessiné la disposition.
    ///
    /// Il n'en existe pas dans ce dépôt — `candeo_device::Key` porte un
    /// rectangle obligatoire — mais c'est ce que sera un appareil contribué sans
    /// la capacité `geometry` de `docs/design/device-sdk.md` §3.2. Deux
    /// positions suffisent : ce qui est testé est l'absence des champs.
    const SANS_GEOMETRIE: &str = r#"{"name":"Gabarit non dessiné","rows":1,"cols":2,"keys":[{"index":0,"row":0,"col":0,"label":"A"},{"index":1,"row":0,"col":1,"label":"B"}]}"#;

    /// **Le motif qu'on combat.** Sans géométrie, `key.x` vaut `undefined`, la
    /// distance `NaN`, et la couleur serait bornée à zéro : un clavier noir,
    /// sans une erreur, et une session de diagnostic pour comprendre pourquoi.
    /// L'effet doit échouer en nommant ce qui manque.
    #[test]
    fn l_onde_radiale_refuse_un_gabarit_sans_geometrie() {
        let js = crate::builtins::find("onde-radiale").expect("intégré").js;
        let (_rt, ctx) =
            prepare_with_layout(js, 2, SANS_GEOMETRIE.to_string(), None).expect("chargement");

        let err = render_once(&ctx, 0.0, 0, "{}", 2).unwrap_err();
        assert!(err.contains("géométrie"), "message inattendu : {err}");
        assert!(
            err.contains('A') || err.contains('B'),
            "le message doit nommer la touche fautive : {err}"
        );
    }

    /// Et l'onde de matrice, elle, y tourne : c'est tout l'intérêt de l'avoir
    /// gardée plutôt que corrigée. Un gabarit non dessiné garde un effet.
    #[test]
    fn l_onde_matricielle_tourne_sans_geometrie() {
        let js = crate::builtins::find("onde-matricielle")
            .expect("intégré")
            .js;
        let (_rt, ctx) =
            prepare_with_layout(js, 2, SANS_GEOMETRIE.to_string(), None).expect("chargement");

        let bytes = render_once(&ctx, 0.0, 0, "{}", 2).expect("rendu");
        assert!(
            bytes.iter().any(|&c| c != 0),
            "l'onde matricielle n'a besoin que de `row` et `col`"
        );
    }

    // ------------------------------------------------------- bornes d'exécution

    /// Une boucle qui ne rend jamais la main, écrite comme on l'écrit par
    /// accident : une condition de sortie qui n'arrive pas.
    const RENDU_SANS_FIN: &str = r#"
        export default {
          name: 'Sans fin',
          render() {
            let i = 0
            while (i >= 0) i += 1
          },
        }
    "#;

    /// Un état qui grandit à chaque image, et que rien ne libère.
    const ALLOCATION_SANS_FIN: &str = r#"
        const garde = []
        export default {
          name: 'Fuite',
          render() {
            garde.push(new Uint8Array(4 * 1024 * 1024))
          },
        }
    "#;

    /// Comme [`demarrer`], mais avec un effet donné, et sans exiger qu'il parte.
    fn demarrer_js(
        engine: &Engine,
        device: DeviceRef,
        js: &str,
        out: Arc<Sortie>,
    ) -> Result<(), String> {
        engine.start(
            device,
            "borne".into(),
            js.into(),
            "{}".into(),
            layout(),
            Box::new(out),
        )
    }

    /// Lance `f` à côté, et rend son résultat — ou échoue si elle ne revient pas.
    ///
    /// Appeler directement une fonction qu'on soupçonne de ne jamais revenir
    /// donne un test qui ne peut pas échouer : il gèle, et c'est l'intégration
    /// continue qui finit par le tuer, des heures plus tard. Ici, c'est le test
    /// qui tranche. Le fil laissé derrière tournerait dans le vide, mais il
    /// n'empêche rien de se terminer — et un test déjà échoué n'a plus rien à
    /// protéger.
    fn sans_geler<T: Send + 'static>(
        patience: Duration,
        quoi: &str,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(patience)
            .unwrap_or_else(|_| panic!("{quoi} : rien en {patience:?}"))
    }

    /// **Un effet qui ne rend jamais la main ne gèle plus son fil.**
    ///
    /// C'est la panne que les bornes existent pour traiter : sans elles, le
    /// drapeau `stop` ne serait jamais relu, et fermer la fenêtre ne sauverait
    /// pas — un effet tourne fenêtre fermée. L'arrêt doit donc venir du moteur,
    /// par le chemin d'erreur ordinaire.
    #[test]
    fn un_effet_qui_boucle_a_chaque_image_est_arrete_proprement() {
        let engine = Engine::default();
        let out = Arc::new(Sortie::default());
        demarrer_js(&engine, PREMIER, RENDU_SANS_FIN, Arc::clone(&out)).expect("démarrage");

        // Le garde-fou du test : si rien n'arrêtait la boucle, c'est lui qui
        // échouerait, et non l'intégration continue qui expirerait des heures
        // plus tard. Trois fois ce que la borne promet — trente images coupées,
        // chacune de tout son budget.
        attendre_au_plus(
            BUDGET_IMAGE * MAX_CONSECUTIVE_ERRORS * 3,
            "la boucle ne s'est pas arrêtée",
            || !etat(&engine, PREMIER).running,
        );

        let erreur = etat(&engine, PREMIER)
            .error
            .expect("aucune erreur consignée");
        assert!(
            erreur.contains("temps de calcul"),
            "la cause n'est pas nommée : {erreur}"
        );
        assert_eq!(
            out.ecrites.load(Ordering::Relaxed),
            0,
            "une image est sortie d'un effet qui n'en a jamais terminé une"
        );

        // La boucle a bien rendu son fil : sans quoi c'est ici que le test
        // s'arrêterait pour toujours, sur l'attente de la fin.
        engine.stop(PREMIER);
    }

    /// **Un effet qui alloue sans fin n'emporte plus que lui-même.**
    ///
    /// Les premières images passent — l'effet a le droit de garder un état —,
    /// puis la borne tombe et le dépassement devient une erreur d'image comme
    /// une autre.
    #[test]
    fn un_effet_qui_alloue_sans_fin_est_arrete_proprement() {
        let engine = Engine::default();
        let out = Arc::new(Sortie::default());
        demarrer_js(&engine, PREMIER, ALLOCATION_SANS_FIN, Arc::clone(&out)).expect("démarrage");

        attendre("la boucle ne s'est pas arrêtée", || {
            !etat(&engine, PREMIER).running
        });

        let erreur = etat(&engine, PREMIER)
            .error
            .expect("aucune erreur consignée");
        assert!(
            erreur.contains("mémoire"),
            "la cause n'est pas nommée : {erreur}"
        );

        engine.stop(PREMIER);
    }

    /// **Une boucle hors de `render` ne bloque plus le démarrage.**
    ///
    /// Le corps du module s'exécute au chargement, hors de toute image, et
    /// `start` en attend le verdict, le verrou de l'appareil à la main : sans
    /// borne, ce clavier ne démarrerait ni n'arrêterait plus jamais rien.
    #[test]
    fn un_effet_qui_boucle_au_chargement_rend_la_main() {
        let erreur = sans_geler(
            BUDGET_CHARGEMENT * 3,
            "le démarrage n'est jamais revenu",
            || {
                let engine = Engine::default();
                demarrer_js(
                    &engine,
                    PREMIER,
                    "while (true) {}\nexport default { name: 'X', render() {} }",
                    Arc::new(Sortie::default()),
                )
                .expect_err("le chargement aurait dû être interrompu")
            },
        );

        assert!(
            erreur.contains("temps de calcul"),
            "la cause n'est pas nommée : {erreur}"
        );
    }

    /// Une exception ordinaire garde son message : les bornes n'expliquent que
    /// ce qu'elles ont coupé, et un `throw` de l'effet se lit déjà tout seul.
    #[test]
    fn une_exception_ordinaire_garde_son_message() {
        let engine = Engine::default();
        let js = "export default { name: 'X', render() { throw new Error('boum') } }";
        demarrer_js(&engine, PREMIER, js, Arc::new(Sortie::default())).expect("démarrage");

        attendre("aucune erreur consignée", || {
            etat(&engine, PREMIER).error.is_some()
        });
        let erreur = etat(&engine, PREMIER).error.unwrap();
        assert!(erreur.contains("boum"), "message réécrit : {erreur}");

        engine.stop(PREMIER);
    }

    /// **Les bornes ne doivent étrangler aucun effet honnête.**
    ///
    /// Le tampon d'image ne coûte rien — `bootstrap.js` le réutilise — mais un
    /// effet a le droit de garder un état et de le faire vivre. Deux mille
    /// particules et une traînée d'une seconde d'images, pour un clavier qui
    /// compte 132 LED, c'est démesuré à dessein : si les bornes laissent passer
    /// celui-là, elles laissent passer tout ce qu'on écrira.
    ///
    /// Le rendu va au-delà de la profondeur de la traînée, jusqu'à son régime
    /// permanent : un état qui ne se libère qu'à la soixantième image ne se voit
    /// pas sur trente.
    ///
    /// La marge est vérifiée, et pas seulement le succès : un effet qui
    /// passerait de justesse ne passerait plus sur la machine du voisin.
    #[test]
    fn les_bornes_laissent_passer_un_effet_qui_garde_un_etat() {
        let js = r#"
            const particules = Array.from({ length: 2000 }, (_, i) => ({
              x: (i * 7) % 20,
              y: (i * 3) % 6,
              vx: 0.11,
              vy: 0.07,
            }))
            const trainee = []

            export default {
              name: 'Particules',
              render({ layout, frame }) {
                for (const p of particules) {
                  p.x = (p.x + p.vx) % layout.cols
                  p.y = (p.y + p.vy) % layout.rows
                }
                // Une seconde d'images conservées, la plus ancienne libérée.
                trainee.push(particules.map((p) => (p.x + p.y) | 0))
                if (trainee.length > 60) trainee.shift()

                for (const key of layout.keys) {
                  frame.set(key, { r: (key.col * 8) % 256, g: (key.row * 40) % 256, b: 60 })
                }
              },
            }
        "#;

        let budget = Rc::new(Budget::default());
        budget.accorder(BUDGET_CHARGEMENT);
        let (rt, ctx) = prepare_budgeted(js, layout(), &budget).expect("chargement");

        for image in 0..90u32 {
            budget.accorder(BUDGET_IMAGE);
            render_once(
                &ctx,
                f64::from(image) / f64::from(FPS),
                image,
                "{}",
                layout().led_count(),
            )
            .unwrap_or_else(|e| panic!("image {image} : {e}"));
        }

        let utilise = rt.memory_usage().malloc_size as usize;
        assert!(
            utilise * 4 < BUDGET_MEMOIRE,
            "un effet à état frôle la borne mémoire : {utilise} octets sur {BUDGET_MEMOIRE}"
        );
    }

    // ------------------------------------------------------------ intégrés
    //
    // Les effets livrés passent par le même moteur que ceux de l'utilisateur,
    // donc par les mêmes tests. Un effet intégré cassé ne doit pas se découvrir
    // à l'exécution, chez celui qui l'ouvre en premier.

    /// Instants d'échantillonnage. Plusieurs, et pas seulement zéro : une
    /// division par la durée d'un cycle ou un dépassement de la dernière rangée
    /// ne se voit qu'une fois l'animation commencée.
    const INSTANTS: [f64; 4] = [0.0, 0.4, 1.3, 2.7];

    #[test]
    fn chaque_effet_integre_rend_une_image_complete() {
        for b in &crate::builtins::ALL {
            let (_rt, ctx) =
                prepare(b.js, layout()).unwrap_or_else(|e| panic!("« {} » : {e}", b.id));

            for (i, time) in INSTANTS.iter().enumerate() {
                let bytes = render_once(&ctx, *time, i as u32, "{}", layout().led_count())
                    .unwrap_or_else(|e| panic!("« {} » à t={time} : {e}", b.id));

                assert_eq!(bytes.len(), 132 * 3, "« {} » à t={time}", b.id);
                // (0, 1) est un trou de la matrice. Un effet qui l'atteint
                // n'itère pas `layout.keys` : il travaille sur les 132 cases au
                // lieu des 106 positions éclairées.
                assert_eq!(
                    &bytes[3..6],
                    &[0, 0, 0],
                    "« {} » écrit sur une position sans LED",
                    b.id
                );
            }
        }
    }

    /// Un effet livré doit être visible dès sa première image : une image noire
    /// au démarrage ressemble à un effet qui n'a pas démarré.
    #[test]
    fn chaque_effet_integre_allume_quelque_chose_des_la_premiere_image() {
        for b in &crate::builtins::ALL {
            let (_rt, ctx) =
                prepare(b.js, layout()).unwrap_or_else(|e| panic!("« {} » : {e}", b.id));
            let bytes = render_once(&ctx, 0.0, 0, "{}", layout().led_count()).expect("rendu");

            assert!(
                bytes.iter().any(|&c| c != 0),
                "« {} » rend une image entièrement noire",
                b.id
            );
        }
    }

    /// Le manifeste annoncé en Rust et celui que le module déclare décrivent le
    /// même effet. Sans ce test, la galerie pourrait promettre un paramètre que
    /// le code ne lit pas — un réglage sans effet, que rien ne signale.
    #[test]
    fn les_manifestes_integres_correspondent_aux_modules() {
        for b in &crate::builtins::ALL {
            let (_rt, ctx) =
                prepare(b.js, layout()).unwrap_or_else(|e| panic!("« {} » : {e}", b.id));

            let raw: String = ctx.with(|ctx| {
                ctx.globals()
                    .get("__candeo_manifest")
                    .expect("manifeste déclaré")
            });
            let declare: serde_json::Value = serde_json::from_str(&raw).expect("manifeste JSON");

            let annonce = serde_json::json!({
                "name": b.name,
                "description": b.description,
                "params": serde_json::from_str::<serde_json::Value>(b.params).expect("params JSON"),
            });
            assert_eq!(declare, annonce, "« {} »", b.id);
        }
    }

    // ------------------------------------------------- les deux ondes, côte à côte
    //
    // Deux couples de touches du **vrai** gabarit, choisis pour que chacun
    // départage les deux espaces. Rien n'est simulé ici : la géométrie vient de
    // `layout.rs`, et c'est elle qui rend la vérification possible sans clavier.

    /// La couleur d'une LED dans une image rendue.
    fn couleur(image: &[u8], index: usize) -> &[u8] {
        &image[index * 3..index * 3 + 3]
    }

    /// La première image d'un effet livré, sur le gabarit par défaut.
    fn premiere_image(id: &str) -> Vec<u8> {
        let js = crate::builtins::find(id)
            .unwrap_or_else(|| panic!("« {id} » n'est pas livré"))
            .js;
        let (_rt, ctx) = prepare(js, layout()).unwrap_or_else(|e| panic!("« {id} » : {e}"));
        render_once(&ctx, 0.0, 0, "{}", layout().led_count()).expect("rendu")
    }

    /// « L » (index 75) et « ù » (index 77) sont à la **même distance physique**
    /// du centre du dessin — 1 u de part et d'autre, 0,75 u plus bas — et à deux
    /// distances de matrice différentes : 1,5 case contre 0,5.
    ///
    /// L'onde radiale doit donc les peindre de la même couleur, et l'onde
    /// matricielle non. C'est la définition de « radiale », vérifiée plutôt
    /// qu'annoncée.
    #[test]
    fn l_onde_radiale_mesure_en_distance_physique() {
        let radiale = premiere_image("onde-radiale");
        assert_eq!(
            couleur(&radiale, 75),
            couleur(&radiale, 77),
            "deux touches à égale distance physique doivent avoir la même couleur"
        );

        let matricielle = premiere_image("onde-matricielle");
        assert_ne!(
            couleur(&matricielle, 75),
            couleur(&matricielle, 77),
            "en distance de matrice, elles ne sont pas à égale distance"
        );
    }

    /// Le couple symétrique : « L » (index 75) et « * » (index 78) sont à la
    /// **même distance de matrice** — 1,5 case de part et d'autre — mais à
    /// 1,25 u et 2,14 u du centre du dessin, parce que la rangée est décalée et
    /// que l'Entrée en L ne tombe pas sur la grille.
    ///
    /// C'est ce qui fait de l'onde matricielle un effet à part entière, et non
    /// une version fausse de l'autre : elle rend exactement ce qu'elle annonce.
    #[test]
    fn l_onde_matricielle_mesure_en_distance_de_matrice() {
        let matricielle = premiere_image("onde-matricielle");
        assert_eq!(
            couleur(&matricielle, 75),
            couleur(&matricielle, 78),
            "deux touches à égale distance de matrice doivent avoir la même couleur"
        );

        let radiale = premiere_image("onde-radiale");
        assert_ne!(
            couleur(&radiale, 75),
            couleur(&radiale, 78),
            "physiquement, elles ne sont pas à égale distance"
        );
    }

    /// La barre d'espace est peinte d'après le **milieu de son capuchon**.
    ///
    /// Une case de matrice, 6,25 u de large : son centre est à 6,875 u, pas au
    /// bord gauche (3,75 u) ni à la colonne 6. La touche de la rangée du dessus
    /// dont le capuchon est centré au même endroit — « B », à 6,75 u — doit donc
    /// être presque à la même distance du centre, alors que rien dans la matrice
    /// ne le dit.
    #[test]
    fn l_onde_radiale_place_la_barre_d_espace_au_milieu_de_son_capuchon() {
        let radiale = premiere_image("onde-radiale");

        // Distances au centre du dessin (11,25 ; 3,25) : « Espace » à 5,17 u,
        // « B » à 4,83 u — 0,34 u d'écart, donc des teintes voisines. Mesurer
        // depuis le bord gauche du capuchon (3,75 u) porterait l'écart à 2,7 u,
        // et les deux couleurs n'auraient plus rien à voir.
        let espace = couleur(&radiale, 116);
        let touche_b = couleur(&radiale, 94);
        let ecart = espace
            .iter()
            .zip(touche_b)
            .map(|(e, t)| e.abs_diff(*t) as u32)
            .max()
            .expect("trois composantes");

        assert!(
            ecart < 60,
            "« Espace » et « B » sont physiquement voisins, leurs couleurs devraient l'être : {espace:?} contre {touche_b:?}"
        );
    }
}
