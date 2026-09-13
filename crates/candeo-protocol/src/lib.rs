//! Construction des rapports du protocole d'éclairage Razer, et lecture des
//! réponses.
//!
//! Structure établie par capture du bus USB — voir `docs/protocol/`.
//! Aucun code tiers n'a été consulté : ce module dérive uniquement de trames
//! observées sur le matériel.

/// Taille du rapport, hors identifiant de rapport HID.
///
/// ⚠️ **`pub` pour les sondes, et c'est délibéré.** Le seul consommateur hors de
/// cette crate est `apps/desktop/src-tauri/src/sonde.rs`, qui est déclaré
/// `#[cfg(test)] mod sonde;` et dont toutes les sondes sont `#[ignore]` : cette
/// constante ne part donc jamais dans le binaire livré, et un balayage de code
/// mort la donnera toujours pour restreignable.
///
/// Ne pas la restreindre. Ces sondes sont conservées pour **rejouer le relevé**
/// sur un autre micrologiciel ou un autre exemplaire — c'est ce qui a établi le
/// protocole, et c'est la seule façon de le réétablir le jour où un appareil
/// répondra autrement. Les couper pour gagner deux caractères de visibilité
/// coûterait cette capacité, et le lien ne se reverrait pas.
pub const REPORT_LEN: usize = 90;

/// Taille du tampon passé à `HidD_SetFeature` : identifiant de rapport + données.
pub(crate) const FEATURE_BUF_LEN: usize = REPORT_LEN + 1;

/// Classe de commande « éclairage ».
const CLASS_LIGHTING: u8 = 0x0f;

/// Classe de commande « informations » : version, numéro de série, mode.
///
/// ⚠️ **On n'y fait que lire.** La même classe porte l'écriture du mode de
/// l'appareil (`0x04`), qui ferait cesser au micrologiciel le traitement de
/// certaines touches — décision consignée au §8 du relevé de ne jamais y
/// toucher. Aucun constructeur de ce module ne sait donc former une commande de
/// cette classe sous `0x80`, et c'est la seule garantie qui ne dépende pas de la
/// vigilance d'un relecteur.
const CLASS_INFO: u8 = 0x00;

/// Identifiant de transaction observé sur toutes les trames capturées.
/// Aucune variation n'a été constatée ; l'appareil ne semble pas le vérifier,
/// mais on reproduit la valeur d'origine par prudence.
const TRANSACTION_ID: u8 = 0x9f;

/// Une commande, sans ses arguments : le couple classe / commande.
///
/// C'est **exactement** ce que l'appareil valide, et rien de plus : une commande
/// inconnue rend l'état `0x05`, mais une valeur d'argument absurde sur une
/// commande connue rend quand même `0x02` (§8 du relevé). Le type ne porte donc
/// que ce qu'un octet d'état peut confirmer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommandId {
    pub class: u8,
    pub command: u8,
}

impl std::fmt::Display for CommandId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#04x}/{:#04x}", self.class, self.command)
    }
}

/// Définir l'effet. Commande `0x0f` / `0x02`.
pub const SET_EFFECT: CommandId = CommandId {
    class: CLASS_LIGHTING,
    command: 0x02,
};
/// Écrire un segment de rangée. Commande `0x0f` / `0x03`.
pub const WRITE_ROW: CommandId = CommandId {
    class: CLASS_LIGHTING,
    command: 0x03,
};
/// Définir la luminosité. Commande `0x0f` / `0x04`.
pub const SET_BRIGHTNESS: CommandId = CommandId {
    class: CLASS_LIGHTING,
    command: 0x04,
};

