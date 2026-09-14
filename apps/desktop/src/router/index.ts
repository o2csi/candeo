/**
 * Routage.
 *
 * Historique en mémoire et non dans l'URL : c'est une fenêtre d'application,
 * pas une page. Il n'y a ni barre d'adresse, ni lien à partager, ni bouton
 * « précédent » du navigateur à respecter.
 *
 * **La galerie est la racine**, pas l'éditeur : la plupart des lancements
 * servent à choisir un effet, pas à en écrire un.
 */

import { createMemoryHistory, createRouter } from 'vue-router'

export const router = createRouter({
  history: createMemoryHistory(),
  routes: [
    {
      path: '/',
      name: 'effects',
      component: () => import('../views/EffectsView.vue'),
    },
    {
      path: '/devices',
      name: 'devices',
      component: () => import('../views/DevicesView.vue'),
    },
    {
      path: '/settings',
      name: 'settings',
      component: () => import('../views/SettingsView.vue'),
    },
    {
      // L'éditeur est un **mode** : il remplace le contenu de la fenêtre.
      // `:id` absent = nouvel effet.
      path: '/editor/:id?',
      name: 'editor',
      component: () => import('../views/EditorView.vue'),
      meta: { full: true },
    },
  ],
})
