/**
 * Text an effect declares, in the interface's language.
 *
 * An effect's description and parameter labels are a string or a map of
 * languages (`Text` in `@candeo/effects-api`). They do not go through the
 * interface catalogs: they belong to the effect's author.
 */

import type { Text } from '@candeo/effects-api'

import { currentLanguage } from '.'

/**
 * The text in `language`, then English, then the first entry, else empty.
 *
 * Reading the current language inside a computed makes it follow a language
 * change.
 */
export function localized(text: Text | undefined, language: string = currentLanguage()): string {
  if (text === undefined) return ''
  if (typeof text === 'string') return text
  return text[language] ?? text.en ?? Object.values(text)[0] ?? ''
}
