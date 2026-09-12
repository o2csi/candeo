<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { useRoute } from 'vue-router'

import { useDevice } from './composables/useDevice'

const route = useRoute()
const { layout, error, restore } = useDevice()

/** L'éditeur occupe toute la fenêtre : c'est un mode, pas un onglet. */
const full = computed(() => route.meta.full === true)

onMounted(() => {
  void restore()
})
</script>

<template>
  <div class="app" :class="{ full }">
    <nav v-if="!full" class="rail">
      <span class="brand" aria-hidden="true">◈</span>

      <RouterLink to="/" class="tab">Effets</RouterLink>
      <RouterLink to="/devices" class="tab">Périphériques</RouterLink>

      <span class="spacer" />

      <RouterLink to="/devices" class="state" :class="{ on: layout }">
        <span class="dot" />
        {{ layout ? layout.name : 'Aucun périphérique' }}
      </RouterLink>
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

/* L'état de connexion est lisible sans couleur seule : la pastille est doublée
   du nom du périphérique, ou de son absence dite en toutes lettres. */
.state {
  display: flex;
  gap: var(--gap-2);
  align-items: center;
  color: var(--text-faint);
  font-size: 12px;
  text-decoration: none;
}

.state.on {
  color: var(--text-muted);
}

.dot {
  width: 7px;
  height: 7px;
  border-radius: 99px;
  background: var(--line-strong);
}

.state.on .dot {
  background: var(--ok);
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
