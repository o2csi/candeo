<script setup lang="ts">
/**
 * A region's pictogram (`docs/design/regions.md` §2): the device's silhouette,
 * the region's lights filled, the others faint. Interface, not light: it is
 * drawn in the text colour and follows the theme, where the simulator paints
 * what the device emits.
 *
 * Decorative: the region's name is always written next to it.
 */
import { computed } from 'vue'

import type { KeyInfo, OutlinePart } from '../api/types'
import { pictogram } from '../keyboard/pictogram'

const props = defineProps<{
  /** The device's layout; `null` until it is read, and a blank of the same size is drawn. */
  layout: { keys: readonly KeyInfo[]; outline?: readonly OutlinePart[] } | null
  positions: readonly number[]
}>()

const drawn = computed(() =>
  props.layout ? pictogram(props.layout.keys, props.layout.outline ?? [], props.positions) : null,
)
</script>

<template>
  <svg
    class="rglyph"
    :viewBox="drawn?.viewBox ?? '0 0 16 18'"
    width="16"
    height="18"
    preserveAspectRatio="xMidYMid meet"
    aria-hidden="true"
  >
    <template v-if="drawn">
      <rect
        v-for="(part, i) in drawn.outline"
        :key="`part-${i}`"
        class="part"
        :x="part.x"
        :y="part.y"
        :width="part.w"
        :height="part.h"
        :rx="part.r"
        vector-effect="non-scaling-stroke"
      />
      <template v-for="light in drawn.lights" :key="light.index">
        <path
          v-if="light.path"
          :class="{ lit: light.lit }"
          class="arch"
          :d="light.path"
          :stroke-width="light.stroke"
        />
        <ellipse
          v-else-if="light.disc"
          :class="{ lit: light.lit }"
          class="light"
          :cx="light.x + light.w / 2"
          :cy="light.y + light.h / 2"
          :rx="light.w / 2"
          :ry="light.h / 2"
        />
        <rect
          v-else
          :class="{ lit: light.lit }"
          class="light"
          :x="light.x"
          :y="light.y"
          :width="light.w"
          :height="light.h"
        />
      </template>
    </template>
  </svg>
</template>

<style scoped>
.rglyph {
  flex: none;
  color: var(--text-muted);
}

.part {
  fill: none;
  stroke: currentColor;
  stroke-opacity: 0.6;
  stroke-width: 0.8;
}

/* Faint, not gone: the other regions place this one on the device. */
.light {
  fill: currentColor;
  fill-opacity: 0.2;
}

.arch {
  fill: none;
  stroke: currentColor;
  stroke-opacity: 0.2;
}

.light.lit {
  fill-opacity: 1;
}

.arch.lit {
  stroke-opacity: 1;
}
</style>
