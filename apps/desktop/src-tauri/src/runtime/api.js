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
