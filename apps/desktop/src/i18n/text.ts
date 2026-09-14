/**
 * Text an effect declares, in the interface's language.
 *
 * An effect's description and parameter labels are a string or a map of
 * languages (`Text` in `@candeo/effects-api`). They do not go through the
 * interface catalogs: they belong to the effect's author.
 */

import type { Text } from '@candeo/effects-api'

/**
 * The interface's language.
 *
 * French, like every string of the interface today. The i18n catalogs (#73)
 * replace this with the language setting.
 */
export function interfaceLanguage(): string {
  return 'fr'
}

/** The text in `language`, then English, then the first entry, else empty. */
export function localized(text: Text | undefined, language = interfaceLanguage()): string {
  if (text === undefined) return ''
  if (typeof text === 'string') return text
  return text[language] ?? text.en ?? Object.values(text)[0] ?? ''
}
