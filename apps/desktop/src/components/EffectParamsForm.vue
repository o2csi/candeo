<script setup lang="ts">
/**
 * Les réglages d'un effet, en formulaire.
 *
 * **Un contrôle par sorte de `ParamSpec`, engendré depuis le manifeste** — rien
 * n'est écrit ici pour un effet en particulier. C'est ce qui sert le public qui
 * n'écrira jamais de code : « la vague, mais plus lente » demande un curseur,
 * pas un éditeur.
 *
 * | Sorte | Contrôle | Ce qui l'accompagne |
 * |---|---|---|
 * | `number` | curseur `min`/`max`/`step` | la valeur, en chiffres |
 * | `color` | sélecteur de couleur | le code `#rrggbb`, en toutes lettres |
 * | `boolean` | case à cocher | « activé » / « désactivé » |
 * | `choice` | liste | l'option retenue |
 *
 * ## La couleur ne porte jamais l'information seule
 *
 * Un sélecteur de couleur *est* une couleur — c'est le seul endroit où elle est
 * le sujet, et non un code. Le code hexadécimal l'accompagne donc toujours : il
 * se lit, se relève et se dicte, ce qu'une pastille ne permet pas.
 *
 * ## Pourquoi un `fieldset`, et pourquoi il porte `min-width: 0`
 *
 * L'état inerte se décide **une fois**, sur le groupe : `<fieldset disabled>`
 * neutralise tous les contrôles descendants, le bouton de rétablissement
 * compris, et un champ ajouté demain l'est sans que personne y pense — même
 * forme que le repliement des colonnes.
 *
 * Le piège est ailleurs : un `fieldset` a une largeur minimale implicite
 * (`min-width: min-content`) qu'aucune remise à plat ne supprime. Sans
 * `min-width: 0`, le plus long libellé impose sa largeur au groupe, et la
 * colonne de droite déborde au lieu de se comprimer.
 */

import { computed, useId } from 'vue'
import type { ParamSpec, ParamValue, Rgb } from '@candeo/effects-api'
import type { EffectParams } from '../api/candeo'
import { sameValue } from '../composables/useEffectParams'

const props = defineProps<{
  /** Les paramètres déclarés par l'effet. Vide est un cas normal. */
  specs: Record<string, ParamSpec>
  /** Leurs valeurs courantes, déjà complètes — voir `useEffectParams`. */
  values: EffectParams
  /**
   * Pourquoi les contrôles sont inertes, ou `null` s'ils sont vivants.
   *
   * La raison **est** le message : un formulaire grisé sans explication laisse
   * chercher ce qu'on a mal fait. Le texte dit ce qui manque et ce qui le lève.
   */
  frozen: string | null
  /** Ce qu'on affiche quand l'effet ne déclare aucun paramètre. */
  empty: string
}>()

const emit = defineEmits<{
  change: [id: string, value: ParamValue]
  reset: []
}>()

/** Préfixe d'identifiant unique : les `for` d'un formulaire doivent l'être. */
const uid = useId()

// ---------------------------------------------------------------- couleurs

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
 * Autant de décimales que le pas en demande, pas une de plus.
 *
 * Un pas de `0.5` affiche « 2.5 » ; un pas entier affiche « 120 ». Sans cela,
 * les arrondis du binaire finissent par écrire « 2.5000000000000004 » sous un
 * curseur.
 *
 * Le point et non la virgule : c'est le nombre tel que l'effet l'écrit dans son
 * manifeste, et tel qu'on le relit dans l'éditeur. Une virgule ici obligerait à
 * traduire mentalement entre les deux écrans.
 */
function decimals(step: number): number {
  const written = String(step)
  const dot = written.indexOf('.')
  return dot === -1 ? 0 : written.length - dot - 1
}

// ---------------------------------------------------------------- champs

/**
 * Un champ prêt à dessiner, la sorte déjà tranchée.
 *
 * Le tri se fait ici et non dans le patron : `ParamSpec` est une union
 * discriminée, et la restreindre dans un `v-if` de gabarit revient à écrire
 * quatre transtypages pour retrouver ce que le type disait déjà.
 */
interface Common {
  id: string
  label: string
  /** La valeur courante, en toutes lettres. */
  shown: string
}

type Field =
  | (Common & { kind: 'number'; value: number; min: number; max: number; step: number })
  | (Common & { kind: 'color'; hex: string })
  | (Common & { kind: 'boolean'; on: boolean })
  | (Common & { kind: 'choice'; value: string; options: readonly string[] })

const fields = computed<Field[]>(() =>
  Object.entries(props.specs).map(([id, spec]): Field => {
    const head = { id, label: spec.label }
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
        return { ...head, kind: 'boolean', on, shown: on ? 'activé' : 'désactivé' }
      }
      case 'choice': {
        const value = typeof v === 'string' ? v : spec.default
        return { ...head, kind: 'choice', value, options: spec.options, shown: value }
      }
    }
  }),
)

