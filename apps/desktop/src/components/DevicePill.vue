<script setup lang="ts">
/**
 * La pastille d'état des appareils.
 *
 * Elle dit l'**état**, pas l'identité. Le nom de l'appareil est dans la première
 * colonne de la galerie ; le répéter ici serait un doublon, et il deviendrait
 * faux dès le second appareil piloté.
 *
 * Elle reste pourtant nécessaire, et c'est pour cela qu'elle est un composant et
 * non un morceau de la barre de navigation : l'éditeur est un **mode**, il n'a
 * ni barre ni colonne des appareils, et c'est alors le seul endroit qui signale
 * une perte. Un seul composant, posé à deux endroits — pas deux libellés à tenir
 * d'accord.
 *
 * « Piloté » est l'état décidé, celui de l'écran Périphériques : adopté. Un
 * appareil adopté mais débranché est donc compté, et dit injoignable — fondre
 * les deux rendrait « piloté mais absent » indicible, ce qui est exactement la
 * perte qu'on veut voir.
 */
import { computed } from 'vue'

import { useDevice } from '../composables/useDevice'

const { devices } = useDevice()

const piloted = computed(() => devices.value.filter((d) => d.state === 'adopted'))
/** Réellement ouverts : c'est ce qui fait la différence entre décidé et effectif. */
const reached = computed(() => piloted.value.filter((d) => d.open).length)

/**
 * Le texte porte tout : la couleur de la pastille ne fait que doubler. Une
 * information qui n'existe qu'en couleur n'existe pas pour tout le monde.
 */
const label = computed(() => {
  const total = piloted.value.length
  if (total === 0) return 'aucun appareil piloté'

  const perdus = total - reached.value
  const base = total === 1 ? '1 appareil piloté' : `${total} appareils pilotés`
  return perdus === 0 ? base : `${base} · ${perdus} injoignable${perdus > 1 ? 's' : ''}`
})
</script>

<template>
  <RouterLink to="/devices" class="pill" :class="{ on: reached > 0 }">
    <span class="dot" aria-hidden="true" />
    {{ label }}
  </RouterLink>
</template>

<style scoped>
.pill {
  display: flex;
  flex: none;
  gap: var(--gap-2);
  align-items: center;
  color: var(--text-faint);
  font-size: 12px;
  text-decoration: none;
  white-space: nowrap;
}

.pill.on {
  color: var(--text-muted);
}

.dot {
  width: 7px;
  height: 7px;
  border-radius: 99px;
  background: var(--line-strong);
}

.pill.on .dot {
  background: var(--ok);
}
</style>
