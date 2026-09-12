/**
 * Effet de référence — onde circulaire partant du centre du clavier.
 *
 * Sert de modèle : un effet tient en quelques lignes et se lit comme ce qu'il
 * fait. C'est aussi la démonstration du contrat attendu par le moteur —
 * **un export par défaut**, et rien d'autre.
 */

import { hsv, type EffectModule } from './index'

export default {
  name: 'Onde',
  description: 'Une onde de teinte se propage depuis le centre',
  params: {
    speed: { kind: 'number', label: 'Vitesse', min: 0, max: 400, default: 120 },
    scale: { kind: 'number', label: 'Échelle', min: 1, max: 60, default: 18 },
  },
  render({ layout, time, frame, params }) {
    const cx = (layout.cols - 1) / 2
    const cy = (layout.rows - 1) / 2
    const speed = Number(params.speed ?? 120)
    const scale = Number(params.scale ?? 18)

    // On itère `layout.keys`, donc uniquement les positions portant une LED.
    // Les trous de la matrice restent noirs, ce qui est le bon défaut : une
    // image couvre 132 positions, le clavier n'en éclaire que 106.
    for (const key of layout.keys) {
      const d = Math.hypot(key.col - cx, key.row - cy)
      frame.set(key, hsv(time * speed + d * scale, 1, 1))
    }
  },
} satisfies EffectModule