/// Lire l'effet courant. `0x0f` / `0x82`, taille `0x03` comme au relevé.
const READ_EFFECT: CommandId = CommandId {
    class: CLASS_LIGHTING,
    command: 0x82,
};
/// Lire la luminosité courante. `0x0f` / `0x84`, taille `0x03` comme au relevé.
const READ_BRIGHTNESS: CommandId = CommandId {
    class: CLASS_LIGHTING,
    command: 0x84,
};
/// Lire la version du micrologiciel. `0x00` / `0x81`.
const READ_FIRMWARE: CommandId = CommandId {
    class: CLASS_INFO,
    command: 0x81,
};
/// Lire le numéro de série. `0x00` / `0x82`.
const READ_SERIAL: CommandId = CommandId {
    class: CLASS_INFO,
    command: 0x82,
};

/// Taille annoncée pour les lectures de l'éclairage : celle du relevé.
const READ_LIGHTING_LEN: u8 = 0x03;

/// Taille annoncée pour les lectures de la classe `0x00` : celle de la sonde
/// qui a établi le tableau du §8. Le numéro de série y tient — 15 caractères.
const READ_INFO_LEN: u8 = 0x16;

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
    fn new(id: CommandId, args: &[u8]) -> Self {
        assert!(args.len() <= REPORT_LEN - 10, "arguments trop longs");
        let mut r = Self::header(id, args.len() as u8);
        r[8..8 + args.len()].copy_from_slice(args);
        r[88] = checksum(&r);
        Self(r)
    }

    /// Une demande de lecture : aucun argument, seulement la taille qu'on
    /// attend en retour.
    ///
    /// Séparée de [`Self::new`] parce que la taille n'y décrit pas les octets
    /// envoyés — il n'y en a pas — mais la réponse. Les tailles sont celles du
    /// relevé, et c'est délibéré : c'est sous cette forme que l'appareil a
    /// répondu, et rien n'a établi qu'il répondrait pareil à une autre.
    fn query(id: CommandId, len: u8) -> Self {
        let mut r = Self::header(id, len);
        r[88] = checksum(&r);
        Self(r)
    }

    fn header(id: CommandId, len: u8) -> [u8; REPORT_LEN] {
        let mut r = [0u8; REPORT_LEN];
        r[1] = TRANSACTION_ID;
        r[5] = len;
        r[6] = id.class;
        r[7] = id.command;
        r
    }

    /// Le couple classe / commande que porte ce rapport.
    pub fn id(&self) -> CommandId {
        CommandId {
            class: self.0[6],
            command: self.0[7],
        }
    }

    /// Sélectionne un effet. Commande `0x0f` / `0x02`.
    pub fn set_effect(effect: Effect) -> Self {
        Self::new(SET_EFFECT, &effect.args())
    }

    /// Règle la luminosité globale. Commande `0x0f` / `0x04`.
    pub fn set_brightness(level: u8) -> Self {
        Self::new(SET_BRIGHTNESS, &[0, 0, level])
    }

    /// Demande la version du micrologiciel. Voir [`Response::firmware`].
    pub fn read_firmware() -> Self {
        Self::query(READ_FIRMWARE, READ_INFO_LEN)
    }

    /// Demande le numéro de série. Voir [`Response::serial`].
    pub fn read_serial() -> Self {
        Self::query(READ_SERIAL, READ_INFO_LEN)
    }

    /// Demande l'effet courant. Voir [`Response::effect`].
    pub fn read_effect() -> Self {
        Self::query(READ_EFFECT, READ_LIGHTING_LEN)
    }

    /// Demande la luminosité courante. Voir [`Response::brightness`].
    pub fn read_brightness() -> Self {
        Self::query(READ_BRIGHTNESS, READ_LIGHTING_LEN)
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
        Self::new(WRITE_ROW, &args)
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
///
/// ⚠️ **`pub` pour les sondes**, comme [`REPORT_LEN`] et pour la même raison :
/// le constructeur interne de [`Report`] l'appelle déjà pour tout ce que
/// l'application envoie, et le seul appelant externe est le `sonde.rs` de
/// `candeo-desktop`, compilé
/// uniquement en test. Une sonde fabrique ses trames **à la main**, sans passer
/// par [`Report`] — c'est tout l'intérêt : elle interroge des commandes que le
/// constructeur ne sait pas former, dont celles qui n'existent peut-être pas.
/// Lui retirer la somme de contrôle reviendrait à lui faire émettre des trames
/// que l'appareil refuse, et le relevé ne serait plus rejouable.
pub fn checksum(report: &[u8; REPORT_LEN]) -> u8 {
    report[2..88].iter().fold(0u8, |acc, b| acc ^ b)
}

// ---------------------------------------------------------------- réponses

/// L'octet d'état d'une réponse (offset 0) — §8 du relevé.
///
/// ⚠️ **`Understood` veut dire « cette commande existe », pas « elle a fait ce
/// que je voulais ».** Mesuré : poser l'effet `0x05`, que ce clavier refuse,
/// rend `0x02` — l'appareil valide le couple classe / commande, jamais la valeur
/// d'un argument. Aucun octet d'état ne remplace une relecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// `0x00` — aucune réponse posée.
    Empty,
    /// `0x01` — l'appareil n'a pas fini.
    Busy,
    /// `0x02` — le couple classe / commande est connu.
    Understood,
    /// `0x03`
    Failed,
    /// `0x04`
    TimedOut,
    /// `0x05` — classe ou commande inconnue de ce micrologiciel. **Vérifié qu'il
    /// discrimine** : une classe et une commande inexistantes le rendent toutes
    /// deux, là où une commande valide rend `0x02`.
    Unsupported,
    /// Aucune valeur relevée ne s'écrit ainsi.
    Other(u8),
}

