//! Description physique des périphériques pris en charge.

/// Position sans LED dans la matrice.
///
/// Interne : la sentinelle ne sert qu'à écrire et à lire [`Layout::matrix`], et
/// tout ce qui sort d'ici l'a déjà traversée — [`Layout::lit_count`] l'écarte du
/// compte, [`Layout::at`] la traduit en `None`. Un appelant qui lirait `matrix`
/// directement en aurait besoin ; aucun ne le fait, et la valeur brute
/// `u16::MAX` est écrite dans la documentation du champ pour celui qui s'y
/// mettrait.
pub(crate) const EMPTY: u16 = u16::MAX;

/// Une touche : sa LED, son nom gravé, et son rectangle physique.
///
/// Les coordonnées sont en **unités de pas de clavier** : 1 u = la largeur
/// d'une touche alphabétique. L'origine est en haut à gauche, `y` croît vers
/// le bas. Une touche occupe `[x, x + w[ × [y, y + h[`.
///
/// # D'où viennent ces rectangles
///
/// **Pas du périphérique.** Celui-ci n'expose que la grille logique 6 × 22 et
/// ne déclare aucune dimension. Le dessin est une **transcription à la main**
/// de la disposition ISO pleine taille standard, alignée à l'œil sur le
/// clavier réel. Il est exact au sens d'une convention, pas d'un relevé : une
/// erreur de dessin ne se détecte qu'en regardant le simulateur.
///
/// Seuls [`Key::index`] et [`Key::name`] proviennent du relevé matériel, décrit
/// dans `docs/protocol/deathstalker-v2-pro.md` §6.
pub struct Key {
    /// Index de LED, tel qu'il apparaît dans [`Layout::matrix`].
    pub index: u16,
    /// Nom lisible, dans la variante French (ISO) que le clavier déclare.
    pub name: &'static str,
    /// Bord gauche, en unités de pas de clavier.
    pub x: f32,
    /// Bord supérieur.
    pub y: f32,
    /// Largeur.
    pub w: f32,
    /// Hauteur.
    pub h: f32,
}

/// Gabarit d'un périphérique : identification, transport et matrice.
pub struct Layout {
    pub name: &'static str,
    pub vid: u16,
    pub pid: u16,
    /// Interface du composite USB portant l'éclairage.
    pub interface: u8,
    pub rows: u8,
    pub cols: u8,
    /// Index de LED par position, ligne par ligne. `u16::MAX` = pas de LED.
    pub matrix: &'static [u16],
    /// Nom et géométrie de chaque position occupée, dans l'ordre des index.
    pub keys: &'static [Key],
}

impl Layout {
    /// Nombre de positions de la matrice — **pas** le nombre de LED physiques.
    ///
    /// C'est cette valeur que doit couvrir une image complète.
    pub const fn led_count(&self) -> usize {
        self.rows as usize * self.cols as usize
    }

    /// Nombre de positions portant réellement une LED.
    pub fn lit_count(&self) -> usize {
        self.matrix.iter().filter(|&&i| i != EMPTY).count()
    }

    /// Index de LED à une position donnée.
    pub fn at(&self, row: u8, col: u8) -> Option<u16> {
        let i = row as usize * self.cols as usize + col as usize;
        match self.matrix.get(i) {
            Some(&v) if v != EMPTY => Some(v),
            _ => None,
        }
    }

    /// Touche portant un index de LED donné.
    ///
    /// Chaque position allumée en a une : c'est l'invariant que vérifie le test
    /// `every_lit_position_has_a_key`.
    pub fn key(&self, index: u16) -> Option<&'static Key> {
        let keys: &'static [Key] = self.keys;
        keys.iter().find(|k| k.index == index)
    }
}

/// Touche standard, 1 u × 1 u.
const fn k(index: u16, name: &'static str, x: f32, y: f32) -> Key {
    Key {
        index,
        name,
        x,
        y,
        w: 1.0,
        h: 1.0,
    }
}

/// Touche large, haute d'une rangée.
const fn kw(index: u16, name: &'static str, x: f32, y: f32, w: f32) -> Key {
    Key {
        index,
        name,
        x,
        y,
        w,
        h: 1.0,
    }
}

