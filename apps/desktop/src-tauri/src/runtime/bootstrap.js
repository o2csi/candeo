// Colle entre l'hôte Rust et le module de l'utilisateur.
//
// Évalué une fois au démarrage d'un effet. Il importe le module de
// l'utilisateur sous le nom `effect`, construit le contexte de rendu, et
// installe `globalThis.__candeo_render`, que la boucle Rust appelle à chaque
// image.
//
// L'hôte pose au préalable deux globales : `__candeo_frame_len` et
// `__candeo_layout` (JSON). Les passer ainsi évite de fabriquer le contexte à
// chaque image.

// Import de l'espace de noms, et non `import effect from 'effect'` : ce dernier
// échoue à la **liaison** du module quand l'export par défaut manque, avec un
// message de QuickJS que personne ne peut relier à son code. Ici l'import
// réussit toujours, et c'est nous qui disons ce qui ne va pas.
import * as module from 'effect'

const FRAME_LEN = globalThis.__candeo_frame_len
const layout = JSON.parse(globalThis.__candeo_layout)

const effect = module.default

if (typeof effect?.render !== 'function') {
  throw new TypeError(
    'the effect must have a default export with a `render` function, ' +
      'for example: export default defineEffect({ render(ctx) { … } })',
  )
}

// The manifest as the module declares it, set once at load. The library reads it
// when it compiles an effect, and keeps it in the cache, so that listing runs no
// engine.
globalThis.__candeo_manifest = JSON.stringify({
  name: effect.name ?? '',
  // The version of the effects API the module says it was written against;
  // `null` when it says nothing, which the library reads as the first one.
  apiVersion: effect.apiVersion ?? null,
  description: effect.description ?? '',
  params: effect.params ?? {},
  inputs: Array.isArray(effect.inputs) ? effect.inputs : [],
})

// Read by the render loop once loaded: only an effect that declares keys gets
// presses, and makes the engine read them (`docs/design/key-input.md` §3).
globalThis.__candeo_reads_keys = Array.isArray(effect.inputs) && effect.inputs.includes('keys')

// Frozen and shared: an effect that declares no keys receives this one list on
// every frame, with nothing to allocate or to modify.
const NO_PRESSES = Object.freeze([])

// Tampon réutilisé d'une image à l'autre : l'allouer 30 fois par seconde
// ferait travailler le ramasse-miettes pour rien. La cadence a baissé, pas
// l'argument — le ramasse-miettes de QuickJS se déclenche sur le volume alloué,
// et 30 tableaux de 396 entrées par seconde restent 30 de trop quand un seul
// suffit.
const buf = new Array(FRAME_LEN * 3).fill(0)

// Borne ici, et pas seulement dans `rgb()` : rien n'oblige un effet à passer
// par l'API, il peut fabriquer `{r, g, b}` à la main. Une valeur hors bornes ou
// NaN doit devenir un octet valide, sinon c'est la conversion côté Rust qui
// échoue — loin de la cause.
function byte(v) {
  const n = Math.round(v)
  if (!(n >= 0)) return 0
  return n > 255 ? 255 : n
}

function put(index, c) {
  // Une position hors image est ignorée plutôt que de faire échouer l'effet :
  // un gabarit peut changer, le code de l'utilisateur non.
  if (!(index >= 0) || index >= FRAME_LEN) return
  const i = index * 3
  buf[i] = byte(c.r)
  buf[i + 1] = byte(c.g)
  buf[i + 2] = byte(c.b)
}

const frame = {
  set(key, color) {
    put(key?.index, color ?? { r: 0, g: 0, b: 0 })
  },
  fill(color) {
    // Sur **toutes** les positions de la matrice, pas seulement les touches :
    // une image en couvre 132, pas 106.
    for (let i = 0; i < FRAME_LEN; i++) put(i, color)
  },
}

// `pressesJson` is `[{ "k": <position in layout.keys>, "at": <seconds> }]`, or
// empty when the effect reads no keys. Positions, so that the effect receives the
// layout's own `Key` objects rather than copies.
function presses(pressesJson) {
  if (!pressesJson) return NO_PRESSES
  return JSON.parse(pressesJson)
    .map((p) => ({ key: layout.keys[p.k], at: p.at }))
    .filter((p) => p.key !== undefined)
}

globalThis.__candeo_render = (time, frameIndex, paramsJson, pressesJson) => {
  // Chaque image repart du noir : une image est complète par définition, et un
  // effet qui n'écrit qu'une partie du clavier ne doit pas hériter en silence
  // de ce qu'il y avait avant.
  buf.fill(0)

  effect.render({
    layout,
    time,
    frameIndex,
    frame,
    params: JSON.parse(paramsJson),
    presses: presses(pressesJson),
  })

  return buf
}
