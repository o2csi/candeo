import { ref } from 'vue'

import { getSettings, setTheme, type ThemeSetting } from '../api/candeo'
import { alerte, message } from '../api/journal'

/**
 * The theme shown, at module level: the switch in the top bar and the
 * configuration reset in Settings read the same one.
 */
const theme = ref<ThemeSetting>('system')

/**
 * Sets `data-theme` on the document. `system` removes it, and `tokens.css`
 * follows the system's setting, live.
 */
function show(setting: ThemeSetting): void {
  theme.value = setting
  const root = document.documentElement
  if (setting === 'system') delete root.dataset.theme
  else root.dataset.theme = setting
}

export function useTheme() {
  /** Reads the saved theme. Unreadable settings leave the system's: the window must still open. */
  async function load(): Promise<void> {
    try {
      show((await getSettings()).preferences.theme ?? 'system')
    } catch (e) {
      alerte('theme', `theme not read: ${message(e, 'en')}`, e)
    }
  }

  /**
   * Shows the theme at once, then saves it. A failed save keeps it for this
   * session: nobody can act on the reason, the log keeps it.
   */
  async function choose(setting: ThemeSetting): Promise<void> {
    show(setting)
    try {
      await setTheme(setting)
    } catch (e) {
      alerte('theme', `theme not saved: ${message(e, 'en')}`, e)
    }
  }

  return { theme, load, choose }
}
