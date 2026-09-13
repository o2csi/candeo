//! Ce qu'on demande à un appareil **une fois**, à l'ouverture.
//!
//! # Pourquoi ce module existe
//!
//! `reachingKeyboard` prouve qu'une écriture a été *acceptée* par le système, pas
//! qu'elle a été *comprise* par l'appareil. Qu'une mise à jour du micrologiciel
//! déplace un octet ou renumérote une commande, et `hidapi` accepterait toujours
//! le transfert, `present()` rendrait `Ok`, et le clavier jetterait nos images en
//! silence — un voyant vert au-dessus d'un clavier figé. Le relevé a établi que
//! l'appareil **répond** (§8) : c'est ici qu'on lui fait dire ce qu'il est.
//!
//! # Ce que l'inspection établit, et ce qu'elle n'établit pas
//!
//! | Question | Réponse | Portée |
//! |---|---|---|
//! | quel micrologiciel ? | `0x00`/`0x81` | comparé au relevé : **avertit, ne bloque pas** |
//! | quel exemplaire ? | `0x00`/`0x82` | le descripteur USB n'en porte aucun |
//! | cette commande existe-t-elle ? | état `0x05` ou `0x02` | le couple classe / commande, **rien de plus** |
//!
//! ⚠️ **L'octet d'état ne valide jamais un argument.** Poser l'effet `0x05`, que
//! ce clavier refuse, rend `0x02` « compris ». Une commande [`Verdict::Understood`]
//! est donc une commande **connue**, et c'est tout : ce module ne peut pas dire
//! « mes arguments sont bons », et rien de ce qu'il rend ne doit le laisser
//! croire.
//!
//! # Émettre sans rien changer
//!
//! Vérifier qu'une commande existe demande de l'**émettre**, et une commande
//! d'éclairage émise se voit. On n'émet donc que ce qu'on sait réécrire **à
//! l'identique** : on relit l'état courant, on le réécrit tel quel, on relit
//! encore.
//!
//! - **luminosité** — relue par `0x0f`/`0x84`, réécrite telle quelle ;
//! - **effet** — relu par `0x0f`/`0x82`, réécrit tel quel **seulement** s'il est de
//!   ceux dont la relecture rend tous les arguments. `Statique` et `Respiration`
//!   portent une couleur dont la place dans la relecture n'est pas établie : les
//!   réécrire pourrait les éteindre, on s'abstient et on le dit. Au pire, un effet
//!   animé par le micrologiciel repart du début de son cycle ;
//! - **rangée** — **jamais émise.** Aucune relecture de couleur n'existe, donc
//!   aucune rangée écrite n'est invisible.
//!
//! La seconde relecture ne détecte pas une commande **ignorée** — réécrire la
//! valeur courante sans effet la laisse identique. Elle détecte une commande
//! **comprise autrement** : un micrologiciel qui lirait l'argument ailleurs
//! poserait une autre valeur, et c'est ce qu'on relirait.
//!
//! Sans relecture réussie, aucune écriture ne part : chaque réécriture est
//! conditionnée à la lecture qui la précède.
//!
//! # Une fois, jamais dans la boucle
//!
//! Relire coûte un aller-retour USB, et la boucle de rendu est serrée : l'écriture
//! d'une image complète prend 13 à 14 ms dans une période de 33,3 ms. Le verdict
//! est donc pris à l'ouverture et **gardé** : [`crate::Keyboard`] refuse ensuite
//! d'envoyer une commande rendue `0x05`, sans rien relire.

use std::time::Duration;

use candeo_protocol::{
    CommandId, Effect, Firmware, Report, Response, Status, SET_BRIGHTNESS, SET_EFFECT, WRITE_ROW,
};

use crate::Layout;

/// Relectures accordées à une réponse « occupé ».
///
/// Jamais observé au relevé — les lectures immédiates y rendaient directement
/// leur état — mais prévu par le protocole. Borné : une ouverture ne doit pas se
/// suspendre sur un appareil qui se dirait occupé indéfiniment.
const RELECTURES: u32 = 5;

