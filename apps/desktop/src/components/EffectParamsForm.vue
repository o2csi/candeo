<script setup lang="ts">
/**
 * An effect's settings, as a form.
 *
 * **One control per kind of `ParamSpec`, generated from the manifest**: nothing
 * here is written for a particular effect. That is what serves the audience who
 * will never write code: "the wave, but slower" calls for a slider, not an
 * editor.
 *
 * | Kind | Control | What goes with it |
 * |---|---|---|
 * | `number` | `min`/`max`/`step` slider | the value, in figures |
 * | `color` | color picker | the `#rrggbb` code, spelled out |
 * | `boolean` | checkbox | "on" / "off" |
 * | `choice` | list | the selected option |
 *
 * ## Color never carries the information alone
 *
 * A color picker *is* a color: it is the only place where color is the subject,
 * and not a code. So the hexadecimal code always goes with it: it can be read,
 * noted down and dictated, which a patch of color does not allow.
 *
 * ## Why a `fieldset`, and why it carries `min-width: 0`
 *
 * The inert state is decided **once**, on the group: `<fieldset disabled>`
 * disables every descendant control, the restore button included, and a field
 * added tomorrow is too without anyone thinking about it: the same shape as the
 * column collapsing.
 *
 * The trap is elsewhere: a `fieldset` has an implicit minimum width
 * (`min-width: min-content`) that no reset removes. Without `min-width: 0`, the
 * longest label imposes its width on the group, and the right-hand column
 * overflows instead of shrinking.
 *
 * ## A value or a signal
 *
 * A parameter can read a signal instead of its value
 * (`docs/design/inputs-and-automations.md` §2.3.1), and it is written here, in
 * the parameter's own row: this form is the gallery's and a rule's, so both get
 * it by it existing once. The row's switch is the pattern the cron field settled
 * (§3.2): one of the two holds the parameter, the other is shown disabled. The
 * value stays visible because it is still used: the effect shows it while the
 * signal is absent or does not fit.
 */

import { computed, nextTick, onBeforeUnmount, reactive, useId, watch } from 'vue'
import type { ParamSpec, ParamValue, Rgb } from '@candeo/effects-api'
import type { Bindings, EffectParams, HeldSignal } from '../api/candeo'
import { bindingState, boundSignal, signalSource, type BindingState } from '../composables/bindings'
import { validSignalName } from '../composables/rules'
import { signalText } from '../composables/signals'
import { sameValue } from '../composables/useSettings'
import { t } from '../i18n'
import { localized } from '../i18n/text'

const props = defineProps<{
  /** The parameters the effect declares. Empty is a normal case. */
  specs: Record<string, ParamSpec>
  /** Their current values, already complete: see `useSettings`. */
  values: EffectParams
  /** The parameters reading a signal, `{ colour: 'signal:status' }`: see `useSettings`. */
  bindings: Bindings
  /**
   * Whether a parameter can read a signal. Not for an effect the firmware runs:
   * no loop of the application reads anything for it.
   */
  bindable: boolean
  /** The signals held now: their names are suggested, and a bound row says what its signal holds. */
  signals: readonly HeldSignal[]
  /**
   * Why the controls are inert, or `null` if they are live.
   *
   * The reason **is** the message: a greyed-out form with no explanation leaves
   * people looking for what they did wrong. The text says what is missing and
   * what lifts it.
   */
  frozen: string | null
  /** What is shown when the effect declares no parameter. */
  empty: string
}>()

const emit = defineEmits<{
  /** The value moves: continuously while a slider is dragged. */
  change: [id: string, value: ParamValue]
  /**
   * The gesture is over: slider released, box checked, option chosen.
   *
   * Separate from `change` because the two are not addressed to the same place:
   * `change` feeds the render loop on the fly, `commit` says it is time to write
   * to disk. Without it, the only guarantee would be a timer that closing the
   * window would take away.
   */
  commit: []
  /**
   * A parameter reads a signal, `signal:<name>`, or its value again (`null`).
   * Once a name is written, never while it is typed: each one goes to disk.
   */
  bind: [id: string, source: string | null]
  /** Back to what the effect declares: its values, and no parameter reading a signal. */
  reset: []
}>()

