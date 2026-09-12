/**
 * Mise en route de Monaco : ouvriers, thème, service de langage TypeScript.
 *
 * ## Pourquoi Monaco
 *
 * Pour son service de langage. En lui donnant la déclaration de
 * `@candeo/effects-api`, on obtient types, autocomplétion et erreurs en ligne
 * sans écrire la moindre règle (`docs/design/studio.md` §2).
 *
 * ## Comment la déclaration lui parvient — sans copie
 *
 * `?raw` lit `packages/effects-api/src/index.ts` **à la construction** et
 * l'inscrit dans le paquet sous forme de chaîne. Le fichier reste donc la seule
 * source : il n'est ni recopié, ni régénéré, ni converti en `.d.ts`. Monaco
 * n'exige pas une déclaration — `addExtraLib` accepte n'importe quel
 * TypeScript, et le service en tire la même chose. Corps de fonctions compris,
 * ce qui vaut mieux : l'infobulle de `hsv` montre alors le code réel.
 *
 * Le fichier est déposé là où un paquet vit, `node_modules/<nom>/index.ts`, et
 * `paths` l'y envoie directement : la résolution ne dépend alors d'aucune
 * subtilité de la recherche dans `node_modules` à l'intérieur de l'ouvrier.
 *
 * ## Les ouvriers ne viennent pas d'un CDN
 *
 * `?worker` de Vite empaquette chaque ouvrier comme un actif du projet, en
 * développement comme dans l'application livrée. Rien n'est téléchargé à
 * l'usage : l'application s'ouvre hors ligne.
 */

import * as monaco from 'monaco-editor/editor/editor.api.js'
import EditorWorker from 'monaco-editor/editor/editor.worker.js?worker'
import TsWorker from 'monaco-editor/language/typescript/ts.worker.js?worker'
// Enregistre l'identifiant de langage « typescript » et sa coloration. Sans
// lui, `languages.onLanguage('typescript')` n'est jamais déclenché et le
// service de langage ne démarre pas.
import 'monaco-editor/languages/definitions/typescript/register.js'
import {
  getTypeScriptWorker,
  ModuleKind,
  ModuleResolutionKind,
  ScriptTarget,
  typescriptDefaults,
} from 'monaco-editor/languages/features/typescript/register.js'

import effectsApiSource from '@candeo/effects-api/src/index.ts?raw'

/** Là où le service de langage trouve `@candeo/effects-api`. */
const API_PATH = 'node_modules/@candeo/effects-api/index.ts'
const API_URI = `file:///${API_PATH}`

/** Le fichier qu'on édite, vu par le service de langage. */
export const EFFECT_URI = monaco.Uri.parse('file:///effet.ts')

const THEME = 'candeo'

self.MonacoEnvironment = {
  getWorker(_id, label) {
    return label === 'typescript' || label === 'javascript' ? new TsWorker() : new EditorWorker()
  },
}

/**
 * Couleurs de l'éditeur, lues dans les jetons de style.
 *
 * Monaco veut des couleurs en clair : il ne sait pas lire une variable CSS. On
 * les lui donne donc résolues, mais elles restent définies au seul endroit qui
 * les définit — `styles/tokens.css`. Un jeton renommé fait disparaître une
 * couleur de l'éditeur, il ne le fait pas tomber.
 */
function palette(style: CSSStyleDeclaration): Record<string, string> {
  const wanted: Record<string, string> = {
    'editor.background': '--ground',
    'editor.foreground': '--text',
    'editorLineNumber.foreground': '--text-faint',
    'editorLineNumber.activeForeground': '--text-muted',
    'editorCursor.foreground': '--accent',
    'editor.lineHighlightBackground': '--raised',
    'editor.selectionBackground': '--raised-2',
    'editorIndentGuide.background1': '--line',
    'editorWidget.background': '--raised',
    'editorWidget.border': '--line',
    'editorSuggestWidget.background': '--raised',
    'editorSuggestWidget.border': '--line',
    'editorSuggestWidget.selectedBackground': '--raised-2',
    'editorHoverWidget.background': '--raised',
    'editorHoverWidget.border': '--line',
    'editorError.foreground': '--bad',
    'editorWarning.foreground': '--warn',
    'scrollbarSlider.background': '--line',
    'scrollbarSlider.hoverBackground': '--line-strong',
  }

  const colors: Record<string, string> = {}
  for (const [key, token] of Object.entries(wanted)) {
    const value = style.getPropertyValue(token).trim()
    if (value) colors[key] = value
  }
  return colors
}

/**
 * Accorde l'éditeur au thème de l'application.
 *
 * Le sens clair/sombre n'est pas redevine ici : `tokens.css` pose déjà
 * `color-scheme` sur la racine, en tenant compte du réglage système **et** de
 * `data-theme`. On le lit, on ne le recalcule pas.
 */
