<script setup lang="ts">
/**
 * Settings › System (§2.4): what the computer is doing, each value this
 * computer offers read live, the others said unreadable. The one place that
 * says what a setting or a brightness following the system can have here.
 *
 * The sampler runs while this block is shown and the window visible; nowhere
 * else is it started by Settings.
 *
 * Its root is a `.block` of the Settings page, which gives it its place and its
 * separator; the rest is styled here.
 */
import { computed, onBeforeUnmount, onMounted, ref, useId } from 'vue'

import { systemNow, watchSystem, type SystemNow } from '../api/candeo'
import { message, warn } from '../api/journal'
import { SYSTEM_SOURCES, type SystemSource } from '../composables/bindings'
import { SYSTEM_LABELS } from '../composables/systemSources'
import { t } from '../i18n'

const uid = useId()
const now = ref<SystemNow | null>(null)

/** Twice a sample: each value shows within half a second of being read. */
const READ_MS = 500
let polling: ReturnType<typeof setInterval> | null = null

async function read(): Promise<void> {
  try {
    now.value = await systemNow()
  } catch {
    // A missed reading is the next one's to show.
  }
}

async function watch(on: boolean): Promise<void> {
  if (polling !== null) clearInterval(polling)
  polling = null
  await watchSystem(on).catch((e: unknown) => warn('system', `readout: ${message(e, 'en')}`, e))
  if (on) {
    void read()
    polling = setInterval(() => void read(), READ_MS)
  }
}

function onVisibility(): void {
  void watch(document.visibilityState === 'visible')
}

onMounted(() => {
  document.addEventListener('visibilitychange', onVisibility)
  void watch(document.visibilityState === 'visible')
})

onBeforeUnmount(() => {
  document.removeEventListener('visibilitychange', onVisibility)
  if (polling !== null) clearInterval(polling)
  void watchSystem(false).catch(() => null)
})

/** A source's value in its unit; unreadable, or not read yet. */
function shown(source: SystemSource, n: SystemNow | null): string {
  if (n === null) return t('settings.system.waiting')
  if (!n.offers[source]) return t('settings.system.notReadable')
  const value = {
    cpu: n.cpu,
    memory: n.memory,
    gpu: n.gpu,
    cpuTemp: n.cpuCelsius,
    gpuTemp: n.gpuCelsius,
    cpuPower: n.cpuWatts,
  }[source]
  if (value === null) return t('settings.system.waiting')
  switch (source) {
    case 'cpuTemp':
    case 'gpuTemp':
      return t('settings.system.celsius', { n: Math.round(value) })
    case 'cpuPower':
      return t('settings.system.watts', { n: Math.round(value) })
    default:
      return t('settings.system.percent', { n: Math.round(value * 100) })
  }
}

const rows = computed(() =>
  SYSTEM_SOURCES.map((s) => ({
    source: s,
    label: t(SYSTEM_LABELS[s]),
    value: shown(s, now.value),
    readable: now.value === null || now.value.offers[s],
  })),
)
</script>

<template>
  <section class="block" :aria-labelledby="`${uid}-title`">
    <h2 :id="`${uid}-title`">{{ t('settings.system.title') }}</h2>
    <!-- Not a live region: it changes twice a second. -->
    <dl class="readout">
      <template v-for="r in rows" :key="r.source">
        <dt>{{ r.label }}</dt>
        <dd :class="{ faint: !r.readable }">{{ r.value }}</dd>
      </template>
    </dl>
  </section>
</template>

<style scoped>
.readout {
  display: grid;
  grid-template-columns: max-content max-content;
  gap: 6px var(--gap-4);
  margin: 0;
}

dt {
  color: var(--text-faint);
  font-size: 12px;
}

dd {
  margin: 0;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}

.faint {
  color: var(--text-faint);
}
</style>
