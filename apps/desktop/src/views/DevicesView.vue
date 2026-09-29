<script setup lang="ts">
/**
 * Devices: one list, what is plugged in first, every device Candeo knows under
 * *All*, then your definitions (`docs/design/device-sdk.md` §9). What concerns
 * the whole application lives in Settings.
 */

import { computed, onMounted, reactive, ref } from 'vue'
import { useRouter } from 'vue-router'

import {
  chooseDeviceDefinition,
  openDevicesDir,
  openSiteDevices,
  reloadDeviceDefinitions,
  reportDeviceDefinition,
  tryDeviceDefinition,
  type DefinitionProblem,
} from '../api/candeo'
import { message } from '../api/journal'
import type { DeviceInfo } from '../api/types'
import DeviceStatusDot from '../components/DeviceStatusDot.vue'
import FailureNote from '../components/FailureNote.vue'
import {
  FILTER_PAST,
  definitionChoice,
  ids,
  listed,
  type Shown,
  yourFiles,
} from '../composables/devicesPage'
import { useDevice } from '../composables/useDevice'
import { refreshFirmwareEffects } from '../composables/useEffects'
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

/** The list, as filtered: what is plugged in, or every device described. */
const shown = ref<Shown>('plugged')
const text = ref('')
const list = computed(() => listed(devices.value, shown.value, text.value))
const pluggedCount = computed(() => devices.value.filter((d) => d.present).length)

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
  await Promise.all([refresh(), refreshFirmwareEffects()])
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

/**
 * Which unverified definition *Try* uses, per device: the first, unless another
 * firmware's variant is picked (#300).
 */
const picked = reactive<Record<string, string>>({})

/**
 * Tries it: the definition goes into your folder and drives the device, which
 * is controlled; the list is read again, where it is now yours.
 */
async function tryIt(d: DeviceInfo): Promise<void> {
  const file = picked[ids(d)] ?? d.file
  if (file === null) return
  await attempt(() => tryDeviceDefinition(file))
  await reread()
}