function applyTheme(): void {
  const style = getComputedStyle(document.documentElement)
  monaco.editor.defineTheme(THEME, {
    base: style.colorScheme.includes('dark') ? 'vs-dark' : 'vs',
    inherit: true,
    rules: [],
    colors: palette(style),
  })
  monaco.editor.setTheme(THEME)
}

let started = false

/**
 * Configure le service de langage. Idempotent : les réglages sont globaux à
 * Monaco, les poser deux fois n'aurait pas de sens.
 */
export function setupMonaco(): void {
  if (started) return
  started = true

  typescriptDefaults.setCompilerOptions({
    target: ScriptTarget.ES2020,
    // **ESNext, jamais CommonJS** : `import { hsv } from '@candeo/effects-api'`
    // doit rester un import à la sortie, c'est le chargeur de modules de
    // rquickjs qui le résout vers le module interne de l'hôte.
    module: ModuleKind.ESNext,
    moduleResolution: ModuleResolutionKind.NodeJs,
    // `strict` entier, `noImplicitAny` compris.
    //
    // Le désactiver rendrait `layout`, `time` et `frame` implicitement `any`
    // dès que l'auteur retire le `satisfies EffectModule` du modèle — c'est-à-
    // dire qu'il **supprimerait l'autocomplétion** dans le seul cas où elle
    // manque, et sans rien dire. Or c'est elle qui justifie Monaco.
    //
    // `satisfies EffectModule` fait donc partie du contrat, et le modèle de
    // départ comme la documentation l'écrivent. Un auteur qui l'enlève voit une
    // erreur explicite plutôt qu'un typage qui s'évapore.
    strict: true,
    allowNonTsExtensions: true,
    // Ni DOM ni Node : un effet tourne dans QuickJS, côté Rust. Ni `document`,
    // ni `fetch`, ni même `console` n'y existent — les proposer en
    // autocomplétion serait promettre ce que le moteur ne fournit pas.
    lib: ['es2020'],
    baseUrl: 'file:///',
    paths: { '@candeo/effects-api': [API_PATH] },
  })

  typescriptDefaults.addExtraLib(effectsApiSource, API_URI)

  // L'ouvrier reçoit les modèles dès leur création, sans attendre que l'éditeur
  // les lui pousse. Sans cela, interroger le service juste après l'ouverture
  // peut porter sur un fichier qu'il n'a pas encore.
  typescriptDefaults.setEagerModelSync(true)

  applyTheme()
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme)
}

/**
 * Le modèle du fichier édité.
 *
 * Un seul, réutilisé : l'URI est ce qui relie l'éditeur au service de langage,
 * et deux modèles ne peuvent pas porter la même. Rouvrir l'éditeur remplace
 * donc le contenu plutôt que de créer un second fichier.
 */
export function effectModel(source: string): monaco.editor.ITextModel {
  const existing = monaco.editor.getModel(EFFECT_URI)
  if (existing) {
    if (existing.getValue() !== source) existing.setValue(source)
    return existing
  }
  return monaco.editor.createModel(source, 'typescript', EFFECT_URI)
}

/** Une erreur du service de langage, réduite à ce que l'interface affiche. */
export interface EffectError {
  line: number
  message: string
}

/**
 * Les erreurs que le service de langage voit dans le fichier.
 *
 * **Demandées à l'ouvrier, et non lues dans les marqueurs de l'éditeur.** Les
 * marqueurs sont posés de façon asynchrone : une validation lancée peu après
 * l'ouverture pouvait lire ceux d'une passe antérieure, effectuée avant que la
 * déclaration de `@candeo/effects-api` n'ait atteint l'ouvrier. Le service
 * annonçait alors des paramètres implicitement `any` — donc un refus, sur un
 * effet parfaitement correct, avec un message qui ne correspondait à rien de
 * visible à l'écran.
 *
 * L'ouvrier, lui, répond sur l'état courant. Il n'y a plus de fenêtre pendant
 * laquelle la réponse est fausse.
 */
export async function errors(): Promise<EffectError[]> {
  const uri = EFFECT_URI.toString()
  const worker = await (await getTypeScriptWorker())(EFFECT_URI)
  const [syntactic, semantic] = await Promise.all([
    worker.getSyntacticDiagnostics(uri),
    worker.getSemanticDiagnostics(uri),
  ])

  const model = monaco.editor.getModel(EFFECT_URI)
  return [...syntactic, ...semantic].map((d) => ({
    line: model && d.start !== undefined ? model.getPositionAt(d.start).lineNumber : 1,
    message:
      typeof d.messageText === 'string' ? d.messageText : d.messageText.messageText,
  }))
}

export { monaco }
