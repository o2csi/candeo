import { BLACK, defineEffect, mix } from '@candeo/effects-api'

// What Dota 2 says of your hero, once connected in the Games tab
// (`docs/design/game-state-integration.md`): health on the number row, mana on
// the function row, the day or the night under them, a stun flickering yellow,
// smoke dimming everything, and the keyboard breathing red while you wait to
// respawn. Before the game says anything, both bars glow faintly: waiting, not
// broken.

const HEALTH_FULL = { r: 32, g: 200, b: 64 }
const HEALTH_LOW = { r: 235, g: 24, b: 24 }
const MANA = { r: 40, g: 110, b: 255 }
const DAY = { r: 60, g: 36, b: 8 }
const NIGHT = { r: 10, g: 14, b: 48 }
const STUN = { r: 255, g: 210, b: 40 }
const DEAD = { r: 180, g: 0, b: 0 }
const SMOKE = { r: 70, g: 40, b: 110 }

export default defineEffect({
  description: {
    en: 'Your hero in Dota 2 — health, mana, day and night, stuns — once connected in Games',
    fr: 'Votre héros dans Dota 2 — vie, mana, jour et nuit, étourdissements — une fois connecté dans Jeux',
  },
  inputs: ['signals'],
  game: 'dota2',
  params: {
    healthFull: { kind: 'color', label: { en: 'Full health', fr: 'Vie pleine' }, default: HEALTH_FULL },
    healthLow: { kind: 'color', label: { en: 'Low health', fr: 'Vie basse' }, default: HEALTH_LOW },
    mana: { kind: 'color', label: { en: 'Mana', fr: 'Mana' }, default: MANA },
    day: { kind: 'color', label: { en: 'Day', fr: 'Jour' }, default: DAY },
    night: { kind: 'color', label: { en: 'Night', fr: 'Nuit' }, default: NIGHT },
    stun: { kind: 'color', label: { en: 'Stunned', fr: 'Étourdi' }, default: STUN },
    dead: { kind: 'color', label: { en: 'Waiting to respawn', fr: 'En attente de réapparition' }, default: DEAD },
  },
  render({ layout, signals, time, frame, params }) {
    if (layout.keys.length === 0) return
    const level = (name = '') => {
      const value = signals[name]
      return typeof value === 'number' ? Math.min(100, Math.max(0, value)) / 100 : null
    }
    const health = level('dota2.health')
    const mana = level('dota2.mana')
    const dead = signals['dota2.respawn'] !== undefined && signals['dota2.respawn'] !== ''
    const stunned = signals['dota2.stunned'] === 'yes'
    const smoked = signals['dota2.smoked'] === 'yes'
    const daytime = signals['dota2.daytime']

    const full = params.healthFull ?? HEALTH_FULL
    const low = params.healthLow ?? HEALTH_LOW
    const ground = daytime === 'day' ? (params.day ?? DAY) : daytime === 'night' ? (params.night ?? NIGHT) : BLACK

    // The two top rows carry the bars: the function row, then the numbers.
    const rows = [...new Set(layout.keys.map((key) => key.row))].sort((a, b) => a - b)
    const bars = new Map()
    bars.set(rows[0], { level: mana, colour: params.mana ?? MANA })
    if (rows.length > 1) {
      bars.set(rows[1], { level: health, colour: health === null ? full : mix(low, full, health) })
    }
    const inRow = new Map()
    for (const key of [...layout.keys].sort((a, b) => a.col - b.col)) {
      inRow.set(key.row, [...(inRow.get(key.row) ?? []), key.index])
    }

    const breath = 0.2 + 0.5 * (0.5 + 0.5 * Math.sin(time * Math.PI))
    const flicker = 0.5 + 0.5 * Math.sin(time * Math.PI * 12)
    for (const key of layout.keys) {
      let colour = ground
      if (dead) {
        colour = mix(BLACK, params.dead ?? DEAD, breath)
      } else {
        const bar = bars.get(key.row)
        if (bar) {
          const row = inRow.get(key.row) ?? []
          const at = (row.indexOf(key.index) + 1) / row.length
          const lit = bar.level !== null && at <= bar.level
          colour = lit ? bar.colour : mix(ground, bar.colour, 0.08)
        }
        if (stunned) colour = mix(colour, params.stun ?? STUN, 0.3 + 0.4 * flicker)
      }
      if (smoked) colour = mix(colour, SMOKE, 0.5)
      frame.set(key, colour)
    }
  },
})