/** The report form, in the browser: what it looks like is the person's to say. */
function report(d: DeviceInfo): Promise<void> {
  return attempt(() => reportDeviceDefinition(d.vid, d.pid))
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
      <span class="seg-group" role="group" :aria-label="t('devices.shown')">
        <button
          type="button"
          class="seg"
          :aria-pressed="shown === 'plugged'"
          @click="shown = 'plugged'"
        >
          {{ t('devices.pluggedIn', { n: pluggedCount }) }}
        </button>
        <button type="button" class="seg" :aria-pressed="shown === 'all'" @click="shown = 'all'">
          {{ t('devices.all', { n: devices.length }) }}
        </button>
      </span>
      <button class="ghost" :disabled="busy" :title="t('devices.refreshTitle')" @click="reread">
        {{ t('devices.refresh') }}
      </button>
    </header>

    <input
      v-if="shown === 'all' && devices.length > FILTER_PAST"
      v-model="text"
      class="filter"
      type="search"
      spellcheck="false"
      :placeholder="t('devices.filter')"
      :aria-label="t('devices.filter')"
    />

    <!--
      One list: a device plugged in is a card to act on; one known but not
      plugged in, under *All*, only has its definition to read.
    -->
    <ul v-if="list.length" class="cards">
      <li
        v-for="d in list"
        :key="ids(d)"
        class="card"
        :class="{ controlled: d.present && d.state === 'adopted', away: !d.present }"
      >
        <div class="main">
          <!--
            The dot speaks of a device someone decided to control: orange on one
            never controlled and not plugged in would warn about nothing.
          -->
          <DeviceStatusDot v-if="d.present || d.state === 'adopted'" :device="d" />
          <span v-else class="spacer" aria-hidden="true" />
          <div class="id">
            <span class="name">
              {{ d.name }}
              <span v-if="d.origin === 'yours'" class="tag yours">{{ t('devices.groups.yours') }}</span>
              <span v-else-if="d.origin === 'unverified'" class="tag yours">{{
                t('devices.groups.unverified')
              }}</span>
            </span>
            <span class="sub">
              <span class="mono">{{ ids(d) }}</span> · {{ count(d) }}
              <template v-if="!d.present"> · {{ t('devices.status.unplugged') }}</template>
            </span>
          </div>

          <!--
            Which definition drives it, when there is a choice: the built-in one,
            or a file of yours for the same ids. None takes over by being in
            the folder (`docs/design/device-sdk.md` §9).
          -->
          <!-- Two unverified files for one model are firmware variants: which to try. -->
          <label
            v-if="d.origin === 'unverified' && d.present && d.definitions.length > 1"
            class="choice"
          >
            <span class="sr-only">{{ t('devices.definition') }}</span>
            <select
              :value="picked[ids(d)] ?? d.file ?? ''"
              :disabled="busy"
              :title="t('devices.definition')"
              @change="picked[ids(d)] = ($event.target as HTMLSelectElement).value"
            >
              <option v-for="o in d.definitions" :key="o.file" :value="o.file">{{ o.file }}</option>
            </select>
          </label>
          <label
            v-else-if="d.present && d.origin !== 'unverified' && definitionChoice(d) !== null"
            class="choice"
          >
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

          <!-- Not *Open*: on a device, it would read as its state, *not open*. -->
          <button v-if="d.file && d.origin !== 'unverified'" class="ghost" @click="edit(d.origin, d.file)">
            {{ t('devices.definition') }}
          </button>

          <!--
            The decision, one action: *Release* what is controlled, *Control*
            the rest; the dot says the state. A controlled device that is not
            open keeps *Release*: *Refresh* tries again, and *Release* then
            *Control* targets another unit of the same model, its serial read
            on open. Released and never decided look alike: neither is opened.
          -->
          <!-- Not verified: nothing drives it yet, trying it is the one action, once plugged in. -->
          <button
            v-if="d.origin === 'unverified' && d.present"
            class="solid"
            :disabled="busy"
            @click="tryIt(d)"
          >
            {{ t('devices.try') }}
          </button>
          <template v-else-if="d.present && d.origin !== 'unverified'">
            <button v-if="d.state === 'adopted'" class="ghost" :disabled="busy" @click="ignore(d)">
              {{ t('devices.release') }}
            </button>
            <button v-else class="solid" :disabled="busy" @click="adopt(d)">
              {{ t('devices.control') }}
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
          </template>
        </div>

        <template v-if="d.present">
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
          <!-- Said once, on its card: what trying it means (#300). -->
          <p v-if="d.origin === 'unverified'" class="warn" role="status">{{ t('devices.tryNote') }}</p>
          <!-- Tried from an unverified definition: the one place to say how it lights. -->
          <p v-else-if="d.unverified" class="elsewhere">
            {{ t('devices.reportNote') }}
            <button type="button" class="ghost small" @click="report(d)">{{ t('devices.report') }}</button>
          </p>
        </template>
      </li>
    </ul>
    <p v-else class="note">
      {{ shown === 'plugged' ? t('devices.nothingPlugged') : t('devices.noMatch') }}
    </p>
    <p
      v-if="list.some((d) => d.present && d.state === 'detected' && d.origin !== 'unverified')"
      class="note"
    >
      {{ t('devices.neverSeen') }}
    </p>
    <!--
      This list is what this computer has a definition for; more are published,
      some waiting for someone with the device. Opened in the browser: Candeo
      fetches nothing.
    -->
    <p class="elsewhere">
      {{ t('devices.moreOnSite') }}
      <button type="button" class="ghost small" @click="attempt(openSiteDevices)">
        {{ t('devices.moreOnSiteLink') }}
      </button>
    </p>

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

.head h1 {
  flex: 1;
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

/* Known, not plugged in: listed under *All*, with only its definition to read. */
.card.away {
  padding: var(--gap-2) var(--gap-4);
  background: transparent;
}

.spacer {
  flex: none;
  width: 8px;
}

.card.away .name {
  color: var(--text-muted);
  font-weight: 400;
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

/* Yours: nobody reviewed it, and it shows wherever the device does. */
.tag.yours {
  margin-left: var(--gap-2);
  color: var(--warn);
  background: color-mix(in srgb, var(--warn) 14%, transparent);
  vertical-align: 1px;
}

.filter {
  align-self: flex-end;
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

.elsewhere {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: var(--gap-2);
  margin: 0;
  color: var(--text-faint);
  font-size: 12px;
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
