<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window'
import { computed, onMounted, watch } from 'vue'
import { useRoute } from 'vue-router'

import * as api from './api/candeo'
import { alerte, message } from './api/journal'
import { controlledSummary } from './composables/deviceStatus'
import { useDevice } from './composables/useDevice'
import { useSettings } from './composables/useSettings'
import { refreshLibrary } from './editor/library'
import { t } from './i18n'

const route = useRoute()
const { devices, error, restore } = useDevice()
const { reload } = useSettings()

/**
 * The controlled-device count lives in the window title, not in the window:
 * it is read at a glance in the title bar and the taskbar, and each device card
 * already says its own state. Here rather than in a view, since this component
 * lives as long as the webview.
 */
watch(
  // Follows the language too: the summary is translated as it is computed.
  () => controlledSummary(devices.value),
  (summary) => {
    getCurrentWindow()
      .setTitle(t('app.title', { summary }))
      .catch((e: unknown) => alerte('App', `window title not updated: ${message(e)}`, e))
  },
  { immediate: true },
)

/** L'éditeur occupe toute la fenêtre : c'est un mode, pas un onglet. */
const full = computed(() => route.meta.full === true)


onMounted(() => {
  void restore()
  // Effects dropped in the folder are compiled now, not when the gallery opens:
  // the tray only offers what is compiled, and it may be all someone uses.
  refreshLibrary().catch((e: unknown) =>
    alerte('App', `library not compiled: ${message(e)}`, e),
  )

  // L'état peut changer **sans la fenêtre** : l'icône de zone de notification
  // lance, arrête et éteint sans elle. Et la fenêtre lui survit maintenant
  // repliée — son instantané peut donc vieillir des jours avant de revenir à
  // l'écran. Ce que le moteur dit est déjà réinterrogé chaque seconde ; ce qui
  // ne l'est pas, c'est ce qu'on ne lit qu'au montage.
  //
  // Aucun désabonnement : ce composant vit aussi longtemps que la vue web, et
  // une vue web détruite emporte ses écouteurs. En poser un ici, c'est en poser
  // un par page chargée, donc un.
  void api
    .onStateChanged(() => {
      void restore()
      void reload()
    })
    .catch((e: unknown) => {
      // Dégradé mais fonctionnel : la fenêtre affichera l'état de son montage
      // jusqu'à ce qu'on la rouvre. Rien à montrer à l'écran — l'utilisateur ne
      // peut rien en faire —, mais un journal qui l'explique évite de chercher
      // une panne d'écriture là où il n'y a qu'un écouteur manquant.
      alerte('App', `resynchronisation hors fenêtre inactive : ${message(e)}`, e)
    })
})
</script>

<template>
  <div class="app" :class="{ full }">
    <nav v-if="!full" class="rail">
      <span class="brand" aria-hidden="true">◈</span>

      <RouterLink to="/" class="tab">{{ t('app.tabs.effects') }}</RouterLink>
      <RouterLink to="/devices" class="tab">{{ t('app.tabs.devices') }}</RouterLink>
      <RouterLink to="/settings" class="tab">{{ t('app.tabs.settings') }}</RouterLink>

      <span class="spacer" />

      <!--
        What the close button does, said at all times: closing no longer quits,
        which lets an effect run on, and makes quitting less obvious. Permanent,
        not a message to dismiss, since the gesture is always available. It gives
        way before the tabs when the window narrows; the tooltip keeps the detail,
        and the README explains it in full.
      -->
      <span class="repli" :title="t('app.closeHidesDetail')">{{ t('app.closeHides') }}</span>
    </nav>

    <main class="body">
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <RouterView />
    </main>
  </div>
</template>

<style scoped>
.app {
  display: grid;
  grid-template-rows: auto 1fr;
  height: 100%;
}

.app.full {
  grid-template-rows: 1fr;
}

.rail {
  display: flex;
  gap: var(--gap-2);
  align-items: center;
  padding: 0 var(--gap-4);
  background: var(--raised);
  border-bottom: 1px solid var(--line);
  height: 48px;
}

.brand {
  color: var(--accent);
  font-size: 15px;
  margin-right: var(--gap-2);
}

.tab {
  padding: 5px var(--gap-3);
  border-radius: var(--r-md);
  color: var(--text-muted);
  text-decoration: none;
}

.tab:hover {
  color: var(--text);
  background: var(--raised-2);
}

.tab.router-link-active {
  color: var(--accent);
  background: var(--accent-soft);
}

.spacer {
  flex: 1;
}

.repli {
  /* Il cède la place avant tout le reste : les onglets sont la navigation, ceci
     est un rappel. `min-width: 0` est ce qui autorise l'ellipse dans un flex. */
  flex: 0 1 auto;
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  margin-right: var(--gap-3);
  color: var(--text-muted);
  font-size: 12px;
  cursor: default;
}

.body {
  overflow: auto;
  min-height: 0;
}

.error {
  margin: var(--gap-3) var(--gap-4) 0;
  padding: var(--gap-3);
  border: 1px solid var(--bad);
  border-radius: var(--r-md);
  background: color-mix(in srgb, var(--bad) 10%, transparent);
  color: var(--text);
  font-size: 13px;
}
</style>
