/**
 * Is a newer version published? (#139)
 *
 * The releases of this repository are public, and the answer is a comparison
 * made here: nothing is sent but the request itself — no identifier, no version
 * reported, no counting — and nothing is downloaded or installed.
 */

/** A published release, as far as this is concerned. */
export interface Release {
  /** Its version, without the tag's `v`. */
  version: string
  /** Its page, which a link opens in the browser. */
  url: string
}

/**
 * The three numbers of a version, or `null` when it is not one.
 *
 * A tag carries a `v`; anything after the numbers — `-rc.1`, a build — is not
 * compared, and makes the version unreadable rather than equal: saying "up to
 * date" on a version nobody here understands is the one answer that misleads.
 */
export function parseVersion(text: string): [number, number, number] | null {
  const match = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(text.trim())
  if (!match) {
    return null
  }
  return [Number(match[1]), Number(match[2]), Number(match[3])]
}

/**
 * Whether `latest` is newer than `current`; `null` when either is unreadable.
 *
 * Number by number, never as text: `0.10.0` is newer than `0.9.0`.
 */
export function isNewer(current: string, latest: string): boolean | null {
  const [running, published] = [parseVersion(current), parseVersion(latest)]
  if (!running || !published) {
    return null
  }
  for (let i = 0; i < 3; i++) {
    if (published[i] !== running[i]) {
      return published[i] > running[i]
    }
  }
  return false
}

/**
 * Asks GitHub for the latest release. Throws when it cannot be read.
 *
 * The address comes from the Rust side, which builds it from `repository` in
 * `Cargo.toml`: a repository that moves is one line there.
 */
export async function latestRelease(url: string): Promise<Release> {
  const answer = await fetch(url, { headers: { Accept: 'application/vnd.github+json' } })
  if (!answer.ok) {
    throw new Error(`GitHub answered ${answer.status}`)
  }
  const release: unknown = await answer.json()
  return readRelease(release)
}

/** The release in GitHub's answer, or an error naming what is missing. */
export function readRelease(answer: unknown): Release {
  const release = answer as { tag_name?: unknown; html_url?: unknown }
  const { tag_name: tag, html_url: url } = release
  if (typeof tag !== 'string' || typeof url !== 'string') {
    throw new Error('the answer carries no release')
  }
  return { version: tag.replace(/^v/, ''), url }
}
