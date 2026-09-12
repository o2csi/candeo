//! Construction des rapports du protocole d'éclairage Razer.
//!
//! Structure établie par capture du bus USB — voir `docs/protocol/`.
//! Aucun code tiers n'a été consulté : ce module dérive uniquement de trames
//! observées sur le matériel.

/// Taille du rapport, hors identifiant de rapport HID.
pub const REPORT_LEN: usize = 90;

/// Taille du tampon passé à `HidD_SetFeature` : identifiant de rapport + données.
pub const FEATURE_BUF_LEN: usize = REPORT_LEN + 1;

/// Classe de commande « éclairage ».
const CLASS_LIGHTING: u8 = 0x0f;

/// Identifiant de transaction observé sur toutes les trames capturées.
/// Aucune variation n'a été constatée ; l'appareil ne semble pas le vérifier,
/// mais on reproduit la valeur d'origine par prudence.
const TRANSACTION_ID: u8 = 0x9f;

/// Effets pris en charge par le micrologiciel.
///
/// Les variantes autres que [`Effect::Custom`] sont animées par l'appareil
/// lui-même : elles survivent à l'extinction du logiciel hôte et ne coûtent
/// aucun temps processeur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Off,
    SpectrumCycle,
    /// `direction` et `speed` observés à 0x02 et 0x28 respectivement.
    Wave {
        direction: u8,
        speed: u8,
    },
    /// Mode piloté par l'hôte : l'appareil n'affiche que ce qu'on lui pousse.
    Custom,
}

impl Effect {
    fn args(self) -> [u8; 6] {
        match self {
            Effect::Off => [0, 0, 0x00, 0, 0, 0],
            Effect::SpectrumCycle => [0, 0, 0x03, 0, 0, 0],
            Effect::Wave { direction, speed } => [0, 0, 0x04, direction, speed, 0],
            Effect::Custom => [0, 0, 0x08, 0, 0, 0],
        }
    }
}

/// Un rapport de 90 octets prêt à être envoyé.
#[derive(Debug, Clone)]
pub struct Report(pub [u8; REPORT_LEN]);

impl Report {
    fn new(command_id: u8, args: &[u8]) -> Self {
        assert!(args.len() <= REPORT_LEN - 10, "arguments trop longs");
        let mut r = [0u8; REPORT_LEN];
        r[1] = TRANSACTION_ID;
        r[5] = args.len() as u8;
        r[6] = CLASS_LIGHTING;
        r[7] = command_id;
        r[8..8 + args.len()].copy_from_slice(args);
        r[88] = checksum(&r);
        Self(r)
    }

    /// Sélectionne un effet. Commande `0x0f` / `0x02`.
    pub fn set_effect(effect: Effect) -> Self {
        Self::new(0x02, &effect.args())
    }

    /// Règle la luminosité globale. Commande `0x0f` / `0x04`.
    pub fn set_brightness(level: u8) -> Self {
        Self::new(0x04, &[0, 0, level])
    }

    /// Écrit un segment de rangée. Commande `0x0f` / `0x03`.
    ///
    /// L'écriture partielle est prise en charge : `col_start` et `col_end`
    /// délimitent le segment, ce qui a été vérifié sur le matériel.
    pub fn write_row(row: u8, col_start: u8, colors: &[Rgb]) -> Self {
        assert!(!colors.is_empty(), "segment vide");
        let col_end = col_start + colors.len() as u8 - 1;
        let mut args = Vec::with_capacity(5 + colors.len() * 3);
        args.extend_from_slice(&[0, 0, row, col_start, col_end]);
        for c in colors {
            args.extend_from_slice(&[c.r, c.g, c.b]);
        }
        Self::new(0x03, &args)
    }

    /// Tampon prêt pour `HidD_SetFeature` : octet 0 = identifiant de rapport.
    pub fn to_feature_buffer(&self) -> [u8; FEATURE_BUF_LEN] {
        let mut buf = [0u8; FEATURE_BUF_LEN];
        buf[1..].copy_from_slice(&self.0);
        buf
    }
}

/// Somme de contrôle : XOR des octets 2 à 87 inclus, placée en octet 88.
///
/// Vérifiée sur l'intégralité des trames capturées, toutes commandes
/// confondues, sans exception.
pub fn checksum(report: &[u8; REPORT_LEN]) -> u8 {
    report[2..88].iter().fold(0u8, |acc, b| acc ^ b)
}

/// Couleur, dans l'ordre du protocole : R, G, B.
///
/// Attention : le SDK Chroma de Razer utilise `0x00BBGGRR`. Le protocole de
/// l'appareil, lui, est bien en RGB — ne pas déduire l'un de l'autre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trame réellement capturée : rangée 0 entièrement en rouge.
    /// La somme de contrôle observée sur le bus était `0x5e`.
    #[test]
    fn checksum_matches_captured_frame() {
        let colors = [Rgb::new(0xff, 0, 0); 22];
        let report = Report::write_row(0, 0, &colors);
        assert_eq!(report.0[88], 0x5e);
    }

    #[test]
    fn header_layout_matches_capture() {
        let colors = [Rgb::new(0xff, 0, 0); 22];
        let r = Report::write_row(0, 0, &colors).0;
        assert_eq!(r[0], 0x00, "status");
        assert_eq!(r[1], 0x9f, "identifiant de transaction");
        assert_eq!(r[5], 0x47, "taille des arguments : 5 + 22*3");
        assert_eq!(r[6], 0x0f, "classe");
        assert_eq!(r[7], 0x03, "identifiant de commande");
        assert_eq!(r[12], 0x15, "colonne de fin = 21");
        assert_eq!(&r[13..16], &[0xff, 0x00, 0x00], "ordre RGB");
    }

    #[test]
    fn partial_row_sets_boundaries() {
        let colors = [Rgb::new(255, 255, 255); 6];
        let r = Report::write_row(2, 5, &colors).0;
        assert_eq!(r[10], 2, "rangée");
        assert_eq!(r[11], 5, "colonne de début");
        assert_eq!(r[12], 10, "colonne de fin");
        assert_eq!(r[5], 5 + 6 * 3, "taille des arguments");
    }

    #[test]
    fn effect_ids_match_capture() {
        assert_eq!(Report::set_effect(Effect::Off).0[10], 0x00);
        assert_eq!(Report::set_effect(Effect::SpectrumCycle).0[10], 0x03);
        assert_eq!(Report::set_effect(Effect::Custom).0[10], 0x08);
        let w = Report::set_effect(Effect::Wave {
            direction: 0x02,
            speed: 0x28,
        })
        .0;
        assert_eq!(&w[10..13], &[0x04, 0x02, 0x28]);
    }

    #[test]
    fn feature_buffer_has_leading_report_id() {
        let buf = Report::set_brightness(0xff).to_feature_buffer();
        assert_eq!(buf.len(), 91);
        assert_eq!(buf[0], 0x00, "identifiant de rapport HID");
        assert_eq!(buf[7], 0x0f, "classe, décalée d'un octet");
    }
}
