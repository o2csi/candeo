<script setup lang="ts">
/**
 * Automations: rules that interrupt the effect applied on a device for a while,
 * then give it back (#106, `docs/design/inputs-and-automations.md` §3.5).
 *
 * Each rule reads as a sentence of chips — *every hour on weekdays, on
 * DeathStalker V2 Pro, show Clock for 10 s* — and every chip opens what changes
 * it. When is a cron expression: the chips write the common ones, and the
 * advanced field takes any, with the format explained beside it. One of the two
 * holds the rule at a time; the other is shown disabled. Or when is "after 10 min
 * idle" (#179), which lasts until someone is back: no days, no duration.
 *
 * Rules are saved as soon as a gesture ends: there is no Save button to forget,
 * and the scheduler, in Rust, sees the change within a second, window closed or
 * not. Their order is their priority, changed by dragging a rule, or with its
 * arrows from the keyboard.
 */
import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue'
import type { UnlistenFn } from '@tauri-apps/api/event'
import type { ParamValue } from '@candeo/effects-api'

import {
  getLayout,
  getSettings,
  idleAvailable,
  listDevices,
  listEffects,
  onStateChanged,
  setAutomationsPaused,
  setRules,
  tryRule,
  type EffectEntry,
  type EffectParams,
  type Rule,
} from '../api/candeo'
import { message } from '../api/journal'
import type { DeviceInfo, LayoutInfo } from '../api/types'
import DaysChip from '../components/DaysChip.vue'
import DurationChip from '../components/DurationChip.vue'
import EffectParamsForm from '../components/EffectParamsForm.vue'
import FailureNote from '../components/FailureNote.vue'
import FrequencyChip from '../components/FrequencyChip.vue'
import {
  hardwareEffects,
  hardwareEffectsFor,
  type HardwareEffect,
} from '../composables/useEffects'
import {
  EVERY_DAY,
  FOR_PRESETS,
  blankRule,
  editable,
  examples,
  expression,
  moved,
  readSimple,
  ruleValues,
  writeSimple,
  type Days,
  type Frequency,
  type Simple,
} from '../composables/rules'
import { t } from '../i18n'

/** The rules as the file holds them: a hand-edited one may not be a `Rule`. */
const rules = ref<unknown[]>([])
const paused = ref(false)
const library = ref<EffectEntry[]>([])
const devices = ref<DeviceInfo[]>([])
/**
 * Each adopted device's layout, keyed "vid:pid".
 *
 * It says which effects its firmware runs: a rule's sentence must not offer a
 * mode the device would refuse.
 */
const layouts = reactive<Record<string, LayoutInfo>>({})
const problem = ref<string | null>(null)
const loaded = ref(false)
/** Whether this system says how long the computer has been idle. */
const idleHere = ref(false)

/**
 * Settings being dragged, per rule, before the gesture ends: the form streams
 * values while a slider moves, and writing the file on each would be dozens of
 * writes for one gesture.
 */
const drafts = reactive<Record<string, EffectParams>>({})

/**
 * Which rules someone switched to the advanced field, or back. Unset, a rule
 * opens where its expression can be shown: chips when they can say it.
 */
const advancedChosen = reactive<Record<string, boolean>>({})

let unlisten: UnlistenFn | null = null

async function load(): Promise<void> {
  try {
    const [settings, effects, found, idle] = await Promise.all([
      getSettings(),
      listEffects(),
      listDevices(),
      idleAvailable(),
    ])
    rules.value = settings.rules
    paused.value = settings.preferences.automationsPaused ?? false
    library.value = effects
    devices.value = found.filter((d) => d.state === 'adopted')
    idleHere.value = idle
    // A layout that cannot be read is not a failure here: the whole catalogue is
    // then offered, as it was before a device said what it runs.
    for (const d of devices.value) {
      const known = await getLayout({ vid: d.vid, pid: d.pid }).catch(() => null)
      if (known) layouts[deviceKey(d)] = known
    }
  } catch (e) {
    problem.value = message(e)
  } finally {
    loaded.value = true
  }
}

onMounted(async () => {
  await load()
  // The tray pauses automations too: what this screen shows follows.
  unlisten = await onStateChanged(() => void load()).catch(() => null)
})

onBeforeUnmount(() => unlisten?.())

// ---------------------------------------------------------------- saving

