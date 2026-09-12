//! Moteur d'effets : un fil de rendu, indépendant de la fenêtre.
//!
//! C'est le seul endroit où du code d'effet s'exécute. Le front n'en exécute
//! jamais : il envoie la source et reçoit les images. L'aperçu du simulateur
//! est donc la production, par construction — et non une ressemblance obtenue
//! en faisant tourner le même code dans un second moteur JavaScript.
//!
//! Voir `docs/design/effects-runtime.md`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use candeo_device::{Keyboard, Layout};
use candeo_protocol::Rgb;
use rquickjs::loader::{BuiltinLoader, BuiltinResolver};
use rquickjs::{CatchResultExt, Context, Function, Module, Runtime};
use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};

/// Le module que l'hôte fournit, et que l'éditeur décrit par son `.d.ts`.
const API_JS: &str = include_str!("api.js");

/// La colle qui importe l'effet et installe la fonction de rendu.
const BOOTSTRAP_JS: &str = include_str!("bootstrap.js");

/// 60 images par seconde. Le clavier n'en demande pas tant, mais c'est la
/// cadence à laquelle un mouvement cesse de se voir saccadé.
const FPS: u32 = 60;

/// Au-delà, on arrête. Un effet qui lève à chaque image ne se rétablira pas
/// tout seul, et continuer reviendrait à remplir le journal en silence.
const MAX_CONSECUTIVE_ERRORS: u32 = 30;

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
            effect_id: Mutex::new(None),
        }
    }
}

/// État du moteur, tel que l'interface le lit.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub running: bool,
    pub effect_id: Option<String>,
    /// Message d'erreur, déjà lisible : il est affiché tel quel.
    pub error: Option<String>,
    pub to_keyboard: bool,
}