/// Attente entre deux relectures « occupé ». Cinq fois dix millisecondes restent
/// sous la période d'une image, et ne sont payées qu'une fois par ouverture.
const PATIENCE: Duration = Duration::from_millis(10);

/// Ce qu'une inspection demande d'un périphérique.
///
/// Un joint plutôt que `hidapi::HidDevice` en dur, pour la seule raison qui
/// vaille : c'est ce qui rend le déroulé vérifiable **sans matériel** — un
/// appareil qui refuse, qui répond occupé, qui comprend un argument de travers.
/// Sans lui, ces cas ne se vérifieraient qu'avec le micrologiciel qui les
/// produit, donc jamais avant qu'il ne sorte.
pub(crate) trait Transport {
    fn send(&self, data: &[u8]) -> Result<(), String>;
    fn receive(&self, buf: &mut [u8]) -> Result<usize, String>;
}

impl Transport for hidapi::HidDevice {
    fn send(&self, data: &[u8]) -> Result<(), String> {
        hidapi::HidDevice::send_feature_report(self, data).map_err(|e| e.to_string())
    }

    fn receive(&self, buf: &mut [u8]) -> Result<usize, String> {
        hidapi::HidDevice::get_feature_report(self, buf).map_err(|e| e.to_string())
    }
}

/// Ce que l'appareil a dit de lui-même à l'ouverture.
///
/// Chaque champ porte **sa** raison d'échec plutôt qu'un `Option` muet : « non
/// lu » sans dire pourquoi est exactement le genre de silence qui envoie
/// chercher une panne d'appareil là où il n'y a qu'une permission manquante.
#[derive(Clone, PartialEq, Eq)]
pub struct Inspection {
    /// La version, ou pourquoi elle n'a pas été lue.
    pub firmware: Result<Firmware, String>,
    /// Le numéro de série, ou pourquoi il n'a pas été lu.
    ///
    /// ⚠️ **Il identifie un exemplaire précis.** Rien de ce qui se journalise ou
    /// se copie dans un rapport de bogue ne doit le porter en clair.
    pub serial: Result<String, String>,
    /// Une vérification par commande d'écriture dont dépend l'éclairage.
    pub checks: Vec<Check>,
}

/// Écrit à la main, et pour une seule raison : **la série n'y figure pas.**
///
/// Un `{:?}` glissé dans un `tracing::debug!` le jour où l'on cherche une panne
/// est le chemin le plus court vers un numéro de série collé dans une issue.
/// L'empreinte stable qui la remplace dans le journal vit côté application ;
/// ici, on se contente de ne rien divulguer.
impl std::fmt::Debug for Inspection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let serie = match &self.serial {
            Ok(s) => format!("Ok(<masquée, {} caractères>)", s.chars().count()),
            Err(e) => format!("Err({e:?})"),
        };
        f.debug_struct("Inspection")
            .field("firmware", &self.firmware)
            .field("serial", &format_args!("{serie}"))
            .field("checks", &self.checks)
            .finish()
    }
}

/// Le verdict sur **une** commande.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub command: CommandId,
    /// Le nom qu'on lui donne en parlant à quelqu'un : « luminosité »…
    pub name: &'static str,
    pub verdict: Verdict,
}

/// Ce que l'appareil a répondu à une commande émise à l'ouverture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Réécrite à l'identique, rendue `0x02`, et relue telle quelle.
    ///
    /// **Connue**, et rien de plus : l'octet d'état ne valide aucun argument.
    Understood,
    /// L'appareil a rendu `0x05` : ce micrologiciel ne connaît pas la commande.
    /// [`crate::Keyboard`] ne l'enverra plus.
    Unsupported,
    /// Rendue `0x02`, mais la relecture ne rend pas ce qui a été réécrit.
    ///
    /// Le seul cas où l'appareil dit « compris » et montre le contraire : il
    /// interprète l'argument autrement qu'au relevé.
    ReadBackDiffers { wrote: String, read: String },
    /// Pas émise, ou sans conclusion possible — et pourquoi.
    Unverified(String),
}

