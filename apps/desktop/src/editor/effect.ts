/**
 * De la source TypeScript à ce qu'attend `install_effect` : du JavaScript et un
 * manifeste.
 *
 * ## Le transpileur était déjà dans la boîte
 *
 * Le service de langage de Monaco **est** le compilateur TypeScript. Retirer
 * les types est donc gratuit : `transpileModule()` est une fonction de texte
 * vers texte, déterministe, sans sémantique d'exécution. Ni `esbuild-wasm`, ni
 * `oxc`, ni `swc` — le seul lieu qui comptait, l'**exécution**, est côté Rust
 * (`docs/design/studio.md` §3).
 *
 * Le compilateur est chargé à la demande, et non à l'ouverture de l'éditeur :
 * il ne sert qu'au moment de valider, et il pèse quelques mégaoctets.
 *
 * ## Le manifeste est lu, pas exécuté
 *
 * `name`, `description` et `params` sont déclarés dans le module de
 * l'utilisateur. Les obtenir en évaluant ce module reviendrait à exécuter du
 * code d'effet dans la fenêtre — exactement ce que cette conception s'interdit,
 * et pour deux raisons : le front n'a alors aucun accès au DOM à offrir à un
 * effet, et l'aperçu **est** la production parce qu'un seul moteur exécute.
 *
 * On les lit donc dans l'arbre syntaxique, avec le même compilateur. La
 * contrepartie est explicite : ces trois champs doivent être des **littéraux**.
 * Un nom calculé est refusé, avec un message qui le dit.
 */

import type * as TS from 'typescript'

import {
  EFFECTS_API_VERSION,
  startingParams,
  type EffectManifest,
  type EffectParams,
} from '../api/candeo'

/** Nom de fichier donné au compilateur : il n'apparaît que dans les messages. */
const FILE = 'effet.ts'

let loading: Promise<typeof TS> | null = null

/**
 * Le compilateur de Monaco, chargé une fois.
 *
 * Import dynamique : Vite en fait un morceau à part, lu au premier
 * enregistrement plutôt qu'à l'ouverture de la fenêtre.
 */
function compiler(): Promise<typeof TS> {
  loading ??= import(
    'monaco-editor/languages/features/typescript/lib/typescriptServices.js'
  ).then((m) => m.typescript)
  return loading
}

export interface Compiled {
  /** Ce que le moteur exécutera. */
  js: string
  manifest: EffectManifest
  /** Valeurs de départ des paramètres, telles que l'effet les déclare. */
  params: EffectParams
}

/**
 * Le nom déclaré par la source, sans rien exécuter.
 *
 * `null` quand la source n'est pas (encore) analysable — on écrit du code, il
 * est incomplet la plupart du temps. L'appelant garde alors le dernier nom
 * connu plutôt que de vider son champ à chaque frappe.
 */
export async function nameInSource(source: string): Promise<string | null> {
  const ts = await compiler()
  const file = ts.createSourceFile(FILE, source, ts.ScriptTarget.ES2020, true)

  let exported: TS.Expression
  try {
    exported = unwrap(ts, defaultExport(ts, file))
  } catch {
    return null
  }
  if (!ts.isObjectLiteralExpression(exported)) return null

  const literal = member(ts, exported, 'name')
  if (!literal || literal === 'méthode' || !ts.isStringLiteralLike(literal)) return null
  return literal.text
}

/**
 * Renomme l'effet **dans sa source**.
 *
 * Sert à ouvrir un effet intégré comme une copie : l'utilisateur voit le
 * nouveau nom dans son code, il n'a pas à le changer lui-même et rien ne lui
 * est demandé.
 *
 * Le littéral est localisé par l'analyse et remplacé sur son étendue exacte.
 * Un remplacement de texte se tromperait dès qu'une description reprend le
 * nom — ce qui est le cas de plusieurs effets livrés.
 */
export async function renameInSource(source: string, name: string): Promise<string> {
  const ts = await compiler()
  const file = ts.createSourceFile(FILE, source, ts.ScriptTarget.ES2020, true)

  // Renommer est un confort, jamais une condition pour ouvrir : une source que
  // l'analyse ne reconnaît pas doit rester lisible et modifiable. On la rend
  // telle quelle, et c'est la validation qui dira ce qui ne va pas.
  let exported: TS.Expression
  try {
    exported = unwrap(ts, defaultExport(ts, file))
  } catch {
    return source
  }

  if (!ts.isObjectLiteralExpression(exported)) return source

  const literal = member(ts, exported, 'name')
  if (!literal || literal === 'méthode' || !ts.isStringLiteralLike(literal)) return source

  // On conserve le guillemet d'origine plutôt que d'en imposer un.
  const quote = source[literal.getStart(file)] ?? "'"
  const replacement = `${quote}${name.replace(quote, `\\${quote}`)}${quote}`
  return source.slice(0, literal.getStart(file)) + replacement + source.slice(literal.getEnd())
}

