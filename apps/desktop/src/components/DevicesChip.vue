<script setup lang="ts">
/**
 * Which devices something applies to, as a chip that opens a list: "Razer
 * DeathStalker V2 Pro", "3 devices". A row of checkboxes stops being readable
 * past two or three devices; a list stays one line whatever their number.
 *
 * At least one stays chosen: something applying to no device is something
 * switched off, which is said elsewhere.
 */
import { computed } from 'vue'

import type { DeviceInfo, DeviceRef } from '../api/types'
import { ids } from '../composables/devicesPage'
import { t } from '../i18n'

const props = defineProps<{
  /** The devices to choose from. */
  devices: readonly DeviceInfo[]
  /** Those chosen now. */
  chosen: readonly DeviceRef[]
  disabled?: boolean
}>()

const emit = defineEmits<{ change: [devices: DeviceInfo[]] }>()

const picked = computed(() => props.devices.filter((d) => props.chosen.some((c) => ids(c) === ids(d))))

const text = computed(() => {
  if (picked.value.length === 1) return picked.value[0].name
  if (picked.value.length === props.devices.length && props.devices.length > 1) {
    return t('devices.chip.all', { n: props.devices.length })
  }
  return t('devices.chip.some', { n: picked.value.length }, picked.value.length)
})

function toggle(device: DeviceInfo): void {
  const next = picked.value.some((d) => ids(d) === ids(device))
    ? picked.value.filter((d) => ids(d) !== ids(device))
    : [...picked.value, device]
  if (next.length) emit('change', next)
}
</script>

<template>
  <span v-if="disabled" class="chip off" aria-disabled="true">{{ text }}</span>
  <details v-else class="chip">
    <summary :aria-label="`${t('devices.chip.label')}: ${text}`">{{ text }}</summary>
    <ul class="picker">
      <li v-for="d in devices" :key="ids(d)">
        <label>
          <input
            type="checkbox"
            :checked="picked.some((p) => ids(p) === ids(d))"
            :disabled="picked.length === 1 && ids(picked[0]) === ids(d)"
            @change="toggle(d)"
          />
          {{ d.name }}
        </label>
      </li>
    </ul>
  </details>
</template>

<style scoped>
.chip {
  position: relative;
  display: inline-block;
}

summary,
.off {
  display: inline-block;
  list-style: none;
  padding: 3px var(--gap-2);
  border-radius: var(--r-md);
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 500;
  white-space: nowrap;
}

summary {
  cursor: pointer;
}

summary::-webkit-details-marker {
  display: none;
}

summary:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.off {
  background: var(--raised-2);
  color: var(--text-faint);
}

.picker {
  position: absolute;
  z-index: 10;
  top: calc(100% + 4px);
  left: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 220px;
  max-height: 280px;
  overflow-y: auto;
  margin: 0;
  padding: var(--gap-2);
  list-style: none;
  background: var(--raised);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  box-shadow: 0 6px 20px rgb(0 0 0 / 18%);
}

.picker label {
  display: flex;
  align-items: center;
  gap: var(--gap-2);
  padding: 4px var(--gap-2);
  border-radius: var(--r-sm);
  white-space: nowrap;
  cursor: pointer;
}

.picker label:hover {
  background: var(--raised-2);
}
</style>
