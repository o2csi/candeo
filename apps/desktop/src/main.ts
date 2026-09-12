import { createApp } from 'vue'

// Polices empaquetées, pas chargées depuis le réseau : une application de
// bureau doit s'ouvrir correctement hors ligne.
//
// Tous les sous-ensembles sont inclus (~200 ko). Les découper serait un
// réflexe web sans objet ici : rien n'est téléchargé à l'usage.
import '@fontsource-variable/ibm-plex-sans'
import '@fontsource-variable/jetbrains-mono'

import './styles/tokens.css'
import './styles/base.css'

import App from './App.vue'
import { router } from './router'

createApp(App).use(router).mount('#app')
