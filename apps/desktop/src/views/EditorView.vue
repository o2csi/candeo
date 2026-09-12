<script setup lang="ts">
/**
 * Éditeur d'effets — mode plein cadre.
 *
 * `meta.full` fait disparaître la barre de navigation : l'éditeur est un
 * **mode**, pas un onglet. L'éditeur Monaco reste un emplacement réservé
 * (issue #10) ; le simulateur, lui, est en place et permanent — itérer sur un
 * effet ne doit exiger ni de regarder le vrai clavier, ni d'en posséder un
 * (`docs/design/studio.md` §2).
 */
import { computed } from 'vue'
import { useRouter } from 'vue-router'

import KeyboardSimulator from '../components/KeyboardSimulator.vue'
import { useDevice } from '../composables/useDevice'
import { useDemoFrames } from '../keyboard/demoFrames'
import { DEFAULT_LAYOUT, type LayoutView } from '../keyboard/layout'

const router = useRouter()
const { layout } = useDevice()

/**
 * Sans périphérique ouvert, le gabarit de repli. `get_layout()` refuse hors
 * connexion, et c'est précisément le cas qu'il faut servir : on écrit un effet
 * avant de brancher quoi que ce soit, ou sans posséder le clavier.
 */
const board = computed<LayoutView>(() => layout.value ?? DEFAULT_LAYOUT)

/**
 * Source d'images **provisoire**. Le moteur d'effets n'existe pas encore
 * (issues #5 à #7) : aucune image réelle ne circule. Quand le canal
 * `subscribe_frames` arrivera, seule cette ligne changera — le simulateur ne
 * calcule rien, il ne sait pas d'où viennent les couleurs.
 */
const { frame } = useDemoFrames(() => board.value)
</script>

<template>
  <section class="page">
    <header class="head">
      <button class="ghost" @click="router.push('/')">Retour</button>
      <h1>Éditeur</h1>
    </header>

    <div class="split">
      <div class="pane">Éditeur Monaco — issue #10</div>

      <div class="pane sim">
        <div class="sim-head">
          <h2>Simulateur</h2>
          <p class="sim-note">
            Images de démonstration : le moteur d'effets reste à écrire. Le simulateur n'en calcule
            aucune — il affiche celles qu'on lui donne, et ce seront les mêmes que celles envoyées
            au clavier.
            <span v-if="!layout">
              Aucun clavier connecté : dessin d'après le gabarit par défaut.
            </span>
          </p>
        </div>

        <KeyboardSimulator class="sim-board" :layout="board" :frame="frame" />
      </div>
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

/* Le panneau du simulateur n'est pas centré comme un emplacement réservé : il
   empile son en-tête et laisse tout le reste au dessin. `min-width` et
   `min-height` à zéro, sans quoi une cellule de grille refuse de descendre
   sous la taille intrinsèque de son contenu et déborderait la fenêtre. */
.sim {
  display: flex;
  flex-direction: column;
  gap: var(--gap-3);
  padding: var(--gap-4);
  min-width: 0;
  min-height: 0;
}

.sim-head {
  display: flex;
  flex-direction: column;
  gap: var(--gap-1);
}

.sim-note {
  max-width: 68ch;
  color: var(--text-faint);
  font-size: 12px;
}

.sim-board {
  flex: 1;
  min-height: 0;
}
</style>