/** Writes the rules, and shows the list as it will be before the answer. */
async function save(next: unknown[]): Promise<void> {
  problem.value = null
  const before = rules.value
  rules.value = next
  try {
    await setRules(next as Rule[])
  } catch (e) {
    // Refused — an expression that is not cron, most often: the list goes back to
    // what the file holds, and the field shows it again.
    rules.value = before
    problem.value = message(e)
  }
}

/** A copy that holds none of the reactive proxy: what goes to Rust is plain JSON. */
function plain(rule: Rule): Rule {
  return JSON.parse(JSON.stringify(rule)) as Rule
}

function update(index: number, change: (rule: Rule) => Rule): void {
  const current = rules.value[index]
  if (!editable(current)) return
  void save(rules.value.map((r, i) => (i === index ? change(plain(current)) : r)))
}

function remove(index: number): void {
  void save(rules.value.filter((_, i) => i !== index))
}

function move(from: number, to: number): void {
  void save(moved(rules.value, from, to))
}

async function pause(event: Event): Promise<void> {
  const on = (event.target as HTMLInputElement).checked
  problem.value = null
  try {
    await setAutomationsPaused(on)
    paused.value = on
  } catch (e) {
    problem.value = message(e)
  }
}

async function attempt(rule: Rule): Promise<void> {
  problem.value = null
  try {
    await tryRule(rule.id)
  } catch (e) {
    problem.value = message(e)
  }
}

// ---------------------------------------------------------------- new rules

const firstDevice = computed(() =>
  devices.value.length ? { vid: devices.value[0].vid, pid: devices.value[0].pid } : null,
)

function addRule(): void {
  void save([...rules.value, blankRule(firstDevice.value, 'shipped:Clock')])
}

function addExamples(): void {
  const names = {
    hourly: t('automations.hourlyClock'),
    night: t('automations.nightOff'),
    away: t('automations.awayOff'),
  }
  void save([...rules.value, ...examples(firstDevice.value, names, idleHere.value)])
}

// ---------------------------------------------------------------- when

/**
 * What the chips show for a rule: its expression, or the default they start from
 * — which is also what an idle rule becomes when a time is chosen instead.
 */
function simple(rule: Rule): Simple {
  return readSimple(expression(rule) ?? '') ?? { frequency: { kind: 'hour' }, days: [...EVERY_DAY] }
}

function advanced(rule: Rule): boolean {
  const expr = expression(rule)
  return expr !== null && (advancedChosen[rule.id] ?? readSimple(expr) === null)
}

function chooseFrequency(index: number, rule: Rule, frequency: Frequency): void {
  const expr = writeSimple({ frequency, days: simple(rule).days })
  update(index, (r) => ({ ...r, when: { kind: 'cron', expr } }))
}

function chooseIdle(index: number, rule: Rule, minutes: number): void {
  // Back to a time later, the rule opens in the chips, not in an old advanced field.
  delete advancedChosen[rule.id]
  update(index, (r) => ({ ...r, when: { kind: 'idle', minutes } }))
}

function chooseDays(index: number, rule: Rule, days: Days): void {
  const expr = writeSimple({ frequency: simple(rule).frequency, days })
  update(index, (r) => ({ ...r, when: { kind: 'cron', expr } }))
}

/**
 * Switches a rule between the chips and the advanced field. Back to the chips, an
 * expression they cannot say becomes what they start from — every hour — rather
 * than chips pretending to show something else.
 */
function chooseAdvanced(index: number, rule: Rule, event: Event): void {
  const on = (event.target as HTMLInputElement).checked
  advancedChosen[rule.id] = on
  if (!on && readSimple(expression(rule) ?? '') === null) {
    const expr = writeSimple(simple(rule))
    update(index, (r) => ({ ...r, when: { kind: 'cron', expr } }))
  }
}

function typeExpression(index: number, event: Event): void {
  const expr = (event.target as HTMLInputElement).value.trim()
  update(index, (r) => ({ ...r, when: { kind: 'cron', expr } }))
}

// ---------------------------------------------------------------- what and where

const deviceKey = (d: { vid: number; pid: number }) => `${d.vid}:${d.pid}`

function chooseDevice(index: number, event: Event): void {
  const [vid, pid] = (event.target as HTMLSelectElement).value.split(':').map(Number)
  update(index, (r) => ({ ...r, devices: [{ vid, pid }] }))
}

function chooseEffect(index: number, event: Event): void {
  const effect = (event.target as HTMLSelectElement).value
  // Another effect's settings mean nothing to this one.
  update(index, (r) => ({ ...r, show: { effect, params: {} } }))
}

