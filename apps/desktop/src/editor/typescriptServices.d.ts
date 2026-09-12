/**
 * Le compilateur TypeScript que Monaco embarque déjà.
 *
 * Monaco ne livre pas un analyseur maison : son service de langage est le
 * **compilateur TypeScript complet**, publié dans le paquet sous le nom
 * `typescriptServices` et réexporté par `ts.worker.js` sous le nom `ts`. Ce
 * module n'a pas de déclaration de types — d'où celle-ci.
 *
 * L'import passe par le chemin direct plutôt que par `ts.worker.js` : ce
 * dernier pose `self.onmessage`, ce qu'un module chargé dans la fenêtre n'a
 * aucune raison de faire.
 *
 * **Les types viennent du paquet `typescript`, l'implémentation de Monaco.**
 * `typescript` est déjà une dépendance de développement (c'est `vue-tsc` qui
 * l'utilise) : elle ne pèse rien dans la construction, puisque seuls ses types
 * sont lus. Rien de tout cela n'ajoute un second transpileur — voir
 * `docs/design/studio.md` §2.
 */
declare module 'monaco-editor/languages/features/typescript/lib/typescriptServices.js' {
  import type * as ts from 'typescript'

  export const typescript: typeof ts
}
