<script setup lang="ts">
/**
 * Devices: what is plugged in first, then every device Candeo knows, then your
 * definitions (`docs/design/device-sdk.md` §9). What concerns the whole
 * application lives in Settings.
 */

import { computed, onMounted, reactive, ref } from 'vue'
import { useRouter } from 'vue-router'

import {
  chooseDeviceDefinition,
  openDevicesDir,
  reloadDeviceDefinitions,
  type DefinitionProblem,
} from '../api/candeo'
import { message } from '../api/journal'
import type { DeviceInfo } from '../api/types'
import DeviceStatusDot from '../components/DeviceStatusDot.vue'
import FailureNote from '../components/FailureNote.vue'
import {
  definitionChoice,
  ids,
  known,
  pluggedIn,
  type OriginFilter,
  yourFiles,
} from '../composables/devicesPage'
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

function trouble(d: DeviceInfo): string | null {
  const said = d.error ? message(d.error) : null
  return said === hushed[ids(d)] ? null : said
}

function hush(d: DeviceInfo): void {
  const said = trouble(d)
  if (said) hushed[ids(d)] = said
}

const { devices, layout, busy, refresh, adopt, ignore } = useDevice()

const plugged = computed(() => pluggedIn(devices.value))

/** The list of known devices, as filtered: it grows with every device described. */
const text = ref('')
const origin = ref<OriginFilter>('all')
const listed = computed(() => known(devices.value, text.value, origin.value))

/** Which cards show their technical details. */
const opened = reactive<Record<string, boolean>>({})

/** Your files that drive nothing, and why. */
const problems = ref<DefinitionProblem[]>([])
const files = computed(() => yourFiles(devices.value, problems.value))
/** What went wrong with a file or the folder, said once, under the list. */
const fileError = ref<string | null>(null)

/** Reads your definitions again first: a file just saved may define a device to look for. */
async function reread(): Promise<void> {
  problems.value = await reloadDeviceDefinitions().catch(() => problems.value)
  await refresh()
}

async function attempt(action: () => Promise<unknown>): Promise<void> {
  fileError.value = null
  await action().catch((e: unknown) => {
    fileError.value = message(e)
  })
}

const router = useRouter()

/** What went wrong choosing a device's definition, on its card. */
const choiceError = reactive<Record<string, string>>({})

/** The device opens again on the definition chosen: the list is read again after. */
async function choose(d: DeviceInfo, value: string): Promise<void> {
  delete choiceError[ids(d)]
  await chooseDeviceDefinition({ vid: d.vid, pid: d.pid }, value || null).catch((e: unknown) => {
    choiceError[ids(d)] = message(e)
  })
  await refresh()
}

/** The definition in use when the file chosen does not load. */
function unloaded(d: DeviceInfo): string {
  const file = d.unloadedChoice ?? ''
  return d.origin === 'builtIn'
    ? t('devices.unloadedChoice', { file })
    : t('devices.unloadedChoiceOther', { file, used: d.file ?? '' })
}

/** A definition opens in the editor: read only when built in, where *Copy to yours* is. */
function edit(origin: DeviceInfo['origin'], file: string): void {
  void router.push({ name: 'definition', params: { origin, file } })
}

function count(d: DeviceInfo): string {
  return t(`devices.count.${d.lights}`, { n: d.lightCount }, d.lightCount)
}

onMounted(reread)
</script>

