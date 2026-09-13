//! The tray icon: controlling candeo **without the window**.
//!
//! # This module does not deliver a shortcut, it delivers a promise
//!
//! The architecture has stated from the start that **an effect runs with the
//! window closed**: the engine lives in an independent thread
//! ([`crate::runtime`]), and that is the argument that ruled out running effects
//! in the WebView. The design was right; the implementation stopped short.
//! Nothing prevented `RunEvent::ExitRequested`, so on Windows as on Linux the
//! process ended with its last window — and with it the render thread. Closing
//! the window turned the effect off.
//!
//! **This module is what makes the sentence true**, and the two halves do not
//! come apart one without the other: an icon without interception would still
//! let the application die, an interception without an icon would leave a live
//! process that nothing controls any more — and that nothing can quit.
//! Hence [`installee`], and the two interceptions in [`crate::run`] that
//! consult it: **as long as there is no icon, the close button remains an exit**.
//!
//! # What the window's close button does
//!
//! It **hides** the window, it does not quit. That was the choice to make, and it
//! follows from everything above: closing to stop the effect would break the
//! promise at the very place where it has just been kept.
//!
//! The cost is real — the application no longer has an obvious exit — and it is
//! paid twice: "Quitter candeo" (Quit candeo) is **the only** clean exit, isolated
//! at the bottom of the menu by its own separator, and the window says so in so
//! many words (see `App.vue`). An application you cannot figure out how to quit
//! is an application you uninstall.
//!
//! # What quitting does not do: turn the keyboard off
//!
//! See [`quitter`]. It is a choice, and the reasoning is given there.
//!
//! # The menu is a view, never a source
//!
//! It is rebuilt from the **real** state — `settings.json`, the library,
//! [`crate::runtime::Engine::status`] — when the pointer hovers the icon, and
//! after every action. But nothing guarantees it is current at click time: a
//! keyboard can be unplugged while the menu is open, an effect deleted from the
//! window, a loop can stop on its own after thirty failures. **Every action
//! therefore re-reads the state when it runs** instead of trusting the item that
//! was just clicked; whatever fails goes to the log, the only visible place in
//! `release`.
//!
//! # What is not guaranteed on Linux
//!
//! `TrayIconEvent` is not emitted there at all — the icon shows and its menu
//! opens, but no hover is reported. The menu is therefore refreshed there by
//! actions alone. This is a limit of the GTK/AppIndicator stack, not an
//! oversight; the workaround would be to rebuild the menu on a timer, that is,
//! to enumerate USB and read the disk in a loop for a menu nobody is looking at.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use candeo_protocol::Effect;
use tauri::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, Wry};

use candeo_device::{Inspection, Layout};

use crate::runtime::DeviceEngineStatus;
use crate::storage::{self, DeviceState, EffectEntry};
use crate::{journal, single_instance, AppState, CmdResult, DeviceRef};

/// The icon's id, used to find it again and give it a new menu.
const ICONE: &str = "candeo";

/// What the window must learn when the state changed **without it**.
///
/// The window already polls the engine every second, but it re-reads neither
/// the device list nor `settings.json`: it read them once, on mount, because it
/// was until now the only one writing them. It no longer is, and it now
/// outlives its own closing — hidden, its snapshot can age for days.
///
/// Written here **and** in `src/api/candeo.ts`; the test at the end of the module
/// checks the two against each other, otherwise renaming the event would compile
/// without a word and yield a window that never resynchronizes again.
pub(crate) const ETAT_CHANGE: &str = "candeo://etat-change";

/// True when the icon is actually in place.
///
/// **This is not a convenience: it is what keeps candeo from becoming
/// impossible to quit.** If placing it fails — no system tray, no icon in the
/// bundle, a desktop environment without a tray area — only the window is left
/// to control the application. Preventing the exit then, or hiding the window on
/// its close button, would leave a process that no ordinary gesture terminates.
static INSTALLEE: AtomicBool = AtomicBool::new(false);

/// Last menu build failure, so that only the transition is logged.
///
/// The menu is rebuilt **on every hover of the icon**. An unreadable
/// `settings.json` or a broken USB enumeration would produce one line per mouse
/// pass: the same flood as per-frame logging, at a different rate, and the same
/// rule shuts it. See [`journal::bascule`].
static DERNIER_ECHEC: Mutex<Option<String>> = Mutex::new(None);

