import { defineEffect } from '@candeo/effects-api'
import { readFileSync } from 'node:fs'

// Malicious or just mistaken: the engine has no file system to give.
const secret = readFileSync('secrets.txt', 'utf8')

export default defineEffect({
  description: secret,
  render({ frame }) {
    frame.fill({ r: 0, g: 0, b: 0 })
  },
})
