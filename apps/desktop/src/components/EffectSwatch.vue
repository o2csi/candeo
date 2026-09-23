<script setup lang="ts">
/**
 * An effect's color swatch, in the list.
 *
 * A few bands side by side, taken as is from `EffectEntry.swatch`: the front end
 * computes nothing. These colors are **sampled by running the effect**, on the
 * Rust side, at install time: that is what keeps them from lying, and it is also
 * why they arrive with the list rather than through one call per thumbnail.
 *
 * `aria-hidden`: the swatch helps find an effect at a glance, it brings nothing
 * to someone who cannot see the screen; the name and description are read out.
 * A screen reader would only have "four colors" to announce.
 */
import { computed } from 'vue'

import { t } from '../i18n'

const props = defineProps<{
  /** `#rrggbb` colors. Can be empty: see `EffectEntry.swatch`. */
  colors: string[]
}>()

/**
 * A missing swatch is not an error: an effect that throws during sampling is
 * still installed. The same room is then kept, in a neutral tone, rather than
 * shifting the neighboring rows.
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
 * These colors are not interface: they are bytes out of the engine, the very
 * ones that would go to the LEDs. So they follow no theme, and they arrive as a
 * computed style: there is nothing to write about them here.
 */
.swatch {
  display: flex;

  /* `flex: none`: the row can be squeezed, the swatch does not shrink to a line;
     it is what serves to recognize the effect. */
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

/* Without a swatch: a muted patch, not a hole. It says "nothing to show"
   without claiming to show a color the effect never produced. */
.swatch.unknown {
  background: repeating-linear-gradient(45deg, var(--raised-2) 0 4px, transparent 4px 8px);
}
</style>
