/**
 * Drafts — what prevents losing an effect while it is being written.
 *
 * ## The problem
 *
 * As long as an effect is not validated, it exists nowhere: `install_effect` is
 * the only path to disk, and it asks for code that compiles. Yet one leaves the
 * editor well before getting there — a click on "Back", a closed window, a hot
 * reload in development.
 *
 * ## What was chosen
 *
 * Continuous saving in the web view's local storage, under one key per effect,
 * restored on opening and erased only after a successful install.
 *
 * **Why not a "do you want to save?" dialog**: it asks a question that can be
 * answered wrongly, and only once. A restored draft, for its part, loses
 * nothing, asks nothing, and is thrown away with a button when it is no longer
 * wanted.
 *
 * **Why not a file on disk**: it would need a Rust command to write a draft. The
 * web view's storage survives closing the application as well as navigation,
 * which covers exactly the cases targeted; the durable copy, for its part,
 * remains the `source.ts` written at install.
 *
 * Storage can be refused — hardened web view, read-only profile. That is no
 * reason to fail: losing a draft is annoying, preventing an effect from being
 * written would be more so.
 */

const PREFIX = 'candeo:brouillon:'

/** The effect being written has no identifier yet: `null`. */
function key(id: string | null): string {
  return PREFIX + (id ?? '')
}

export function readDraft(id: string | null): string | null {
  try {
    return localStorage.getItem(key(id))
  } catch {
    return null
  }
}

export function writeDraft(id: string | null, source: string): void {
  try {
    localStorage.setItem(key(id), source)
  } catch {
    // Without storage, the editor works — it simply no longer catches
    // mistakes.
  }
}

export function clearDraft(id: string | null): void {
  try {
    localStorage.removeItem(key(id))
  } catch {
    // See `writeDraft`.
  }
}

/**
 * Moves a draft to another effect id, when the effect is renamed.
 *
 * A draft already stored under `to` is newer than the one being moved: it stays,
 * and the moved one is left where it was rather than lost.
 */
export function moveDraft(from: string, to: string): void {
  const draft = readDraft(from)
  if (draft === null || readDraft(to) !== null) return
  writeDraft(to, draft)
  if (readDraft(to) === draft) clearDraft(from)
}

let migration: Promise<void> | null = null

/**
 * Moves drafts saved under what an effect was called before this run's
 * migration — a directory id, a name — to the key it became. Once per window
 * load, and again after a failure.
 */
export function migrateDrafts(renames: () => Promise<Record<string, string>>): Promise<void> {
  migration ??= renames()
    .then((table) => {
      for (const [from, to] of Object.entries(table)) moveDraft(from, to)
    })
    .catch(() => {
      migration = null
    })
  return migration
}