impl Inspection {
    /// Vrai si l'appareil a déclaré ne pas connaître cette commande.
    ///
    /// Seul [`Verdict::Unsupported`] refuse : une commande non vérifiée n'est pas
    /// une commande refusée, et la bloquer rendrait l'appareil muet pour une
    /// question à laquelle on n'a simplement pas pu répondre.
    pub fn refuses(&self, command: CommandId) -> bool {
        self.checks
            .iter()
            .any(|c| c.command == command && c.verdict == Verdict::Unsupported)
    }

    /// Ce qui mérite d'être **vu**, dans la langue de l'interface.
    ///
    /// Une liste vide veut dire « rien à signaler », **pas** « compatible » : une
    /// commande non vérifiée n'y figure pas, parce que ce n'est pas une anomalie
    /// — c'est la limite de ce qu'on peut demander sans rien changer au clavier.
    ///
    /// Aucun avertissement ne bloque quoi que ce soit. Bloquer sur une version
    /// différente rendrait l'application inutile après une mise à jour de
    /// routine, alors que le protocole n'aura très probablement pas bougé ;
    /// l'avertissement transforme une panne muette en soupçon énoncé.
    pub fn warnings(&self, layout: &Layout) -> Vec<String> {
        let releve = layout.surveyed_firmware;
        let mut out = Vec::new();
        match &self.firmware {
            Ok(lu) if *lu != releve => out.push(format!(
                "Micrologiciel {lu}, alors que ce gabarit a été relevé sur {releve}. Le protocole \
                 n'a probablement pas bougé ; mais si le clavier n'obéit pas, c'est la première \
                 piste."
            )),
            Ok(_) => {}
            Err(e) => out.push(format!(
                "Version du micrologiciel non lue ({e}) : impossible de la comparer à celle du \
                 relevé ({releve})."
            )),
        }
        for c in &self.checks {
            match &c.verdict {
                Verdict::Unsupported => out.push(format!(
                    "Ce micrologiciel ne connaît pas la commande « {} » ({}) : elle n'est plus \
                     envoyée.",
                    c.name, c.command
                )),
                Verdict::ReadBackDiffers { wrote, read } => out.push(format!(
                    "Commande « {} » ({}) acceptée, mais l'appareil relit {read} après qu'on a \
                     réécrit {wrote} : il ne la comprend plus comme au relevé.",
                    c.name, c.command
                )),
                Verdict::Understood | Verdict::Unverified(_) => {}
            }
        }
        out
    }
}

/// Inspecte un appareil qu'on vient d'ouvrir. **Ne peut pas échouer** : chaque
/// question sans réponse devient une raison, jamais une erreur d'ouverture.
pub(crate) fn inspect(t: &impl Transport) -> Inspection {
    let firmware = read(t, &Report::read_firmware())
        .and_then(|r| r.firmware().ok_or_else(|| "réponse vide".to_string()));
    let serial = read(t, &Report::read_serial())
        .and_then(|r| r.serial().ok_or_else(|| "réponse illisible".to_string()));

    let checks = vec![
        Check {
            command: SET_BRIGHTNESS,
            name: "luminosité",
            verdict: check_brightness(t),
        },
        Check {
            command: SET_EFFECT,
            name: "effet",
            verdict: check_effect(t),
        },
        Check {
            command: WRITE_ROW,
            name: "rangée",
            verdict: Verdict::Unverified(
                "jamais émise à l'ouverture : aucune relecture de couleur n'existe, toute rangée \
                 écrite se verrait sur le clavier"
                    .into(),
            ),
        },
    ];

    Inspection {
        firmware,
        serial,
        checks,
    }
}

fn check_brightness(t: &impl Transport) -> Verdict {
    let niveau = match read(t, &Report::read_brightness()) {
        Ok(r) => match r.brightness() {
            Some(n) => n,
            None => return Verdict::Unverified("luminosité relue sous une forme inconnue".into()),
        },
        Err(e) => return Verdict::Unverified(format!("luminosité non relue, rien émis : {e}")),
    };
    rewrite(
        t,
        &Report::set_brightness(niveau),
        &Report::read_brightness(),
        niveau,
        Response::brightness,
        |n| n.to_string(),
    )
}

