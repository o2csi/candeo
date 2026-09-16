/**
 * Whether a newer version is published (#139).
 *
 * The state is shared by the whole window: the check runs **once**, when the
 * application starts and if the setting is on, and Settings shows what it found
 * rather than asking again. Turned off, nothing is ever requested.
 */

import { ref } from 'vue'

import { getUpdateCheck, setCheckForUpdates, type UpdateCheck } from '../api/candeo'
import { isNewer, latestRelease, type Release } from '../api/version'

/** What the check found, as Settings reads it. */
export type Found =
  | { state: 'upToDate' }
  | { state: 'newer'; release: Release }
  | { state: 'failed' }
  /** Versions this build cannot compare — a pre-release, a name. */
  | { state: 'unreadable'; release: Release }

const status = ref<UpdateCheck | null>(null)
const found = ref<Found | null>(null)
const asking = ref(false)

/** One check per window, whatever mounts this. */
let asked = false

async function ask(): Promise<void> {
  if (asking.value) {
    return
  }
  asking.value = true
  found.value = null
  try {
    const release = await latestRelease()
    const newer = isNewer(status.value?.version ?? '', release.version)
    found.value =
      newer === null
        ? { state: 'unreadable', release }
        : newer
          ? { state: 'newer', release }
          : { state: 'upToDate' }
  } catch {
    // What failed — no network, GitHub answering something else — belongs to
    // the moment, not to a bug report: the window says it could not check.
    found.value = { state: 'failed' }
  } finally {
    asking.value = false
  }
}

export function useUpdateCheck() {
  /**
   * Reads the setting, and asks once if it is on.
   *
   * Called when the application starts, not when Settings opens: what this is
   * for is telling someone who would not have gone looking.
   */
  async function start(): Promise<void> {
    status.value = await getUpdateCheck()
    if (!status.value.available || !status.value.enabled || asked) {
      return
    }
    asked = true
    await ask()
  }

  /** Asks now, whatever the setting says: that is what the button is for. */
  async function checkNow(): Promise<void> {
    asked = true
    await ask()
  }

  /** Turns the check on or off, and asks straight away when turning it on. */
  async function choose(on: boolean): Promise<void> {
    await setCheckForUpdates(on)
    status.value = status.value ? { ...status.value, enabled: on } : await getUpdateCheck()
    if (on) {
      await checkNow()
    } else {
      found.value = null
    }
  }

  return { status, found, asking, start, checkNow, choose }
}

/** Test seam: the shared state outlives a component, so tests reset it. */
export function forgetUpdateCheck(): void {
  status.value = null
  found.value = null
  asking.value = false
  asked = false
}