impl Status {
    fn from_byte(b: u8) -> Self {
        match b {
            0x00 => Self::Empty,
            0x01 => Self::Busy,
            0x02 => Self::Understood,
            0x03 => Self::Failed,
            0x04 => Self::TimedOut,
            0x05 => Self::Unsupported,
            autre => Self::Other(autre),
        }
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("0x00 (aucune)"),
            Self::Busy => f.write_str("0x01 (occupé)"),
            Self::Understood => f.write_str("0x02 (compris)"),
            Self::Failed => f.write_str("0x03 (échec)"),
            Self::TimedOut => f.write_str("0x04 (expiré)"),
            Self::Unsupported => f.write_str("0x05 (non pris en charge)"),
            Self::Other(b) => write!(f, "{b:#04x} (inconnu)"),
        }
    }
}

/// Version du micrologiciel, telle que `0x00`/`0x81` la rend.
///
/// Deux octets gardés comme deux nombres plutôt qu'une chaîne : le relevé écrit
/// tantôt « v1.5 » (§1, et ce que l'appareil déclare par ailleurs) tantôt
/// « 1.05 » (§8), pour les mêmes octets `01 05`. Comparer des chaînes ferait de
/// cette différence de plume une différence de version.
///
/// **Ce n'est pas `release_number`.** L'énumération HID rend `0x0200` sur tout
/// le composite : c'est le `bcdDevice`, une révision matérielle figée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Firmware {
    pub major: u8,
    pub minor: u8,
}

/// Comme l'appareil se déclare : `v1.5`.
impl std::fmt::Display for Firmware {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "v{}.{}", self.major, self.minor)
    }
}

/// La réponse du périphérique à la dernière commande reçue.
///
/// Même structure que [`Report`] (§3), avec deux différences qui la rendent
/// utile : l'octet 0 porte un [`Status`], et les octets 6 et 7 renvoient **en
/// écho** la classe et la commande. L'écho n'est pas décoratif — c'est lui qui
/// dit si l'on relit bien la réponse attendue et non celle d'une commande
/// émise entre-temps par quelqu'un d'autre.
///
/// La somme de contrôle n'est pas vérifiée : rien au relevé n'établit que
/// l'appareil en pose une juste dans ses réponses.
#[derive(Debug, Clone)]
pub struct Response([u8; REPORT_LEN]);

