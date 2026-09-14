/**
 * La façade de journalisation, côté fenêtre.
 *
 * Avant elle, tout ce que la fenêtre avait à dire partait dans une console que
 * personne n'ouvre — et qui, en `release`, n'existe même pas : le binaire est
 * compilé sans console. Les enregistrements rejoignent désormais le **même
 * fichier** que ceux du Rust : une panne se lit d'un bout à l'autre, et l'ordre
 * entre ce qu'a vu la fenêtre et ce qu'a vu le moteur est celui du fichier.
 *
 * ## `source` plutôt qu'une cible
 *
 * Chaque appel nomme d'où il vient — un composant, un module. `tracing` exige
 * une cible constante à la compilation, donc tout ce qui vient d'ici arrive sous
 * une seule cible, `candeo_webview`, et `source` est un champ. C'est aussi ce
 * qu'on veut : « tout ce qui vient de la fenêtre » se filtre alors d'un mot.
 *
 * ## Journaliser ne peut pas échouer
 *
 * Rien n'est rendu, rien ne lève. La première utilisatrice de ce module est
 * `app.config.errorHandler` : une façade qui rejette y produirait une seconde
 * panne à traiter dans le gestionnaire de la première, et l'application
 * tournerait en rond.
 */

import { i18n, type MessageKey } from '../i18n'
import { logFromWebview, type WebviewLevel } from './candeo'
import type { Failure } from './types'

/**
 * Envoie au Rust, et **double sur la console en développement**.
 *
 * Ce n'est pas une redondance : un fichier ne reçoit qu'une chaîne, alors que la
 * console garde l'objet — donc la pile d'appels, qui est l'essentiel quand
 * l'erreur vient d'un composant. En `release` il ne reste que le fichier, qui est
 * précisément ce que le rapport de bogue transporte.
 */
function consigner(level: WebviewLevel, source: string, message: string, detail?: unknown): void {
  void logFromWebview(level, source, message).catch(() => {
    // Le Rust est injoignable : il n'y a personne de plus à prévenir, et
    // insister ferait de la journalisation la panne suivante.
  })
  if (import.meta.env.DEV) {
    const ecrire = level === 'error' ? console.error : level === 'warn' ? console.warn : console.info
    ecrire(`[${source}] ${message}`, ...(detail === undefined ? [] : [detail]))
  }
}

/** L'éclairage de l'utilisateur est cassé. */
export function erreur(source: string, message: string, detail?: unknown): void {
  consigner('error', source, message, detail)
}

/** Dégradé mais fonctionnel. */
export function alerte(source: string, message: string, detail?: unknown): void {
  consigner('warn', source, message, detail)
}

/** Cycle de vie : ce qui a démarré, ce qui s'est arrêté. */
export function info(source: string, message: string, detail?: unknown): void {
  consigner('info', source, message, detail)
}

function isFailure(e: unknown): e is Failure {
  return typeof e === 'object' && e !== null && 'code' in e && 'params' in e
}

/**
 * An error as shown, in the interface language — or in `locale`: the log is
 * written in English.
 *
 * Rust sends a {@link Failure}, the window throws `Error`s; both read the same.
 */
export function message(e: unknown, locale?: 'en'): string {
  if (isFailure(e)) {
    const key = `errors.${e.code}` as MessageKey
    return locale ? i18n.global.t(key, e.params, { locale }) : i18n.global.t(key, e.params)
  }
  return typeof e === 'string' ? e : e instanceof Error ? e.message : String(e)
}
