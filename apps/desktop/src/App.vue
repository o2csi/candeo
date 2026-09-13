<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { useRoute } from 'vue-router'

import * as api from './api/candeo'
import { alerte, message } from './api/journal'
import DevicePill from './components/DevicePill.vue'
import { useDevice } from './composables/useDevice'
import { useEffectParams } from './composables/useEffectParams'

const route = useRoute()
const { error, restore } = useDevice()
const { reload } = useEffectParams()

/** L'éditeur occupe toute la fenêtre : c'est un mode, pas un onglet. */
const full = computed(() => route.meta.full === true)

/**
 * Ce que la croix fait désormais, dit en toutes lettres.
 *
 * Fermer ne quitte plus : c'est ce qui permet à un effet de continuer, et c'est
 * aussi ce qui rend la sortie non évidente. Le taire laisserait croire à une
 * application qui refuse de se fermer — la lecture la plus naturelle, et la
 * pire.
 */
const REPLI = 'Fermer replie dans la zone de notification'
const REPLI_DETAIL =
  "Fermer la fenêtre n'arrête pas candeo : l'effet continue de tourner sur le clavier, " +
  "et l'icône de la zone de notification garde la main dessus. Pour quitter vraiment, " +
  'clic droit sur cette icône, puis « Quitter candeo ».'

onMounted(() => {
  void restore()

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

      <RouterLink to="/" class="tab">Effets</RouterLink>
      <RouterLink to="/devices" class="tab">Périphériques</RouterLink>

      <span class="spacer" />

      <!--
        Permanent, et non un message qu'on ferme : le geste dont il parle — la
        croix — reste disponible à tout instant, et personne ne lit deux fois un
        avertissement qu'il a déjà écarté. Il s'efface avant les onglets quand la
        fenêtre rétrécit ; le détail reste alors dans l'infobulle, et la section
        « Fenêtre et sortie » des périphériques le porte en entier.
      -->
      <span class="repli" :title="REPLI_DETAIL">{{ REPLI }}</span>

      <DevicePill />
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