impl Response {
    /// Le tampon qu'attend `HidD_GetFeature` : identifiant de rapport compris.
    pub const fn buffer() -> [u8; FEATURE_BUF_LEN] {
        [0u8; FEATURE_BUF_LEN]
    }

    /// La réponse contenue dans un tampon rempli par `get_feature_report`.
    ///
    /// `read` est le compte rendu par l'appel. Le seuil est celui des sondes qui
    /// ont établi le sens retour — au moins les 90 octets du rapport — et non un
    /// seuil plus strict jamais essayé : une lecture refusée ici pour un octet
    /// d'identifiant manquant serait une panne qu'on aurait fabriquée.
    pub fn from_feature_buffer(buf: &[u8; FEATURE_BUF_LEN], read: usize) -> Option<Self> {
        if read < REPORT_LEN {
            return None;
        }
        let mut r = [0u8; REPORT_LEN];
        r.copy_from_slice(&buf[1..]);
        Some(Self(r))
    }

    pub fn status(&self) -> Status {
        Status::from_byte(self.0[0])
    }

    /// Le couple renvoyé en écho.
    pub fn id(&self) -> CommandId {
        CommandId {
            class: self.0[6],
            command: self.0[7],
        }
    }

    /// Vrai si cette réponse est celle de `request`, d'après l'écho.
    pub fn answers(&self, request: &Report) -> bool {
        self.id() == request.id()
    }

    fn args(&self) -> &[u8] {
        &self.0[8..88]
    }

    /// La version, lue dans la réponse à [`Report::read_firmware`].
    ///
    /// `None` pour `00 00` : c'est ce que rend un tampon resté vide, pas un
    /// micrologiciel — et annoncer « v0.0 » ferait comparer au relevé une
    /// version qui n'existe pas.
    pub fn firmware(&self) -> Option<Firmware> {
        let a = self.args();
        let version = Firmware {
            major: a[0],
            minor: a[1],
        };
        (version != Firmware { major: 0, minor: 0 }).then_some(version)
    }

    /// Le numéro de série, lu dans la réponse à [`Report::read_serial`].
    ///
    /// ASCII jusqu'au premier octet nul, et imprimable de bout en bout : 15
    /// caractères au relevé. Ce qui ne ressemble pas à ça rend `None` plutôt
    /// qu'une chaîne approximative — une série sert à **apparier**, et deux
    /// lectures abîmées différemment désapparieraient le même exemplaire.
    pub fn serial(&self) -> Option<String> {
        let a = &self.args()[..READ_INFO_LEN as usize];
        let fin = a.iter().position(|&b| b == 0).unwrap_or(a.len());
        let serie = &a[..fin];
        if serie.is_empty() || !serie.iter().all(|b| b.is_ascii_graphic()) {
            return None;
        }
        Some(serie.iter().map(|&b| char::from(b)).collect())
    }

    /// L'effet courant, lu dans la réponse à [`Report::read_effect`].
    ///
    /// La relecture place l'identifiant et ses deux paramètres **aux mêmes
    /// positions** que l'écriture (`00 00 <effet> <p1> <p2>`) — c'est ce qui a
    /// permis de relire la Vague « à l'identique, paramètres compris ».
    ///
    /// `None` pour tout ce que [`Effect`] ne sait pas réécrire à l'identique :
    /// `Statique` et `Respiration` portent une couleur dont la place dans la
    /// relecture n'est pas établie, et un octet inattendu là où le relevé n'a vu
    /// que des zéros veut dire qu'on ne comprend pas ce qu'on lit. Dans les deux
    /// cas, mieux vaut ne rien conclure que réécrire autre chose que ce qui est
    /// affiché.
    pub fn effect(&self) -> Option<Effect> {
        let a = self.args();
        if a[0] != 0 || a[1] != 0 || a[5] != 0 {
            return None;
        }
        match (a[2], a[3], a[4]) {
            (0x00, 0, 0) => Some(Effect::Off),
            (0x03, 0, 0) => Some(Effect::SpectrumCycle),
            // Direction bornée à `00`–`02` au relevé ; au-delà, ce n'est plus
            // une Vague qu'on sait décrire.
            (0x04, direction @ 0..=2, speed) => Some(Effect::Wave { direction, speed }),
            (0x08, 0, 0) => Some(Effect::Custom),
            _ => None,
        }
    }

