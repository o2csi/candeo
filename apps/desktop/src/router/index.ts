/**
 * Routing.
 *
 * History in memory and not in the URL: it is an application window, not a
 * page. There is no address bar, no link to share, no browser "back" button to
 * honour.
 *
 * **The gallery is the root**, not the editor: most launches are for choosing
 * an effect, not for writing one.
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
      path: '/automations',
      name: 'automations',
      component: () => import('../views/AutomationsView.vue'),
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
      // The editor is a **mode**: it replaces the window's content.
      // `:id` absent = new effect.
      path: '/editor/:id?',
      name: 'editor',
      component: () => import('../views/EditorView.vue'),
      meta: { full: true },
    },
  ],
})