/// True if the icon is there, hence if the application outlives its windows.
pub(crate) fn installee() -> bool {
    INSTALLEE.load(Ordering::Relaxed)
}

/// Tells the window that the state changed without it.
///
/// Generic because [`single_instance`] is: it is what brings the window back,
/// and a window returning after being hidden is exactly the case where its
/// snapshot is oldest.
///
/// Failure is swallowed: nobody listens when no window is open, and that is the
/// nominal case for this module.
pub(crate) fn signaler<R: Runtime>(app: &AppHandle<R>) {
    let _ = app.emit(ETAT_CHANGE, ());
}

// ---------------------------------------------------------------- items

/// What a menu item triggers.
///
/// A type, and not a string compared by hand in the handler: muda carries only a
/// text identifier, and this is the one place in the code where a typo would
/// show neither at compile time nor at run time — the item would simply do
/// nothing. Going through [`Action::identifiant`] and [`Action::depuis`] brings
/// that binding back under the compiler, and the round-trip test checks it
/// without any clicking.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Action {
    /// Bring the window back into view.
    Ouvrir,
    /// **The only clean exit.**
    Quitter,
    /// Start this effect on this device.
    Effet { device: DeviceRef, effet: String },
    /// Toggle this device's keyboard output.
    Sortie { device: DeviceRef },
    /// The firmware's `Effect::Off` on this device.
    Eteindre { device: DeviceRef },
}

const OUVRIR: &str = "ouvrir";
const QUITTER: &str = "quitter";
const EFFET: &str = "effet";
const SORTIE: &str = "sortie";
const ETEINDRE: &str = "eteindre";

/// The separator between the fields of an identifier.
///
/// `:` can stay unescaped: an effect identifier goes through
/// [`storage::validate_id`], which accepts only `a-z`, `0-9` and the hyphen — so
/// it can never contain one. The test
/// `the_effect_alphabet_excludes_the_separator` holds that dependency; without
/// it, widening the identifier alphabet one day would silently break the menu.
const SEP: char = ':';

/// The device, written to be read back — four hex digits per field.
///
/// Not the `Display` of [`DeviceRef`]: that one is made for a human reading a
/// log (`0x1532:0x0292`), and what is written here must simply parse back
/// without ambiguity.
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

    /// The action this identifier designates, if it designates one.
    ///
    /// `None` rather than a panic: the handler is **global** — it receives the
    /// events of every menu in the application — and an item from elsewhere is
    /// not a programming error.
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

// ---------------------------------------------------------------- the menu

/// A controlled device, as the menu must present it.
struct Pilote {
    layout: &'static Layout,
    device: DeviceRef,
    availability: Availability,
}

/// What the tray can do with a controlled device **right now**.
///
/// Distinct from "controlled", as everywhere else: an adopted device keeps its
/// place in the menu when it is away. Removing it would hide a keyboard the
/// user decided to control, only because it is momentarily elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Availability {
    /// Open: its actions can succeed.
    Open,
    /// Plugged in but **not open**: another unit of the same model released at
    /// startup, an adopted unit whose opening failed, or a device closed by its
    /// render loop after its writes kept failing. Every action would fail with
    /// "no device open": the menu must not offer them (#72).
    NotOpen,
    /// Unplugged.
    Unplugged,
}

/// The controlled devices, plugged in or not.
///
/// USB enumeration can fail — that is the case when HID is unavailable — and
/// that is no reason to empty the menu: it then falls back to "no known serial",
/// which [`storage::DeviceRecord::matches`] tolerates precisely for this
/// situation.
fn pilotes(settings: &storage::Settings, state: &AppState) -> Vec<Pilote> {
    let api = crate::hid().ok();
    crate::LAYOUTS
        .iter()
        .copied()
        .filter_map(|layout| {
            let branche = api.as_ref().and_then(|api| crate::plugged(api, layout));
            let inspection = state.inspection(DeviceRef::of(layout));
            controlled_device(layout, settings, branche, inspection.as_ref())
        })
        .collect()
}