/** Unique identifier prefix: a form's `for` attributes must be unique. */
const uid = useId()

// ---------------------------------------------------------------- colors

const byte = (n: number) =>
  Math.max(0, Math.min(255, Math.round(n)))
    .toString(16)
    .padStart(2, '0')

const toHex = (c: Rgb) => `#${byte(c.r)}${byte(c.g)}${byte(c.b)}`

const fromHex = (hex: string): Rgb => ({
  r: parseInt(hex.slice(1, 3), 16),
  g: parseInt(hex.slice(3, 5), 16),
  b: parseInt(hex.slice(5, 7), 16),
})

/**
 * As many decimals as the step calls for, not one more.
 *
 * A step of `0.5` shows "2.5"; an integer step shows "120". Without this, binary
 * rounding ends up writing "2.5000000000000004" under a slider.
 *
 * A dot and not a comma: it is the number as the effect writes it in its
 * manifest, and as it is read back in the editor. A comma here would force a
 * mental translation between the two screens.
 */
function decimals(step: number): number {
  const written = String(step)
  const dot = written.indexOf('.')
  return dot === -1 ? 0 : written.length - dot - 1
}

// ---------------------------------------------------------------- signals

/**
 * Rows switched to Signal whose name is not written yet. Nothing is bound: the
 * value, shown disabled, still holds, as it does for a signal not received.
 */
const pending = reactive(new Set<string>())

/**
 * A row's name that is not what it reads: typed and not written yet, refused,
 * or kept from the signal the row stopped reading, so that switching back to
 * Signal reads it again.
 */
const drafts = reactive<Record<string, string>>({})

/** Rows whose name cannot be one, to say so beside the field. */
const refused = reactive(new Set<string>())

function clearDrafts(): void {
  pending.clear()
  refused.clear()
  for (const id of Object.keys(drafts)) delete drafts[id]
}

// Another effect, or the same one installed again: what was typed for its rows
// is not about these. The gallery keeps one form for every effect.
watch(() => props.specs, clearDrafts)

/** The signal a row reads, or `null` while it holds its value. */
function readBy(id: string): string | null {
  const source = props.bindable ? props.bindings[id] : undefined
  return source === undefined ? null : boundSignal(source)
}

/** The row is on Signal: reading one, or waiting for its name. */
function signalChosen(id: string): boolean {
  return readBy(id) !== null || pending.has(id)
}

function reads(state: BindingState, name: string): string {
  switch (state.kind) {
    case 'absent':
      return t('effects.params.signalAbsent', { name })
    case 'fits':
      return t('effects.params.signalHeld', { value: state.value })
    case 'unfit':
      return t('effects.params.signalUnfit', { value: state.value })
  }
}

// ---------------------------------------------------------------- fields

/**
 * A field ready to draw, its kind already settled.
 *
 * The sorting happens here and not in the template: `ParamSpec` is a
 * discriminated union, and narrowing it in a template `v-if` amounts to writing
 * four casts to get back what the type already said.
 */
interface Common {
  id: string
  label: string
  /** The current value, spelled out. */
  shown: string
  /** What holds the parameter: its value, or a signal. The other is shown disabled. */
  source: 'value' | 'signal'
  /** The name in the signal field. */
  name: string
  /** What the signal read holds now, spelled out; `null` while none is read. */
  reads: string | null
  /** The name typed cannot be one. */
  refused: boolean
}

type Field =
  | (Common & { kind: 'number'; value: number; min: number; max: number; step: number })
  | (Common & { kind: 'color'; hex: string })
  | (Common & { kind: 'boolean'; on: boolean })
  | (Common & { kind: 'choice'; value: string; options: { value: string; label: string }[] })