#[derive(Default)]
pub struct Engine {
    shared: Mutex<Option<Arc<Shared>>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Engine {
    fn current(&self) -> Option<Arc<Shared>> {
        self.shared.lock().unwrap().clone()
    }

    pub fn status(&self) -> EngineStatus {
        match self.current() {
            None => EngineStatus::default(),
            Some(s) => EngineStatus {
                running: !s.stop.load(Ordering::Relaxed),
                effect_id: s.effect_id.lock().unwrap().clone(),
                error: s.error.lock().unwrap().clone(),
                to_keyboard: s.to_keyboard.load(Ordering::Relaxed),
            },
        }
    }

    /// Arrête la boucle et **attend** sa fin.
    ///
    /// L'attente n'est pas un détail : sans elle, démarrer un effet juste après
    /// en avoir arrêté un laisserait deux boucles écrire sur le même clavier le
    /// temps que la première s'aperçoive qu'elle doit s'arrêter.
    pub fn stop(&self) {
        if let Some(s) = self.shared.lock().unwrap().take() {
            s.stop.store(true, Ordering::Relaxed);
        }
        if let Some(h) = self.thread.lock().unwrap().take() {
            let _ = h.join();
        }
    }

    pub fn set_params(&self, params: String) {
        if let Some(s) = self.current() {
            *s.params.lock().unwrap() = params;
        }
    }

    pub fn set_to_keyboard(&self, on: bool) {
        if let Some(s) = self.current() {
            s.to_keyboard.store(on, Ordering::Relaxed);
        }
    }

    pub fn set_channel(&self, channel: Option<Channel<InvokeResponseBody>>) {
        if let Some(s) = self.current() {
            *s.frames.lock().unwrap() = channel;
        }
    }

    /// Démarre un effet. Remplace celui qui tournait, s'il y en avait un.
    pub fn start(
        &self,
        effect_id: String,
        js: String,
        params: String,
        layout: &'static Layout,
        keyboard: Arc<Mutex<Option<Keyboard>>>,
    ) -> Result<(), String> {
        self.stop();

        let shared = Arc::new(Shared::default());
        *shared.params.lock().unwrap() = params;
        *shared.effect_id.lock().unwrap() = Some(effect_id);

        // Le contexte JavaScript est bâti **dans** le fil et n'en sort jamais :
        // les types de QuickJS ne traversent pas les fils, et les enfermer ici
        // est plus sûr que de les rendre partageables.
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let s = Arc::clone(&shared);

        let handle = std::thread::Builder::new()
            .name("candeo-effect".into())
            .spawn(move || render_loop(s, js, layout, keyboard, ready_tx))
            .map_err(|e| format!("impossible de démarrer le fil de rendu : {e}"))?;

        // On attend le verdict du chargement : une erreur de syntaxe doit
        // remonter à l'appel, pas se découvrir dans un état plus tard.
        match ready_rx.recv() {
            Ok(Ok(())) => {
                *self.shared.lock().unwrap() = Some(shared);
                *self.thread.lock().unwrap() = Some(handle);
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(e)
            }
            Err(_) => {
                let _ = handle.join();
                Err("le fil de rendu s'est arrêté avant d'avoir chargé l'effet".into())
            }
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Prépare le contexte QuickJS, puis tourne jusqu'à l'arrêt.
fn render_loop(
    shared: Arc<Shared>,
    js: String,
    layout: &'static Layout,
    keyboard: Arc<Mutex<Option<Keyboard>>>,
    ready: std::sync::mpsc::Sender<Result<(), String>>,
) {
    let frame_len = layout.led_count();

    // `_rt` doit vivre aussi longtemps que le contexte : c'est lui qui porte
    // le résolveur de modules. Le laisser tomber ici rendrait tout `import`
    // introuvable à la première image.
    let (_rt, ctx) = match prepare(&js, layout) {
        Ok(c) => {
            let _ = ready.send(Ok(()));
            c
        }
        Err(e) => {
            let _ = ready.send(Err(e));
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

        match render_once(&ctx, time, frame_index, &params, frame_len) {
            Ok(bytes) => {
                consecutive_errors = 0;
                // L'effet s'est rétabli : on efface, sinon l'interface
                // afficherait une erreur périmée indéfiniment.
                *shared.error.lock().unwrap() = None;
                emit(&shared, &keyboard, &bytes);
            }
            Err(e) => {
                consecutive_errors += 1;
                *shared.error.lock().unwrap() = Some(e);
                if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
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

/// Contexte JavaScript prêt à rendre : modules résolus, `__candeo_render` posé.
///
/// Le `Runtime` est renvoyé avec le contexte, et non gardé ici : c'est lui qui
/// porte le résolveur de modules, il doit donc vivre aussi longtemps.
fn prepare(js: &str, layout: &'static Layout) -> Result<(Runtime, Context), String> {
    let rt = Runtime::new().map_err(|e| format!("QuickJS : {e}"))?;

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

    let layout_json = layout_json(layout);
    ctx.with(|ctx| -> Result<(), String> {
        let g = ctx.globals();
        g.set("__candeo_frame_len", layout.led_count() as u32)
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

/// Les deux sorties, indépendantes : chacune peut être absente.
fn emit(shared: &Shared, keyboard: &Mutex<Option<Keyboard>>, bytes: &[u8]) {
    if shared.to_keyboard.load(Ordering::Relaxed) {
        if let Some(kb) = keyboard.lock().unwrap().as_ref() {
            let colors: Vec<Rgb> = bytes
                .chunks_exact(3)
                .map(|c| Rgb::new(c[0], c[1], c[2]))
                .collect();
            if kb.present(&colors).is_err() {
                // Un clavier débranché en cours de route n'est pas une erreur
                // de l'effet : le simulateur doit continuer, et la reconnexion
                // se fait par les commandes existantes.
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
                r#"{{"index":{index},"row":{row},"col":{col},"label":{}}}"#,
                json_string(k.name)
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

use crate::{AppState, CmdResult};
use tauri::{AppHandle, State};

/// Démarre un effet, intégré ou installé.
///
/// La résolution `identifiant → JavaScript` est celle de la bibliothèque, donc
/// les intégrés d'abord : voir [`crate::storage`]. Le moteur, lui, ne fait
/// aucune différence — un effet livré est un module chargé exactement comme
/// celui qu'on vient d'écrire.
///
/// Le gabarit vient du périphérique connecté ; à défaut, du gabarit par
/// défaut. C'est délibéré : on doit pouvoir écrire et prévisualiser un effet
/// **sans posséder le clavier**.
#[tauri::command]
pub fn start_effect(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    params: serde_json::Value,
) -> CmdResult<()> {
    let js = crate::storage::store(&app)?.effect_js(&id)?;

    let layout = match state.keyboard.lock().unwrap().as_ref() {
        Some(kb) => kb.layout(),
        None => crate::default_layout(),
    };

    let params = serde_json::to_string(&params)
        .map_err(|e| format!("paramètres non sérialisables : {e}"))?;

    state
        .engine
        .start(id, js, params, layout, Arc::clone(&state.keyboard))
}

#[tauri::command]
pub fn stop_effect(state: State<'_, AppState>) {
    state.engine.stop();
}

/// Ajuste les paramètres à chaud. La boucle ne redémarre pas : elle relit le
/// JSON à chaque image.
#[tauri::command]
pub fn set_effect_params(state: State<'_, AppState>, params: serde_json::Value) -> CmdResult<()> {
    let params = serde_json::to_string(&params)
        .map_err(|e| format!("paramètres non sérialisables : {e}"))?;
    state.engine.set_params(params);
    Ok(())
}

/// Active ou coupe la sortie clavier, sans toucher au simulateur.
#[tauri::command]
pub fn set_output_to_keyboard(state: State<'_, AppState>, on: bool) {
    state.engine.set_to_keyboard(on);
}

/// Ouvre le flux d'images vers le simulateur.
///
/// Un canal, et non un événement global : la destination est connue, la portée
/// est explicite, et le binaire passe brut. Libérer le canal côté front, ou
/// appeler [`unsubscribe_frames`], arrête le flux **sans arrêter l'effet**, qui
/// continue d'alimenter le clavier fenêtre fermée.
#[tauri::command]
pub fn subscribe_frames(state: State<'_, AppState>, channel: Channel<InvokeResponseBody>) {
    state.engine.set_channel(Some(channel));
}

#[tauri::command]
pub fn unsubscribe_frames(state: State<'_, AppState>) {
    state.engine.set_channel(None);
}

/// État du moteur, y compris la dernière erreur de l'effet.
///
/// Interrogé plutôt que poussé : une erreur survenue fenêtre fermée doit
/// pouvoir être lue à la réouverture, ce qu'un événement ponctuel ne permet
/// pas.
#[tauri::command]
pub fn engine_status(state: State<'_, AppState>) -> EngineStatus {
    state.engine.status()
}

#[cfg(test)]
mod tests {
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
                const manquants = ['rgb','hsv','mix','lerp','BLACK'].filter(n => api[n] === undefined)
                if (manquants.length) throw new Error('absents de api.js : ' + manquants.join(', '))
                frame.fill(api.BLACK)
              },
            }
        "#;
        let (_rt, ctx) = prepare(js, layout()).expect("chargement");
        render_once(&ctx, 0.0, 0, "{}", layout().led_count()).expect("rendu");
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
}