    /// La luminosité courante, lue dans la réponse à [`Report::read_brightness`].
    ///
    /// Relevée sous la forme `00 00 <niveau>`, comme l'écriture.
    pub fn brightness(&self) -> Option<u8> {
        let a = self.args();
        (a[0] == 0 && a[1] == 0).then_some(a[2])
    }
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

    // -------------------------------------------------------- lectures

    /// Une réponse telle que `get_feature_report` la remplit : identifiant de
    /// rapport, état, écho, arguments.
    fn reponse(etat: u8, id: CommandId, args: &[u8]) -> Response {
        let mut buf = Response::buffer();
        buf[1] = etat;
        buf[1 + 6] = id.class;
        buf[1 + 7] = id.command;
        buf[1 + 8..1 + 8 + args.len()].copy_from_slice(args);
        Response::from_feature_buffer(&buf, buf.len()).expect("tampon complet")
    }

    /// Les demandes sont formées comme la sonde qui a établi le §8 les formait :
    /// aucun argument, la taille attendue en octet 5, et une somme de contrôle
    /// juste — sans quoi l'appareil refuserait la trame et non la commande.
    #[test]
    fn une_demande_de_lecture_est_formee_comme_au_releve() {
        let r = Report::read_firmware().0;
        assert_eq!(r[1], 0x9f, "identifiant de transaction");
        assert_eq!(r[5], 0x16, "taille attendue");
        assert_eq!((r[6], r[7]), (0x00, 0x81));
        assert!(
            r[8..88].iter().all(|&b| b == 0),
            "une lecture n'a pas d'argument"
        );
        assert_eq!(r[88], checksum(&r));

        let e = Report::read_effect().0;
        assert_eq!((e[5], e[6], e[7]), (0x03, 0x0f, 0x82));
        let l = Report::read_brightness().0;
        assert_eq!((l[5], l[6], l[7]), (0x03, 0x0f, 0x84));
    }

    /// **Le mode pilote ne s'écrit pas d'ici.** Dans la classe `0x00`, seules
    /// les lectures (`0x80` et au-delà) sont formables ; `0x00`/`0x04` ferait
    /// cesser au micrologiciel le traitement de certaines touches.
    #[test]
    fn aucun_constructeur_n_ecrit_dans_la_classe_information() {
        let tous = [
            Report::read_firmware(),
            Report::read_serial(),
            Report::read_effect(),
            Report::read_brightness(),
            Report::set_effect(Effect::Custom),
            Report::set_brightness(0x80),
            Report::write_row(0, 0, &[Rgb::default()]),
        ];
        for r in tous {
            let id = r.id();
            assert!(
                id.class != 0x00 || id.command >= 0x80,
                "{id} écrit dans la classe information"
            );
        }
    }

    #[test]
    fn l_etat_se_decode_comme_au_releve() {
        let s = |b| reponse(b, SET_BRIGHTNESS, &[]).status();
        assert_eq!(s(0x02), Status::Understood);
        assert_eq!(s(0x05), Status::Unsupported);
        assert_eq!(s(0x01), Status::Busy);
        assert_eq!(s(0x07), Status::Other(0x07));
    }

    /// L'écho désigne la commande répondue : une réponse à la luminosité ne se
    /// lit pas comme une version, même si ses octets s'y prêteraient.
    #[test]
    fn l_echo_designe_la_commande_repondue() {
        let r = reponse(0x02, READ_FIRMWARE, &[0x01, 0x05]);
        assert!(r.answers(&Report::read_firmware()));
        assert!(!r.answers(&Report::read_serial()));
        assert!(!r.answers(&Report::read_brightness()));
    }

