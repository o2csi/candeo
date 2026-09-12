//! Sondes matérielles — la forme exécutable du §9 « Reproduire le relevé ».
//!
//! Chaque test ici répond à une question de
//! `docs/protocol/deathstalker-v2-pro.md` en interrogeant un vrai
//! DeathStalker V2 Pro. Tous sont `#[ignore]` : ils exigent l'appareil, donc
//! la CI ne les lance jamais.
//!
//! ```text
//! cargo test -p candeo-desktop sonde -- --ignored --nocapture
//! ```
//!
//! **Le résultat de ces sondes est déjà consigné dans le relevé.** On les garde
//! parce qu'elles sont la manière de le refaire — sur un autre micrologiciel,
//! sur un autre exemplaire, ou pour lever une des questions encore ouvertes.
//!
//! ⚠️ Leçon coûteuse, gravée ici : la sonde des identifiants d'effet a d'abord
//! filtré les réponses tout-à-zéro pour écarter le bruit, **masquant exactement
//! le cas intéressant** (le mode « normal » se lit `00 00`) ; et elle posait
//! `Statique` et `Respiration` sans couleur, donc en **noir** —
//! indistinguables à l'œil d'un effet inexistant. Vérifier par relecture
//! (`0x0f`/`0x82`), jamais par l'œil seul.

#![cfg(test)]

use std::time::Duration;

use candeo_protocol::{checksum, REPORT_LEN};

const VID: u16 = 0x1532;
const PID: u16 = 0x0292;
/// L'éclairage passe par cette interface du composite. Ouvrir la mauvaise donne
/// un handle valide sur lequel toute écriture échoue sans erreur explicite.
const INTERFACE: i32 = 3;

const CLASS_LIGHTING: u8 = 0x0f;
const TRANSACTION: u8 = 0x9f;

/// Construit un rapport de 90 octets, somme de contrôle comprise.
fn report(command: u8, args: &[u8]) -> [u8; REPORT_LEN + 1] {
    let mut r = [0u8; REPORT_LEN];
    r[1] = TRANSACTION;
    r[5] = args.len() as u8;
    r[6] = CLASS_LIGHTING;
    r[7] = command;
    r[8..8 + args.len()].copy_from_slice(args);
    r[88] = checksum(&r);

    // Le tampon de `HidD_SetFeature` fait 91 octets : identifiant de rapport,
    // puis les 90 du rapport.
    let mut buf = [0u8; REPORT_LEN + 1];
    buf[1..].copy_from_slice(&r);
    buf
}

fn ouvrir() -> hidapi::HidDevice {
    let api = hidapi::HidApi::new().expect("HID");
    let info = api
        .device_list()
        .find(|d| {
            d.vendor_id() == VID && d.product_id() == PID && d.interface_number() == INTERFACE
        })
        .expect("DeathStalker V2 Pro introuvable — branché ?");
    info.open_device(&api).expect("ouverture")
}

/// Balaie les identifiants d'effet, sans argument de couleur.
///
/// Connus avant ce relevé : `0x00` Off, `0x03` Spectrum Cycle, `0x04` Wave,
/// `0x08` Direct/custom. Les autres n'ont jamais été essayés.
#[test]
#[ignore]
fn sonde_identifiants_d_effet() {
    let dev = ouvrir();
    let connus = |id: u8| match id {
        0x00 => " (connu : Éteint)",
        0x03 => " (connu : Spectrum Cycle)",
        0x04 => " (connu : Wave)",
        0x08 => " (connu : Direct/custom)",
        _ => "",
    };

    // Pleine luminosité d'abord : un effet invisible parce que le clavier est
    // éteint se lirait comme un identifiant sans effet.
    let _ = dev.send_feature_report(&report(0x04, &[0, 0, 0xff]));
    std::thread::sleep(Duration::from_millis(400));

    println!("\n>>> Balayage des identifiants d'effet, 3 s chacun.");
    println!(">>> Regarde le clavier et note ce que fait chaque numéro.\n");

    for id in 0x00u8..=0x0f {
        println!(">>> identifiant 0x{id:02x}{}", connus(id));
        match dev.send_feature_report(&report(0x02, &[0, 0, id, 0, 0, 0])) {
            Ok(()) => {}
            Err(e) => println!("    écriture refusée : {e}"),
        }
        std::thread::sleep(Duration::from_secs(3));
    }

    // On rend le clavier à un état visible plutôt qu'au dernier essai.
    let _ = dev.send_feature_report(&report(0x02, &[0, 0, 0x03, 0, 0, 0]));
    println!("\n>>> terminé — remis sur Spectrum Cycle.");
}

/// Lit la réponse du périphérique après une commande.
///
/// `[0]` porte l'état : `0x02` = compris, et c'est exactement ce que
/// `reachingKeyboard` ne sait pas distinguer aujourd'hui.
fn lire(dev: &hidapi::HidDevice) -> Option<[u8; REPORT_LEN]> {
    let mut buf = [0u8; REPORT_LEN + 1];
    match dev.get_feature_report(&mut buf) {
        Ok(n) if n >= REPORT_LEN => {
            let mut r = [0u8; REPORT_LEN];
            r.copy_from_slice(&buf[1..=REPORT_LEN]);
            Some(r)
        }
        Ok(n) => {
            println!("    réponse tronquée : {n} octets");
            None
        }
        Err(e) => {
            println!("    lecture refusée : {e}");
            None
        }
    }
}

