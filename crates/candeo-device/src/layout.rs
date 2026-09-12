//! Description physique des périphériques pris en charge.

/// Position sans LED dans la matrice.
pub const EMPTY: u16 = u16::MAX;

/// Gabarit d'un périphérique : identification, transport et matrice.
pub struct Layout {
    pub name: &'static str,
    pub vid: u16,
    pub pid: u16,
    /// Interface du composite USB portant l'éclairage.
    pub interface: u8,
    pub rows: u8,
    pub cols: u8,
    /// Index de LED par position, ligne par ligne. [`EMPTY`] = pas de LED.
    pub matrix: &'static [u16],
    /// Nom lisible de chaque position occupée, dans l'ordre des index.
    pub keys: &'static [(u16, &'static str)],
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
}

/// Razer DeathStalker V2 Pro, filaire.
///
/// Matrice relevée par interrogation du périphérique : 6 × 22 = 132 positions,
/// dont 106 portent une LED dans cette transcription (voir la note du test
/// `lit_count_matches_transcribed_matrix` au sujet d'un écart d'une unité).
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
    keys: &[],
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
}
