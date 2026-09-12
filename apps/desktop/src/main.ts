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

import { erreur, message } from './api/journal'
import App from './App.vue'
import { router } from './router'

const app = createApp(App)

/**
 * Le filet de Vue, relié au journal.
 *
 * Ce que Vue attrape ici, c'est ce qu'aucun composant n'a rattrapé : une erreur
 * dans un `setup`, un `watch` ou un gestionnaire d'événement. Sans destination,
 * elle allait dans la console — invisible en `release`, où le binaire est compilé
 * sans console — et la fenêtre restait figée sans que rien n'en garde trace.
 *
 * `info` porte le crochet de Vue (« render function », « watcher callback »…) :
 * c'est ce qui distingue une erreur de rendu d'une erreur de gestionnaire, et
 * c'est la première question qu'on se pose en lisant le fichier.
 *
 * Le gestionnaire n'affiche rien : il journalise. Ce que l'utilisateur doit voir
 * est déjà porté par les écrans, qui rattrapent leurs propres erreurs ; ce qui
 * arrive ici est précisément ce que personne n'a su présenter.
 */
app.config.errorHandler = (e, _instance, info) => {
  erreur('vue', `${info} : ${message(e)}`, e)
}

app.use(router).mount('#app')
