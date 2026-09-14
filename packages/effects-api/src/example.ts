/**
 * Effet de référence — onde partant du centre de la matrice.
 *
 * Sert de modèle : un effet tient en quelques lignes et se lit comme ce qu'il
 * fait. C'est aussi la démonstration du contrat attendu par le moteur —
 * **un export par défaut**, et rien d'autre.
 *
 * Il mesure en **coordonnées de matrice** : une case par touche, quelle que soit
 * sa taille. C'est le calcul qui fonctionne partout, y compris sur un gabarit
 * dont personne n'a dessiné la disposition. Pour une onde ronde sur le bureau
 * plutôt que dans la matrice, l'effet livré « Onde radiale » lit le rectangle
 * des touches — voir `Key.x`, qui dit ce que ça engage.
 */

import { defineEffect, hsv } from './index'

export default defineEffect({
  description: 'Une onde de teinte se propage depuis le centre de la matrice',
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
})