/**
 * Transpile et relève le manifeste.
 *
 * Lève une erreur au message lisible : il est affiché tel quel, comme ceux qui
 * viennent du Rust.
 */
export async function compile(source: string): Promise<Compiled> {
  const ts = await compiler()

  const js = ts.transpileModule(source, {
    fileName: FILE,
    compilerOptions: {
      target: ts.ScriptTarget.ES2020,
      // **Pas CommonJS.** `import { hsv } from '@candeo/effects-api'` doit
      // rester un import : c'est le chargeur de modules de rquickjs qui le
      // résout vers le module interne de l'hôte.
      module: ts.ModuleKind.ESNext,
    },
  }).outputText

  const manifest = readManifest(ts, source)
  return { js, manifest, params: startingParams(manifest) }
}

// ---------------------------------------------------------------- manifeste

function readManifest(ts: typeof TS, source: string): EffectManifest {
  const file = ts.createSourceFile(FILE, source, ts.ScriptTarget.ES2020, true)
  const exported = defaultExport(ts, file)

  if (!ts.isObjectLiteralExpression(exported)) {
    throw new Error(
      "l'export par défaut doit être écrit sur place, sous forme d'objet : " +
        'son nom et ses paramètres sont relevés dans le code, pas en exécutant ' +
        "l'effet — voir `export default { name: …, render(ctx) { … } }`",
    )
  }

  if (!member(ts, exported, 'render')) {
    throw new Error(
      "l'effet doit porter une fonction `render` : c'est elle que le moteur " +
        'appelle à chaque image',
    )
  }

  const name = text(ts, file, exported, 'name')
  if (name === null || name.trim() === '') {
    throw new Error("l'effet doit avoir un `name`, sous forme de chaîne littérale")
  }

  const description = text(ts, file, exported, 'description')
  const declared = member(ts, exported, 'params')

  return {
    name,
    ...(description === null ? {} : { description }),
    ...(declared === undefined ? {} : { params: params(ts, file, declared) }),
    apiVersion: EFFECTS_API_VERSION,
  }
}

function defaultExport(ts: typeof TS, file: TS.SourceFile): TS.Expression {
  for (const statement of file.statements) {
    if (ts.isExportAssignment(statement) && !statement.isExportEquals) {
      return unwrap(ts, statement.expression)
    }
  }
  throw new Error(
    "l'effet doit avoir un export par défaut, et rien d'autre : " +
      'export default { name: "Mon effet", render(ctx) { … } }',
  )
}

/** Retire ce qui n'existe qu'à la compilation : `satisfies`, `as`, parenthèses. */
function unwrap(ts: typeof TS, node: TS.Expression): TS.Expression {
  let current = node
  for (;;) {
    if (
      ts.isSatisfiesExpression(current) ||
      ts.isAsExpression(current) ||
      ts.isParenthesizedExpression(current)
    ) {
      current = current.expression
      continue
    }
    // `defineEffect({ … })` — l'enveloppe recommandée. Elle ne fait rien à
    // l'exécution ; son seul rôle est de donner un type contextuel à l'objet,
    // pour que les paramètres de `render` soient typés sans `satisfies`.
    if (
      ts.isCallExpression(current) &&
      ts.isIdentifier(current.expression) &&
      current.expression.text === 'defineEffect' &&
      current.arguments.length === 1
    ) {
      current = current.arguments[0]
      continue
    }
    return current
  }
}

/** La valeur associée à `name`, ou `undefined` si la propriété est absente. */
function member(
  ts: typeof TS,
  object: TS.ObjectLiteralExpression,
  name: string,
): TS.Expression | 'méthode' | undefined {
  for (const property of object.properties) {
    if (!property.name || !ts.isIdentifier(property.name) || property.name.text !== name) continue
    return ts.isPropertyAssignment(property) ? unwrap(ts, property.initializer) : 'méthode'
  }
  return undefined
}