const builtins = computed(() => library.value.filter((e) => e.kind === 'builtin'))
const yours = computed(() => library.value.filter((e) => e.kind === 'user'))

function manifest(rule: Rule): EffectEntry | undefined {
  return library.value.find((e) => e.id === rule.show.effect)
}

/** An effect the rule names that is neither firmware nor in the library any more. */
function missing(rule: Rule): boolean {
  return !rule.show.effect.startsWith('hardware:') && manifest(rule) === undefined
}

function effectLabel(rule: Rule): string {
  const hardware = hardwareEffects.find((h) => h.id === rule.show.effect)
  if (hardware) return t(`effects.hardwareEffects.${hardware.key}.name`)
  return manifest(rule)?.name ?? rule.show.effect
}

/**
 * The firmware effects of the rule's **first device**: a mode its firmware does
 * not know has no place in the sentence.
 */
function hardwareFor(rule: Rule): readonly HardwareEffect[] {
  const target = rule.devices[0]
  return hardwareEffectsFor(target ? layouts[deviceKey(target)] : null)
}

function hasSettings(rule: Rule): boolean {
  return Object.keys(manifest(rule)?.params ?? {}).length > 0
}

function values(rule: Rule): EffectParams {
  return ruleValues(manifest(rule), drafts[rule.id] ?? rule.show.params)
}

function onParam(rule: Rule, id: string, value: ParamValue): void {
  drafts[rule.id] = { ...(drafts[rule.id] ?? rule.show.params), [id]: value }
}

function onParamsCommit(index: number, rule: Rule): void {
  const params = drafts[rule.id]
  delete drafts[rule.id]
  if (params) update(index, (r) => ({ ...r, show: { ...r.show, params } }))
}

function onParamsReset(index: number, rule: Rule): void {
  delete drafts[rule.id]
  update(index, (r) => ({ ...r, show: { ...r.show, params: {} } }))
}

// ---------------------------------------------------------------- dragging

const dragging = ref<number | null>(null)

function onDrop(to: number): void {
  if (dragging.value !== null) move(dragging.value, to)
  dragging.value = null
}
</script>

