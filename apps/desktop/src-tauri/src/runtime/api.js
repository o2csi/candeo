// Module interne `@candeo/effects-api`, fourni par l'hôte au moteur QuickJS.
//
// ⚠️ Ce fichier et `packages/effects-api/src/index.ts` décrivent la MÊME API :
// le `.d.ts` est ce que l'éditeur montre en autocomplétion, ceci est ce que le
// moteur fournit réellement. S'ils divergent, l'éditeur promet une fonction qui
// n'existe pas, et l'erreur ne se voit qu'à la première image.
//
// Le test `api_js_exports_match_the_typescript_surface` échoue si un nom
// disparaît d'ici. Toute modification doit toucher les deux fichiers.

export const BLACK = { r: 0, g: 0, b: 0 }

/**
 * Déclare un effet. Identité à l'exécution — elle n'existe que pour donner un
 * type contextuel côté éditeur, et éviter à l'auteur d'écrire
 * `satisfies EffectModule`.
 */
export function defineEffect(effect) {
  return effect
}

function clampByte(v) {
  // `| 0` tronque vers zéro et écarte NaN — un effet qui produit NaN doit
  // donner du noir, pas une couleur indéterminée.
  const n = Math.round(v)
  if (!(n >= 0)) return 0
  return n > 255 ? 255 : n | 0
}

export function rgb(r, g, b) {
  return { r: clampByte(r), g: clampByte(g), b: clampByte(b) }
}

/** Teinte 0-360, saturation et valeur 0-1. */
export function hsv(h, s, v) {
  const c = v * s
  const hp = (((h % 360) + 360) % 360) / 60
  const x = c * (1 - Math.abs((hp % 2) - 1))
  let r, g, b
  if (hp < 1) [r, g, b] = [c, x, 0]
  else if (hp < 2) [r, g, b] = [x, c, 0]
  else if (hp < 3) [r, g, b] = [0, c, x]
  else if (hp < 4) [r, g, b] = [0, x, c]
  else if (hp < 5) [r, g, b] = [x, 0, c]
  else [r, g, b] = [c, 0, x]
  const m = v - c
  return rgb((r + m) * 255, (g + m) * 255, (b + m) * 255)
}

export function lerp(a, b, t) {
  return a + (b - a) * t
}

export function mix(a, b, t) {
  return rgb(lerp(a.r, b.r, t), lerp(a.g, b.g, t), lerp(a.b, b.b, t))
}

// ------------------------------------------------------------------ géométrie
//
// Les deux lisent un rectangle **ou lèvent**. Un gabarit sans géométrie relevée
// donnerait `undefined`, donc NaN, donc du noir borné à zéro sans une erreur :
// c'est le silence qu'on refuse ici.

function sansRectangle(key) {
  const quoi = key?.label === undefined ? `la position ${key?.index}` : `« ${key.label} »`
  return (
    `${quoi} n'a pas de rectangle : ce gabarit n'a pas de géométrie relevée, ` +
    "et un effet qui mesure des distances physiques n'a rien à y mesurer."
  )
}

/** Le centre du capuchon — là où est la LED, et non son coin. */
export function center(key) {
  const { x, y, w, h } = key ?? {}
  if (x === undefined || y === undefined || w === undefined || h === undefined) {
    throw new TypeError(sansRectangle(key))
  }
  return { x: x + w / 2, y: y + h / 2 }
}

/** L'encombrement du dessin, en unités de pas. */
export function bounds(layout) {
  const keys = layout?.keys ?? []
  if (keys.length === 0) return { x: 0, y: 0, w: 0, h: 0 }

  let x0 = Infinity
  let y0 = Infinity
  let x1 = -Infinity
  let y1 = -Infinity

  for (const key of keys) {
    const { x, y, w, h } = key ?? {}
    if (x === undefined || y === undefined || w === undefined || h === undefined) {
      throw new TypeError(sansRectangle(key))
    }
    if (x < x0) x0 = x
    if (y < y0) y0 = y
    if (x + w > x1) x1 = x + w
    if (y + h > y1) y1 = y + h
  }

  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
}
