<script setup lang="ts">
/**
 * Galerie d'effets — écran d'accueil.
 *
 * Emplacement réservé : le contenu est l'objet de l'issue #8. Cette vue existe
 * dès maintenant parce qu'elle est la **racine** du routage, et que la placer
 * après coup aurait obligé à repenser la navigation.
 */
</script>

<template>
  <section class="page">
    <h1>Effets</h1>
    <p class="todo">
      La galerie arrive avec l'issue #8 : effets intégrés, les vôtres, et ceux du
      micrologiciel — ces derniers ne coûtent aucun temps processeur et survivent à la
      fermeture de l'application.
    </p>
  </section>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  gap: var(--gap-3);
  padding: var(--gap-4);
  max-width: 640px;
}

.todo {
  color: var(--text-muted);
}
</style>
