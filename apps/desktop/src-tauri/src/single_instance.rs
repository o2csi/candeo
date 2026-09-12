//! Une seule instance de candeo à la fois.
//!
//! Deux processus ne se partagent ni le clavier ni les réglages, et les deux
//! pannes se lisent mal :
//!
//! - **Le matériel.** Chacun ouvrirait sa poignée HID et ferait tourner ses
//!   propres boucles à 60 img/s sur la même interface. Le clavier papillonnerait
//!   entre deux effets sans qu'aucune des deux fenêtres ne montre quoi que ce
//!   soit d'anormal : chacune affiche *son* rendu au simulateur, et il est juste.
//! - **Les réglages.** [`crate::storage::Store::write_settings`] écrit dans un
//!   temporaire de nom **fixe** puis renomme. Le nom peut rester fixe parce que
//!   les commandes Tauri synchrones s'exécutent sur le fil principal — mais ce
//!   raisonnement vaut *à l'intérieur* d'un processus. À deux, l'un renommerait
//!   ce que l'autre est en train d'écrire : le renommage resterait atomique, ce
//!   qu'il publie ne le serait plus.
//!
//! **L'instance unique est donc ce qui rend sûre la simplification du nom de
//! temporaire.** Les deux décisions se tiennent, et ne se défont pas l'une sans
//! l'autre : rendre candeo multi-instance obligerait à reprendre ce nom, et
//! reprendre ce nom sans cela n'achèterait rien.
//!
//! # Ce que ça ne protège pas
//!
//! Le runtime du constructeur — ou n'importe quel autre logiciel d'éclairage —
//! écrit sur le même clavier, et rien ici ne peut l'en empêcher. C'est un
//! problème distinct, et plutôt une mention dans la documentation qu'une
//! fonctionnalité.

use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Manager, Runtime, WebviewWindow, WebviewWindowBuilder};

/// Étiquette de la fenêtre principale.
///
/// Elle est écrite à trois endroits que rien ne relie à la compilation : ici,
/// dans `tauri.conf.json` — une fenêtre sans `label` prend « main », c'est le
/// défaut de Tauri — et dans `capabilities/default.json`, qui n'accorde ses
/// permissions qu'à elle. Le test en fin de module confronte les trois.
const MAIN_WINDOW: &str = "main";

/// Le plugin d'instance unique, **à enregistrer avant tous les autres**.
///
/// C'est pendant l'initialisation du plugin que le second processus se découvre
/// en trop, prévient l'instance vivante et s'arrête. Les plugins sont initialisés
/// dans l'ordre d'enregistrement, et tous le sont avant la création des fenêtres
/// comme avant le `setup` de l'application : le mettre en tête, c'est mourir
/// avant d'avoir ouvert la moindre poignée HID. Plus bas dans la liste, le
/// processus de trop toucherait au clavier le temps de s'apercevoir qu'il est de
/// trop — exactement ce qu'on cherche à empêcher.
pub(crate) fn init<R: Runtime>() -> TauriPlugin<R> {
    tauri_plugin_single_instance::init(|app, _args, _cwd| {
        // Les arguments et le répertoire courant du second lancement sont
        // ignorés : candeo n'a pas de ligne de commande. Le jour où il en aura
        // une — ouvrir un effet, par exemple — c'est ici qu'elle serait relayée à
        // l'instance vivante.
        if let Err(e) = reveal(app) {
            // Une trace, et rien de plus. Le processus de trop est déjà mort, il
            // n'y a plus personne à qui refuser quoi que ce soit ; paniquer
            // emporterait l'instance survivante et les effets qu'elle fait
            // tourner, pour une fenêtre qui n'est pas venue au premier plan.
            eprintln!("instance unique : {e}");
        }
    })
}

