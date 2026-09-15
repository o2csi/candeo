/**
 * An effect's key: `<source>:<name>`, such as `shipped:Breathing`, `user:Rain`
 * or `hardware:wave`. See `docs/design/effects-sources.md`.
 *
 * Names exclude `:`, so the source is everything before the first one. What the
 * interface shows is the name: the source is said by the section the effect is
 * listed in.
 */

/** The key of one of the user's effects. */
export function userKey(name: string): string {
  return `user:${name}`
}

/** The name a key holds, shown to people; a string that is not a key is shown as it is. */
export function effectName(key: string): string {
  const colon = key.indexOf(':')
  return colon < 0 ? key : key.slice(colon + 1)
}

/** True for the key of a shipped effect. */
export function isShippedKey(key: string): boolean {
  return key.startsWith('shipped:')
}