<template>
  <section class="page">
    <header class="head">
      <div>
        <h1>{{ t('automations.title') }}</h1>
        <p class="lead">{{ t('automations.lead') }}</p>
      </div>
      <label class="pause">
        <input type="checkbox" :checked="paused" @change="pause" />
        {{ t('automations.pause') }}
      </label>
    </header>

    <FailureNote v-if="problem" class="err" @close="problem = null">{{ problem }}</FailureNote>
    <p v-if="loaded && !devices.length" class="warn">{{ t('automations.noDevice') }}</p>

    <ol v-if="rules.length" class="list">
      <li
        v-for="(raw, index) in rules"
        :key="editable(raw) ? raw.id : index"
        class="rule"
        :class="{ off: editable(raw) && !raw.enabled }"
        @dragover.prevent
        @drop="onDrop(index)"
      >
        <template v-if="editable(raw)">
          <div class="line">
            <div class="sentence">
            <span
              class="grip"
              draggable="true"
              :title="t('automations.dragHint')"
              aria-hidden="true"
              @dragstart="dragging = index"
              @dragend="dragging = null"
              >⋮⋮</span
            >
            <input
              type="checkbox"
              class="switch"
              :checked="raw.enabled"
              :aria-label="t('automations.enabled')"
              @change="update(index, (r) => ({ ...r, enabled: !r.enabled }))"
            />

            <!--
              The advanced field holds the rule. When the chips could say its
              expression they show it, disabled; when they could not, they would
              show something the rule does not do — one inert chip says where
              "when" is written instead.
            -->
            <span
              v-if="advanced(raw) && readSimple(expression(raw) ?? '') === null"
              class="by-expression"
            >
              {{ t('automations.byExpression') }}
            </span>
            <template v-else>
              <FrequencyChip
                :frequency="simple(raw).frequency"
                :disabled="advanced(raw)"
                :idle="raw.when.kind === 'idle' ? raw.when.minutes : null"
                :idle-available="idleHere"
                @change="(f) => chooseFrequency(index, raw, f)"
                @idle="(m) => chooseIdle(index, raw, m)"
              />
              <DaysChip
                v-if="raw.when.kind === 'cron'"
                :days="simple(raw).days"
                :disabled="advanced(raw)"
                @change="(d) => chooseDays(index, raw, d)"
              />
            </template>

            <span class="word">{{ t('automations.on') }}</span>
            <select
              class="pick"
              :aria-label="t('automations.on')"
              :value="raw.devices.length ? deviceKey(raw.devices[0]) : ''"
              @change="chooseDevice(index, $event)"
            >
              <option v-if="!raw.devices.length" value="" disabled>—</option>
              <option v-for="d in devices" :key="deviceKey(d)" :value="deviceKey(d)">
                {{ d.name }}
              </option>
            </select>

            <span class="word">{{ t('automations.show') }}</span>
            <select
              class="pick"
              :aria-label="t('automations.show')"
              :value="raw.show.effect"
              @change="chooseEffect(index, $event)"
            >
              <optgroup :label="t('automations.hardware')">
                <option v-for="h in hardwareFor(raw)" :key="h.id" :value="h.id">
                  {{ t(`effects.hardwareEffects.${h.key}.name`) }}
                </option>
              </optgroup>
              <optgroup :label="t('automations.builtin')">
                <option v-for="e in builtins" :key="e.id" :value="e.id">{{ e.name }}</option>
              </optgroup>
              <optgroup v-if="yours.length" :label="t('automations.user')">
                <option v-for="e in yours" :key="e.id" :value="e.id">{{ e.name }}</option>
              </optgroup>
              <option v-if="missing(raw)" :value="raw.show.effect">{{ raw.show.effect }}</option>
            </select>

            <template v-if="raw.when.kind === 'cron'">
              <span class="word">{{ t('automations.for') }}</span>
              <DurationChip
                :seconds="raw.for?.seconds ?? 10"
                :presets="FOR_PRESETS"
                :label="t('automations.for')"
                @change="(s) => update(index, (r) => ({ ...r, for: { seconds: s } }))"
              />
            </template>
            <span v-else class="word">{{ t('automations.untilBack') }}</span>
            </div>

            <div class="actions">
            <button type="button" class="ghost" @click="attempt(raw)">
              {{ t('automations.try') }}
            </button>
            <button
              type="button"
              class="icon"
              :disabled="index === 0"
              :aria-label="t('automations.moveUp')"
              :title="t('automations.moveUp')"
              @click="move(index, index - 1)"
            >
              ↑
            </button>
            <button
              type="button"
              class="icon"
              :disabled="index === rules.length - 1"
              :aria-label="t('automations.moveDown')"
              :title="t('automations.moveDown')"
              @click="move(index, index + 1)"
            >
              ↓
            </button>
            <button type="button" class="ghost" @click="remove(index)">
              {{ t('automations.delete') }}
            </button>
            </div>
          </div>

          <div class="more">
            <div v-if="raw.when.kind === 'cron'" class="advanced">
              <label class="toggle">
                <input
                  type="checkbox"
                  :checked="advanced(raw)"
                  @change="chooseAdvanced(index, raw, $event)"
                />
                {{ t('automations.advanced') }}
              </label>
              <input
                class="expr mono"
                type="text"
                spellcheck="false"
                :aria-label="t('automations.expression')"
                :value="expression(raw) ?? ''"
                :disabled="!advanced(raw)"
                @change="typeExpression(index, $event)"
              />
            </div>
            <details v-if="advanced(raw)" class="format">
              <summary>{{ t('automations.format') }}</summary>
              <p class="help">{{ t('automations.expressionHelp') }}</p>
            </details>

            <input
              class="name"
              type="text"
              :placeholder="t('automations.name')"
              :aria-label="t('automations.name')"
              :value="raw.name"
              @change="
                update(index, (r) => ({ ...r, name: ($event.target as HTMLInputElement).value }))
              "
            />
            <details v-if="hasSettings(raw)" class="settings">
              <summary>{{ t('automations.settingsOf', { effect: effectLabel(raw) }) }}</summary>
              <EffectParamsForm
                :specs="manifest(raw)?.params ?? {}"
                :values="values(raw)"
                :frozen="null"
                :empty="t('automations.noSettings')"
                @change="(id, value) => onParam(raw, id, value)"
                @commit="onParamsCommit(index, raw)"
                @reset="onParamsReset(index, raw)"
              />
            </details>
          </div>

          <p v-if="missing(raw)" class="warn">
            {{ t('automations.missingEffect', { effect: raw.show.effect }) }}
          </p>
          <p v-if="raw.when.kind === 'idle' && !idleHere" class="warn">
            {{ t('automations.idleRuleUnavailable') }}
          </p>
        </template>

        <template v-else>
          <div class="line">
            <span class="unreadable">{{ t('automations.unreadable') }}</span>
            <span class="spacer" />
            <button type="button" class="ghost" @click="remove(index)">
              {{ t('automations.delete') }}
            </button>
          </div>
        </template>
      </li>
    </ol>

    <div v-else-if="loaded" class="empty">
      <p>{{ t('automations.empty') }}</p>
      <button type="button" class="solid" @click="addExamples">
        {{ t('automations.addExamples') }}
      </button>
    </div>

    <footer class="foot">
      <button type="button" class="ghost" @click="addRule">{{ t('automations.newRule') }}</button>
      <p v-if="rules.length > 1" class="note">{{ t('automations.priority') }}</p>
    </footer>
  </section>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  gap: var(--gap-4);
  padding: var(--gap-4);
  /* Wide enough for a rule's sentence on one line; it wraps below that. */
  max-width: 1280px;
}

