<script setup lang="ts">
/**
 * An effect's presets, above its settings (`docs/design/presets.md` §5).
 *
 * The list in the order the presets were saved, each one's name a button that
 * gives the settings its values; *Save as preset*; and, per preset, *Update*,
 * *Rename* and *Delete*.
 *
 * ## Picking is not applying
 *
 * Clicking a name emits `pick`, and the parent writes the settings as if every
 * slider had moved at once: they reach the device only when this effect runs
 * there. Nothing here starts an effect; *Apply* stays the one gesture that
 * does.
 *
 * ## The preset the settings hold is pressed
 *
 * `aria-pressed`, a tick and the accent: computed from the settings shown
 * (`holds`), never stored, so moving a slider releases it.
 *
 * ## Saving, renaming and deleting answer here
 *
 * The parent hands the commands as functions: a name refused is said under the
 * list, the field stays open with what was typed, and closes only once Rust
 * has kept it.
 */

import { computed, nextTick, ref, useId, watch } from 'vue'
import type { ParamSpec } from '@candeo/effects-api'

import type { PresetRecord } from '../api/candeo'
import { message } from '../api/journal'
import {
  holds as holdsSettings,
  PRESET_NAME_MAX,
  presetName,
  type PresetSettings,
} from '../composables/presets'
import { t } from '../i18n'

const props = defineProps<{
  /** The effect's presets, in their order. */
  presets: readonly PresetRecord[]
  /** What the effect declares: a preset is read against it. */
  specs: Record<string, ParamSpec>
  /** The settings shown, complete: what *Save* and *Update* keep, and what marks a preset. */
  shown: PresetSettings
  /**
   * Whether a preset can be picked: there is a device or region to keep its
   * settings for, and the form is live.
   */
  pickable: boolean
  save: (name: string) => Promise<void>
  update: (name: string) => Promise<void>
  rename: (from: string, to: string) => Promise<void>
  remove: (name: string) => Promise<void>
}>()

const emit = defineEmits<{
  /** The settings take this preset's. */
  pick: [preset: PresetRecord]
}>()

const uid = useId()

/** A command is on its way: the buttons wait for its answer. */
const busy = ref(false)
/** Why the last command was refused, as Rust says it. */
const problem = ref<string | null>(null)

/** The name field of *Save as preset* is open. */
const naming = ref(false)
const draft = ref('')
/**
 * The row whose name is being changed, by position: a name hand-written twice
 * in the file would otherwise open two fields.
 */
const renaming = ref<number | null>(null)
const renameDraft = ref('')
/** The row whose deletion waits for its answer, by position. */
const deleting = ref<number | null>(null)

function closeAll(): void {
  naming.value = false
  renaming.value = null
  deleting.value = null
  problem.value = null
}

// Another effect: what was typed or asked was about the previous one's presets.
watch(() => props.specs, closeAll)

const held = computed(() => props.presets.map((p) => holdsSettings(props.specs, props.shown, p)))

function focus(id: string, select = false): void {
  void nextTick(() => {
    const el = document.getElementById(id)
    el?.focus()
    if (select && el instanceof HTMLInputElement) el.select()
  })
}

/** Runs a command; on refusal, says why and leaves the field as it was. */
async function run(command: () => Promise<void>): Promise<boolean> {
  busy.value = true
  problem.value = null
  try {
    await command()
    return true
  } catch (e) {
    problem.value = message(e)
    return false
  } finally {
    busy.value = false
  }
}

function startNaming(): void {
  closeAll()
  draft.value = ''
  naming.value = true
  focus(`${uid}-name`)
}

function stopNaming(): void {
  naming.value = false
  problem.value = null
  focus(`${uid}-save`)
}

async function onSave(): Promise<void> {
  const name = presetName(draft.value)
  if (name === '') return
  if (await run(() => props.save(name))) stopNaming()
}

function startRenaming(p: PresetRecord, i: number): void {
  closeAll()
  renameDraft.value = p.name
  renaming.value = i
  focus(`${uid}-rename`, true)
}

/** Back to the row's buttons, the focus on the one that opened the field. */
function stopRenaming(i: number): void {
  renaming.value = null
  problem.value = null
  focus(`${uid}-renamer-${i}`)
}