/// Remet l'instance vivante sous les yeux de qui vient de relancer candeo.
///
/// Sans cela, le second lancement disparaîtrait en silence, et un lancement sans
/// effet visible se lit comme un refus de démarrer.
fn reveal<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let window = match app.get_webview_window(MAIN_WINDOW) {
        Some(window) => window,
        None => reopen(app)?,
    };

    // Les trois, parce qu'aucune n'implique les autres : une fenêtre masquée que
    // l'on ne fait que mettre au premier plan reste invisible, une fenêtre
    // réduite que l'on montre reste réduite, et une fenêtre visible derrière une
    // autre y reste tant qu'on ne lui donne pas le focus. On ne cherche pas à
    // savoir laquelle des trois s'appliquait : les demander toutes coûte moins
    // que de relever l'état de la fenêtre pour en déduire la même chose.
    window
        .show()
        .and_then(|()| window.unminimize())
        .and_then(|()| window.set_focus())
        .map_err(|e| format!("fenêtre « {MAIN_WINDOW} » non ramenée au premier plan : {e}"))
}

/// Rouvre la fenêtre principale **à partir de sa déclaration**.
///
/// La fenêtre n'est construite nulle part dans ce code : elle est déclarée dans
/// `tauri.conf.json`, et Tauri la bâtit au démarrage. La rouvrir, c'est donc
/// relire cette déclaration — pas recopier ici une taille et un titre, qui
/// feraient une seconde source de vérité et divergeraient dès la première fenêtre
/// redimensionnée dans la configuration. Elle reprend au passage son étiquette,
/// donc les permissions que la capability n'accorde qu'à elle.
///
/// Aujourd'hui le processus s'arrête avec sa dernière fenêtre : cette branche ne
/// sert donc jamais. Elle est écrite quand même parce que l'icône de zone de
/// notification (#46) sépare précisément les deux — fenêtre fermée, effets
/// toujours en cours dans leurs fils, qui ne dépendent pas d'elle. Son absence se
/// paierait alors par un relancement qui ne fait rien de visible, sur une
/// application qui tourne : la panne la plus longue à diagnostiquer.
fn reopen<R: Runtime>(app: &AppHandle<R>) -> Result<WebviewWindow<R>, String> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        // Sur l'étiquette seule, et surtout pas sur le `create` que Tauri
        // consulte au démarrage : une fenêtre déclarée `create: false` — c'est
        // ainsi qu'on démarrerait replié dans la zone de notification — reste
        // exactement celle qu'il faut ouvrir quand quelqu'un relance candeo.
        .find(|w| w.label == MAIN_WINDOW)
        .cloned()
        .ok_or_else(|| format!("aucune fenêtre « {MAIN_WINDOW} » déclarée dans tauri.conf.json"))?;

    WebviewWindowBuilder::from_config(app, &config)
        .and_then(|builder| builder.build())
        .map_err(|e| format!("réouverture de la fenêtre « {MAIN_WINDOW} » impossible : {e}"))
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    /// Les trois écritures de l'étiquette doivent rester d'accord.
    ///
    /// Rien ne les relie à la compilation : renommer la fenêtre dans
    /// `tauri.conf.json` compilerait sans un mot et donnerait une panne muette —
    /// [`reveal`] ne retrouverait plus jamais la fenêtre existante, en rouvrirait
    /// une à chaque relancement, et celle-ci n'aurait aucune des permissions que
    /// la capability réserve à « main ».
    #[test]
    fn l_etiquette_de_la_fenetre_est_la_meme_partout() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        let declarees: Vec<&str> = config["app"]["windows"]
            .as_array()
            .expect("aucune fenêtre déclarée")
            .iter()
            // Une fenêtre sans `label` prend « main » : c'est ce défaut de Tauri
            // dont dépendent les deux autres écritures, et il est donc rejoué ici
            // plutôt que supposé absent.
            .map(|w| w["label"].as_str().unwrap_or(MAIN_WINDOW))
            .collect();
        assert!(
            declarees.contains(&MAIN_WINDOW),
            "fenêtres déclarées : {declarees:?}"
        );

        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json"))
                .expect("capabilities/default.json");
        let visees = capability["windows"]
            .as_array()
            .expect("la capability ne vise aucune fenêtre");
        assert!(
            visees.iter().any(|w| w.as_str() == Some(MAIN_WINDOW)),
            "la capability ne vise pas « {MAIN_WINDOW} » : {visees:?}"
        );
    }
}
