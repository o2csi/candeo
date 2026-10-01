<script setup lang="ts">
/**
 * *Support Candeo*, in the top bar: the ways to support it, each naming who
 * takes the payment, as the Microsoft Store asks of a donation made outside it
 * (policy 10.8.2). The payment happens there; Candeo sees none of it.
 */
import { onBeforeUnmount, onMounted, ref, useId } from 'vue'

import { openSponsors } from '../api/candeo'
import { message, warn } from '../api/journal'
import { t } from '../i18n'

const uid = useId()
const open = ref(false)
const root = ref<HTMLElement | null>(null)

function toggle(): void {
  open.value = !open.value
}

async function sponsors(): Promise<void> {
  open.value = false
  await openSponsors().catch((e: unknown) => warn('support', `sponsors page: ${message(e, 'en')}`, e))
}

/** Closed by a click elsewhere or Escape, as a menu is. */
function outside(e: MouseEvent): void {
  if (open.value && root.value && !root.value.contains(e.target as Node)) open.value = false
}

function escape(e: KeyboardEvent): void {
  if (e.key === 'Escape') open.value = false
}

onMounted(() => {
  document.addEventListener('mousedown', outside)
  document.addEventListener('keydown', escape)
})
onBeforeUnmount(() => {
  document.removeEventListener('mousedown', outside)
  document.removeEventListener('keydown', escape)
})
</script>

<template>
  <div ref="root" class="support">
    <button
      type="button"
      class="cta"
      :aria-expanded="open"
      :aria-controls="`${uid}-ways`"
      @click="toggle"
    >
      <svg
        viewBox="0 0 16 16"
        width="14"
        height="14"
        aria-hidden="true"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        stroke-linejoin="round"
      >
        <path d="M8 13.5S2 10 2 5.75A2.75 2.75 0 0 1 8 4.5a2.75 2.75 0 0 1 6 1.25C14 10 8 13.5 8 13.5Z" />
      </svg>
      {{ t('app.support.button') }}
    </button>
    <div v-if="open" :id="`${uid}-ways`" class="ways" role="dialog" :aria-label="t('app.support.button')">
      <p class="lead">{{ t('app.support.lead') }}</p>
      <button type="button" class="way" @click="sponsors">
        <span class="name">{{ t('app.support.github') }}</span>
        <span class="detail">{{ t('app.support.githubDetail') }}</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.support {
  position: relative;
  flex: none;
}

.cta {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px var(--gap-3);
  border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
  border-radius: var(--r-md);
  color: var(--accent);
  font-size: 13px;
}

.cta:hover,
.cta[aria-expanded='true'] {
  background: var(--accent-soft);
}

.ways {
  position: absolute;
  top: calc(100% + 8px);
  left: 50%;
  z-index: 20;
  width: 300px;
  padding: var(--gap-3);
  transform: translateX(-50%);
  background: var(--raised);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-lg);
  box-shadow: 0 8px 24px rgb(0 0 0 / 0.25);
}

.lead {
  margin: 0 0 var(--gap-2);
  color: var(--text-muted);
  font-size: 12px;
}

.way {
  display: flex;
  flex-direction: column;
  gap: 2px;
  width: 100%;
  padding: var(--gap-2) var(--gap-3);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  text-align: left;
}

.way:hover {
  background: var(--raised-2);
}

.name {
  color: var(--text);
  font-size: 13px;
  font-weight: 500;
}

.detail {
  color: var(--text-faint);
  font-size: 12px;
}
</style>
