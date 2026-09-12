/**
 * Gabarit de repli, et contrôle de cohérence gabarit ↔ image.
 *
 * ## Pourquoi une copie du gabarit ici
 *
 * Le simulateur doit s'afficher **sans clavier branché** — c'est tout son
 * intérêt : écrire un effet sans posséder l'appareil. Or `get_layout()` refuse
 * quand rien n'est ouvert (« aucun périphérique connecté ») et aucune commande
 * ne rend un gabarit hors connexion. Faute de mieux, la table ci-dessous est une
 * **transcription** de `crates/candeo-device/src/layout.rs`.
 *
 * C'est une seconde source de vérité, donc une dette assumée : elle ne diverge
 * pas bruyamment, elle diverge en silence. La bonne correction est une commande
 * Rust rendant le gabarit d'un modèle **connu** sans l'ouvrir ; elle sort du
 * périmètre de l'issue #3. En attendant, un seul endroit à corriger, et ce
 * commentaire pour dire lequel.
 *
 * La géométrie elle-même n'est pas un relevé : le périphérique n'expose que sa
 * grille 6 × 22 et ne déclare aucune dimension. Voir `docs/api/commands.md`,
 * « La géométrie n'est pas une lecture du périphérique ».
 */

import type { KeyInfo, Rgb } from '../api/types'

/**
 * Ce que le simulateur demande d'un gabarit.
 *
 * Identique au `LayoutInfo` de `api/types`, au détail près de `keys`, ici en
 * **lecture seule** : `readonly()` de Vue renvoie un `readonly KeyInfo[]`, qui
 * n'est pas assignable à `KeyInfo[]`. Un `LayoutInfo` reste accepté tel quel,
 * l'inverse ne l'est pas — et c'est le bon sens de la relation, le simulateur
 * ne modifie jamais le gabarit qu'on lui donne.
 */
export interface LayoutView {
  name: string
  rows: number
  cols: number
  frameLen: number
  keys: readonly KeyInfo[]
}

const ROWS = 6
const COLS = 22

/** `[index, nom, x, y, w, h]` — l'ordre des constructeurs `k`/`kw`/`kh` du Rust. */
type Entry = readonly [index: number, name: string, x: number, y: number, w: number, h: number]

/**
 * Les 106 positions éclairées, dans l'ordre des index.
 *
 * ⚠️ « Entrée » figure **deux fois** (57 et 79) : l'Entrée ISO porte deux LED,
 * et les deux rectangles sont les deux bras jointifs du L. « Espace » n'y figure
 * qu'une fois pour 6,25 u. Une touche n'est pas une LED, dans les deux sens.
 */
