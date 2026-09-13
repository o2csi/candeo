// Onde matricielle — une onde de teinte se propage de proche en proche dans la
// matrice.
//
// C'est l'onde d'avant la géométrie, et elle reste un effet à part entière : sa
// distance se compte en **touches**, pas en millimètres. Deux voisines sont à un
// pas l'une de l'autre quelle que soit leur taille, et les trous de la matrice
// comptent comme de la distance. L'image en est plus serrée là où le clavier est
// large — la rangée du bas, le pavé numérique — et c'est exactement ce qui la
// distingue d'« Onde radiale ».
//
// Elle ne lit ni `x` ni `y` : c'est la seule des deux qui tourne sur un gabarit
// dont personne n'a dessiné la disposition.

import { defineEffect, hsv } from '@candeo/effects-api'

export default defineEffect({
  name: 'Onde matricielle',
  description: 'Une onde de teinte se propage de proche en proche dans la matrice',
  params: {
    speed: { kind: 'number', label: 'Vitesse', min: 0, max: 400, default: 120 },
    scale: { kind: 'number', label: 'Échelle', min: 1, max: 60, default: 18 },
  },
  render({ layout, time, frame, params }) {
    const speed = Number(params.speed ?? 120)
    const scale = Number(params.scale ?? 18)
    const cx = (layout.cols - 1) / 2
    const cy = (layout.rows - 1) / 2

    // `layout.keys` ne contient que les positions portant une LED : les trous
    // de la matrice restent noirs, ce qui est le bon défaut.
    for (const key of layout.keys) {
      const d = Math.hypot(key.col - cx, key.row - cy)
      frame.set(key, hsv(time * speed + d * scale, 1, 1))
    }
  },
})
