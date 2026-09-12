/**
 * Le point de départ d'un nouvel effet.
 *
 * Ce n'est pas un modèle écrit pour l'occasion : c'est **l'effet de référence**
 * de `@candeo/effects-api`, lu tel quel. Un modèle recopié ici se serait mis à
 * diverger du jour où l'API aurait changé, et personne ne l'aurait vu — un
 * exemple n'a pas de test qui le contredise.
 *
 * Seule retouche, et elle est mécanique : l'exemple vit **dans** le paquet, il
 * importe donc `./index`. Un effet de l'utilisateur, lui, est résolu par le
 * chargeur de modules de rquickjs, qui ne connaît que le nom du paquet.
 */

import example from '@candeo/effects-api/src/example.ts?raw'

import { erreur } from '../api/journal'

const LOCAL = "'./index'"
const PUBLIC = "'@candeo/effects-api'"

export const NEW_EFFECT = example.replace(LOCAL, PUBLIC)

// Une substitution qui ne trouve rien passerait inaperçue : le modèle
// importerait `./index`, que le moteur ne sait pas résoudre, et l'erreur
// n'arriverait qu'au démarrage de l'effet. `import.meta.env.DEV` est remplacé à
// la compilation — rien de ceci ne subsiste dans l'application livrée.
if (import.meta.env.DEV && !example.includes(LOCAL)) {
  erreur(
    'modèle',
    `${LOCAL} est introuvable dans example.ts — le modèle importe un module que le ` +
      'moteur ne saura pas résoudre.',
  )
}