<template>
  <section class="page">
    <header class="head">
      <h1>{{ t('devices.title') }}</h1>
      <button class="ghost" :disabled="busy" :title="t('devices.refreshTitle')" @click="reread">
        {{ t('devices.refresh') }}
      </button>
    </header>

    <!-- ------------------------------------------------ plugged in -->
    <h2 class="group">{{ t('devices.pluggedIn', { n: plugged.length }) }}</h2>
    <ul v-if="plugged.length" class="cards">
      <li
        v-for="d in plugged"
        :key="ids(d)"
        class="card"
        :class="{ controlled: d.state === 'adopted', ignored: d.state === 'ignored' }"
      >
        <div class="main">
          <DeviceStatusDot :device="d" />
          <div class="id">
            <span class="name">
              {{ d.name }}
              <span v-if="d.origin === 'yours'" class="tag yours">{{ t('devices.groups.yours') }}</span>
            </span>
            <span class="sub">
              <span class="mono">{{ ids(d) }}</span> · {{ count(d) }}
            </span>
          </div>

          <!--
            Which definition drives it, when there is a choice: the built-in one,
            or a file of yours for the same ids. None takes over by being in
            the folder (`docs/design/device-sdk.md` §9).
          -->
          <label v-if="definitionChoice(d) !== null" class="choice">
            <span class="sr-only">{{ t('devices.definition') }}</span>
            <select
              :value="definitionChoice(d)"
              :disabled="busy"
              :title="t('devices.definition')"
              @change="choose(d, ($event.target as HTMLSelectElement).value)"
            >
              <option
                v-for="o in d.definitions"
                :key="`${o.origin}:${o.file}`"
                :value="o.origin === 'builtIn' ? '' : o.file"
              >
                {{ o.origin === 'builtIn' ? t('devices.groups.builtIn') : o.file }}
              </option>
              <option v-if="d.unloadedChoice" :value="d.unloadedChoice" disabled>
                {{ d.unloadedChoice }}
              </option>
            </select>
          </label>

          <!--
            The decision, not the connection: every card here is plugged in.
            *Control* stays offered on a controlled device that is closed: the
            decision may target another unit of the same model (its serial says
            so on open), and it could otherwise only be adopted again through
            *Ignore*.
          -->
          <span v-if="d.state !== 'detected'" class="tag" :class="d.state">
            {{ t(`devices.state.${d.state}`) }}
          </span>
          <button
            v-if="d.state !== 'adopted' || !d.open"
            class="solid"
            :disabled="busy"
            @click="adopt(d)"
          >
            {{ t('devices.control') }}
          </button>
          <button v-if="d.state !== 'ignored'" class="ghost" :disabled="busy" @click="ignore(d)">
            {{ t('devices.ignore') }}
          </button>
          <button
            class="ghost more"
            :aria-expanded="Boolean(opened[ids(d)])"
            :title="t('devices.details')"
            :aria-label="t('devices.details')"
            @click="opened[ids(d)] = !opened[ids(d)]"
          >
            ⋯
          </button>
        </div>

        <!--
          The version read, next to the survey's: the first question in front of
          a keyboard that does not obey. 132 and 106 are named apart, since
          confusing them is this hardware's trap.
        -->
        <div v-if="opened[ids(d)]" class="details mono">
          <span>
            {{
              t('devices.firmware', {
                version:
                  d.firmware ??
                  (d.open ? t('devices.firmwareNotRead') : t('devices.firmwareNotReadClosed')),
                surveyed: d.surveyedFirmware,
              })
            }}
          </span>
          <span v-if="d.open && layout?.name === d.name">
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

        <!-- The error belongs to the device that produced it, on its card. -->
        <FailureNote v-if="trouble(d)" class="err" @close="hush(d)">{{ trouble(d) }}</FailureNote>
        <FailureNote v-if="choiceError[ids(d)]" class="err" @close="delete choiceError[ids(d)]">
          {{ choiceError[ids(d)] }}
        </FailureNote>
        <!-- Said where the choice is: the reason is with your definitions. -->
        <p v-if="d.unloadedChoice" class="warn" role="status">{{ unloaded(d) }}</p>
        <!--
          A warning, not an error: nothing is blocked. On the card rather than
          behind *Details*: a version different from the survey's is the first
          lead, and nobody would go looking for it.
        -->
        <p v-for="w in d.warnings" :key="w" class="warn" role="status">{{ w }}</p>
      </li>
    </ul>
    <p v-else class="note">{{ t('devices.nothingPlugged') }}</p>
    <p v-if="plugged.some((d) => d.state === 'detected')" class="note">{{ t('devices.neverSeen') }}</p>

    <!-- ------------------------------------------------ every device known -->
    <div class="bar">
      <h2 class="group">{{ t('devices.known', { n: devices.length }) }}</h2>
      <input
        v-model="text"
        class="filter"
        type="search"
        spellcheck="false"
        :placeholder="t('devices.filter')"
        :aria-label="t('devices.filter')"
      />
      <span class="seg-group" role="group" :aria-label="t('devices.columns.origin')">
        <button
          v-for="o in ['all', 'builtIn', 'yours'] as const"
          :key="o"
          type="button"
          class="seg"
          :aria-pressed="origin === o"
          @click="origin = o"
        >
          {{ t(`devices.groups.${o}`) }}
        </button>
      </span>
    </div>
    <table v-if="listed.length" class="known">
      <thead>
        <tr>
          <th>{{ t('devices.columns.device') }}</th>
          <th>{{ t('devices.columns.kind') }}</th>
          <th>{{ t('devices.columns.origin') }}</th>
          <th>{{ t('devices.columns.here') }}</th>
          <th />
        </tr>
      </thead>
      <tbody>
        <tr v-for="d in listed" :key="ids(d)">
          <td>
            {{ d.name }}
            <span class="mono faint">{{ ids(d) }}</span>
          </td>
          <td class="faint">{{ t(`devices.kinds.${d.lights}`) }}</td>
          <td :class="d.origin === 'yours' ? 'mine' : 'faint'">{{ t(`devices.groups.${d.origin}`) }}</td>
          <td :class="d.present ? 'ok' : 'faint'">
            {{ d.present ? t('devices.plugged') : t('devices.unplugged') }}
          </td>
          <td class="act">
            <button v-if="d.file" class="ghost small" @click="edit(d.origin, d.file)">
              {{ t('devices.open') }}
            </button>
          </td>
        </tr>
      </tbody>
    </table>
    <p v-else class="note">{{ t('devices.noMatch') }}</p>

    <!-- ------------------------------------------------ yours -->
    <!--
      What the folder holds, loaded or not, chosen or not. A file that drives
      nothing is listed with why, like an effect that does not compile, and the
      warning is said once, above what it is about.
    -->
    <div class="bar">
      <h2 class="group">{{ t('devices.yoursTitle') }}</h2>
      <button class="ghost small" @click="attempt(openDevicesDir)">{{ t('devices.openFolder') }}</button>
    </div>
    <p v-if="files.length" class="warn" role="status">{{ t('devices.yoursNote') }}</p>
    <ul v-if="files.length" class="cards">
      <li v-for="f in files" :key="f.file" class="card" :class="{ problem: f.reason }">
        <div class="main">
          <div class="id">
            <span class="mono">{{ f.file }}</span>
            <span v-if="f.device" class="sub">
              {{ f.device.name }} · <span class="mono">{{ ids(f.device) }}</span>
            </span>
            <span v-else class="mono reason">{{ f.reason }}</span>
          </div>
          <span v-if="f.inUse" class="tag adopted">{{ t('devices.inUse') }}</span>
          <button class="ghost small" @click="edit('yours', f.file)">{{ t('devices.open') }}</button>
        </div>
      </li>
    </ul>
    <p v-else class="note">{{ t('devices.yoursNone') }}</p>
    <FailureNote v-if="fileError" class="err" @close="fileError = null">{{ fileError }}</FailureNote>
  </section>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  gap: var(--gap-3);
  padding: var(--gap-4);
  max-width: 820px;
}

