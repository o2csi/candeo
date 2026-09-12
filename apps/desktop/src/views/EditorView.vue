<script setup lang="ts">
/**
 * Éditeur d'effets — mode plein cadre.
 *
 * Emplacement réservé : l'éditeur Monaco est l'objet de l'issue #10, le
 * simulateur celui de l'issue #3. La route existe dès maintenant pour fixer
 * le contrat : `meta.full` fait disparaître la barre de navigation, parce que
 * l'éditeur est un **mode** et non un onglet.
 */
import { useRouter } from 'vue-router'

const router = useRouter()
</script>

<template>
  <section class="page">
    <header class="head">
      <button class="ghost" @click="router.push('/')">Retour</button>
      <h1>Éditeur</h1>
    </header>

    <div class="split">
      <div class="pane">Éditeur Monaco — issue #10</div>
      <div class="pane">Simulateur — issue #3</div>
    </div>
  </section>
</template>

<style scoped>
.page {
  display: grid;
  grid-template-rows: auto 1fr;
  height: 100%;
}

.head {
  display: flex;
  align-items: center;
  gap: var(--gap-3);
  padding: var(--gap-3) var(--gap-4);
  border-bottom: 1px solid var(--line);
}

.ghost {
  padding: 4px var(--gap-3);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  color: var(--text-muted);
  font-size: 13px;
}

.split {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 1px;
  background: var(--line);
  min-height: 0;
}

@media (width <= 900px) {
  .split {
    grid-template-columns: 1fr;
    grid-template-rows: 1fr 1fr;
  }
}

.pane {
  display: grid;
  place-items: center;
  background: var(--ground);
  color: var(--text-faint);
  font-size: 13px;
}
</style>
