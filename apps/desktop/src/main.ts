import { createApp } from 'vue'

// Bundled fonts, not loaded from the network: a desktop application must open
// properly offline.
//
// Every subset is included (~200 kB). Splitting them would be a web reflex
// with no purpose here: nothing is downloaded during use.
import '@fontsource-variable/ibm-plex-sans'
import '@fontsource-variable/jetbrains-mono'

import './styles/tokens.css'
import './styles/base.css'

import { getLanguage } from './api/candeo'
import { error, message } from './api/journal'
import App from './App.vue'
import { useTheme } from './composables/useTheme'
import { i18n, showIn } from './i18n'
import { router } from './router'

const app = createApp(App)

/**
 * Vue's safety net, wired to the log.
 *
 * What Vue catches here is what no component caught: an error in a `setup`, a
 * `watch` or an event handler. With no destination, it went to the console —
 * invisible in `release`, where the binary is built without a console — and the
 * window stayed frozen without anything keeping a trace of it.
 *
 * `info` carries Vue's hook ("render function", "watcher callback"…): it is
 * what tells a render error from a handler error, and it is the first question
 * one asks when reading the file.
 *
 * The handler shows nothing: it logs. What the user must see is already
 * carried by the screens, which catch their own errors; what arrives here is
 * precisely what nobody knew how to present.
 */
app.config.errorHandler = (e, _instance, info) => {
  error('vue', `${info}: ${message(e, 'en')}`, e)
}

// The language and the theme first, so that the window does not show English or
// the other theme for a moment before switching. If Rust cannot answer, English
// and the system's theme: the window must still open.
void Promise.all([
  getLanguage()
    .then((status) => showIn(status.language))
    .catch((e: unknown) => error('i18n', `interface language not read: ${message(e, 'en')}`, e)),
  useTheme().load(),
]).finally(() => app.use(i18n).use(router).mount('#app'))
