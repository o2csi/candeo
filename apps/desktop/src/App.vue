<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window'
import { computed, onMounted, watch } from 'vue'
import { useRoute } from 'vue-router'

import * as api from './api/candeo'
import { alerte, message } from './api/journal'
import { controlledSummary } from './composables/deviceStatus'
import { useDevice } from './composables/useDevice'
import { useSettings } from './composables/useSettings'
import { useTheme } from './composables/useTheme'
import type { ThemeSetting } from './api/candeo'
import { refreshLibrary } from './editor/library'
import { t } from './i18n'

const route = useRoute()
const { devices, error, restore } = useDevice()
const { reload } = useSettings()
const { theme, choose: chooseTheme } = useTheme()

/** In the order a switch reads: follow the system, or force one side. */
const THEMES: readonly ThemeSetting[] = ['system', 'light', 'dark']

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
      .catch((e: unknown) => alerte('App', `window title not updated: ${message(e, 'en')}`, e))
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
    alerte('App', `library not compiled: ${message(e, 'en')}`, e),
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
      alerte('App', `no resynchronisation after changes made outside the window: ${message(e, 'en')}`, e)
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
        The theme, reachable from every screen. What the close button does is
        said by a system notification on the first close instead (#110).
      -->
      <div class="theme" role="group" :aria-label="t('app.theme.label')">
        <button
          v-for="option in THEMES"
          :key="option"
          type="button"
          class="theme-option"
          :aria-pressed="theme === option"
          :aria-label="t(`app.theme.${option}`)"
          :title="t(`app.theme.${option}`)"
          @click="chooseTheme(option)"
        >
          <svg
            viewBox="0 0 16 16"
            width="14"
            height="14"
            aria-hidden="true"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <template v-if="option === 'system'">
              <rect x="1.75" y="2.5" width="12.5" height="8.5" rx="1.25" />
              <path d="M5.5 13.75h5M8 11v2.75" />
            </template>
            <template v-else-if="option === 'light'">
              <circle cx="8" cy="8" r="2.75" />
              <path
                d="M8 1.5v1.5M8 13v1.5M1.5 8H3M13 8h1.5M3.4 3.4l1.06 1.06M11.54 11.54l1.06 1.06M3.4 12.6l1.06-1.06M11.54 4.46l1.06-1.06"
              />
            </template>
            <path v-else d="M13.25 9.75A5.5 5.5 0 0 1 6.25 2.75a5.5 5.5 0 1 0 7 7Z" />
          </svg>
        </button>
      </div>
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

.theme {
  display: flex;
  flex: none;
  gap: 2px;
  padding: 2px;
  border: 1px solid var(--line);
  border-radius: var(--r-md);
}

.theme-option {
  display: grid;
  place-items: center;
  width: 26px;
  height: 24px;
  border-radius: calc(var(--r-md) - 2px);
  color: var(--text-muted);
}

.theme-option:hover {
  color: var(--text);
  background: var(--raised-2);
}

.theme-option[aria-pressed='true'] {
  color: var(--accent);
  background: var(--accent-soft);
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
