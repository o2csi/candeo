/**
 * Contrôle de cohérence gabarit ↔ image, pour le simulateur.
 *
 * ⚠️ Ce fichier ne contient **aucune géométrie**. Elle vit au seul endroit où
 * elle est testée — `crates/candeo-device/src/layout.rs` — et arrive par la
 * commande `get_default_layout()` quand rien n'est branché. Une transcription
 * ici serait une seconde source de vérité, et celle-là ne diverge pas
 * bruyamment : elle diverge en silence.
 *
 * La géométrie n'est d'ailleurs pas un relevé : le périphérique n'expose que sa
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
  /**
   * Ce que sont ces lumières : le dessin est le même, ce qu'on en dit ne l'est
   * pas — « 103 touches éclairées » sur un cerclage et un logo serait faux.
   * Absent, on lit des touches, comme avant qu'un appareil sache le dire.
   */
  lights?: 'keys' | 'zones'
}

/**
 * Encombrement du dessin, en unités de pas — 22,5 × 6,5 sur ce clavier.
 *
 * Calculé et non écrit en dur : c'est ce qui permet au `viewBox` de suivre
 * n'importe quel gabarit, quel que soit le modèle que le Rust renvoie.
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
 * Contrôle de cohérence gabarit ↔ image.
 *
 * La géométrie elle-même est vérifiée côté Rust, où elle est écrite. Restent
 * les invariants que seul l'assemblage peut trahir, et ce sont exactement ceux
 * que ce matériel piège : lire une couleur au rang de la touche plutôt qu'à son
 * `index`, recevoir 106 couleurs au lieu de 132, voir deux touches partager un
 * index.
 *
 * Rendu sous forme de liste plutôt que par une exception : on veut **tous** les
 * défauts d'un coup, et un simulateur qui refuse de s'afficher n'aide personne.
 * Appelé par `KeyboardSimulator` en développement uniquement.
 */
export function layoutProblems(layout: LayoutView, frame: readonly Rgb[]): string[] {
  const problems: string[] = []

  if (frame.length !== layout.frameLen) {
    problems.push(`frame of ${frame.length} colors for a layout expecting ${layout.frameLen}`)
  }

  const seen = new Set<number>()
  for (const k of layout.keys) {
    if (!Number.isInteger(k.index) || k.index < 0 || k.index >= layout.frameLen) {
      problems.push(`key ${k.index} is outside the frame`)
    } else if (frame[k.index] === undefined) {
      problems.push(`key ${k.index} has no color`)
    }

    if (seen.has(k.index)) problems.push(`index ${k.index} is used by two keys`)
    seen.add(k.index)

    if (!(k.w > 0) || !(k.h > 0)) {
      problems.push(`key ${k.index} has no area`)
    }
    if (k.x < 0 || k.y < 0) {
      problems.push(`key ${k.index} leaves the drawing at the top or left`)
    }
  }

  // Deux capuchons ne peuvent pas occuper le même espace — y compris les deux
  // bras de l'Entrée en L, qui sont jointifs et non superposés. Doublon assumé
  // du test Rust `key_rectangles_do_not_overlap` : celui-ci vaut pour le
  // gabarit qui arrive réellement, quel qu'il soit.
  const keys = layout.keys
  for (let i = 0; i < keys.length; i++) {
    const a = keys[i]
    for (let j = i + 1; j < keys.length; j++) {
      const b = keys[j]
      const disjoint =
        a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y
      if (!disjoint) {
        problems.push(`keys ${a.index} and ${b.index} overlap`)
      }
    }
  }

  return problems
}