fn check_effect(t: &impl Transport) -> Verdict {
    let effet = match read(t, &Report::read_effect()) {
        Ok(r) => match r.effect() {
            Some(e) => e,
            None => {
                return Verdict::Unverified(
                    "l'effet en cours ne se réécrit pas à l'identique (couleur ou forme non \
                     établie au relevé), rien émis"
                        .into(),
                )
            }
        },
        Err(e) => return Verdict::Unverified(format!("effet non relu, rien émis : {e}")),
    };
    rewrite(
        t,
        &Report::set_effect(effet),
        &Report::read_effect(),
        effet,
        Response::effect,
        nommer,
    )
}

/// Réécrit une valeur relue, puis la relit.
fn rewrite<T: PartialEq + Copy>(
    t: &impl Transport,
    write: &Report,
    reread: &Report,
    avant: T,
    decode: impl Fn(&Response) -> Option<T>,
    dire: impl Fn(T) -> String,
) -> Verdict {
    let reponse = match exchange(t, write) {
        Ok(r) => r,
        Err(e) => return Verdict::Unverified(format!("réécriture sans réponse : {e}")),
    };
    match reponse.status() {
        Status::Understood => {}
        Status::Unsupported => return Verdict::Unsupported,
        autre => return Verdict::Unverified(format!("réécriture rendue {autre}")),
    }
    match read(t, reread) {
        Ok(r) => match decode(&r) {
            Some(apres) if apres == avant => Verdict::Understood,
            Some(apres) => Verdict::ReadBackDiffers {
                wrote: dire(avant),
                read: dire(apres),
            },
            None => Verdict::ReadBackDiffers {
                wrote: dire(avant),
                read: "une forme inconnue".into(),
            },
        },
        Err(e) => Verdict::Unverified(format!("réécriture comprise, mais pas relue : {e}")),
    }
}

/// Un effet, tel qu'on le nomme à quelqu'un — pas le nom d'une variante Rust.
fn nommer(effet: Effect) -> String {
    match effet {
        Effect::Off => "éteint".into(),
        Effect::SpectrumCycle => "spectre".into(),
        Effect::Wave { direction, speed } => {
            format!("vague (direction {direction}, vitesse {speed})")
        }
        Effect::Custom => "piloté par l'hôte".into(),
    }
}

/// Une lecture, qui n'a de valeur que comprise.
fn read(t: &impl Transport, request: &Report) -> Result<Response, String> {
    let r = exchange(t, request)?;
    match r.status() {
        Status::Understood => Ok(r),
        autre => Err(format!("{} rend l'état {autre}", request.id())),
    }
}

