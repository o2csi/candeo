/**
 * The logging facade, on the window's side.
 *
 * Before it, everything the window had to say went to a console nobody opens —
 * and which, in `release`, does not even exist: the binary is built without a
 * console. The records now join the **same file** as Rust's: a failure reads
 * from one end to the other, and the order between what the window saw and
 * what the engine saw is the file's.
 *
 * ## `source` rather than a target
 *
 * Each call names where it comes from — a component, a module. `tracing`
 * requires a target that is constant at compile time, so everything coming from
 * here arrives under a single target, `candeo_webview`, and `source` is a field.
 * It is also what we want: "everything coming from the window" is then filtered
 * with one word.
 *
 * ## Logging cannot fail
 *
 * Nothing is returned, nothing throws. The first user of this module is
 * `app.config.errorHandler`: a facade that rejects would produce there a second
 * failure to handle inside the handler of the first, and the application would
 * go round in circles.
 */

import { i18n, type MessageKey } from '../i18n'
import { logFromWebview, type WebviewLevel } from './candeo'
import type { Failure } from './types'

/**
 * Sends to Rust, and **doubles on the console in development**.
 *
 * It is not a redundancy: a file only receives a string, whereas the console
 * keeps the object — hence the call stack, which is the essential part when the
 * error comes from a component. In `release` only the file remains, which is
 * precisely what the bug report carries.
 */
function record(level: WebviewLevel, source: string, message: string, detail?: unknown): void {
  void logFromWebview(level, source, message).catch(() => {
    // Rust is unreachable: there is nobody further to warn, and insisting would
    // make logging the next failure.
  })
  if (import.meta.env.DEV) {
    const write = level === 'error' ? console.error : level === 'warn' ? console.warn : console.info
    write(`[${source}] ${message}`, ...(detail === undefined ? [] : [detail]))
  }
}

/** The user's lighting is broken. */
export function erreur(source: string, message: string, detail?: unknown): void {
  record('error', source, message, detail)
}

/** Degraded but working. */
export function alerte(source: string, message: string, detail?: unknown): void {
  record('warn', source, message, detail)
}

/** Lifecycle: what started, what stopped. */
export function info(source: string, message: string, detail?: unknown): void {
  record('info', source, message, detail)
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
