// Balayage — une rangée éclairée descend le clavier, en laissant une traînée.
//
// Le seul des effets livrés où la majeure partie du clavier est éteinte à tout
// instant : c'est ce qui le rend reconnaissable au premier coup d'œil.
//
// Il compte en **rangées de matrice**, et ce n'est pas le défaut qu'était celui
// d'« Onde radiale » : c'est son sujet. Ses deux réglages se lisent « rangées
// par seconde » et « traînée en rangées », sa tête avance d'une rangée à la
// fois, et une rangée est une notion de matrice. Le porter en unités physiques
// en ferait un autre effet — qui s'attarderait sur les 1,5 u qui séparent la
// rangée de fonctions du reste — sans que celui-ci ait rien à se reprocher, et
// lui coûterait de tourner sur les gabarits qui ne sont pas dessinés.

import { defineEffect, rgb } from '@candeo/effects-api'

const DEFAUT = { r: 0, g: 180, b: 255 }

export default defineEffect({
  uid: 'f2c7bd61-c7fe-4731-964f-7c526b7052ad',
  name: 'Balayage',
  description: 'Une rangée éclairée descend le clavier en laissant une traînée',
  params: {
    color: { kind: 'color', label: 'Couleur', default: DEFAUT },
    speed: { kind: 'number', label: 'Rangées par seconde', min: 0.5, max: 12, step: 0.5, default: 3 },
    trail: { kind: 'number', label: 'Traînée (rangées)', min: 0.5, max: 6, step: 0.5, default: 2 },
    bounce: { kind: 'boolean', label: 'Rebond', default: false },
  },
  render({ layout, time, frame, params }) {
    const color = params.color ?? DEFAUT
    const speed = Number(params.speed ?? 3)
    const trail = Number(params.trail ?? 2)
    const last = layout.rows - 1

    // Position de la tête. En rebond le cycle couvre l'aller *et* le retour,
    // d'où sa longueur doublée et le repli de la seconde moitié.
    const cycle = params.bounce ? 2 * last : layout.rows
    const p = (time * speed) % cycle
    const head = params.bounce && p > last ? cycle - p : p

    // L'intensité décroît avec la distance à la tête, et s'annule au-delà de la
    // traînée : une touche lointaine doit être éteinte, pas faiblement allumée.
    for (const key of layout.keys) {
      const k = Math.max(0, 1 - Math.abs(key.row - head) / trail)
      frame.set(key, rgb(color.r * k, color.g * k, color.b * k))
    }
  },
})