const fields = computed<Field[]>(() =>
  Object.entries(props.specs).map(([id, spec]): Field => {
    const signal = readBy(id)
    const head = {
      id,
      label: localized(spec.label),
      source: signalChosen(id) ? ('signal' as const) : ('value' as const),
      name: drafts[id] ?? signal ?? '',
      reads: signal === null ? null : reads(bindingState(spec, signal, props.signals), signal),
      refused: refused.has(id),
    }
    const v = props.values[id] ?? spec.default

    switch (spec.kind) {
      case 'number': {
        const step = spec.step ?? 1
        const value = typeof v === 'number' ? v : spec.default
        return {
          ...head,
          kind: 'number',
          value,
          min: spec.min,
          max: spec.max,
          step,
          shown: value.toFixed(decimals(step)),
        }
      }
      case 'color': {
        const hex = toHex(typeof v === 'object' ? v : spec.default)
        return { ...head, kind: 'color', hex, shown: hex }
      }
      case 'boolean': {
        const on = typeof v === 'boolean' ? v : spec.default
        const shown = on ? t('effects.params.on') : t('effects.params.off')
        return { ...head, kind: 'boolean', on, shown }
      }
      case 'choice': {
        const value = typeof v === 'string' ? v : spec.default
        const options = spec.options.map((o) =>
          typeof o === 'string' ? { value: o, label: o } : { value: o.value, label: localized(o.label) },
        )
        const shown = options.find((o) => o.value === value)?.label ?? value
        return { ...head, kind: 'choice', value, options, shown }
      }
    }
  }),
)

/**
 * What the live region says, or nothing.
 *
 * Empty when the effect declares no parameter: the "none" message is there when
 * the screen loads, it is nothing like a change to announce.
 */
const announced = computed(() => (fields.value.length ? (props.frozen ?? '') : ''))

/**
 * True as soon as a setting departs from the manifest, a value or a signal read:
 * that is what can be restored.
 */
const touched = computed(() =>
  Object.entries(props.specs).some(([id, spec]) => {
    const v = props.values[id]
    return (v !== undefined && !sameValue(v, spec.default)) || readBy(id) !== null
  }),
)

// ---------------------------------------------------------------- inputs

const input = (e: Event) => e.target as HTMLInputElement

function onNumber(id: string, e: Event) {
  emit('change', id, Number(input(e).value))
}

function onColor(id: string, e: Event) {
  emit('change', id, fromHex(input(e).value))
}

// The end of a gesture reads the value again before saying "write". Emitting
// `commit` alone would assume an `input` has just gone by: true for a drag, not
// guaranteed for a color picker, whose system dialog may only give its verdict
// on `change`.
function onNumberEnd(id: string, e: Event) {
  onNumber(id, e)
  emit('commit')
}

function onColorEnd(id: string, e: Event) {
  onColor(id, e)
  emit('commit')
}

// A checkbox and a list have no intermediate state: their `change` is both the
// movement and the end of the gesture.
function onBoolean(id: string, e: Event) {
  emit('change', id, input(e).checked)
  emit('commit')
}

function onChoice(id: string, e: Event) {
  emit('change', id, (e.target as HTMLSelectElement).value)
  emit('commit')
}

// ---------------------------------------------------------------- binding

/** Back to the value. The name stays in the field for a switch back to Signal. */
function toValue(id: string): void {
  pending.delete(id)
  refused.delete(id)
  const signal = readBy(id)
  if (signal === null) return
  drafts[id] ??= signal
  emit('bind', id, null)
}

/**
 * To a signal: the name the row had, when it has one, is read again at once;
 * otherwise the field waits for one, and the value holds meanwhile.
 */
function toSignal(id: string): void {
  if (signalChosen(id)) return
  const name = (drafts[id] ?? '').trim()
  if (validSignalName(name)) {
    delete drafts[id]
    emit('bind', id, signalSource(name))
    return
  }
  pending.add(id)
  void nextTick(() => document.getElementById(`${uid}-${id}-signal`)?.focus())
}

/**
 * How long typing pauses before the name is taken. Enter and leaving the field
 * take it at once; this is for neither: in the first try in the application, a
 * name typed and left as it was bound nothing, and the setting looked broken.
 */
const NAME_PAUSE_MS = 700
const pauses = new Map<string, ReturnType<typeof setTimeout>>()