.head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--gap-3);
}

.head h1 {
  margin: 0;
}

.lead {
  margin: 4px 0 0;
  color: var(--text-muted);
  font-size: 13px;
}

.pause {
  display: flex;
  align-items: center;
  gap: var(--gap-2);
  color: var(--text-muted);
  font-size: 13px;
  white-space: nowrap;
}

.list {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.rule {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  padding: var(--gap-3) var(--gap-4);
  background: var(--raised);
  border: 1px solid var(--line);
  border-radius: var(--r-lg);
}

/* Switched off: kept, readable, visibly resting. */
.rule.off {
  background: none;
  border-style: dashed;
}

.line {
  display: flex;
  align-items: flex-start;
  gap: var(--gap-3);
  font-size: 14px;
}

/* The sentence wraps on a narrow window; the actions stay together, on the right. */
.sentence {
  display: flex;
  flex: 1;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--gap-2);
  min-width: 0;
}

.actions {
  display: flex;
  flex: none;
  align-items: center;
  gap: var(--gap-2);
}

.by-expression {
  padding: 3px var(--gap-2);
  border-radius: var(--r-md);
  background: var(--raised-2);
  color: var(--text-faint);
  font-style: italic;
  white-space: nowrap;
}

.format summary {
  color: var(--text-muted);
  font-size: 12px;
  cursor: pointer;
}

.grip {
  color: var(--text-faint);
  cursor: grab;
  user-select: none;
  letter-spacing: -2px;
}

.word {
  color: var(--text-muted);
}

.pick {
  padding: 3px var(--gap-2);
  border-radius: var(--r-md);
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 500;
  border: none;
}

.spacer {
  flex: 1;
}

.more {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  padding-left: 44px;
}

.advanced {
  display: flex;
  align-items: center;
  gap: var(--gap-3);
}

.toggle {
  display: flex;
  align-items: center;
  gap: var(--gap-2);
  color: var(--text-muted);
  font-size: 13px;
  white-space: nowrap;
}

.expr {
  width: 16em;
  font-size: 13px;
}

.expr:disabled {
  opacity: 0.5;
}

.help {
  margin: 0;
  max-width: 70ch;
  color: var(--text-faint);
  font-size: 12px;
  white-space: pre-line;
}

.name {
  max-width: 280px;
  font-size: 13px;
}

.settings summary {
  color: var(--text-muted);
  font-size: 13px;
  cursor: pointer;
}

.solid,
.ghost {
  padding: 5px var(--gap-3);
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

.icon {
  width: 26px;
  height: 26px;
  border-radius: var(--r-md);
  color: var(--text-muted);
}

.icon:hover:not(:disabled) {
  color: var(--text);
  background: var(--raised-2);
}

.icon:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.unreadable {
  color: var(--text-faint);
  font-size: 13px;
}

.empty {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--gap-3);
  color: var(--text-muted);
}

.empty p {
  margin: 0;
}

.foot {
  display: flex;
  align-items: center;
  gap: var(--gap-3);
}

.note {
  margin: 0;
  color: var(--text-faint);
  font-size: 12px;
}

.err {
  margin: 0;
  padding: var(--gap-2) var(--gap-3);
  border: 1px solid var(--bad);
  border-radius: var(--r-md);
  background: color-mix(in srgb, var(--bad) 10%, transparent);
  font-size: 13px;
}

.warn {
  margin: 0;
  padding: var(--gap-2) var(--gap-3);
  border: 1px solid var(--warn);
  border-radius: var(--r-md);
  background: color-mix(in srgb, var(--warn) 10%, transparent);
  font-size: 13px;
}
</style>
