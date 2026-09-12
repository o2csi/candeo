<script setup lang="ts">
/**
 * Une vignette de la galerie.
 *
 * Le composant ne connaît ni la couche IPC ni le routeur : il affiche et émet
 * `select`. Décider ce qu'un clic déclenche appartient à la vue, qui seule sait
 * si un périphérique est branché.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'

import type { Preview } from '../composables/useEffects'

const props = defineProps<{
  name: string
  summary: string
  preview: Preview
  /** Nature de l'effet, en toutes lettres : la couleur seule ne porte rien. */
  nature: string
  /** Coût processeur, dit en clair. */
  cost: string
  /** Ce qu'il advient de l'effet quand l'application se ferme. */
  persistence: string
  applied?: boolean
  busy?: boolean
  disabled?: boolean
}>()

defineEmits<{ select: [] }>()

/**
 * Ligne d'état réservée en permanence, même vide : la remplir au clic
 * changerait la hauteur de la carte, donc celle de toute la rangée de la
 * grille, et la galerie sauterait sous le curseur au moment précis où l'on
 * vise une carte.
 */
const state = computed(() => (props.busy ? 'Application…' : props.applied ? 'Appliqué' : ''))

const root = ref<HTMLElement | null>(null)
const onScreen = ref(false)

/**
 * L'aperçu ne s'anime que visible. Laisser tourner des animations hors champ
 * serait la pire réclame possible pour une galerie dont l'argument est qu'un
 * effet matériel ne coûte rien.
 *
 * `prefers-reduced-motion` est déjà traité globalement dans `base.css`.
 */
let watcher: IntersectionObserver | null = null

onMounted(() => {
  watcher = new IntersectionObserver((entries) => {
    onScreen.value = entries.some((e) => e.isIntersecting)
  })
  if (root.value) watcher.observe(root.value)
})

onBeforeUnmount(() => {
  watcher?.disconnect()
  watcher = null
})
</script>

<template>
  <button
    ref="root"
    type="button"
    class="card"
    :class="{ applied }"
    :disabled="disabled"
    :aria-pressed="applied === true"
    @click="$emit('select')"
  >
    <span class="thumb" :class="[`is-${preview}`, { live: onScreen }]" aria-hidden="true" />

    <span class="head">
      <span class="name">{{ name }}</span>
      <span class="tag">{{ nature }}</span>
    </span>

    <span class="summary">{{ summary }}</span>

    <span class="facts">
      <span class="fact">{{ cost }}</span>
      <span class="fact">{{ persistence }}</span>
    </span>

    <span class="state" :class="{ on: applied && !busy }">{{ state }}</span>
  </button>
</template>

<style scoped>
.card {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  width: 100%;
  height: 100%;
  padding: var(--gap-3);
  text-align: left;
  background: var(--raised);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
  transition:
    border-color 120ms ease,
    background-color 120ms ease;
}

/* `:not(.applied)` n'est pas décoratif : sans lui le survol, plus spécifique,
   remplacerait la bordure ambrée par la bordure neutre — la carte appliquée
   perdrait sa marque au moment exact où on la pointe. */
.card:not(.applied):hover:not(:disabled) {
  border-color: var(--line-strong);
}

.card:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

/* Doublé du libellé « Appliqué » plus bas : la bordure ambrée ne porte rien
   seule. */
.card.applied {
  border-color: var(--accent);
  background: var(--accent-soft);
}

/*
 * Aperçu.
 *
 * Les couleurs ci-dessous ne sont pas de l'interface : elles figurent ce que le
 * clavier émet. C'est précisément pourquoi les jetons n'en fournissent pas — le
 * spectre RVB y est réservé à ce que le matériel affiche réellement. Seul
 * endroit du front où une couleur est écrite en clair, et elle ne suit pas le
 * thème : un clavier éteint est sombre, y compris sous un thème clair.
 */
.thumb {
  --led-off: #17161d;
  --led-base: #f0324b;
  --led-spectrum: linear-gradient(
    90deg,
    #f0324b,
    #f2a33c,
    #ecec3c,
    #4ec94e,
    #3cc9f2,
    #4b56f2,
    #b13cf2,
    #f0324b
  );

  position: relative;
  display: block;

  /* `flex: none` sinon la carte, comprimée, écraserait l'aperçu avant de
     tronquer le texte. */
  flex: none;
  height: 56px;
  border-radius: var(--r-md);

  /* Fond par défaut, qui est aussi l'aperçu de `is-off` : un clavier éteint est
     sombre et ne bouge pas, il n'y a rien à animer. */
  background: var(--led-off);
  overflow: hidden;
}

/* Séparations de touches. Tracées en noir translucide plutôt qu'à la couleur du
   fond : la vignette doit rester lisible par-dessus n'importe quelle teinte, et
   la carte change de fond quand l'effet est appliqué. */
.thumb::after {
  content: "";
  position: absolute;
  inset: 0;
  background:
    repeating-linear-gradient(90deg, transparent 0 15px, rgb(0 0 0 / 55%) 15px 18px),
    repeating-linear-gradient(180deg, transparent 0 15px, rgb(0 0 0 / 55%) 15px 18px);
}

/* Spectrum Cycle : une seule teinte partout, qui dérive. Le clavier ne montre
   pas de dégradé dans ce mode, il change de couleur d'un bloc. */
.thumb.is-cycle {
  background: var(--led-base);
}

.thumb.is-cycle.live {
  animation: cycle 8s linear infinite;
}

/* Wave : le dégradé est spatial, et c'est lui qui se déplace. La différence
   avec Spectrum Cycle tient entièrement là. */
.thumb.is-wave {
  background-image: var(--led-spectrum);
  background-size: 200% 100%;
}

.thumb.is-wave.live {
  animation: wave 6s linear infinite;
}

/* Les deux images de départ sont écrites explicitement : laisser le navigateur
   interpoler depuis `none` ou depuis une valeur héritée marche, mais rend le
   cycle dépendant d'un implicite qu'on n'a pas choisi. */
@keyframes cycle {
  from {
    filter: hue-rotate(0deg);
  }

  to {
    filter: hue-rotate(360deg);
  }
}

/* Le dégradé fait deux fois la largeur de la vignette et se referme sur sa
   propre teinte de départ : le décaler d'exactement une largeur d'image boucle
   sans raccord visible. */
@keyframes wave {
  from {
    background-position: 0 0;
  }

  to {
    background-position: -200% 0;
  }
}

.head {
  display: flex;
  align-items: baseline;
  gap: var(--gap-2);
}

.name {
  flex: 1;
  min-width: 0;
  font-weight: 500;
  overflow-wrap: anywhere;
}

.tag {
  flex: none;
  padding: 2px var(--gap-2);
  border-radius: 99px;
  background: var(--raised-2);
  color: var(--text-muted);
  font-size: 11px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.summary {
  color: var(--text-muted);
  font-size: 13px;
}

.facts {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-1) var(--gap-2);
  margin-top: auto;
  padding-top: var(--gap-1);
  color: var(--text-faint);
  font-size: 11px;
}

.fact::before {
  content: "· ";
}

.fact:first-child::before {
  content: none;
}

.state {
  /* Exactement la hauteur d'une ligne à cette taille : la ligne occupe la même
     place vide que pleine. */
  min-height: 1.5em;
  color: var(--text-faint);
  font-size: 11px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.state.on {
  color: var(--accent);
}
</style>