fn etat(code: u8) -> &'static str {
    match code {
        0x00 => "aucune",
        0x01 => "occupé",
        0x02 => "compris",
        0x03 => "échec",
        0x04 => "expiré",
        0x05 => "non pris en charge",
        _ => "?",
    }
}

/// Construit un rapport pour une classe arbitraire, pas seulement l'éclairage.
fn report_classe(class: u8, command: u8, taille: u8) -> [u8; REPORT_LEN + 1] {
    let mut r = [0u8; REPORT_LEN];
    r[1] = TRANSACTION;
    r[5] = taille;
    r[6] = class;
    r[7] = command;
    r[88] = checksum(&r);
    let mut buf = [0u8; REPORT_LEN + 1];
    buf[1..].copy_from_slice(&r);
    buf
}

/// **Le périphérique répond-il, et que dit-il ?**
///
/// Répond à la case `GET_REPORT` du §9 du relevé, et conditionne #35 : sans
/// lecture, aucune version de micrologiciel n'est accessible — `release_number`
/// ne donne que le `bcdDevice`, figé à 0x0200 alors que le micrologiciel se
/// déclare v1.5.
#[test]
#[ignore]
fn sonde_lecture_reponse() {
    let dev = ouvrir();

    // D'abord une commande dont on SAIT qu'elle agit : si celle-là ne se relit
    // pas, c'est le chemin de lecture qui manque, pas la commande sondée.
    println!("\n>>> Témoin : luminosité (commande validée en écriture).");
    dev.send_feature_report(&report(0x04, &[0, 0, 0xff]))
        .expect("écriture témoin");
    std::thread::sleep(Duration::from_millis(60));
    match lire(&dev) {
        Some(r) => println!(
            "    état 0x{:02x} ({}) · classe 0x{:02x} · commande 0x{:02x} · args {:02x?}",
            r[0],
            etat(r[0]),
            r[6],
            r[7],
            &r[8..16]
        ),
        None => println!("    aucune réponse — le reste de cette sonde ne vaudra rien."),
    }

    // Puis on cherche ce qui rend des octets NON nuls : une version, un nom, un
    // numéro de série — le clavier n'en déclare aucun par USB.
    println!("\n>>> Balayage des commandes de la classe 0x00 (informations).");
    for command in 0x80u8..=0x8f {
        for taille in [0x02u8, 0x04, 0x10, 0x16] {
            dev.send_feature_report(&report_classe(0x00, command, taille))
                .ok();
            std::thread::sleep(Duration::from_millis(40));
            let Some(r) = lire(&dev) else { continue };
            let utile = &r[8..8 + taille as usize];
            if r[0] == 0x02 && utile.iter().any(|&b| b != 0) {
                println!(
                    "    classe 0x00 · commande 0x{command:02x} · taille 0x{taille:02x} → {utile:02x?}  {:?}",
                    String::from_utf8_lossy(utile)
                );
            }
        }
    }

    println!("\n>>> terminé.");
}

/// Relit la classe `0x00` **sans filtrer les octets nuls** — le filtre du
/// premier passage avait masqué `0x84`, dont la réponse attendue est `00 00`.
///
/// Les hypothèses à confirmer sur NOTRE matériel :
/// `0x85` = fréquence d'interrogation (`01` → 1000 Hz),
/// `0x86` = disposition du clavier (`04` → fr_FR).
#[test]
#[ignore]
fn sonde_classe_information() {
    let dev = ouvrir();

    for command in 0x80u8..=0x8f {
        dev.send_feature_report(&report_classe(0x00, command, 0x16))
            .ok();
        std::thread::sleep(Duration::from_millis(40));
        let Some(r) = lire(&dev) else { continue };
        println!(
            "0x{command:02x} → état 0x{:02x} ({:<18}) · écho classe 0x{:02x}/cmd 0x{:02x} · {:02x?}",
            r[0],
            etat(r[0]),
            r[6],
            r[7],
            &r[8..24]
        );
    }
}