    /// `01 05`, relevé le 12/09/2026 — ce que l'appareil déclare par ailleurs
    /// comme **v1.5**. La concordance a servi de validation.
    #[test]
    fn la_version_01_05_se_lit_v1_5() {
        let v = reponse(0x02, READ_FIRMWARE, &[0x01, 0x05])
            .firmware()
            .expect("version");
        assert_eq!(v, Firmware { major: 1, minor: 5 });
        assert_eq!(v.to_string(), "v1.5");
        assert!(Firmware { major: 1, minor: 6 } > v);
    }

    #[test]
    fn un_tampon_vide_n_est_pas_une_version() {
        assert_eq!(reponse(0x02, READ_FIRMWARE, &[0, 0]).firmware(), None);
    }

    /// Série inventée, de la forme relevée : 15 caractères ASCII.
    #[test]
    fn le_numero_de_serie_s_arrete_au_premier_octet_nul() {
        let r = reponse(0x02, READ_SERIAL, b"XY24ABCDEFG0001\0\0\0");
        assert_eq!(r.serial().as_deref(), Some("XY24ABCDEFG0001"));
    }

    /// Une série sert à apparier : une lecture abîmée ne doit pas en inventer
    /// une seconde pour le même exemplaire.
    #[test]
    fn une_serie_illisible_ne_se_devine_pas() {
        assert_eq!(reponse(0x02, READ_SERIAL, &[]).serial(), None);
        assert_eq!(reponse(0x02, READ_SERIAL, b"XY24\x07BCD").serial(), None);
        assert_eq!(reponse(0x02, READ_SERIAL, b"XY 24").serial(), None);
    }

    /// La relecture place l'effet aux positions de l'écriture : un effet posé se
    /// relit tel quel. C'est la condition pour pouvoir le **réécrire à
    /// l'identique** sans rien changer à ce que montre le clavier.
    #[test]
    fn un_effet_relu_se_reecrit_a_l_identique() {
        for effet in [
            Effect::Off,
            Effect::SpectrumCycle,
            Effect::Wave {
                direction: 0x01,
                speed: 0x28,
            },
            Effect::Custom,
        ] {
            let pose = Report::set_effect(effet).0;
            let relu = reponse(0x02, READ_EFFECT, &pose[8..14]).effect();
            assert_eq!(relu, Some(effet));
        }
    }

    /// `Statique` porte une couleur dont la place dans la relecture n'est pas
    /// établie : la réécrire « à l'identique » pourrait l'éteindre.
    #[test]
    fn un_effet_colore_ne_se_relit_pas() {
        let statique = [0, 0, 0x01, 0, 0, 0x01, 0xff, 0x00, 0x00];
        assert_eq!(reponse(0x02, READ_EFFECT, &statique).effect(), None);
        // Identifiant de LED non nul : ce n'est plus la forme relevée.
        assert_eq!(reponse(0x02, READ_EFFECT, &[0x05, 0, 0x03]).effect(), None);
    }

    #[test]
    fn la_luminosite_se_relit_comme_elle_s_ecrit() {
        let pose = Report::set_brightness(0x80).0;
        let relu = reponse(0x02, READ_BRIGHTNESS, &pose[8..11]).brightness();
        assert_eq!(relu, Some(0x80));
        assert_eq!(
            reponse(0x02, READ_BRIGHTNESS, &[0x05, 0, 0x80]).brightness(),
            None
        );
    }

    /// Le seuil est celui des sondes : 90 octets au moins.
    #[test]
    fn une_reponse_tronquee_est_refusee() {
        let buf = Response::buffer();
        assert!(Response::from_feature_buffer(&buf, 89).is_none());
        assert!(Response::from_feature_buffer(&buf, 90).is_some());
    }
}