/// One controlled device, decided from the same facts as `list_devices`.
///
/// **The serial comes from the open handle first.** The USB descriptor of this
/// keyboard carries none, so looking the adoption up with it alone matched any
/// unit of the model, and "plugged in" was taken for "ready". The window never
/// had that problem because it reads [`crate::serie_connue`] and reports `open`
/// separately; the tray now asks the same questions.
///
/// Pure, so the #72 scenario is testable without a second keyboard.
fn controlled_device(
    layout: &'static Layout,
    settings: &storage::Settings,
    plugged: Option<Option<String>>,
    inspection: Option<&Inspection>,
) -> Option<Pilote> {
    let present = plugged.is_some();
    let serial = crate::serie_connue(inspection, plugged.flatten());
    if settings.device_state(layout.vid, layout.pid, serial.as_deref()) != DeviceState::Adopted {
        return None;
    }
    // Unplugged wins over a handle still open: until the render loop closes
    // it, that handle only leads to failures.
    let availability = match (present, inspection.is_some()) {
        (false, _) => Availability::Unplugged,
        (true, true) => Availability::Open,
        (true, false) => Availability::NotOpen,
    };
    Some(Pilote {
        layout,
        device: DeviceRef::of(layout),
        availability,
    })
}

/// What a device submenu shows and allows, derived from its availability.
#[derive(Debug, PartialEq, Eq)]
struct Presentation {
    title: String,
    /// A line explaining why nothing can be done, when that is the case.
    reason: Option<&'static str>,
    effects: bool,
    output: bool,
    turn_off: bool,
}

/// Only an open device offers actions; the others say why they do not.
///
/// The actions still re-read the state when clicked (see the module header):
/// greying items is honesty in the menu, not the protection.
fn presentation(device: &Pilote, loop_running: bool) -> Presentation {
    let name = device.layout.name;
    match device.availability {
        Availability::Open => Presentation {
            title: name.to_owned(),
            reason: None,
            effects: true,
            // The toggle lives in the loop: without a loop there is nothing to toggle.
            output: loop_running,
            turn_off: true,
        },
        Availability::NotOpen => Presentation {
            title: format!("{name} — non ouvert"),
            reason: Some("Branché mais non ouvert — voir la fenêtre"),
            effects: false,
            output: false,
            turn_off: false,
        },
        Availability::Unplugged => Presentation {
            title: format!("{name} — débranché"),
            reason: None,
            effects: false,
            output: false,
            turn_off: false,
        },
    }
}

/// The submenus of the controlled devices, built on the **current** state.
///
/// Kept apart from the rest of the menu because it is the only part that
/// depends on the disk and on USB: see [`menu`], which treats its failure as a
/// degradation and not as a refusal.
fn appareils(app: &AppHandle) -> CmdResult<Vec<Submenu<Wry>>> {
    let store = storage::store(app)?;
    let settings = store.read_settings()?;
    let bibliotheque = store.list_effects()?;
    // The real engine state, not a memory: it is the same source as the
    // `engine_status` command the window reads.
    //
    // **Devices only, never the preview.** This menu describes what the
    // keyboards are doing; ticking here an effect that is only being looked at
    // in the window would be the lie that issue #63 rejects. Nothing to filter —
    // `device_status` cannot return the preview.
    let moteur = app.state::<AppState>().engine.device_status();

    pilotes(&settings, &app.state::<AppState>())
        .iter()
        .map(|pilote| sous_menu(app, pilote, &bibliotheque, &moteur))
        .collect()
}