/// Émet une commande et relit **sa** réponse, quel qu'en soit l'état.
///
/// L'écho est vérifié : sans lui, une commande émise entre-temps sur la même
/// interface — une boucle de rendu tenant une autre poignée — ferait lire sa
/// réponse pour la nôtre. Mieux vaut ne pas conclure que conclure sur la
/// réponse d'un autre.
fn exchange(t: &impl Transport, request: &Report) -> Result<Response, String> {
    t.send(&request.to_feature_buffer())
        .map_err(|e| format!("écriture refusée : {e}"))?;
    for _ in 0..RELECTURES {
        let mut buf = Response::buffer();
        let lus = t
            .receive(&mut buf)
            .map_err(|e| format!("lecture refusée : {e}"))?;
        let r = Response::from_feature_buffer(&buf, lus)
            .ok_or_else(|| format!("réponse tronquée : {lus} octets"))?;
        if r.status() == Status::Busy {
            std::thread::sleep(PATIENCE);
            continue;
        }
        if !r.answers(request) {
            return Err(format!(
                "réponse à {} relue au lieu de {}",
                r.id(),
                request.id()
            ));
        }
        return Ok(r);
    }
    Err(format!("toujours occupé après {RELECTURES} relectures"))
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::DEATHSTALKER_V2_PRO;

    /// Un appareil simulé, qui répond comme le relevé l'a décrit — sauf là où un
    /// test lui demande de ne pas le faire.
    struct Faux {
        etat: RefCell<Etat>,
    }

    struct Etat {
        version: [u8; 2],
        serie: &'static [u8],
        /// Les six octets d'arguments de l'effet courant.
        effet: [u8; 6],
        luminosite: u8,
        /// Commandes auxquelles l'appareil répond `0x05`.
        inconnues: Vec<CommandId>,
        /// Lit l'argument de luminosité un octet trop tôt — un micrologiciel qui
        /// aurait déplacé l'argument.
        luminosite_decalee: bool,
        /// Réponses « occupé » à rendre avant la vraie.
        occupe: u32,
        /// Toute relecture échoue, comme sans accès en lecture.
        sourd: bool,
        /// Écho forcé, comme si une autre commande avait été émise entre-temps.
        echo: Option<CommandId>,
        derniere: Option<Report>,
        emises: Vec<CommandId>,
    }

    impl Faux {
        fn conforme() -> Self {
            let spectre = Report::set_effect(Effect::SpectrumCycle).0;
            let mut effet = [0u8; 6];
            effet.copy_from_slice(&spectre[8..14]);
            Self {
                etat: RefCell::new(Etat {
                    version: [0x01, 0x05],
                    serie: b"XY24ABCDEFG0001",
                    effet,
                    luminosite: 0xff,
                    inconnues: Vec::new(),
                    luminosite_decalee: false,
                    occupe: 0,
                    sourd: false,
                    echo: None,
                    derniere: None,
                    emises: Vec::new(),
                }),
            }
        }

        fn avec(self, f: impl FnOnce(&mut Etat)) -> Self {
            f(&mut self.etat.borrow_mut());
            self
        }

        fn emises(&self) -> Vec<CommandId> {
            self.etat.borrow().emises.clone()
        }
    }

    impl Transport for Faux {
        fn send(&self, data: &[u8]) -> Result<(), String> {
            let mut r = [0u8; 90];
            r.copy_from_slice(&data[1..]);
            let report = Report(r);
            let id = report.id();
            let mut e = self.etat.borrow_mut();
            e.emises.push(id);
            if !e.inconnues.contains(&id) {
                if id == SET_EFFECT {
                    e.effet.copy_from_slice(&r[8..14]);
                }
                if id == SET_BRIGHTNESS {
                    e.luminosite = if e.luminosite_decalee { r[9] } else { r[10] };
                }
            }
            e.derniere = Some(report);
            Ok(())
        }

        fn receive(&self, buf: &mut [u8]) -> Result<usize, String> {
            let mut e = self.etat.borrow_mut();
            if e.sourd {
                return Err("accès en lecture refusé".into());
            }
            let demande = e.derniere.as_ref().expect("lecture sans commande").id();
            buf.fill(0);
            if e.occupe > 0 {
                e.occupe -= 1;
                buf[1] = 0x01;
                return Ok(buf.len());
            }
            let echo = e.echo.unwrap_or(demande);
            buf[1 + 6] = echo.class;
            buf[1 + 7] = echo.command;
            if e.inconnues.contains(&demande) {
                buf[1] = 0x05;
                return Ok(buf.len());
            }
            buf[1] = 0x02;
            let args = &mut buf[1 + 8..];
            match (demande.class, demande.command) {
                (0x00, 0x81) => args[..2].copy_from_slice(&e.version),
                (0x00, 0x82) => args[..e.serie.len()].copy_from_slice(e.serie),
                (0x0f, 0x82) => args[..6].copy_from_slice(&e.effet),
                (0x0f, 0x84) => args[2] = e.luminosite,
                _ => {}
            }
            Ok(buf.len())
        }
    }

    fn verdict(i: &Inspection, command: CommandId) -> &Verdict {
        &i.checks
            .iter()
            .find(|c| c.command == command)
            .expect("commande non vérifiée")
            .verdict
    }

    /// Le cas du relevé : v1.5, série lue, luminosité et effet connus.
    #[test]
    fn un_appareil_conforme_au_releve_ne_souleve_rien() {
        let faux = Faux::conforme();
        let i = inspect(&faux);

        assert_eq!(i.firmware, Ok(Firmware { major: 1, minor: 5 }));
        assert_eq!(i.serial.as_deref(), Ok("XY24ABCDEFG0001"));
        assert_eq!(verdict(&i, SET_BRIGHTNESS), &Verdict::Understood);
        assert_eq!(verdict(&i, SET_EFFECT), &Verdict::Understood);
        assert!(i.warnings(&DEATHSTALKER_V2_PRO).is_empty());
        assert!(!i.refuses(SET_EFFECT));
    }

    /// **L'inspection ne change rien à ce que montre le clavier.** C'est la
    /// condition pour avoir le droit d'émettre à chaque ouverture.
    #[test]
    fn l_inspection_laisse_le_clavier_tel_qu_elle_l_a_trouve() {
        let vague = Report::set_effect(Effect::Wave {
            direction: 0x02,
            speed: 0x28,
        })
        .0;
        let faux = Faux::conforme().avec(|e| {
            e.effet.copy_from_slice(&vague[8..14]);
            e.luminosite = 0x40;
        });
        let i = inspect(&faux);

        assert_eq!(verdict(&i, SET_EFFECT), &Verdict::Understood);
        let e = faux.etat.borrow();
        assert_eq!(e.effet[..], vague[8..14], "l'effet a changé");
        assert_eq!(e.luminosite, 0x40, "la luminosité a changé");
    }

    /// Une autre version **avertit**, et rien d'autre : aucune commande n'est
    /// refusée pour autant.
    #[test]
    fn une_autre_version_avertit_sans_bloquer() {
        let faux = Faux::conforme().avec(|e| e.version = [0x01, 0x06]);
        let i = inspect(&faux);

        let avertissements = i.warnings(&DEATHSTALKER_V2_PRO);
        assert_eq!(avertissements.len(), 1, "{avertissements:?}");
        assert!(avertissements[0].contains("v1.6"), "{avertissements:?}");
        assert!(avertissements[0].contains("v1.5"), "{avertissements:?}");
        assert!(!i.refuses(SET_EFFECT) && !i.refuses(SET_BRIGHTNESS));
    }

    /// L'appareil dit ne pas connaître la commande : elle est nommée à l'écran,
    /// et le clavier ne l'enverra plus.
    #[test]
    fn une_commande_inconnue_est_nommee_et_refusee() {
        let faux = Faux::conforme().avec(|e| e.inconnues.push(SET_EFFECT));
        let i = inspect(&faux);

        assert_eq!(verdict(&i, SET_EFFECT), &Verdict::Unsupported);
        assert!(i.refuses(SET_EFFECT));
        assert!(!i.refuses(SET_BRIGHTNESS));
        let avertissements = i.warnings(&DEATHSTALKER_V2_PRO);
        assert!(
            avertissements.iter().any(|a| a.contains("« effet »")),
            "{avertissements:?}"
        );
    }

    /// Un micrologiciel qui lirait l'argument ailleurs rendrait `0x02` et
    /// poserait autre chose. C'est la seconde relecture qui le voit — l'octet
    /// d'état, lui, n'en saurait rien.
    #[test]
    fn un_argument_compris_autrement_se_voit_a_la_relecture() {
        let faux = Faux::conforme().avec(|e| e.luminosite_decalee = true);
        let i = inspect(&faux);

        assert!(matches!(
            verdict(&i, SET_BRIGHTNESS),
            Verdict::ReadBackDiffers { .. }
        ));
        // Comprise autrement n'est pas inconnue : on avertit, on n'interdit pas.
        assert!(!i.refuses(SET_BRIGHTNESS));
        assert_eq!(i.warnings(&DEATHSTALKER_V2_PRO).len(), 1);
    }

    /// `Statique` porte une couleur dont la place dans la relecture n'est pas
    /// établie : le réécrire pourrait l'éteindre. Rien n'est émis.
    #[test]
    fn un_effet_colore_n_est_jamais_reecrit() {
        let faux = Faux::conforme().avec(|e| e.effet = [0, 0, 0x01, 0, 0, 0x01]);
        let i = inspect(&faux);

        assert!(matches!(verdict(&i, SET_EFFECT), Verdict::Unverified(_)));
        assert!(
            !faux.emises().contains(&SET_EFFECT),
            "l'effet a été réécrit"
        );
        assert!(
            i.warnings(&DEATHSTALKER_V2_PRO).is_empty(),
            "une limite n'est pas une anomalie"
        );
    }

    /// Aucune rangée n'est invisible : l'inspection n'en écrit jamais.
    #[test]
    fn aucune_rangee_n_est_emise() {
        let faux = Faux::conforme();
        let i = inspect(&faux);

        assert!(!faux.emises().contains(&WRITE_ROW));
        assert!(matches!(verdict(&i, WRITE_ROW), Verdict::Unverified(_)));
        // Et une commande non vérifiée n'est pas refusée : le clavier doit
        // pouvoir recevoir ses images.
        assert!(!i.refuses(WRITE_ROW));
    }

    /// **Le mode pilote ne s'écrit pas** (§8 du relevé) : dans la classe `0x00`,
    /// l'inspection ne fait que lire.
    #[test]
    fn l_inspection_n_ecrit_rien_dans_la_classe_information() {
        let faux = Faux::conforme();
        inspect(&faux);
        for id in faux.emises() {
            assert!(
                id.class != 0x00 || id.command >= 0x80,
                "{id} écrit dans la classe information"
            );
        }
    }

    /// Sans lecture possible — hidraw sans droit, pilote qui ne relaie pas —
    /// **aucune écriture ne part** : chaque réécriture attend sa relecture.
    #[test]
    fn sans_relecture_aucune_ecriture_ne_part() {
        let faux = Faux::conforme().avec(|e| e.sourd = true);
        let i = inspect(&faux);

        assert!(i.firmware.is_err());
        assert!(i.serial.is_err());
        let emises = faux.emises();
        assert!(!emises.contains(&SET_EFFECT), "{emises:?}");
        assert!(!emises.contains(&SET_BRIGHTNESS), "{emises:?}");
        // La version non lue se dit, puisqu'on ne peut plus la comparer.
        let avertissements = i.warnings(&DEATHSTALKER_V2_PRO);
        assert_eq!(avertissements.len(), 1, "{avertissements:?}");
        assert!(avertissements[0].contains("non lue"));
    }

    /// Le numéro de série ne sort pas par un `{:?}` distrait.
    #[test]
    fn le_format_de_debogage_ne_divulgue_pas_la_serie() {
        let i = inspect(&Faux::conforme());
        let texte = format!("{i:?}");
        assert!(!texte.contains("XY24ABCDEFG0001"), "{texte}");
        assert!(
            texte.contains("v1.5") || texte.contains("major: 1"),
            "{texte}"
        );
    }

    /// Prévu par le protocole, jamais observé : une réponse « occupé » se relit.
    #[test]
    fn une_reponse_occupee_se_relit() {
        let faux = Faux::conforme().avec(|e| e.occupe = 2);
        let i = inspect(&faux);
        assert_eq!(i.firmware, Ok(Firmware { major: 1, minor: 5 }));
    }

    /// La réponse d'une autre commande n'est pas prise pour la nôtre, même quand
    /// ses octets s'y prêteraient.
    #[test]
    fn la_reponse_d_une_autre_commande_ne_conclut_rien() {
        let faux = Faux::conforme().avec(|e| e.echo = Some(WRITE_ROW));
        let i = inspect(&faux);

        assert!(i.firmware.is_err());
        assert!(matches!(verdict(&i, SET_EFFECT), Verdict::Unverified(_)));
        assert!(!faux.emises().contains(&SET_EFFECT));
    }
}