/// **L'octet d'état veut-il dire quelque chose ?**
///
/// Une réponse toujours à `0x02` ne prouverait rien. Il faut voir l'appareil
/// *refuser* : c'est ce refus qui ferait de la vérification de protocole autre
/// chose qu'un vœu pieux pour #35.
#[test]
#[ignore]
fn sonde_etat_sur_commande_invalide() {
    let dev = ouvrir();

    let cas: [(&str, [u8; REPORT_LEN + 1]); 4] = [
        ("témoin — luminosité, valide", report(0x04, &[0, 0, 0x80])),
        ("classe inexistante 0xee", report_classe(0xee, 0x01, 0x02)),
        (
            "classe éclairage, commande 0xee",
            report_classe(0x0f, 0xee, 0x02),
        ),
        (
            "taille aberrante sur commande valide",
            report_classe(0x0f, 0x04, 0x50),
        ),
    ];

    for (nom, mut trame) in cas {
        // La somme de contrôle doit rester juste : on teste le refus d'une
        // commande, pas celui d'une trame corrompue.
        let mut r = [0u8; REPORT_LEN];
        r.copy_from_slice(&trame[1..]);
        r[88] = checksum(&r);
        trame[1..].copy_from_slice(&r);

        print!(">>> {nom:<40} ");
        match dev.send_feature_report(&trame) {
            Ok(()) => match lire(&dev) {
                Some(rep) => println!("→ 0x{:02x} ({})", rep[0], etat(rep[0])),
                None => println!("→ pas de réponse"),
            },
            Err(e) => println!("→ écriture refusée : {e}"),
        }
        std::thread::sleep(Duration::from_millis(120));
    }

    let _ = dev.send_feature_report(&report(0x04, &[0, 0, 0xff]));
}

/// Cherche une **relecture** dans la classe éclairage.
///
/// Si l'appareil sait redire l'effet courant, on confirme les identifiants
/// d'effet **sans dépendre de l'œil** — ce qui avait justement manqué au premier
/// balayage, jugé « trop rapide pour dire ce que j'ai vu ».
#[test]
#[ignore]
fn sonde_relecture_eclairage() {
    let dev = ouvrir();

    println!("\n>>> Commandes lisibles de la classe 0x0f.");
    for command in 0x80u8..=0x8f {
        dev.send_feature_report(&report_classe(0x0f, command, 0x03))
            .ok();
        std::thread::sleep(Duration::from_millis(40));
        let Some(r) = lire(&dev) else { continue };
        if r[0] == 0x02 {
            println!(
                "0x{command:02x} → état compris · écho 0x{:02x}/0x{:02x} · {:02x?}",
                r[6],
                r[7],
                &r[8..14]
            );
        }
    }

    // Puis : pose un effet, relis-le. Si la relecture suit, les identifiants
    // sont établis objectivement.
    println!("\n>>> Pose d'un effet, puis relecture.");
    for (nom, id) in [
        ("Éteint", 0x00u8),
        ("Statique", 0x01),
        ("Respiration", 0x02),
        ("Spectre", 0x03),
        ("Vague", 0x04),
        ("Réactif", 0x05),
        ("Étoilé", 0x07),
        ("Direct/custom", 0x08),
    ] {
        // Statique et Respiration veulent une couleur : sans elle, le premier
        // balayage les posait en NOIR — donc « rien ne se passe » à l'œil.
        let args: Vec<u8> = match id {
            0x01 | 0x02 => vec![0, 0, id, 0, 0, 0x01, 0xff, 0x00, 0x00],
            0x04 => vec![0, 0, id, 0x01, 0x28, 0],
            _ => vec![0, 0, id, 0, 0, 0],
        };
        dev.send_feature_report(&report(0x02, &args)).ok();
        std::thread::sleep(Duration::from_millis(150));

        dev.send_feature_report(&report_classe(0x0f, 0x82, 0x03))
            .ok();
        std::thread::sleep(Duration::from_millis(60));
        match lire(&dev) {
            Some(r) => println!(
                "{nom:<14} posé 0x{id:02x} → relu état 0x{:02x} · {:02x?}",
                r[0],
                &r[8..14]
            ),
            None => println!("{nom:<14} posé 0x{id:02x} → pas de relecture"),
        }
    }

    let _ = dev.send_feature_report(&report(0x02, &[0, 0, 0x03, 0, 0, 0]));
    println!("\n>>> remis sur Spectre.");
}

/// Détaille les réponses de `0x0f`/`0x80` et `0x81`, qui ressemblent à des
/// descripteurs : l'une porte `06 16` — soit exactement nos 6 rangées × 22
/// colonnes — l'autre une suite `00 01 02 03 04` qui pourrait énumérer les
/// effets réellement pris en charge.
#[test]
#[ignore]
fn sonde_descripteurs_eclairage() {
    let dev = ouvrir();
    for command in [0x80u8, 0x81, 0x86] {
        for taille in [0x03u8, 0x16] {
            dev.send_feature_report(&report_classe(0x0f, command, taille))
                .ok();
            std::thread::sleep(Duration::from_millis(50));
            let Some(r) = lire(&dev) else { continue };
            println!(
                "0x{command:02x} taille 0x{taille:02x} → état 0x{:02x} · {:02x?}",
                r[0],
                &r[8..40]
            );
        }
    }
}

/// Ce que l'énumération HID donne **sans protocole**, sur chaque interface.
#[test]
#[ignore]
fn sonde_descripteur_usb() {
    let api = hidapi::HidApi::new().expect("HID");
    for d in api
        .device_list()
        .filter(|d| d.vendor_id() == VID && d.product_id() == PID)
    {
        let r = d.release_number();
        println!(
            "interface {:>2} | release_number = 0x{r:04x} (soit {}.{:02}) | série {:?} | produit {:?}",
            d.interface_number(),
            r >> 8,
            r & 0xff,
            d.serial_number().unwrap_or("—"),
            d.product_string().unwrap_or("—"),
        );
    }
}
