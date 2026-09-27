import { BLACK, defineEffect, mix } from '@candeo/effects-api'

// What Counter-Strike 2 says of your game, once connected in the Games tab
// (`docs/design/game-state-integration.md`): your health on the number row, the
// clip of the weapon in hand on the function row, a flashbang turning the
// keyboard white, fire turning it orange, and the whole keyboard pulsing while
// the bomb is planted. Tried on 2026-09-26 against the game, through the same
// signals. Before the game says anything, both bars glow faintly: waiting, not
// broken.

const HEALTH_FULL = { r: 32, g: 200, b: 64 }
const HEALTH_LOW = { r: 235, g: 24, b: 24 }
const AMMO = { r: 242, g: 163, b: 60 }
const BOMB = { r: 235, g: 24, b: 24 }
const FIRE = { r: 255, g: 80, b: 0 }
const WHITE = { r: 255, g: 255, b: 255 }

export default defineEffect({
  description: {
    en: 'Your health, ammo, flashes and the bomb in Counter-Strike 2, once connected in Games',
    fr: 'Votre vie, vos munitions, les flashs et la bombe dans Counter-Strike 2, une fois connecté dans Jeux',
  },
  inputs: ['signals'],
  game: 'cs2',
  params: {
    healthFull: { kind: 'color', label: { en: 'Full health', fr: 'Vie pleine' }, default: HEALTH_FULL },
    healthLow: { kind: 'color', label: { en: 'Low health', fr: 'Vie basse' }, default: HEALTH_LOW },
    ammo: { kind: 'color', label: { en: 'Ammo', fr: 'Munitions' }, default: AMMO },
    bomb: { kind: 'color', label: { en: 'Bomb planted', fr: 'Bombe posée' }, default: BOMB },
    fire: { kind: 'color', label: { en: 'On fire', fr: 'En feu' }, default: FIRE },
    flash: {
      kind: 'number',
      label: { en: 'Flash strength (%)', fr: 'Force du flash (%)' },
      min: 0,
      max: 100,
      default: 100,
    },
  },
  render({ layout, signals, time, frame, params }) {
    if (layout.keys.length === 0) return
    const level = (name = '') => {
      const value = signals[name]
      return typeof value === 'number' ? Math.min(100, Math.max(0, value)) / 100 : null
    }
    const health = level('cs2.health')
    const ammo = level('cs2.ammo')
    const flashed = (level('cs2.flashed') ?? 0) * ((params.flash ?? 100) / 100)
    const burning = level('cs2.burning') ?? 0
    const planted = signals['cs2.bomb'] === 'planted'

    const full = params.healthFull ?? HEALTH_FULL
    const low = params.healthLow ?? HEALTH_LOW

    // The two top rows carry the bars: the function row, then the numbers.
    const rows = [...new Set(layout.keys.map((key) => key.row))].sort((a, b) => a - b)
    const bars = new Map()
    bars.set(rows[0], { level: ammo, colour: params.ammo ?? AMMO })
    if (rows.length > 1) {
      bars.set(rows[1], { level: health, colour: health === null ? full : mix(low, full, health) })
    }
    const inRow = new Map()
    for (const key of [...layout.keys].sort((a, b) => a.col - b.col)) {
      inRow.set(key.row, [...(inRow.get(key.row) ?? []), key.index])
    }

    const pulse = 0.35 + 0.35 * Math.sin(time * Math.PI * 2)
    const ground = planted ? mix(BLACK, params.bomb ?? BOMB, pulse) : BLACK
    for (const key of layout.keys) {
      let colour = ground
      const bar = bars.get(key.row)
      if (bar) {
        const row = inRow.get(key.row) ?? []
        const at = (row.indexOf(key.index) + 1) / row.length
        const lit = bar.level !== null && at <= bar.level
        colour = lit ? bar.colour : mix(BLACK, bar.colour, 0.08)
      }
      colour = mix(colour, params.fire ?? FIRE, burning * 0.6)
      colour = mix(colour, WHITE, flashed)
      frame.set(key, colour)
    }
  },
})
