/**
 * The starting point of a new effect.
 *
 * It is not a template written for the occasion: it is **the reference effect**
 * of `@candeo/effects-api`, read as it is. A template copied here would have
 * started to diverge the day the API changed, and nobody would have seen it —
 * an example has no test to contradict it.
 *
 * Only one touch-up, and it is mechanical: the example lives **inside** the
 * package, so it imports `./index`. A user's effect, on the other hand, is
 * resolved by rquickjs's module loader, which only knows the package name.
 */

import example from '@candeo/effects-api/src/example.ts?raw'

import { error } from '../api/journal'

const LOCAL = "'./index'"
const PUBLIC = "'@candeo/effects-api'"

export const NEW_EFFECT = example.replace(LOCAL, PUBLIC)

// A substitution that finds nothing would go unnoticed: the template would
// import `./index`, which the engine cannot resolve, and the error would only
// come when the effect starts. `import.meta.env.DEV` is replaced at build
// time — none of this remains in the shipped application.
if (import.meta.env.DEV && !example.includes(LOCAL)) {
  error(
    'template',
    `${LOCAL} not found in example.ts: the template imports a module the engine cannot resolve`,
  )
}
