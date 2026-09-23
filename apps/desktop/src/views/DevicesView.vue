<script setup lang="ts">
/**
 * Devices: what is plugged in, what was decided for each, and its technical
 * details. What concerns the whole application lives in Settings.
 */

import { onMounted, reactive } from 'vue'

import { message } from '../api/journal'
import type { DeviceInfo } from '../api/types'
import FailureNote from '../components/FailureNote.vue'
import { useDevice } from '../composables/useDevice'
import { t } from '../i18n'

/**
 * What has already been read, per device: closing fixes nothing, the device
 * keeps failing and would say so at every reading of the list.
 *
 * Hidden **while the message does not change**: a device that starts failing
 * differently has something new to say.
 */
const hushed = reactive<Record<string, string>>({})

const deviceKey = (d: DeviceInfo) => `${d.vid}:${d.pid}`

function trouble(d: DeviceInfo): string | null {
  const said = d.error ? message(d.error) : null
  return said === hushed[deviceKey(d)] ? null : said
}

function hush(d: DeviceInfo): void {
  const said = trouble(d)
  if (said) hushed[deviceKey(d)] = said
}

const { devices, layout, busy, refresh, adopt, ignore } = useDevice()

onMounted(refresh)
</script>

<template>
  <section class="page">
    <header class="head">
      <h1>{{ t('devices.title') }}</h1>
      <button class="ghost" :disabled="busy" @click="refresh">{{ t('devices.search') }}</button>
    </header>

    <!--
      A known layout that is unplugged stays shown, marked absent. Hiding it
      would give an empty list, which looks like a failure of the application
      when plugging the keyboard in is all it takes.
    -->
    <ul class="list">
      <li
        v-for="d in devices"
        :key="`${d.vid}:${d.pid}`"
        class="row"
        :class="{ off: !d.present, ignored: d.state === 'ignored', open: d.open }"
      >
        <div class="main">
          <div class="id">
            <h2>{{ d.name }}</h2>
            <span class="mono ids">{{
              `${d.vid.toString(16).padStart(4, '0')}:${d.pid.toString(16).padStart(4, '0')}`
            }}</span>
            <!--
              The version read, next to the one of the survey: it is the first
              question in front of a keyboard that does not obey. Closed, it
              says the version was not read rather than leave a blank that
              would read as "none".
            -->
            <span class="mono ids detail">
              {{
                t('devices.firmware', {
                  version:
                    d.firmware ??
                    (d.open ? t('devices.firmwareNotRead') : t('devices.firmwareNotReadClosed')),
                  surveyed: d.surveyedFirmware,
                })
              }}
            </span>
            <!--
              The layout is read from an open device, and a layout is a model's:
              its name is the device's. 132 and 106 are named apart, since
              confusing them is this hardware's trap: a frame covers all 132
              cells, not the 106 lit keys.
            -->
            <span v-if="d.open && layout?.name === d.name" class="mono ids detail">
              {{
                t('devices.matrix', {
                  rows: layout.rows,
                  cols: layout.cols,
                  frameLen: layout.frameLen,
                  keys: layout.keys.length,
                })
              }}
            </span>
          </div>

          <!--
            Two tags, and they do not say the same thing: the first what the
            system sees, the second what was decided. Merging them into one
            would make "controlled but unplugged" impossible to say.
          -->
          <span class="tag" :class="d.present ? 'ok' : 'absent'">
            {{ d.present ? t('devices.plugged') : t('devices.unplugged') }}
          </span>
          <span class="tag" :class="d.state">{{ t(`devices.state.${d.state}`) }}</span>

          <!--
            A device never seen before is listed, not controlled: it is a button
            to click once, not a box to tick again at every launch.

            It stays offered on a controlled device that is plugged in but
            **closed**: the decision may target a unit of the same model other
            than the one plugged in (its serial says so on open), and without
            this button the plugged-in keyboard could only be adopted again by
            going through "Ignore".
          -->
          <button
            v-if="d.state !== 'adopted' || (d.present && !d.open)"
            class="solid"
            :disabled="busy"
            @click="adopt(d)"
          >
            {{ t('devices.control') }}
          </button>
          <button v-if="d.state !== 'ignored'" class="ghost" :disabled="busy" @click="ignore(d)">
            {{ t('devices.ignore') }}
          </button>
        </div>

        <!--
          The error belongs to the device that produced it: shown on its row, it
          does not suggest the others are affected.
        -->
        <FailureNote v-if="trouble(d)" class="err" @close="hush(d)">{{ trouble(d) }}</FailureNote>
        <!--
          A warning, not an error: nothing is blocked, the device stays open.
          Visible on the row rather than only in the log: a version different
          from the survey's is the first lead in front of a keyboard that does
          not obey, and nobody would go looking for it elsewhere.
        -->
        <p v-for="w in d.warnings" :key="w" class="warn" role="status">{{ w }}</p>
      </li>
    </ul>

    <p v-if="!devices.length" class="empty">{{ t('devices.none') }}</p>
    <p v-else class="note">{{ t('devices.neverSeen') }}</p>
  </section>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  gap: var(--gap-4);
  padding: var(--gap-4);
  max-width: 720px;
}

.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--gap-3);
}

.list {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.row {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  padding: var(--gap-3) var(--gap-4);
  background: var(--raised);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
}

.main {
  display: flex;
  align-items: center;
  gap: var(--gap-3);
}

.row.off {
  background: none;
  border-style: dashed;
}

/* Ignored: present in the list, but visibly set aside. */
.row.ignored {
  opacity: 0.6;
}

/*
 * Open *right now*, not "adopted". It is the sign that was missing: an effect
 * running without a single byte reaching the keyboard showed nowhere.
 */
.row.open {
  border-color: var(--accent);
}

.id {
  flex: 1;
  min-width: 0;
}

.err {
  margin: 0;
  color: var(--bad);
  font-size: 12px;
}

.ids {
  color: var(--text-faint);
  font-size: 12px;
}

/* Technical details, each on its own line under the identifier. */
.detail {
  display: block;
}

.tag {
  padding: 2px var(--gap-2);
  border-radius: 99px;
  font-size: 11px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.tag.ok {
  color: var(--ok);
  background: color-mix(in srgb, var(--ok) 14%, transparent);
}

.tag.absent {
  color: var(--text-faint);
  background: var(--raised-2);
}

.tag.adopted {
  color: var(--accent);
  background: var(--accent-soft);
}

.tag.detected {
  color: var(--text-muted);
  background: var(--raised-2);
}

.tag.ignored {
  color: var(--text-faint);
  border: 1px solid var(--line-strong);
}

.solid,
.ghost {
  padding: 6px var(--gap-3);
  border-radius: var(--r-md);
  font-size: 13px;
}

.solid {
  background: var(--accent);
  color: var(--accent-ink);
  font-weight: 500;
}

.ghost {
  border: 1px solid var(--line-strong);
  color: var(--text-muted);
}

.ghost:hover:not(:disabled) {
  color: var(--text);
  background: var(--raised-2);
}

.solid:disabled,
.ghost:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.empty,
.note {
  margin: 0;
  color: var(--text-faint);
  font-size: 12px;
}

/* A warning, not an error: nothing is broken, but nothing will clear it. */
.warn {
  margin: 0;
  padding: var(--gap-2) var(--gap-3);
  border: 1px solid var(--warn);
  border-radius: var(--r-md);
  background: color-mix(in srgb, var(--warn) 10%, transparent);
  font-size: 13px;
}
</style>
