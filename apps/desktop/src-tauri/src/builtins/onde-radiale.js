// Onde radiale — une onde de teinte se propage depuis le centre du clavier.
//
// L'effet emblématique, et le premier qu'on ouvre : il tient en six lignes de
// rendu et se lit comme sa description.

import { hsv } from '@candeo/effects-api'

export default {
  name: 'Onde radiale',
  description: 'Une onde de teinte se propage depuis le centre du clavier',
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
}