/// The menu, and what was missing to build it fully.
///
/// The second member is `Some` when the menu is **degraded**: the icon is there,
/// "Ouvrir la fenêtre" (Open window) and "Quitter candeo" too, but the device
/// list is missing. That is deliberately a degradation and not an error — the
/// two remaining items are the ones that depend on nothing, and they are the
/// ones most needed when something is wrong. Refusing to place the icon over an
/// unreadable `settings.json` would, on top of that, make the close button fatal
/// to effects again, for an entire run of the application.
///
/// And saying so **in the menu** is not a stopgap: in `release` the binary is
/// built without a console, and the icon is precisely the place where a failure
/// can be seen without opening one.
fn menu(app: &AppHandle) -> CmdResult<(Menu<Wry>, Option<String>)> {
    let mut articles: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();

    let degrade = match appareils(app) {
        Ok(sous_menus) => {
            if sous_menus.is_empty() {
                // An empty section would read as a broken icon. Naming the
                // absence costs one line; "Ouvrir la fenêtre" sits just below.
                articles.push(Box::new(muet(app, "Aucun appareil piloté")?));
            }
            for appareil in sous_menus {
                articles.push(Box::new(appareil));
            }
            None
        }
        Err(e) => {
            // Never the raw error in the menu: it can hold a local file path,
            // and [`consigner`] already logs it.
            articles.push(Box::new(muet(app, "Appareils indisponibles")?));
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
    // Its own separator, and it is not decorative: closing the window no
    // longer quits, so this item is the application's only exit. Burying it in
    // the list above would amount to hiding it.
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

/// A device's submenu: its effect, its output, turning it off.
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

    // `effect_id` survives the stop of a loop that shut itself down after
    // thirty failures: without the filter, the menu would tick an effect that
    // nothing runs any more.
    let en_cours = etat
        .filter(|s| s.running)
        .and_then(|s| s.effect_id.as_deref());

    let view = presentation(pilote, en_cours.is_some());

    let mut articles: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();
    if let Some(reason) = view.reason {
        articles.push(Box::new(muet(app, reason)?));
        articles.push(Box::new(separateur(app)?));
    }
    for entree in bibliotheque {
        articles.push(Box::new(coche(
            app,
            &Action::Effet {
                device: pilote.device,
                effet: entree.id.clone(),
            },
            &entree.manifest.name,
            view.effects,
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
        view.output,
        etat.is_some_and(|s| s.to_keyboard),
    )?));
    articles.push(Box::new(article(
        app,
        &Action::Eteindre {
            device: pilote.device,
        },
        "Éteindre",
        // Greyed out is display comfort only: what really protects is that
        // [`eteindre`] re-reads the state when clicked. See the module header.
        view.turn_off,
    )?));

    let refs: Vec<&dyn IsMenuItem<Wry>> = articles.iter().map(AsRef::as_ref).collect();
    Submenu::with_items(app, view.title, true, &refs)
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

/// An item that does nothing: it informs, and it is greyed out to say so.
///
/// No identifier, hence no [`Action`]: clicking it is impossible, and giving it
/// one would suggest otherwise.
fn muet(app: &AppHandle, texte: &str) -> CmdResult<MenuItem<Wry>> {
    MenuItem::new(app, texte, false, None::<&str>)
        .map_err(|e| format!("article « {texte} » non créé : {e}"))
}

fn separateur(app: &AppHandle) -> CmdResult<PredefinedMenuItem<Wry>> {
    PredefinedMenuItem::separator(app).map_err(|e| format!("séparateur non créé : {e}"))
}

// ---------------------------------------------------------------- the actions

/// Performs what the item asks for, re-reading the state along the way.
fn agir(app: &AppHandle, action: Action) {
    match action {
        // These two do not touch the engine, and so do not go through
        // [`rendre_compte`]: [`reveler`] already notifies the returning window,
        // and [`quitter`] takes the process down — rebuilding a menu or
        // notifying a window that is being destroyed would only add a failure
        // line on every exit. `app.exit` **returns**: what follows a call to
        // [`quitter`] does run.
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

/// What an action has just invalidated: the menu, and the window.
///
/// Rebuilding the menu **now** is what keeps "the menu reflects the real state"
/// true where no hover is reported, that is, on Linux.
fn rendre_compte(app: &AppHandle) {
    rafraichir(app);
    signaler(app);
}

fn reveler(app: &AppHandle) {
    // The single-instance path, not a second one: it can show a hidden window
    // as well as reopen one from its declaration, and it searches by label
    // alone — a `create: false` window is therefore still the one to open.
    // Writing another one here would make two to keep in agreement.
    if let Err(e) = single_instance::reveal(app) {
        tracing::warn!("fenêtre non ramenée depuis la zone de notification : {e}");
    }
}

/// **The only clean exit.**
///
/// # The lighting is left as is, and that is a choice
///
/// Quitting does not turn the keyboard off. A firmware effect outlives the
/// software shutting down anyway — the firmware runs it — and a keyboard that
/// went dark on quit would surprise more than a keyboard that stays as it was
/// left. "Éteindre" (Turn off) is in the menu, one click away, for anyone who
/// wants darkness.
///
/// The consequence is accepted: a host-loop effect leaves the keyboard on its
/// **last frame**, frozen, and a frozen frame looks like an effect still
/// running. That is the price of the opposite choice from
/// [`crate::release_devices`], which turns the keyboard off because that path,
/// for its part, starts over from a known state.
///
/// The loops are still stopped, and **awaited**: a HID write cut off
/// mid-transfer by the end of the process would leave the device on a partial
/// report. The lighting, for its part, does not change — stopping a loop writes
/// nothing more.
fn quitter(app: &AppHandle) {
    app.state::<AppState>().engine.stop_all();
    tracing::info!("candeo s'arrête, demandé depuis la zone de notification");
    // A code, hence `ExitRequested { code: Some(_) }`: that is what tells this
    // exit apart from the one caused by closing the last window, and therefore
    // what lets it through. See [`crate::run`].
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

    // The window's command, not a copy: starting an effect from the menu must
    // do exactly what the gallery does — same library lookup, same layout, same
    // handle shared with the loop. A second implementation of this path would
    // diverge at the first change.
    if let Err(e) = crate::runtime::start_effect(
        app.clone(),
        app.state(),
        device,
        effet.to_owned(),
        serde_json::Value::Object(params),
    ) {
        // The engine may already have named the cause under its own *span*;
        // what would be missing without this line is **where the request came
        // from** — and the most likely failure here, an effect deleted since
        // the menu was built, is refused before the engine knows anything about
        // it. One line per click floods nobody: the transitions rule targets
        // per-frame logging, not a human gesture.
        tracing::error!(appareil = %device, effet, "effet non lancé depuis la zone de notification : {e}");
    }
}

/// The values to start this effect with, re-read now.
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

/// Toggles the keyboard output, **based on the engine state**.
///
/// Not based on the menu checkbox: muda flips it by itself on click, and it
/// dated from the last build. Trusting it would re-enable an output that had
/// just been cut from the window.
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

/// The firmware's `Effect::Off`: zero cost, and it survives closing.
///
/// The order is that of [`crate::release_devices`], and it matters: the loop
/// stops **and its stop is awaited**, otherwise the next frame would light up
/// again what was just turned off.
fn eteindre(app: &AppHandle, device: DeviceRef) {
    let state = app.state::<AppState>();
    state.engine.stop(device);
    // Nothing runs on this device any more, and the file must say so: leaving
    // the identifier in place would make `settings.json` describe an effect
    // nobody asks for any more. It is the same step `stop_effect` takes from
    // the window.
    crate::runtime::retenir_l_effet_actif(app, device, None);

    if let Err(e) = crate::with_keyboard(&state, device, |kb| {
        kb.set_effect(Effect::Off).map_err(|e| e.to_string())
    }) {
        // The expected case: the device was unplugged — or ignored from the
        // window — while the menu was open. The item was enabled when the menu
        // was built; the device was gone by the time of the click.
        tracing::warn!(appareil = %device, "extinction refusée depuis la zone de notification : {e}");
    }
}

// ---------------------------------------------------------------- installation

/// Places the icon. **Cannot fail**, in the sense that nothing propagates.
///
/// A failure here must not prevent the application from starting: it brings it
/// back to what it was before this issue — a window, and the close button to
/// quit it. [`installee`] carries that switch, and [`crate::run`] reads it.
///
/// That leaves the failures that are not really failures: an unreadable
/// `settings.json` or a broken USB enumeration still place the icon, with a
/// menu that says so. See [`menu`].
pub(crate) fn installer(app: &AppHandle) {
    match poser(app) {
        Ok(()) => {
            INSTALLEE.store(true, Ordering::Relaxed);
            tracing::info!(
                "icône de zone de notification posée, la fenêtre n'est plus la seule commande"
            );
        }
        // `error`: without an icon, closing the window stops the effects — that
        // is exactly the failure this issue closes, and it becomes silent again
        // if nobody reports it.
        Err(e) => tracing::error!(
            "aucune icône de zone de notification, fermer la fenêtre arrêtera les effets : {e}"
        ),
    }
}

fn poser(app: &AppHandle) -> CmdResult<()> {
    // The application icon, not a second image to keep up to date: it is the
    // one the bundle already ships, and the one the user recognizes.
    let icone = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| "aucune icône d'application dans le paquet".to_string())?;

    // The initial menu, degradation included; what was missing is recorded
    // through the same path as later rebuilds, so that only the onset is kept.
    let (depart, degrade) = menu(app)?;
    consigner(degrade.as_deref());

    TrayIconBuilder::with_id(ICONE)
        .icon(icone)
        .tooltip("candeo")
        .menu(&depart)
        // Left click opens the window, right click opens the menu: that is the
        // system tray convention, and it puts "Ouvrir la fenêtre" one click
        // away. On Linux no click is reported, only the right-click menu
        // responds — hence the item, which remains the safe path.
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|icone, evenement| match evenement {
            // Hover precedes the right click: it is the last moment the menu
            // can be rebuilt before it is shown.
            TrayIconEvent::Enter { .. } => rafraichir(icone.app_handle()),
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } => reveler(icone.app_handle()),
            _ => {}
        })
        .on_menu_event(|app, evenement: MenuEvent| {
            // The handler is global: it sees the items of every menu go by.
            // What does not concern us is ignored, not refused.
            if let Some(action) = Action::depuis(evenement.id.as_ref()) {
                agir(app, action);
            }
        })
        .build(app)
        // The handle is dropped: the application's manager keeps one, that is
        // what keeps the icon alive, and [`rafraichir`] finds it again by its
        // id.
        .map(|_| ())
        .map_err(|e| format!("icône non posée : {e}"))
}

/// Rebuilds the menu from the current state.
///
/// The cost is that of an ordinary command: one read of `settings.json`, one
/// read of the library, and one USB enumeration. That is exactly what
/// `list_devices` costs each time the devices screen opens, and it is paid here
/// on the same terms — on a user gesture, never on a timer. A menu rebuilt in
/// the background would enumerate USB forever for a menu nobody is looking at.
pub(crate) fn rafraichir(app: &AppHandle) {
    let Some(icone) = app.tray_by_id(ICONE) else {
        return;
    };

    let echec = match menu(app) {
        // A degraded menu is indeed set: it carries a way to open the window
        // and a way to quit, and it **says** what was missing. What the log
        // keeps of it is the start of the failure, not a line per hover.
        Ok((menu, degrade)) => icone
            .set_menu(Some(menu))
            .map_err(|e| format!("menu non remplacé, l'ancien reste affiché : {e}"))
            .err()
            .or(degrade),
        Err(e) => Some(e),
    };
    consigner(echec.as_deref());
}

/// Logs the **start** of a menu failure, and its recovery. Nothing
/// else — see [`DERNIER_ECHEC`].
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
    /// Two devices, because anything "per device" only makes sense from two
    /// on — and mixing up two identifiers would make the menu act on the wrong
    /// keyboard.
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

    /// **The only link between an item and what it does is a string.** muda
    /// carries nothing else: a write and a read that diverged would yield an
    /// item that does nothing, with no error at compile time or at run time.
    #[test]
    fn every_action_reads_back_as_written() {
        for action in toutes() {
            let id = action.identifiant();
            assert_eq!(
                Action::depuis(&id).as_ref(),
                Some(&action),
                "identifiant « {id} »"
            );
        }
    }

    /// Two devices must not be confused: the menu would act on the wrong
    /// keyboard, and nothing would report it.
    #[test]
    fn two_devices_give_two_identifiers() {
        assert_ne!(
            Action::Eteindre { device: APPAREIL }.identifiant(),
            Action::Eteindre { device: AUTRE }.identifiant()
        );
        assert_ne!(cle(APPAREIL), cle(AUTRE));
    }

    /// The handler is global: it receives the items of every menu in the
    /// application. What does not come from here must be ignored, not
    /// misinterpreted.
    #[test]
    fn a_foreign_identifier_triggers_nothing() {
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
            "3", // an identifier muda numbered itself
        ] {
            assert_eq!(Action::depuis(id), None, "« {id} » a été interprété");
        }
    }

    /// **What makes `:` usable without escaping.** The day the effect identifier
    /// alphabet widened, the menu would start targeting the wrong effect — or
    /// none — without anything saying so.
    #[test]
    fn the_effect_alphabet_excludes_the_separator() {
        assert!(storage::validate_id(&format!("a{SEP}b")).is_err());

        let effet = "a".repeat(64);
        let action = Action::Effet {
            device: APPAREIL,
            effet: effet.clone(),
        };
        storage::validate_id(&effet).expect("identifiant refusé");
        assert_eq!(Action::depuis(&action.identifiant()), Some(action));
    }

    fn inspection_with_serial(serial: &str) -> Inspection {
        Inspection {
            firmware: Err("not read".into()),
            serial: Ok(serial.into()),
            checks: Vec::new(),
        }
    }

    fn adopted(serial: &str) -> storage::Settings {
        let mut settings = storage::Settings::default();
        let layout = &candeo_device::DEATHSTALKER_V2_PRO;
        settings.set_device_state(layout.vid, layout.pid, Some(serial), DeviceState::Adopted);
        settings
    }

    /// #72: the adopted unit is plugged in but not open — another unit of the
    /// model released at startup, or a device its render loop closed. The USB
    /// descriptor gives no serial, so the adoption still matches: the menu must
    /// say "not open" and offer nothing, instead of looking ready.
    #[test]
    fn a_plugged_but_unopened_device_offers_no_action() {
        let settings = adopted("XY01");
        let layout = &candeo_device::DEATHSTALKER_V2_PRO;

        let device =
            controlled_device(layout, &settings, Some(None), None).expect("still controlled");
        assert_eq!(device.availability, Availability::NotOpen);

        let view = presentation(&device, false);
        assert!(!view.effects && !view.output && !view.turn_off, "{view:?}");
        assert!(view.reason.is_some());
        assert_ne!(view.title, layout.name, "the title must not look ready");
    }

    #[test]
    fn an_open_device_offers_its_actions() {
        let settings = adopted("XY01");
        let layout = &candeo_device::DEATHSTALKER_V2_PRO;
        let open = inspection_with_serial("XY01");

        let device =
            controlled_device(layout, &settings, Some(None), Some(&open)).expect("controlled");
        assert_eq!(device.availability, Availability::Open);

        let view = presentation(&device, false);
        assert!(
            view.effects && view.turn_off && view.reason.is_none(),
            "{view:?}"
        );
        assert!(!view.output, "no loop, nothing to toggle");
        assert_eq!(view.title, layout.name);
        assert!(presentation(&device, true).output);
    }

    /// A handle can outlive the unplugging until the render loop closes it: the
    /// enumeration wins, since that handle only leads to failures.
    #[test]
    fn unplugged_wins_over_a_stale_handle() {
        let settings = adopted("XY01");
        let layout = &candeo_device::DEATHSTALKER_V2_PRO;
        let stale = inspection_with_serial("XY01");

        let device =
            controlled_device(layout, &settings, None, Some(&stale)).expect("still controlled");
        assert_eq!(device.availability, Availability::Unplugged);
        assert!(!presentation(&device, true).effects);
    }

    /// The open unit's serial decides, not the model: an open unit that is not
    /// the adopted one is not presented as controlled.
    #[test]
    fn the_open_units_serial_decides_adoption() {
        let mut settings = adopted("XY01");
        let layout = &candeo_device::DEATHSTALKER_V2_PRO;
        settings.set_device_state(layout.vid, layout.pid, Some("XY02"), DeviceState::Ignored);
        let other = inspection_with_serial("XY02");

        assert!(controlled_device(layout, &settings, Some(None), Some(&other)).is_none());
    }

    /// The event name is written on both sides of the IPC, and nothing links the
    /// two at compile time: renaming it on one side only would yield a window
    /// that no longer resynchronizes, without a single error anywhere. Same
    /// guard as for the window label, see [`single_instance`].
    #[test]
    fn the_event_has_the_same_name_on_both_sides() {
        let ts = include_str!("../../src/api/candeo.ts");
        assert!(
            ts.contains(ETAT_CHANGE),
            "« {ETAT_CHANGE} » est introuvable dans src/api/candeo.ts"
        );
    }
}