const ENTRIES: readonly Entry[] = [
  // rangée 0 — fonctions et trio d'impression (16)
  [0, 'Échap', 0, 0, 1, 1],
  [2, 'F1', 2, 0, 1, 1],
  [3, 'F2', 3, 0, 1, 1],
  [4, 'F3', 4, 0, 1, 1],
  [5, 'F4', 5, 0, 1, 1],
  [6, 'F5', 6.5, 0, 1, 1],
  [7, 'F6', 7.5, 0, 1, 1],
  [8, 'F7', 8.5, 0, 1, 1],
  [9, 'F8', 9.5, 0, 1, 1],
  [10, 'F9', 11, 0, 1, 1],
  [11, 'F10', 12, 0, 1, 1],
  [12, 'F11', 13, 0, 1, 1],
  [13, 'F12', 14, 0, 1, 1],
  [14, 'ImprÉcran', 15.25, 0, 1, 1],
  [15, 'ArrêtDéfil', 16.25, 0, 1, 1],
  [16, 'Pause', 17.25, 0, 1, 1],

  // rangée 1 — chiffres AZERTY, Retour arrière de 2 u, navigation, haut du pavé (21)
  [22, '²', 0, 1.5, 1, 1],
  [23, '&', 1, 1.5, 1, 1],
  [24, 'é', 2, 1.5, 1, 1],
  [25, '"', 3, 1.5, 1, 1],
  [26, "'", 4, 1.5, 1, 1],
  [27, '(', 5, 1.5, 1, 1],
  [28, '-', 6, 1.5, 1, 1],
  [29, 'è', 7, 1.5, 1, 1],
  [30, '_', 8, 1.5, 1, 1],
  [31, 'ç', 9, 1.5, 1, 1],
  [32, 'à', 10, 1.5, 1, 1],
  [33, ')', 11, 1.5, 1, 1],
  [34, '=', 12, 1.5, 1, 1],
  [35, 'Retour arrière', 13, 1.5, 2, 1],
  [36, 'Inser', 15.25, 1.5, 1, 1],
  [37, 'Origine', 16.25, 1.5, 1, 1],
  [38, 'PgPréc', 17.25, 1.5, 1, 1],
  [39, 'VerrNum', 18.5, 1.5, 1, 1],
  [40, 'Pavé /', 19.5, 1.5, 1, 1],
  [41, 'Pavé *', 20.5, 1.5, 1, 1],
  [42, 'Pavé −', 21.5, 1.5, 1, 1],

  // rangée 2 — Tab de 1,5 u, rangée haute, BRAS HAUT de l'Entrée en L, pavé (21)
  [44, 'Tab', 0, 2.5, 1.5, 1],
  [45, 'A', 1.5, 2.5, 1, 1],
  [46, 'Z', 2.5, 2.5, 1, 1],
  [47, 'E', 3.5, 2.5, 1, 1],
  [48, 'R', 4.5, 2.5, 1, 1],
  [49, 'T', 5.5, 2.5, 1, 1],
  [50, 'Y', 6.5, 2.5, 1, 1],
  [51, 'U', 7.5, 2.5, 1, 1],
  [52, 'I', 8.5, 2.5, 1, 1],
  [53, 'O', 9.5, 2.5, 1, 1],
  [54, 'P', 10.5, 2.5, 1, 1],
  [55, '^', 11.5, 2.5, 1, 1],
  [56, '$', 12.5, 2.5, 1, 1],
  [57, 'Entrée', 13.5, 2.5, 1.5, 1],
  [58, 'Suppr', 15.25, 2.5, 1, 1],
  [59, 'Fin', 16.25, 2.5, 1, 1],
  [60, 'PgSuiv', 17.25, 2.5, 1, 1],
  [61, 'Pavé 7', 18.5, 2.5, 1, 1],
  [62, 'Pavé 8', 19.5, 2.5, 1, 1],
  [63, 'Pavé 9', 20.5, 2.5, 1, 1],
  [64, 'Pavé +', 21.5, 2.5, 1, 2],

  // rangée 3 — VerrMaj de 1,75 u, rangée de repos, BRAS BAS de l'Entrée, pavé (17)
  [66, 'VerrMaj', 0, 3.5, 1.75, 1],
  [67, 'Q', 1.75, 3.5, 1, 1],
  [68, 'S', 2.75, 3.5, 1, 1],
  [69, 'D', 3.75, 3.5, 1, 1],
  [70, 'F', 4.75, 3.5, 1, 1],
  [71, 'G', 5.75, 3.5, 1, 1],
  [72, 'H', 6.75, 3.5, 1, 1],
  [73, 'J', 7.75, 3.5, 1, 1],
  [74, 'K', 8.75, 3.5, 1, 1],
  [75, 'L', 9.75, 3.5, 1, 1],
  [76, 'M', 10.75, 3.5, 1, 1],
  [77, 'ù', 11.75, 3.5, 1, 1],
  [78, '*', 12.75, 3.5, 1, 1],
  [79, 'Entrée', 13.75, 3.5, 1.25, 1],
  [83, 'Pavé 4', 18.5, 3.5, 1, 1],
  [84, 'Pavé 5', 19.5, 3.5, 1, 1],
  [85, 'Pavé 6', 20.5, 3.5, 1, 1],

  // rangée 4 — Maj gauche courte + touche ISO, rangée basse, ↑, pavé (18)
  [88, 'Maj gauche', 0, 4.5, 1.25, 1],
  [89, '<', 1.25, 4.5, 1, 1],
  [90, 'W', 2.25, 4.5, 1, 1],
  [91, 'X', 3.25, 4.5, 1, 1],
  [92, 'C', 4.25, 4.5, 1, 1],
  [93, 'V', 5.25, 4.5, 1, 1],
  [94, 'B', 6.25, 4.5, 1, 1],
  [95, 'N', 7.25, 4.5, 1, 1],
  [96, ',', 8.25, 4.5, 1, 1],
  [97, ';', 9.25, 4.5, 1, 1],
  [98, ':', 10.25, 4.5, 1, 1],
  [99, '!', 11.25, 4.5, 1, 1],
  [101, 'Maj droite', 12.25, 4.5, 2.75, 1],
  [103, '↑', 16.25, 4.5, 1, 1],
  [105, 'Pavé 1', 18.5, 4.5, 1, 1],
  [106, 'Pavé 2', 19.5, 4.5, 1, 1],
  [107, 'Pavé 3', 20.5, 4.5, 1, 1],
  [108, 'Pavé Entrée', 21.5, 4.5, 1, 2],

  // rangée 5 — modificateurs, Espace de 6,25 u pour une seule LED, T inversé (13)
  [110, 'Ctrl gauche', 0, 5.5, 1.25, 1],
  [111, 'Win', 1.25, 5.5, 1.25, 1],
  [112, 'Alt', 2.5, 5.5, 1.25, 1],
  [116, 'Espace', 3.75, 5.5, 6.25, 1],
  [120, 'AltGr', 10, 5.5, 1.25, 1],
  [121, 'Fn', 11.25, 5.5, 1.25, 1],
  [122, 'Menu', 12.5, 5.5, 1.25, 1],
  [123, 'Ctrl droit', 13.75, 5.5, 1.25, 1],
  [124, '←', 15.25, 5.5, 1, 1],
  [125, '↓', 16.25, 5.5, 1, 1],
  [126, '→', 17.25, 5.5, 1, 1],
  [128, 'Pavé 0', 18.5, 5.5, 2, 1],
  [129, 'Pavé .', 20.5, 5.5, 1, 1],
]

