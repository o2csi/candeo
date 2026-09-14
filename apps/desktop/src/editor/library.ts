/**
 * The library as the window sees it, with stale files compiled first.
 *
 * Rust lists the effects folder but cannot strip TypeScript types; the window
 * can. So every read of the library goes through here: whatever Rust reports as
 * `stale` is compiled and recorded before the list is returned
 * (`docs/design/effects-library.md` §3).
 *
 * It runs at startup, from `App.vue`, so that effects dropped in the folder are
 * ready for the tray even if nobody opens the gallery, and on Refresh.
 */

import {
  cacheEffect,
  listEffects,
  readEffectSource,
  type EffectEntry,
} from '../api/candeo'
import { alerte, message } from '../api/journal'
import { transpile } from './effect'

let running: Promise<EffectEntry[]> | null = null

/**
 * The library, once its stale files are compiled.
 *
 * Calls made while a pass runs share it: startup and the gallery opening at
 * the same moment must not compile every file twice.
 */
export function refreshLibrary(): Promise<EffectEntry[]> {
  running ??= compileStale().finally(() => {
    running = null
  })
  return running
}

async function compileStale(): Promise<EffectEntry[]> {
  const entries = await listEffects()
  const compiled = new Map<string, EffectEntry>()

  for (const entry of entries) {
    if (entry.state !== 'stale' || entry.hash === undefined) continue
    try {
      const js = await transpile(await readEffectSource(entry.id))
      compiled.set(entry.id, await cacheEffect(entry.id, entry.hash, js))
    } catch (e) {
      // The file changed or disappeared between the listing and now: it stays
      // stale, and the next Refresh sees its new content. One file must not
      // keep the others from compiling.
      alerte('library', `${entry.id}: not compiled: ${message(e, 'en')}`, e)
    }
  }

  return entries.map((entry) => compiled.get(entry.id) ?? entry)
}