/** Une propriété qui doit être une chaîne littérale, quand elle est présente. */
function text(
  ts: typeof TS,
  file: TS.SourceFile,
  object: TS.ObjectLiteralExpression,
  name: string,
): string | null {
  const value = member(ts, object, name)
  if (value === undefined) return null
  if (value === 'méthode' || !ts.isStringLiteralLike(value)) {
    throw new Error(`\`${name}\` doit être une chaîne écrite sur place${where(ts, file, object)}`)
  }
  return value.text
}

/**
 * Les paramètres déclarés, relus tels quels.
 *
 * Seul `default` est vérifié : c'est la seule chose dont l'éditeur se serve —
 * il en fait les valeurs de départ passées à `start_effect`. Le reste part
 * intact vers `manifest.json`, que le Rust ne relit pas davantage : la forme de
 * `ParamSpec` appartient à `@candeo/effects-api`, la redire ici en ferait une
 * seconde source de vérité.
 */
function params(
  ts: typeof TS,
  file: TS.SourceFile,
  declared: TS.Expression | 'méthode',
): EffectManifest['params'] {
  if (declared === 'méthode' || !ts.isObjectLiteralExpression(declared)) {
    throw new Error(`\`params\` doit être un objet écrit sur place`)
  }

  const specs: Record<string, unknown> = {}
  for (const property of declared.properties) {
    if (!ts.isPropertyAssignment(property)) {
      throw new Error(`chaque paramètre doit être écrit sur place${where(ts, file, property)}`)
    }
    const key = ts.isIdentifier(property.name) || ts.isStringLiteralLike(property.name)
      ? property.name.text
      : null
    if (key === null) {
      throw new Error(`nom de paramètre illisible${where(ts, file, property)}`)
    }

    const spec = literal(ts, file, unwrap(ts, property.initializer))
    if (typeof spec !== 'object' || spec === null || !('default' in spec)) {
      throw new Error(
        `le paramètre « ${key} » doit déclarer un \`default\`${where(ts, file, property)}`,
      )
    }
    specs[key] = spec
  }

  // Cet emplacement est le seul de la chaîne qui interprète `ParamSpec` ; sa
  // forme exacte est vérifiée par le compilateur dans l'éditeur, grâce au
  // `satisfies EffectModule` du modèle.
  return specs as EffectManifest['params']
}

/**
 * Valeur d'un littéral, sans rien évaluer.
 *
 * Refuser plutôt que deviner : un `default` calculé ne peut pas être relu sans
 * exécuter le module, et l'éditeur ne l'exécute jamais.
 */
function literal(ts: typeof TS, file: TS.SourceFile, node: TS.Expression): unknown {
  if (ts.isStringLiteralLike(node)) return node.text
  if (ts.isNumericLiteral(node)) return Number(node.text)
  if (node.kind === ts.SyntaxKind.TrueKeyword) return true
  if (node.kind === ts.SyntaxKind.FalseKeyword) return false
  if (node.kind === ts.SyntaxKind.NullKeyword) return null

  if (ts.isPrefixUnaryExpression(node) && ts.isNumericLiteral(node.operand)) {
    const value = Number(node.operand.text)
    if (node.operator === ts.SyntaxKind.MinusToken) return -value
    if (node.operator === ts.SyntaxKind.PlusToken) return value
  }

  if (ts.isArrayLiteralExpression(node)) {
    return node.elements.map((element) => literal(ts, file, unwrap(ts, element)))
  }

  if (ts.isObjectLiteralExpression(node)) {
    const out: Record<string, unknown> = {}
    for (const property of node.properties) {
      if (!ts.isPropertyAssignment(property)) {
        throw new Error(`valeur non littérale${where(ts, file, property)}`)
      }
      const key = ts.isIdentifier(property.name) || ts.isStringLiteralLike(property.name)
        ? property.name.text
        : null
      if (key === null) throw new Error(`clé illisible${where(ts, file, property)}`)
      out[key] = literal(ts, file, unwrap(ts, property.initializer))
    }
    return out
  }

  throw new Error(
    `valeur non littérale${where(ts, file, node)} : un paramètre déclaré est relu ` +
      "dans le code, il n'est pas calculé",
  )
}

/** « , ligne 12 » — de quoi retrouver le passage en cause. */
function where(ts: typeof TS, file: TS.SourceFile, node: TS.Node): string {
  const { line } = ts.getLineAndCharacterOfPosition(file, node.getStart(file))
  return `, ligne ${line + 1}`
}