async function onRename(p: PresetRecord, i: number): Promise<void> {
  const to = presetName(renameDraft.value)
  if (to === '') return
  // In place: the row keeps its position under its new name.
  if (await run(() => props.rename(p.name, to))) stopRenaming(i)
}

function askDelete(i: number): void {
  closeAll()
  deleting.value = i
  // The row's buttons are replaced by the question: the answer takes the focus.
  focus(`${uid}-confirm`)
}

function keep(i: number): void {
  deleting.value = null
  problem.value = null
  focus(`${uid}-deleter-${i}`)
}

async function onDelete(p: PresetRecord): Promise<void> {
  if (await run(() => props.remove(p.name))) {
    deleting.value = null
    focus(`${uid}-save`)
  }
}

async function onUpdate(p: PresetRecord): Promise<void> {
  closeAll()
  await run(() => props.update(p.name))
}
</script>

<template>
  <section class="presets">
    <div class="head">
      <h2>{{ t('effects.presets.title') }}</h2>
      <button
        v-if="!naming"
        :id="`${uid}-save`"
        class="ghost small"
        type="button"
        :disabled="busy"
        @click="startNaming"
      >
        {{ t('effects.presets.save') }}
      </button>
    </div>

    <form v-if="naming" class="naming" @submit.prevent="onSave">
      <input
        :id="`${uid}-name`"
        v-model="draft"
        class="name"
        type="text"
        spellcheck="false"
        autocomplete="off"
        :maxlength="PRESET_NAME_MAX"
        :placeholder="t('effects.presets.name')"
        :aria-label="t('effects.presets.name')"
        @keydown.esc.prevent="stopNaming"
      />
      <button class="solid small" type="submit" :disabled="busy || presetName(draft) === ''">
        {{ t('effects.presets.keep') }}
      </button>
      <button class="ghost small" type="button" :disabled="busy" @click="stopNaming">
        {{ t('effects.presets.cancel') }}
      </button>
    </form>

    <p v-if="presets.length === 0" class="hint">{{ t('effects.presets.empty') }}</p>

    <ul v-else class="list">
      <!-- By position: a name hand-written twice in the file is two rows still. -->
      <li v-for="(p, i) in presets" :key="i" class="row">
        <div
          v-if="deleting === i"
          class="confirm"
          role="group"
          :aria-labelledby="`${uid}-confirm-title`"
        >
          <p :id="`${uid}-confirm-title`" class="confirm-title" role="alert">
            {{ t('effects.presets.deleteTitle', { name: p.name }) }}
          </p>
          <div class="confirm-actions">
            <button
              :id="`${uid}-confirm`"
              class="solid small danger"
              type="button"
              :disabled="busy"
              @click="onDelete(p)"
            >
              {{ t('effects.presets.delete') }}
            </button>
            <button class="ghost small" type="button" :disabled="busy" @click="keep(i)">
              {{ t('effects.presets.cancel') }}
            </button>
          </div>
        </div>

        <form v-else-if="renaming === i" class="naming" @submit.prevent="onRename(p, i)">
          <input
            :id="`${uid}-rename`"
            v-model="renameDraft"
            class="name"
            type="text"
            spellcheck="false"
            autocomplete="off"
            :maxlength="PRESET_NAME_MAX"
            :aria-label="t('effects.presets.newName', { name: p.name })"
            @keydown.esc.prevent="stopRenaming(i)"
          />
          <button
            class="solid small"
            type="submit"
            :disabled="busy || presetName(renameDraft) === ''"
          >
            {{ t('effects.presets.keep') }}
          </button>
          <button class="ghost small" type="button" :disabled="busy" @click="stopRenaming(i)">
            {{ t('effects.presets.cancel') }}
          </button>
        </form>

        <template v-else>
          <button
            class="pick"
            type="button"
            :aria-pressed="held[i]"
            :title="t('effects.presets.use')"
            :disabled="!pickable || busy"
            @click="emit('pick', p)"
          >
            <span class="tick" aria-hidden="true">{{ held[i] ? '✓' : '' }}</span>
            <span class="label">{{ p.name }}</span>
          </button>
          <span class="tools">
            <button
              v-if="!held[i]"
              class="tool"
              type="button"
              :aria-label="t('effects.presets.updateOf', { name: p.name })"
              :title="t('effects.presets.updateTitle', { name: p.name })"
              :disabled="busy"
              @click="onUpdate(p)"
            >
              {{ t('effects.presets.update') }}
            </button>
            <button
              :id="`${uid}-renamer-${i}`"
              class="tool"
              type="button"
              :aria-label="t('effects.presets.renameOf', { name: p.name })"
              :disabled="busy"
              @click="startRenaming(p, i)"
            >
              {{ t('effects.presets.rename') }}
            </button>
            <button
              :id="`${uid}-deleter-${i}`"
              class="tool danger"
              type="button"
              :aria-label="t('effects.presets.deleteOf', { name: p.name })"
              :disabled="busy"
              @click="askDelete(i)"
            >
              {{ t('effects.presets.delete') }}
            </button>
          </span>
        </template>
      </li>
    </ul>

    <p v-if="problem" class="problem" role="alert">{{ problem }}</p>
  </section>
