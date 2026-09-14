<script setup lang="ts">
/**
 * Le repère de couleurs d'un effet, dans la liste.
 *
 * Quelques bandes côte à côte, prises telles quelles dans `EffectEntry.swatch` :
 * le front ne calcule rien. Ces couleurs sont **échantillonnées en exécutant
 * l'effet**, côté Rust, à l'installation — c'est ce qui les empêche de mentir,
 * et c'est aussi pourquoi elles arrivent avec la liste plutôt que par un appel
 * par vignette.
 *
 * `aria-hidden` : le repère aide à retrouver un effet d'un coup d'œil, il
 * n'apporte rien à qui ne voit pas l'écran — le nom et la description, eux, sont
 * lus. Un lecteur d'écran n'aurait que « quatre couleurs » à annoncer.
 */
import { computed } from 'vue'

import { t } from '../i18n'

const props = defineProps<{
  /** Couleurs `#rrggbb`. Peut être vide : voir `EffectEntry.swatch`. */
  colors: string[]
}>()

/**
 * Un repère absent n'est pas une erreur : un effet qui lève pendant
 * l'échantillonnage s'installe quand même. On réserve alors la même place, en
 * neutre, plutôt que de décaler les lignes voisines.
 */
const known = computed(() => props.colors.length > 0)
</script>

<template>
  <span
    class="swatch"
    :class="{ unknown: !known }"
    aria-hidden="true"
    :title="known ? t('effects.swatchKnown') : t('effects.swatchUnknown')"
  >
    <span v-for="(c, i) in colors" :key="i" :style="{ background: c }" />
  </span>
</template>

<style scoped>
/*
 * Ces couleurs ne sont pas de l'interface : ce sont des octets sortis du moteur,
 * ceux-là mêmes qui partiraient vers les LED. Elles ne suivent donc aucun thème,
 * et elles arrivent en style calculé — il n'y a rien à en écrire ici.
 */
.swatch {
  display: flex;

  /* `flex: none` : la ligne peut être comprimée, le repère ne se réduit pas à
     un trait — c'est ce qui sert à reconnaître l'effet. */
  flex: none;
  align-self: center;
  width: 34px;
  height: 18px;
  border: 1px solid var(--line);
  border-radius: var(--r-sm);
  overflow: hidden;
}

.swatch > span {
  flex: 1;
}

/* Sans repère : une pastille sourde, pas un trou. Elle dit « rien à montrer »
   sans prétendre montrer une couleur que l'effet n'a jamais produite. */
.swatch.unknown {
  background: repeating-linear-gradient(45deg, var(--raised-2) 0 4px, transparent 4px 8px);
}
</style>