.head,
.bar {
  display: flex;
  align-items: center;
  gap: var(--gap-3);
}

.head {
  justify-content: space-between;
}

.bar .group {
  flex: 1;
}

/* The gallery's group labels. */
.group {
  margin: var(--gap-3) 0 0;
  color: var(--text-faint);
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.cards {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.card {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  padding: var(--gap-3) var(--gap-4);
  background: var(--raised);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
}

/* Controlled: what the app drives, set apart from what waits for a decision. */
.card.controlled {
  border-color: var(--accent);
}

.card.ignored {
  opacity: 0.6;
}

.main {
  display: flex;
  align-items: center;
  gap: var(--gap-3);
}

.id {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
}

.name {
  font-weight: 500;
}

.sub,
.details {
  color: var(--text-faint);
  font-size: 12px;
}

.details {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding-left: calc(8px + var(--gap-3));
}

.choice select {
  max-width: 220px;
  padding: 4px var(--gap-2);
  background: var(--raised);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-sm);
  color: var(--text);
  font: inherit;
  font-size: 12px;
}

.more {
  padding: 4px 10px;
  letter-spacing: 0.1em;
}

.tag {
  padding: 2px var(--gap-2);
  border-radius: 99px;
  font-size: 11px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.tag.adopted {
  color: var(--accent);
  background: var(--accent-soft);
}

.tag.ignored {
  color: var(--text-faint);
  border: 1px solid var(--line-strong);
}

/* Yours: nobody reviewed it, and it shows wherever the device does. */
.tag.yours {
  margin-left: var(--gap-2);
  color: var(--warn);
  background: color-mix(in srgb, var(--warn) 14%, transparent);
  vertical-align: 1px;
}

.filter {
  width: min(240px, 40%);
  padding: 5px var(--gap-2);
  background: var(--raised);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-sm);
  color: var(--text);
  font: inherit;
  font-size: 12px;
}

.seg-group {
  display: inline-flex;
}

.seg {
  padding: 4px 10px;
  border: 1px solid var(--line-strong);
  color: var(--text-faint);
  font-size: 12px;
}

.seg + .seg {
  border-left: 0;
}

.seg:first-child {
  border-radius: var(--r-sm) 0 0 var(--r-sm);
}

.seg:last-child {
  border-radius: 0 var(--r-sm) var(--r-sm) 0;
}

.seg[aria-pressed='true'] {
  color: var(--accent);
  background: var(--accent-soft);
}

/* A list of what exists: a row a device, a column a fact. */
.known {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}

.known th {
  padding: 0 var(--gap-3) var(--gap-2) 0;
  border-bottom: 1px solid var(--line);
  color: var(--text-faint);
  font-size: 11px;
  font-weight: 500;
  text-align: left;
}

.known td {
  padding: var(--gap-2) var(--gap-3) var(--gap-2) 0;
  border-bottom: 1px solid var(--line);
}

.known .act {
  padding-right: 0;
  text-align: right;
}

.faint {
  color: var(--text-faint);
}

.known .mono.faint {
  margin-left: var(--gap-2);
  font-size: 11px;
}

.mine {
  color: var(--warn);
}

.ok {
  color: var(--ok);
}

.solid,
.ghost {
  padding: 6px var(--gap-3);
  border-radius: var(--r-md);
  font-size: 13px;
}

.small {
  padding: 3px var(--gap-2);
  font-size: 12px;
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

.note {
  margin: 0;
  color: var(--text-faint);
  font-size: 12px;
}

.err {
  margin: 0;
  color: var(--bad);
  font-size: 12px;
}

/* A file that drives nothing: its name, then why, as its author wrote it. */
.problem {
  gap: 2px;
  border-color: var(--bad);
  font-size: 12px;
}

.problem .reason {
  color: var(--bad);
  overflow-wrap: anywhere;
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
