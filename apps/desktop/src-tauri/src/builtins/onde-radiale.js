// Onde radiale — une onde de teinte se propage en cercles depuis le centre du
// clavier, à la distance **physique** des touches.
//
// « Radiale » est une promesse géométrique, et c'est la géométrie relevée qui la
// tient : la barre d'espace est loin du centre parce qu'elle est large de
// 6,25 u, et non parce qu'elle occuperait plusieurs cases — elle n'en occupe
// qu'une. Les trous de la matrice, eux, ne comptent plus comme de la distance,
// puisqu'ils n'occupent aucun espace.
//
// L'onde qui compte en cases n'a pas disparu pour autant : c'est « Onde
// matricielle », et ce n'est pas le même effet. Les deux se suivent dans la
// galerie parce que la différence ne se voit que côte à côte.

import { bounds, center, defineEffect, hsv } from '@candeo/effects-api'

export default defineEffect({
  name: 'Onde radiale',
  description: 'Une onde de teinte se propage en cercles, à la distance physique des touches',
  params: {
    speed: { kind: 'number', label: 'Vitesse', min: 0, max: 400, default: 120 },
    scale: { kind: 'number', label: 'Échelle', min: 1, max: 60, default: 18 },
  },
  render({ layout, time, frame, params }) {
    const speed = Number(params.speed ?? 120)
    const scale = Number(params.scale ?? 18)

    // Le centre du dessin, et non celui de la matrice : sur un gabarit sans pavé
    // numérique il se déplace avec lui, et l'onde reste centrée sur l'appareil
    // qu'on a. `bounds` lève si le gabarit n'a pas été dessiné, plutôt que de
    // laisser la distance valoir NaN et le clavier rester noir sans un mot.
    const dessin = bounds(layout)
    const cx = dessin.x + dessin.w / 2
    const cy = dessin.y + dessin.h / 2

    // `layout.keys` ne contient que les positions portant une LED : les trous
    // de la matrice restent noirs, ce qui est le bon défaut.
    for (const key of layout.keys) {
      const c = center(key)
      const d = Math.hypot(c.x - cx, c.y - cy)
      frame.set(key, hsv(time * speed + d * scale, 1, 1))
    }
  },
})
