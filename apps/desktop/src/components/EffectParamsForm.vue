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
 */

import { computed, useId } from 'vue'
import type { ParamSpec, ParamValue, Rgb } from '@candeo/effects-api'
import type { EffectParams } from '../api/candeo'
import { sameValue } from '../composables/useSettings'
import { t } from '../i18n'
import { localized } from '../i18n/text'

const props = defineProps<{
  /** The parameters the effect declares. Empty is a normal case. */
  specs: Record<string, ParamSpec>
  /** Their current values, already complete: see `useSettings`. */
  values: EffectParams
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
}

type Field =
  | (Common & { kind: 'number'; value: number; min: number; max: number; step: number })
  | (Common & { kind: 'color'; hex: string })
  | (Common & { kind: 'boolean'; on: boolean })
  | (Common & { kind: 'choice'; value: string; options: { value: string; label: string }[] })

const fields = computed<Field[]>(() =>
  Object.entries(props.specs).map(([id, spec]): Field => {
    const head = { id, label: localized(spec.label) }
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

/** True as soon as a setting departs from the manifest: that is what can be restored. */
const touched = computed(() =>
  Object.entries(props.specs).some(([id, spec]) => {
    const v = props.values[id]
    return v !== undefined && !sameValue(v, spec.default)
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
            @input="onColor(f.id, $event)"
            @change="onColorEnd(f.id, $event)"
          />

          <input
            v-else-if="f.kind === 'boolean'"
            :id="`${uid}-${f.id}`"
            class="check"
            type="checkbox"
            :checked="f.on"
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
            @change="onChoice(f.id, $event)"
          >
            <option v-for="o in f.options" :key="o.value" :value="o.value">{{ o.label }}</option>
          </select>
        </div>

        <!--
          Inside the `fieldset`: restoring is a setting like any other, so it
          follows the same rule as the sliders when the effect is not running.
        -->
        <button v-if="touched" class="revert" type="button" @click="emit('reset')">
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

label {
  min-width: 0;
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
.fields:disabled .revert {
  cursor: not-allowed;
}
</style>