function onName(id: string, e: Event) {
  drafts[id] = input(e).value
  clearTimeout(pauses.get(id))
  pauses.set(
    id,
    setTimeout(() => onNameEnd(id), NAME_PAUSE_MS),
  )
}

onBeforeUnmount(() => {
  for (const pause of pauses.values()) clearTimeout(pause)
})

/**
 * The name is written: Enter, or leaving the field. A name never received is
 * bound all the same, since a setting is often bound before its sender runs;
 * one that cannot be a name stays in the field, said refused. Emptied, the field
 * reads nothing and the row stays on Signal, waiting for a name.
 */
function onNameEnd(id: string) {
  clearTimeout(pauses.get(id))
  pauses.delete(id)
  const name = (drafts[id] ?? readBy(id) ?? '').trim()
  if (name !== '' && !validSignalName(name)) {
    refused.add(id)
    return
  }
  refused.delete(id)
  delete drafts[id]
  if (name === '') {
    pending.add(id)
    if (readBy(id) !== null) emit('bind', id, null)
    return
  }
  pending.delete(id)
  if (name !== readBy(id)) emit('bind', id, signalSource(name))
}

function onReset() {
  clearDrafts()
  emit('reset')
}
</script>

<template>
  <!--
    No accessible name: the column holding this block is already called
    "Settings", and a nested region of the same name would only add a duplicate
    to go through. The `h2` is enough to place the block in the document outline.
  -->
  <section class="settings">
    <h2>{{ t('effects.params.title') }}</h2>

    <!--
      The live region, **always mounted**: outside any `v-if`, including the one
      that tells "no parameter" apart from the form.

      A screen reader only reliably announces a live region already present in
      the document whose content changes; inserted at the same time as its text,
      it often stays silent. Placing it under the `v-else` remounted it on every
      switch from an effect without parameters to one that declares some:
      exactly the case it was meant to serve.

      It costs nothing in layout: `.sr-only` is `position: absolute`, so it is
      not even a flex item and no spacing is added.
    -->
    <p class="sr-only" role="status">{{ announced }}</p>

    <p v-if="!fields.length" class="hint">{{ empty }}</p>

    <template v-else>
      <!--
        The visible sentence, and nothing more: `aria-hidden` because the live
        region above already carries the same text, and it would be read twice.
      -->
      <p v-if="frozen" class="hint frozen" aria-hidden="true">{{ frozen }}</p>

      <fieldset class="fields" :disabled="frozen !== null">
        <div v-for="f in fields" :key="f.id" class="field">
          <div class="field-head">
            <label :for="`${uid}-${f.id}`">{{ f.label }}</label>
            <!-- The value spelled out as well: no information is carried by
                 a slider's position alone or a patch's hue alone. -->
            <span class="num shown">{{ f.shown }}</span>
            <!--
              Two buttons rather than two radios: a radio keeps its own checked
              state, and a binding the parent does not take (no device to keep
              it for) would leave it checked while the row says otherwise.
            -->
            <span
              v-if="bindable"
              class="source"
              role="group"
              :aria-label="t('effects.params.source', { name: f.label })"
            >
              <button
                type="button"
                class="seg"
                :aria-pressed="f.source === 'value'"
                @click="toValue(f.id)"
              >
                {{ t('effects.params.fromValue') }}
              </button>
              <button
                type="button"
                class="seg"
                :aria-pressed="f.source === 'signal'"
                @click="toSignal(f.id)"
              >
                {{ t('effects.params.fromSignal') }}
              </button>
            </span>
          </div>

          <!--
            `input` while dragging, `change` on release: the first feeds the
            render loop, the second triggers the disk write.
          -->
          <input
            v-if="f.kind === 'number'"
            :id="`${uid}-${f.id}`"
            class="slider"
            type="range"
            :min="f.min"
            :max="f.max"
            :step="f.step"
            :value="f.value"
            :disabled="f.source === 'signal'"
            @input="onNumber(f.id, $event)"
            @change="onNumberEnd(f.id, $event)"
          />

          <!--
            The only color this form writes, and the allowed exception: it
            stands for what the keyboard **emits**, not an interface role.
          -->
          <input
            v-else-if="f.kind === 'color'"
            :id="`${uid}-${f.id}`"
            class="color"
            type="color"
            :value="f.hex"
            :disabled="f.source === 'signal'"
            @input="onColor(f.id, $event)"
            @change="onColorEnd(f.id, $event)"
          />

          <input
            v-else-if="f.kind === 'boolean'"
            :id="`${uid}-${f.id}`"
            class="check"
            type="checkbox"
            :checked="f.on"
            :disabled="f.source === 'signal'"
            @change="onBoolean(f.id, $event)"
          />

          <!--
            `:value` is enough, and that is not obvious. Two effects can declare
            a `choice` with the same identifier and different options: the
            `v-for` then reuses this `<select>`, and if the current value is the
            same, one could believe nothing is rewritten: the `<option>`s would
            be replaced and the browser would fall back on the first one,
            showing a selection nothing in the state says. Vue avoids it twice:
            children are rendered before properties, and `value` is **always**
            passed again, even unchanged, then compared with the live
            `el.value` and not with the old property. A fallback key was tried
            and then removed: it defended nothing, and two distinct option lists
            could share it.
          -->
          <select
            v-else
            :id="`${uid}-${f.id}`"
            class="choice"
            :value="f.value"
            :disabled="f.source === 'signal'"
            @change="onChoice(f.id, $event)"
          >
            <option v-for="o in f.options" :key="o.value" :value="o.value">{{ o.label }}</option>
          </select>

          <div v-if="f.source === 'signal'" class="bind">
            <input
              :id="`${uid}-${f.id}-signal`"
              class="signal mono"
              type="text"
              spellcheck="false"
              autocomplete="off"
              :list="`${uid}-held`"
              :placeholder="t('effects.params.signalName')"
              :aria-label="t('effects.params.signalOf', { name: f.label })"
              :aria-invalid="f.refused"
              :value="f.name"
              @input="onName(f.id, $event)"
              @change="onNameEnd(f.id)"
            />
            <p v-if="f.refused" class="reads bad" role="alert">
              {{ t('errors.signalNameInvalid', { name: f.name.trim() }) }}
            </p>
            <!-- Not a live region: a signal can change several times a second,
                 and each change would be read out. -->
            <p v-else-if="f.reads" class="reads">{{ f.reads }}</p>
          </div>
        </div>

        <!-- One list for every row: the signals held now, each with its value. -->
        <datalist v-if="bindable" :id="`${uid}-held`">
          <option v-for="s in signals" :key="s.name" :value="s.name" :label="signalText(s.value)" />
        </datalist>

        <!--
          Inside the `fieldset`: restoring is a setting like any other, so it
          follows the same rule as the sliders when the effect is not running.
        -->
        <button v-if="touched" class="revert" type="button" @click="onReset">
          {{ t('effects.params.reset') }}
        </button>
      </fieldset>
    </template>
  </section>