/** Vrai dès qu'un réglage s'écarte du manifeste : c'est ce qu'on peut rétablir. */
const touched = computed(() =>
  Object.entries(props.specs).some(([id, spec]) => {
    const v = props.values[id]
    return v !== undefined && !sameValue(v, spec.default)
  }),
)

// ---------------------------------------------------------------- saisies

const input = (e: Event) => e.target as HTMLInputElement

function onNumber(id: string, e: Event) {
  emit('change', id, Number(input(e).value))
}

function onColor(id: string, e: Event) {
  emit('change', id, fromHex(input(e).value))
}

function onBoolean(id: string, e: Event) {
  emit('change', id, input(e).checked)
}

function onChoice(id: string, e: Event) {
  emit('change', id, (e.target as HTMLSelectElement).value)
}
</script>

<template>
  <!--
    Sans nom accessible : la colonne qui porte ce bloc s'appelle déjà
    « Réglages », et une région imbriquée du même nom n'ajouterait qu'un doublon
    à parcourir. Le `h2` suffit à situer le bloc dans le plan du document.
  -->
  <section class="settings">
    <h2>Réglages</h2>

    <p v-if="!fields.length" class="hint">{{ empty }}</p>

    <template v-else>
      <!-- `role="status"` : la phrase apparaît et disparaît au gré de ce qui
           tourne sur l'appareil, elle n'est pas là au chargement de l'écran. -->
      <p v-if="frozen" class="hint frozen" role="status">{{ frozen }}</p>

      <fieldset class="fields" :disabled="frozen !== null">
        <div v-for="f in fields" :key="f.id" class="field">
          <div class="field-head">
            <label :for="`${uid}-${f.id}`">{{ f.label }}</label>
            <!-- Le doublage en toutes lettres : aucune information n'est portée
                 par la seule position d'un curseur ou la seule teinte d'une
                 pastille. -->
            <span class="num shown">{{ f.shown }}</span>
          </div>

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
          />

          <!--
            La seule couleur écrite par ce formulaire, et l'exception admise :
            elle représente ce que le clavier **émet**, pas un rôle d'interface.
          -->
          <input
            v-else-if="f.kind === 'color'"
            :id="`${uid}-${f.id}`"
            class="color"
            type="color"
            :value="f.hex"
            @input="onColor(f.id, $event)"
          />

          <input
            v-else-if="f.kind === 'boolean'"
            :id="`${uid}-${f.id}`"
            class="check"
            type="checkbox"
            :checked="f.on"
            @change="onBoolean(f.id, $event)"
          />

          <select
            v-else
            :id="`${uid}-${f.id}`"
            class="choice"
            :value="f.value"
            @change="onChoice(f.id, $event)"
          >
            <option v-for="o in f.options" :key="o" :value="o">{{ o }}</option>
          </select>
        </div>

        <!--
          Dans le `fieldset` : rétablir est un réglage comme un autre, et il suit
          donc la même règle que les curseurs quand l'effet ne tourne pas.
        -->
        <button v-if="touched" class="revert" type="button" @click="emit('reset')">
          Rétablir les valeurs de l'effet
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

/* Un état, pas une alerte : le filet dit « en attente », le texte dit quoi. La
   couleur ne porte rien seule. */
.frozen {
  padding: var(--gap-2) var(--gap-3);
  background: var(--raised);
  border-left: 2px solid var(--line-strong);
  border-radius: var(--r-sm);
  color: var(--text-muted);
}

/*
 * `min-width: 0` : un `fieldset` a une largeur minimale implicite, que la remise
 * à plat ne touche pas. Sans elle, le plus long libellé impose sa largeur et la
 * colonne déborde au lieu de se comprimer — à 400 px comme à 240 px.
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

.field {
  display: flex;
  flex-direction: column;
  gap: var(--gap-1);
  min-width: 0;
}

/*
 * Libellé et valeur sur la même ligne, le contrôle en dessous : c'est ce qui
 * tient dans une colonne étroite sans rien tronquer. Deux colonnes côte à côte
 * imposeraient une largeur au libellé, qui est écrit par l'effet et peut être
 * long.
 */
.field-head {
  display: flex;
  flex-wrap: wrap;
  gap: var(--gap-1) var(--gap-2);
  align-items: baseline;
  justify-content: space-between;
  min-width: 0;
}

label {
  min-width: 0;
  color: var(--text-muted);
  font-size: 12px;
  overflow-wrap: anywhere;
}

.shown {
  flex: none;
  color: var(--text);
  font-size: 12px;
}

/* Les contrôles suivent l'accent, comme le reste de l'application : c'est le
   navigateur qui dessine curseur, case et pastille, `accent-color` suffit. */
.fields input,
.fields select {
  accent-color: var(--accent);
}

.slider {
  width: 100%;

  /* Un `input` a une largeur intrinsèque : sans cela il refuse de descendre
     sous ~150 px et pousse la colonne. */
  min-width: 0;
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
  min-width: 0;
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
 * L'état inerte se lit : les contrôles s'effacent, le curseur dit non, et la
 * phrase au-dessus explique. `:disabled` porté par le `fieldset` descend sur
 * tous les contrôles — il n'y a donc qu'une règle, pas une par sorte.
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