/**
 * `row` et `col` se déduisent de l'index sur ce modèle, parce que la matrice y
 * vaut sa propre position : la case (r, c) porte l'index `r * 22 + c`. Ce n'est
 * pas une règle générale du protocole — c'est un fait relevé sur ce clavier, et
 * la raison pour laquelle ces deux champs ne sont pas retranscrits à la main.
 */
function toKey([index, name, x, y, w, h]: Entry): KeyInfo {
  const col = index % COLS
  return { index, row: (index - col) / COLS, col, name, x, y, w, h }
}

/** Gabarit servi quand aucun périphérique n'est ouvert. */
export const DEFAULT_LAYOUT: LayoutView = {
  name: 'Razer DeathStalker V2 Pro (filaire)',
  rows: ROWS,
  cols: COLS,
  // 132, et non 106 : une image couvre toutes les cases de la matrice.
  frameLen: ROWS * COLS,
  keys: ENTRIES.map(toKey),
}

/**
 * Encombrement du dessin, en unités de pas — 22,5 × 6,5 sur ce clavier.
 *
 * Calculé et non écrit en dur : c'est ce qui permet au `viewBox` de suivre
 * n'importe quel gabarit, y compris celui que rendra un jour le Rust.
 */
export function extent(keys: readonly KeyInfo[]): { w: number; h: number } {
  let w = 0
  let h = 0
  for (const k of keys) {
    if (k.x + k.w > w) w = k.x + k.w
    if (k.y + k.h > h) h = k.y + k.h
  }
  // Un gabarit vide donnerait un `viewBox` de surface nulle, que le navigateur
  // refuse de dessiner : on rend une boîte unitaire plutôt qu'un trou noir.
  return { w: w || 1, h: h || 1 }
}

/**
 * Contrôle de cohérence gabarit ↔ image, exécutable.
 *
 * Un dessin faux ne casse aucun test : il se voit à l'œil, ou pas du tout. Ces
 * invariants-là, en revanche, se vérifient — et ce sont exactement ceux que le
 * matériel piège : lire une couleur au rang de la touche plutôt qu'à son
 * `index`, envoyer 106 couleurs au lieu de 132, dupliquer un index.
 *
 * Rendu sous forme de liste plutôt que par une exception : on veut **tous** les
 * défauts d'un coup, et un simulateur qui refuse de s'afficher n'aide personne.
 * Appelé par `KeyboardSimulator` en développement uniquement.
 */
export function layoutProblems(layout: LayoutView, frame: readonly Rgb[]): string[] {
  const problems: string[] = []

  if (frame.length !== layout.frameLen) {
    problems.push(
      `image de ${frame.length} couleurs pour un gabarit qui en attend ${layout.frameLen}`,
    )
  }

  const seen = new Map<number, string>()
  for (const k of layout.keys) {
    if (!Number.isInteger(k.index) || k.index < 0 || k.index >= layout.frameLen) {
      problems.push(`« ${k.name} » porte l'index ${k.index}, hors de l'image`)
    } else if (frame[k.index] === undefined) {
      problems.push(`« ${k.name} » (index ${k.index}) est sans couleur`)
    }

    const other = seen.get(k.index)
    if (other !== undefined) {
      problems.push(`index ${k.index} partagé par « ${other} » et « ${k.name} »`)
    } else {
      seen.set(k.index, k.name)
    }

    if (!(k.w > 0) || !(k.h > 0)) {
      problems.push(`« ${k.name} » (index ${k.index}) est sans surface`)
    }
    if (k.x < 0 || k.y < 0) {
      problems.push(`« ${k.name} » (index ${k.index}) sort du dessin par le haut ou la gauche`)
    }
  }

  // Deux capuchons ne peuvent pas occuper le même espace — y compris les deux
  // bras de l'Entrée en L, qui sont jointifs et non superposés. C'est le seul
  // contrôle qui attrape une erreur de transcription de la géométrie.
  const keys = layout.keys
  for (let i = 0; i < keys.length; i++) {
    const a = keys[i]
    for (let j = i + 1; j < keys.length; j++) {
      const b = keys[j]
      const disjoint =
        a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y
      if (!disjoint) {
        problems.push(
          `« ${a.name} » (${a.index}) et « ${b.name} » (${b.index}) se chevauchent`,
        )
      }
    }
  }

  return problems
}