</template>

<style scoped>
.settings {
  display: flex;
  flex-direction: column;
  gap: var(--gap-2);
  padding-top: var(--gap-3);
  border-top: 1px solid var(--line);
}

.settings h2 {
  color: var(--text-faint);
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.hint {
  max-width: 68ch;
  color: var(--text-faint);
  font-size: 12px;
}

/* A state, not an alert: the rule says "waiting", the text says what for. Color
   carries nothing alone. */
.frozen {
  padding: var(--gap-2) var(--gap-3);
  background: var(--raised);
  border-left: 2px solid var(--line-strong);
  border-radius: var(--r-sm);
  color: var(--text-muted);
}

/*
 * `min-width: 0`: a `fieldset` has an implicit minimum width, which the reset
 * does not touch. Without it, the longest label imposes its width and the
 * column overflows instead of shrinking, at 400 px as at 240 px.
 */
.fields {
  display: flex;
  flex-direction: column;
  gap: var(--gap-3);
  min-width: 0;
  margin: 0;
  padding: 0;
  border: 0;
}

/*
 * No `min-width: 0` on the children of a flex column: width is the cross axis
 * there, where `min-width: auto` is already zero. Only the `fieldset`, which
 * carries its own minimum width, and the label, a flex item of a row, need it.
 */
.field {
  display: flex;
  flex-direction: column;
  gap: var(--gap-1);
}

/*
 * Label and value on the same line, the control below: that is what fits in a
 * narrow column without truncating anything. Two columns side by side would
 * impose a width on the label, which is written by the effect and can be long.
 */
.field-head {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-1) var(--gap-2);
  align-items: baseline;
  justify-content: space-between;
}

