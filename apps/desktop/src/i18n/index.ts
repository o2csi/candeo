/**
 * The interface's translations (`AGENTS.md`, internationalisation).
 *
 * `en.json` is the reference and gives the type: a key missing from `fr.json`,
 * or a key used that neither declares, fails `vue-tsc`. Missing keys fall back to
 * English at run time.
 *
 * The language comes from Rust (`get_language`), which resolves "system" the
 * same way for the window and the tray.
 */

import { createI18n } from 'vue-i18n'

import type { Language } from '../api/candeo'
import en from '../locales/en.json'
import fr from '../locales/fr.json'

export type MessageSchema = typeof en

/** Every key of the English catalog, as a path: `devices.title`, `settings.log.levels.info`… */
type Leaves<T, Prefix extends string = ''> = {
  [K in keyof T & string]: T[K] extends string ? `${Prefix}${K}` : Leaves<T[K], `${Prefix}${K}.`>
}[keyof T & string]

export type MessageKey = Leaves<MessageSchema>

/**
 * `t`, with keys checked against the catalog. vue-i18n's own accepts any string,
 * so a mistyped key would only show as the key itself on screen.
 */
export interface Translate {
  (key: MessageKey): string
  (key: MessageKey, plural: number): string
  (key: MessageKey, named: Record<string, unknown>, plural?: number): string
}

export const i18n = createI18n<[MessageSchema], Language, false>({
  legacy: false,
  locale: 'en',
  fallbackLocale: 'en',
  messages: { en, fr },
})

/**
 * Translates in the current language. Read in a template or a computed, it
 * follows a language change.
 */
export const t = i18n.global.t as Translate

/** The language the interface shows now. */
export function currentLanguage(): Language {
  return i18n.global.locale.value
}

/** Shows the interface in this language. */
export function showIn(language: Language): void {
  i18n.global.locale.value = language
}
