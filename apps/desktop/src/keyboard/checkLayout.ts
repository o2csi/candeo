/**
 * Contrôle de cohérence du gabarit de repli — exécutable à la main.
 *
 * ```
 * cd apps/desktop && node --experimental-strip-types src/keyboard/checkLayout.ts
 * ```
 *
 * Le dépôt n'a pas de lanceur de tests côté front, et l'intégration continue ne
 * fait tourner que `vue-tsc` et la construction. Plutôt que d'ajouter une
 * dépendance que rien n'exécuterait, ce fichier est un point d'entrée : Node
 * sait retirer les types depuis la 22.6, et `vue-tsc --noEmit` le vérifie
 * comme le reste de `src/`.
 *
 * Ce qu'il attrape, et que l'œil n'attrape pas : un index hors image, un index
 * en double, deux capuchons qui se chevauchent, une touche sans couleur. Ce
 * qu'il n'attrape pas, et qu'il faut regarder : un rectangle juste mais mal
 * placé. Voir `docs/api/commands.md`, « La géométrie n'est pas une lecture du
 * périphérique ».
 *
 * Volontairement sans `process` : le paquet n'a pas les types de Node. Un
 * défaut lève, ce qui suffit à rendre la main sur un code d'erreur non nul.
 */

import type { Rgb } from '../api/types.ts'
import { demoFrame } from './demoFrames.ts'
import { DEFAULT_LAYOUT, extent, layoutProblems } from './layout.ts'

const fails: string[] = []

function expect(what: string, ok: boolean, seen: unknown) {
  console.log(`${ok ? '  ok  ' : ' ÉCHEC'} ${what} — ${JSON.stringify(seen)}`)
  if (!ok) fails.push(what)
}

const layout = DEFAULT_LAYOUT
const size = extent(layout.keys)

expect('une image couvre 132 cases', layout.frameLen === 132, layout.frameLen)
expect('106 touches portent une LED', layout.keys.length === 106, layout.keys.length)
expect('le dessin fait 22,5 u × 6,5 u', size.w === 22.5 && size.h === 6.5, size)

// Le compte par rangée : une rangée décalée d'une touche se voit là, et nulle
// part ailleurs. Mêmes valeurs que le test Rust `rows_have_expected_key_counts`.
const perRow = [0, 0, 0, 0, 0, 0]
for (const k of layout.keys) perRow[k.row]++
expect(
  'les rangées comptent 16, 21, 21, 17, 18 et 13 touches',
  String(perRow) === String([16, 21, 21, 17, 18, 13]),
  perRow,
)

// L'Entrée ISO porte deux LED : deux rectangles jointifs, jamais superposés,
// appuyés sur le même bord droit. Les dédupliquer par nom effacerait le dégradé
// vertical réellement visible sur l'appareil.
const enter = layout.keys.filter((k) => k.name === 'Entrée')
expect("l'Entrée ISO apparaît deux fois", enter.length === 2, enter.map((k) => k.index))
expect(
  'ses deux bras partagent le bord droit du bloc principal',
  enter.every((k) => k.x + k.w === 15),
  enter.map((k) => k.x + k.w),
)
expect(
  'ses deux bras se touchent sans se recouvrir',
  enter.length === 2 && enter[0].y + enter[0].h === enter[1].y && enter[1].x > enter[0].x,
  enter.map((k) => [k.x, k.y, k.w, k.h]),
)

// La barre d'espace est l'inverse : une seule LED pour 6,25 u.
const space = layout.keys.filter((k) => k.name === 'Espace')
expect(
  "la barre d'espace n'a qu'une LED pour 6,25 u",
  space.length === 1 && space[0].w === 6.25 && space[0].index === 116,
  space,
)

// Une image de démonstration : elle doit couvrir la matrice, pas les touches.
const frame = demoFrame(layout, 0.37)
expect("l'image de démonstration fait frameLen couleurs", frame.length === 132, frame.length)
expect(
  'toute touche trouve sa couleur à son index',
  layout.keys.every((k) => frame[k.index] !== undefined),
  layout.keys.filter((k) => frame[k.index] === undefined).map((k) => k.name),
)

const problems = layoutProblems(layout, frame)
expect('aucune incohérence gabarit ↔ image', problems.length === 0, problems)

// Le piège du matériel, pris à l'envers : une image aux dimensions de `keys`
// doit être refusée. Si ce contrôle passait, le simulateur accepterait
// silencieusement une image qui laisse les dernières rangées figées.
const tooShort: readonly Rgb[] = frame.slice(0, layout.keys.length)
expect(
  'une image de 106 couleurs est signalée',
  layoutProblems(layout, tooShort).length > 0,
  layoutProblems(layout, tooShort).length,
)

if (fails.length) {
  throw new Error(`${fails.length} contrôle(s) en échec : ${fails.join(' ; ')}`)
}
console.log(`\nGabarit « ${layout.name} » : tous les contrôles passent.`)
