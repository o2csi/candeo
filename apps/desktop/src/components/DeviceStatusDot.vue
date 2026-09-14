<script setup lang="ts">
/**
 * A device's state as a dot, next to the device it describes.
 *
 * Colour is never the only carrier: the shape differs too (filled when the
 * device is controlled, a ring when it is not open), and the state is in the
 * tooltip and the accessible name.
 */
import { computed } from 'vue'

import type { DeviceInfo } from '../api/types'
import { deviceStatus, statusLabel } from '../composables/deviceStatus'

const props = defineProps<{ device: Pick<DeviceInfo, 'name' | 'open' | 'present'> }>()

const status = computed(() => deviceStatus(props.device))
const label = computed(() => `${props.device.name} · ${statusLabel(status.value)}`)
</script>

<template>
  <span class="dot" :class="status" role="img" :aria-label="label" :title="label" />
</template>

<style scoped>
.dot {
  display: inline-block;
  flex: none;
  box-sizing: border-box;
  width: 8px;
  height: 8px;
  border-radius: 99px;
}

.controlled {
  background: var(--ok);
}

.notOpen {
  border: 1.5px solid var(--text-faint);
}

.unplugged {
  background: var(--warn);
}
</style>