</template>

<style scoped>
.presets {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  padding-top: var(--gap-3);
  border-top: 1px solid var(--line);
}

.head {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-2);
  align-items: center;
  justify-content: space-between;
}

/* The settings form's heading, the same rank and look. */
.head h2 {
  color: var(--text-faint);
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.hint {
  color: var(--text-faint);
  font-size: 12px;
}

.list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-1) var(--gap-2);
  align-items: center;
}

/*
 * The name takes the room left, the tools keep to the right, and go below it
 * once the name would have less than about twenty characters a line: French
 * labels take half a narrow column. A name is free text: it wraps rather than
 * overflowing.
 */
.pick {
  display: flex;
  flex: 1 1 20ch;
  gap: var(--gap-2);
  align-items: baseline;
  min-width: 0;
  padding: 4px var(--gap-2);
  border: 1px solid transparent;
  border-radius: var(--r-md);
  text-align: left;
}

.pick:hover:not(:disabled) {
  background: var(--raised-2);
}

/* Pressed: the settings hold it. The tick says it too: colour never alone. */
.pick[aria-pressed='true'] {
  color: var(--accent);
  background: var(--accent-soft);
  border-color: var(--accent);
}

.pick:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}

.tick {
  flex: none;
  width: 1ch;
}

.label {
  min-width: 0;
  font-size: 13px;
  overflow-wrap: anywhere;
}

.tools {
  display: inline-flex;
  flex-wrap: wrap;
  gap: var(--gap-1);
  margin-left: auto;
}

.tool {
  padding: 2px var(--gap-2);
  border-radius: var(--r-sm);
  color: var(--text-faint);
  font-size: 12px;
}

.tool:hover:not(:disabled) {
  color: var(--text);
  background: var(--raised-2);
}

.tool.danger:hover:not(:disabled) {
  color: var(--bad);
  background: color-mix(in srgb, var(--bad) 12%, transparent);
}

.naming {
  display: flex;
  flex: 1;
  flex-wrap: wrap;
  gap: var(--gap-2);
  align-items: center;
}

.name {
  flex: 1 1 16ch;
  min-width: 0;
  padding: 5px var(--gap-2);
  background: var(--raised);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-sm);
  color: var(--text);
  font: inherit;
  font-size: 13px;
}

.solid {
  background: var(--accent);
  border-radius: var(--r-md);
  color: var(--accent-ink);
  font-weight: 500;
}

.ghost {
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  color: var(--text-muted);
}

.ghost:hover:not(:disabled) {
  color: var(--text);
  background: var(--raised-2);
}

.small {
  padding: 3px var(--gap-2);
  font-size: 12px;
}

.solid:disabled,
.ghost:disabled,
.tool:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.solid.danger {
  background: var(--bad);
  color: var(--accent-ink);
}

/* The effect deletion's question, smaller: the same state, the same look. */
.confirm {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: var(--gap-2);
  padding: var(--gap-2) var(--gap-3);
  border: 1px solid var(--bad);
  border-radius: var(--r-md);
  background: color-mix(in srgb, var(--bad) 8%, var(--raised));
}

.confirm-title {
  font-size: 13px;
  font-weight: 600;
  overflow-wrap: anywhere;
}

.confirm-actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-2);
}

.problem {
  color: var(--bad);
  font-size: 12px;
  overflow-wrap: anywhere;
}
</style>