/* The label takes the room left: the value and the switch keep to the right. */
label {
  min-width: 0;
  margin-right: auto;
  color: var(--text-muted);
  font-size: 12px;
  overflow-wrap: anywhere;
}

/*
 * Same treatment as the label, and for the same reason: on a `choice`, the value
 * shown is the option **as the effect declares it**. A somewhat long unbroken
 * option would otherwise push the row out of the column, and `.detail` would get
 * a horizontal scroll bar.
 */
.shown {
  min-width: 0;
  color: var(--text);
  font-size: 12px;
  text-align: right;
  overflow-wrap: anywhere;
}

/* The controls follow the accent, like the rest of the application: the browser
   draws the slider, checkbox and color patch, `accent-color` is enough. */
.fields input,
.fields select {
  accent-color: var(--accent);
}

/* `width: 100%` and not an `input`'s intrinsic width, which is about 150 px:
   that is what lets the slider follow the column when it narrows. */
.slider {
  width: 100%;
  margin: 0;
}

.color {
  width: 56px;
  height: 26px;
  padding: 2px;
  background: var(--raised);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-sm);
  cursor: pointer;
}

.check {
  width: 16px;
  height: 16px;
  margin: 2px 0;
}

.choice {
  width: 100%;
  padding: 5px var(--gap-2);
  background: var(--raised);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-sm);
  color: var(--text);
  font: inherit;
  font-size: 13px;
}

/*
 * Value | Signal, as one small control. No `overflow: hidden` to round the
 * pair: it would clip the focus ring, which is drawn outside each button.
 */
.source {
  display: inline-flex;
  align-self: center;
}

.seg {
  padding: 1px var(--gap-2);
  border: 1px solid var(--line-strong);
  color: var(--text-faint);
  font-size: 11px;
}

.seg:first-child {
  border-radius: var(--r-sm) 0 0 var(--r-sm);
}

.seg:last-child {
  border-left: 0;
  border-radius: 0 var(--r-sm) var(--r-sm) 0;
}

.seg:hover:not(:disabled, [aria-pressed='true']) {
  color: var(--text);
  background: var(--raised-2);
}

.seg[aria-pressed='true'] {
  color: var(--accent);
  background: var(--accent-soft);
}

.bind {
  display: flex;
  flex-direction: column;
  gap: var(--gap-1);
}

.signal {
  width: 100%;
  padding: 4px var(--gap-2);
  background: var(--raised);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-sm);
  color: var(--text);
  font-size: 12px;
}

.signal[aria-invalid='true'] {
  border-color: var(--bad);
}

/* A signal's value is what a sender wrote, up to 256 characters: it wraps. */
.reads {
  color: var(--text-muted);
  font-size: 12px;
  overflow-wrap: anywhere;
}

.reads.bad {
  color: var(--bad);
}

/*
 * A value a signal holds back: faded as the inert form is, but alone. Not under
 * an inert `fieldset`, which already fades everything: the two would multiply.
 */
.fields:not(:disabled) :is(.slider, .color, .check, .choice):disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.revert {
  align-self: flex-start;
  padding: 4px var(--gap-2);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-md);
  color: var(--text-muted);
  font-size: 12px;
}

.revert:hover:not(:disabled) {
  color: var(--text);
  background: var(--raised-2);
}

/*
 * The inert state can be read: the controls fade, the pointer says no, and the
 * sentence above explains. `:disabled` carried by the `fieldset` goes down to
 * every control, so there is only one rule, not one per kind.
 */
.fields:disabled {
  opacity: 0.5;
}

.fields:disabled input,
.fields:disabled select,
.fields:disabled .seg,
.fields:disabled .revert {
  cursor: not-allowed;
}
</style>
