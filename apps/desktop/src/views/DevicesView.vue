<script setup lang="ts">
import { onMounted } from 'vue'

import type { DeviceInfo } from '../api/types'
import { useDevice } from '../composables/useDevice'

const { devices, layout, busy, refresh, connect, disconnect } = useDevice()

function isOpen(d: DeviceInfo) {
  return layout.value?.name === d.name
}

onMounted(refresh)
</script>

<template>
  <section class="page">
    <header class="head">
      <h1>Périphériques</h1>
      <button class="ghost" :disabled="busy" @click="refresh">Rechercher</button>
    </header>

    <!--
      Un gabarit connu mais débranché reste affiché, marqué absent. Le masquer
      donnerait une liste vide, qui ressemble à une panne de l'application
      alors qu'il suffit de brancher le clavier.
    -->
    <ul class="list">
      <li v-for="d in devices" :key="`${d.vid}:${d.pid}`" class="row" :class="{ off: !d.present }">
        <div class="id">
          <h2>{{ d.name }}</h2>
          <span class="mono ids">{{
            `${d.vid.toString(16).padStart(4, '0')}:${d.pid.toString(16).padStart(4, '0')}`
          }}</span>
        </div>

        <span class="tag" :class="d.present ? 'ok' : 'absent'">
          {{ d.present ? 'Branché' : 'Débranché' }}
        </span>

        <button v-if="isOpen(d)" class="ghost" :disabled="busy" @click="disconnect">
          Déconnecter
        </button>
        <button v-else class="solid" :disabled="busy || !d.present" @click="connect(d)">
          Connecter
        </button>
      </li>
    </ul>

    <p v-if="!devices.length" class="empty">Aucun gabarit connu.</p>

    <!--
      132 et 106 sont affichés séparément, et nommés. Les confondre est le
      piège de ce matériel : une image doit couvrir les 132 cases, pas les 106
      touches, sinon les dernières rangées restent figées.
    -->
    <dl v-if="layout" class="facts">
      <div><dt>Matrice</dt><dd class="num">{{ layout.rows }} × {{ layout.cols }}</dd></div>
      <div><dt>Taille d'une image</dt><dd class="num">{{ layout.frameLen }}</dd></div>
      <div><dt>Touches éclairées</dt><dd class="num">{{ layout.keys.length }}</dd></div>
    </dl>
  </section>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  gap: var(--gap-4);
  padding: var(--gap-4);
  max-width: 720px;
}

.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--gap-3);
}

.list {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.row {
  display: flex;
  align-items: center;
  gap: var(--gap-3);
  padding: var(--gap-3) var(--gap-4);
  background: var(--raised);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
}

.row.off {
  background: none;
  border-style: dashed;
}

.id {
  flex: 1;
  min-width: 0;
}

.ids {
  color: var(--text-faint);
  font-size: 12px;
}

.tag {
  padding: 2px var(--gap-2);
  border-radius: 99px;
  font-size: 11px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.tag.ok {
  color: var(--ok);
  background: color-mix(in srgb, var(--ok) 14%, transparent);
}

.tag.absent {
  color: var(--text-faint);
  background: var(--raised-2);
}

.solid,
.ghost {
  padding: 6px var(--gap-3);
  border-radius: var(--r-md);
  font-size: 13px;
}

.solid {
  background: var(--accent);
  color: var(--accent-ink);
  font-weight: 500;
}

.ghost {
  border: 1px solid var(--line-strong);
  color: var(--text-muted);
}

.ghost:hover:not(:disabled) {
  color: var(--text);
  background: var(--raised-2);
}

.solid:disabled,
.ghost:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.empty {
  color: var(--text-faint);
}

.facts {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-5);
  margin: 0;
  padding-top: var(--gap-4);
  border-top: 1px solid var(--line);
}

dt {
  color: var(--text-faint);
  font-size: 11px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

dd {
  margin: 2px 0 0;
  font-size: 18px;
}
</style>