/// Touche débordant sur deux rangées — le `+` et l'`Entrée` du pavé numérique.
const fn kh(index: u16, name: &'static str, x: f32, y: f32, w: f32, h: f32) -> Key {
    Key {
        index,
        name,
        x,
        y,
        w,
        h,
    }
}

/// Razer DeathStalker V2 Pro, filaire.
///
/// Matrice relevée par interrogation du périphérique : 6 × 22 = **132**
/// positions — la taille d'une image — dont **106** portent une LED.
///
/// Les deux chiffres ne sont pas interchangeables : en envoyer 106 laisse les
/// dernières rangées figées sur leur valeur précédente. Voir
/// `docs/protocol/deathstalker-v2-pro.md` §6.
///
/// La géométrie de [`Layout::keys`] est une transcription manuelle de la
/// disposition ISO pleine taille : le périphérique ne déclare rien de tel. Voir
/// [`Key`]. Le dessin fait 22,5 u de large et 6,5 u de haut : bloc principal de
/// 0 à 15 u, pavé de navigation de 15,25 à 18,25 u, pavé numérique de 18,5 à
/// 22,5 u.
pub static DEATHSTALKER_V2_PRO: Layout = Layout {
    name: "Razer DeathStalker V2 Pro (filaire)",
    vid: 0x1532,
    pid: 0x0292,
    interface: 3,
    rows: 6,
    cols: 22,
    #[rustfmt::skip]
    matrix: &[
        //  0      1    2    3    4    5    6    7    8    9   10   11   12   13   14   15   16   17     18     19     20     21
            0, EMPTY,   2,   3,   4,   5,   6,   7,   8,   9,  10,  11,  12,  13,  14,  15,  16, EMPTY, EMPTY, EMPTY, EMPTY, EMPTY,
           22,    23,  24,  25,  26,  27,  28,  29,  30,  31,  32,  33,  34,  35,  36,  37,  38,    39,    40,    41,    42, EMPTY,
           44,    45,  46,  47,  48,  49,  50,  51,  52,  53,  54,  55,  56,  57,  58,  59,  60,    61,    62,    63,    64, EMPTY,
           66,    67,  68,  69,  70,  71,  72,  73,  74,  75,  76,  77,  78,  79, EMPTY, EMPTY, EMPTY, 83,   84,    85, EMPTY, EMPTY,
           88,    89,  90,  91,  92,  93,  94,  95,  96,  97,  98,  99, EMPTY, 101, EMPTY, 103, EMPTY, 105,  106,   107,   108, EMPTY,
          110,   111, 112, EMPTY, EMPTY, EMPTY, 116, EMPTY, EMPTY, EMPTY, 120, 121, 122, 123, 124, 125, 126, EMPTY, 128,  129, EMPTY, EMPTY,
    ],
    #[rustfmt::skip]
    keys: &[
        // Rangée 0 — fonctions. Trois blocs de quatre, puis le trio d'impression. (16)
        k(  0, "Échap",       0.0,  0.0),
        k(  2, "F1",          2.0,  0.0), k(  3, "F2",     3.0,  0.0), k(  4, "F3",     4.0,  0.0), k(  5, "F4",  5.0,  0.0),
        k(  6, "F5",          6.5,  0.0), k(  7, "F6",     7.5,  0.0), k(  8, "F7",     8.5,  0.0), k(  9, "F8",  9.5,  0.0),
        k( 10, "F9",         11.0,  0.0), k( 11, "F10",   12.0,  0.0), k( 12, "F11",   13.0,  0.0), k( 13, "F12", 14.0,  0.0),
        k( 14, "ImprÉcran",  15.25, 0.0), k( 15, "ArrêtDéfil", 16.25, 0.0), k( 16, "Pause", 17.25, 0.0),

        // Rangée 1 — chiffres AZERTY, Retour arrière de 2 u, navigation, haut du pavé. (21)
        k( 22, "²",           0.0,  1.5), k( 23, "&",      1.0,  1.5), k( 24, "é",      2.0,  1.5),
        k( 25, "\"",          3.0,  1.5), k( 26, "'",      4.0,  1.5), k( 27, "(",      5.0,  1.5),
        k( 28, "-",           6.0,  1.5), k( 29, "è",      7.0,  1.5), k( 30, "_",      8.0,  1.5),
        k( 31, "ç",           9.0,  1.5), k( 32, "à",     10.0,  1.5), k( 33, ")",     11.0,  1.5),
        k( 34, "=",          12.0,  1.5), kw(35, "Retour arrière", 13.0, 1.5, 2.0),
        k( 36, "Inser",      15.25, 1.5), k( 37, "Origine", 16.25, 1.5), k( 38, "PgPréc", 17.25, 1.5),
        k( 39, "VerrNum",    18.5,  1.5), k( 40, "Pavé /", 19.5,  1.5), k( 41, "Pavé *", 20.5,  1.5),
        k( 42, "Pavé −",     21.5,  1.5),

        // Rangée 2 — Tab de 1,5 u, rangée haute, HAUT de l'Entrée en L, navigation, pavé. (21)
        kw(44, "Tab",         0.0,  2.5, 1.5),
        k( 45, "A",           1.5,  2.5), k( 46, "Z",      2.5,  2.5), k( 47, "E",      3.5,  2.5),
        k( 48, "R",           4.5,  2.5), k( 49, "T",      5.5,  2.5), k( 50, "Y",      6.5,  2.5),
        k( 51, "U",           7.5,  2.5), k( 52, "I",      8.5,  2.5), k( 53, "O",      9.5,  2.5),
        k( 54, "P",          10.5,  2.5), k( 55, "^",     11.5,  2.5), k( 56, "$",     12.5,  2.5),
        kw(57, "Entrée",     13.5,  2.5, 1.5),
        k( 58, "Suppr",      15.25, 2.5), k( 59, "Fin",   16.25, 2.5), k( 60, "PgSuiv", 17.25, 2.5),
        k( 61, "Pavé 7",     18.5,  2.5), k( 62, "Pavé 8", 19.5, 2.5), k( 63, "Pavé 9", 20.5,  2.5),
        kh(64, "Pavé +",     21.5,  2.5, 1.0, 2.0),

        // Rangée 3 — VerrMaj de 1,75 u, rangée de repos, BAS de l'Entrée en L, pavé. (17)
        kw(66, "VerrMaj",     0.0,  3.5, 1.75),
        k( 67, "Q",           1.75, 3.5), k( 68, "S",      2.75, 3.5), k( 69, "D",      3.75, 3.5),
        k( 70, "F",           4.75, 3.5), k( 71, "G",      5.75, 3.5), k( 72, "H",      6.75, 3.5),
        k( 73, "J",           7.75, 3.5), k( 74, "K",      8.75, 3.5), k( 75, "L",      9.75, 3.5),
        k( 76, "M",          10.75, 3.5), k( 77, "ù",     11.75, 3.5), k( 78, "*",     12.75, 3.5),
        kw(79, "Entrée",     13.75, 3.5, 1.25),
        k( 83, "Pavé 4",     18.5,  3.5), k( 84, "Pavé 5", 19.5, 3.5), k( 85, "Pavé 6", 20.5,  3.5),

        // Rangée 4 — Maj gauche courte (1,25 u) + touche ISO, rangée basse, ↑, pavé. (18)
        kw(88, "Maj gauche",  0.0,  4.5, 1.25),
        k( 89, "<",           1.25, 4.5),
        k( 90, "W",           2.25, 4.5), k( 91, "X",      3.25, 4.5), k( 92, "C",      4.25, 4.5),
        k( 93, "V",           5.25, 4.5), k( 94, "B",      6.25, 4.5), k( 95, "N",      7.25, 4.5),
        k( 96, ",",           8.25, 4.5), k( 97, ";",      9.25, 4.5), k( 98, ":",     10.25, 4.5),
        k( 99, "!",          11.25, 4.5),
        kw(101, "Maj droite", 12.25, 4.5, 2.75),
        k(103, "↑",          16.25, 4.5),
        k(105, "Pavé 1",     18.5,  4.5), k(106, "Pavé 2", 19.5, 4.5), k(107, "Pavé 3", 20.5,  4.5),
        kh(108, "Pavé Entrée", 21.5, 4.5, 1.0, 2.0),

        // Rangée 5 — modificateurs de 1,25 u, Espace de 6,25 u, flèches en T inversé. (13)
        kw(110, "Ctrl gauche", 0.0,  5.5, 1.25),
        kw(111, "Win",         1.25, 5.5, 1.25),
        kw(112, "Alt",         2.5,  5.5, 1.25),
        kw(116, "Espace",      3.75, 5.5, 6.25),
        kw(120, "AltGr",      10.0,  5.5, 1.25),
        kw(121, "Fn",         11.25, 5.5, 1.25),
        kw(122, "Menu",       12.5,  5.5, 1.25),
        kw(123, "Ctrl droit", 13.75, 5.5, 1.25),
        k(124, "←",           15.25, 5.5), k(125, "↓", 16.25, 5.5), k(126, "→", 17.25, 5.5),
        kw(128, "Pavé 0",     18.5,  5.5, 2.0),
        k(129, "Pavé .",      20.5,  5.5),
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_dimensions_are_consistent() {
        let l = &DEATHSTALKER_V2_PRO;
        assert_eq!(l.matrix.len(), l.led_count());
        assert_eq!(l.led_count(), 132);
    }

    /// Deux chiffres coexistent et ne désignent pas la même chose.
    ///
    /// - **132** : les cases de la matrice 6×22, et le nombre de LED que la
    ///   zone déclare. C'est la taille d'une image — en envoyer moins laisse
    ///   les dernières rangées figées.
    /// - **106** : les cases portant réellement une LED de touche.
    ///
    /// Confirmé par réinterrogation du matériel. Un relevé antérieur annonçait
    /// 107 positions occupées : c'était un artefact de comptage, la valeur
    /// sentinelle [`EMPTY`] ayant été comptée comme un index distinct.
    #[test]
    fn counts_match_device_report() {
        assert_eq!(DEATHSTALKER_V2_PRO.led_count(), 132, "taille d'une image");
        assert_eq!(DEATHSTALKER_V2_PRO.lit_count(), 106, "touches éclairées");
    }

    /// Aucun index de LED ne doit apparaître à deux positions.
    #[test]
    fn led_indices_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for &i in DEATHSTALKER_V2_PRO.matrix.iter().filter(|&&i| i != EMPTY) {
            assert!(
                seen.insert(i),
                "index {i} présent deux fois dans la matrice"
            );
        }
    }

    #[test]
    fn known_positions_resolve() {
        let l = &DEATHSTALKER_V2_PRO;
        assert_eq!(l.at(0, 0), Some(0));
        assert_eq!(l.at(0, 1), None, "trou après Échap");
        assert_eq!(l.at(1, 0), Some(22));
        assert_eq!(l.at(5, 0), Some(110));
    }

    /// Le compte de touches par rangée, tel que relevé sur le matériel.
    ///
    /// C'est le contrôle le plus simple d'une transcription : la somme fait
    /// 106, et une rangée décalée d'une touche le fait voir immédiatement.
    #[test]
    fn rows_have_expected_key_counts() {
        let l = &DEATHSTALKER_V2_PRO;
        let expected = [16, 21, 21, 17, 18, 13];
        assert_eq!(expected.iter().sum::<usize>(), 106);

        for (row, &n) in expected.iter().enumerate() {
            let lit = (0..l.cols)
                .filter(|&c| l.at(row as u8, c).is_some())
                .count();
            assert_eq!(lit, n, "rangée {row}");
        }
    }

    /// La table des touches et la matrice décrivent le même clavier.
    ///
    /// Dans les deux sens : toute position allumée a un nom et une géométrie,
    /// et aucune touche ne décrit une position qui n'existe pas.
    #[test]
    fn every_lit_position_has_a_key() {
        let l = &DEATHSTALKER_V2_PRO;

        for row in 0..l.rows {
            for col in 0..l.cols {
                let Some(index) = l.at(row, col) else {
                    continue;
                };
                let key = l
                    .key(index)
                    .unwrap_or_else(|| panic!("index {index} en ({row}, {col}) sans touche"));
                assert!(!key.name.is_empty(), "index {index} sans nom");
                assert!(key.w > 0.0 && key.h > 0.0, "index {index} sans surface");
            }
        }

        for key in l.keys {
            assert!(
                l.matrix.contains(&key.index),
                "« {} » (index {}) absent de la matrice",
                key.name,
                key.index
            );
        }
        assert_eq!(l.keys.len(), l.lit_count());
    }

    /// Deux touches ne peuvent pas occuper le même espace.
    ///
    /// L'Entrée ISO n'est pas une exception : ses deux LED sont modélisées par
    /// les deux rectangles **jointifs** qui composent le L, pas par un
    /// rectangle dupliqué. Voir [`iso_enter_tiles_the_l_shape`].
    #[test]
    fn key_rectangles_do_not_overlap() {
        let keys = DEATHSTALKER_V2_PRO.keys;
        for (i, a) in keys.iter().enumerate() {
            for b in &keys[i + 1..] {
                let disjoint =
                    a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
                assert!(
                    disjoint,
                    "« {} » ({}) et « {} » ({}) se chevauchent",
                    a.name, a.index, b.name, b.index
                );
            }
        }
    }

    /// L'Entrée ISO porte **deux** LED : 57 en rangée 2, 79 en rangée 3.
    ///
    /// C'est le matériel, pas un défaut de relevé — un dégradé vertical y est
    /// visible. On modélise donc une touche par LED, chacune couvrant la partie
    /// du L qu'elle éclaire : le bras haut large de 1,5 u, le bras bas de
    /// 1,25 u décalé vers la droite. Leur union est exactement l'Entrée en L,
    /// et leur intersection est vide.
    #[test]
    fn iso_enter_tiles_the_l_shape() {
        let l = &DEATHSTALKER_V2_PRO;
        let haut = l.key(57).expect("Entrée rangée 2");
        let bas = l.key(79).expect("Entrée rangée 3");

        assert_eq!(haut.name, "Entrée");
        assert_eq!(bas.name, "Entrée");
        assert_eq!(l.at(2, 13), Some(57));
        assert_eq!(l.at(3, 13), Some(79));

        // Les deux bras s'appuient sur le même bord droit — celui du bloc
        // principal, à 15 u — et se touchent sans se recouvrir.
        assert_eq!(haut.x + haut.w, 15.0);
        assert_eq!(bas.x + bas.w, 15.0);
        assert_eq!(haut.y + haut.h, bas.y);
        assert!(bas.x > haut.x, "l'encoche du L est à gauche du bras bas");
    }

    /// La barre d'espace fait 6,25 u mais n'a qu'**une** LED, en (5, 6).
    #[test]
    fn space_bar_is_wide_but_single() {
        let l = &DEATHSTALKER_V2_PRO;
        assert_eq!(l.at(5, 6), Some(116));

        let espace = l.key(116).expect("Espace");
        assert_eq!(espace.name, "Espace");
        assert_eq!(espace.w, 6.25);
        assert_eq!(l.keys.iter().filter(|k| k.name == "Espace").count(), 1);
    }

    /// Le dessin tient dans l'encombrement d'un ISO pleine taille.
    #[test]
    fn drawing_fits_a_full_size_iso() {
        let keys = DEATHSTALKER_V2_PRO.keys;
        let width = keys.iter().fold(0.0f32, |m, k| m.max(k.x + k.w));
        let height = keys.iter().fold(0.0f32, |m, k| m.max(k.y + k.h));
        assert_eq!(width, 22.5, "largeur totale, pavé numérique compris");
        assert_eq!(height, 6.5, "hauteur totale, rangée de fonctions comprise");
        assert!(keys.iter().all(|k| k.x >= 0.0 && k.y >= 0.0));
    }
}
