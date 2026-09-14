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
import { sameValue } from '../composables/useSettings'
import { t } from '../i18n'
import { localized } from '../i18n/text'

const props = defineProps<{
  /** Les paramètres déclarés par l'effet. Vide est un cas normal. */
  specs: Record<string, ParamSpec>
  /** Leurs valeurs courantes, déjà complètes — voir `useSettings`. */
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
  /** La valeur bouge — en continu pendant qu'on glisse un curseur. */
  change: [id: string, value: ParamValue]
  /**
   * Le geste est terminé : curseur relâché, case cochée, option choisie.
   *
   * Séparé de `change` parce que les deux ne s'adressent pas au même endroit :
   * `change` alimente la boucle de rendu à la volée, `commit` dit qu'il est
   * temps d'écrire sur disque. Sans lui, la seule garantie serait une minuterie
   * que fermer la fenêtre emporterait.
   */
  commit: []
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
 * Ce que la région d'annonce dit, ou rien.
 *
 * Vide quand l'effet ne déclare aucun paramètre : le message d'absence est là au
 * chargement de l'écran, il n'a rien d'un changement à signaler.
 */
const announced = computed(() => (fields.value.length ? (props.frozen ?? '') : ''))

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

// La fin d'un geste relit la valeur avant de dire « écris ». Émettre `commit`
// seul supposerait qu'un `input` vient de passer — vrai pour un glissement, pas
// garanti pour un sélecteur de couleur, dont la boîte de dialogue système peut
// ne rendre son verdict qu'au `change`.
function onNumberEnd(id: string, e: Event) {
  onNumber(id, e)
  emit('commit')
}

function onColorEnd(id: string, e: Event) {
  onColor(id, e)
  emit('commit')
}

// Une case et une liste n'ont pas d'état intermédiaire : leur `change` est à la
// fois le mouvement et la fin du geste.
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
    Sans nom accessible : la colonne qui porte ce bloc s'appelle déjà
    « Réglages », et une région imbriquée du même nom n'ajouterait qu'un doublon
    à parcourir. Le `h2` suffit à situer le bloc dans le plan du document.
  -->
  <section class="settings">
    <h2>{{ t('effects.params.title') }}</h2>

    <!--
      La région d'annonce, **montée en permanence** — hors de tout `v-if`, y
      compris celui qui distingue « aucun paramètre » du formulaire.

      Un lecteur d'écran n'annonce de façon fiable qu'une région vivante déjà
      présente dans le document, dont le contenu change ; insérée en même temps
      que son texte, elle reste souvent muette. La placer sous le `v-else` la
      remontait à chaque passage d'un effet sans paramètre à un effet qui en
      déclare — exactement le cas qu'elle devait servir.

      Elle ne coûte rien en mise en page : `.sr-only` est en `position: absolute`,
      donc ce n'est même pas un élément flexible et aucun espacement ne s'ajoute.
    -->
    <p class="sr-only" role="status">{{ announced }}</p>

    <p v-if="!fields.length" class="hint">{{ empty }}</p>

    <template v-else>
      <!--
        La phrase visible, et rien de plus : `aria-hidden` parce que la région
        d'annonce ci-dessus porte déjà le même texte, et qu'il serait lu deux
        fois.
      -->
      <p v-if="frozen" class="hint frozen" aria-hidden="true">{{ frozen }}</p>

      <fieldset class="fields" :disabled="frozen !== null">
        <div v-for="f in fields" :key="f.id" class="field">
          <div class="field-head">
            <label :for="`${uid}-${f.id}`">{{ f.label }}</label>
            <!-- Le doublage en toutes lettres : aucune information n'est portée
                 par la seule position d'un curseur ou la seule teinte d'une
                 pastille. -->
            <span class="num shown">{{ f.shown }}</span>
          </div>

          <!--
            `input` pendant le glissement, `change` au relâchement : le premier
            alimente la boucle de rendu, le second déclenche l'écriture disque.
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
            `:value` suffit, et ce n'est pas évident. Deux effets peuvent
            déclarer un `choice` de même identifiant avec des options
            différentes : le `v-for` réutilise alors ce `<select>`, et si la
            valeur courante est identique, on pourrait croire que rien n'est
            réécrit — les `<option>` seraient remplacés et le navigateur
            retomberait sur le premier, affichant une sélection que rien dans
            l'état ne dit. Vue l'évite deux fois : les enfants sont rendus avant
            les propriétés, et `value` est **toujours** repassée, même inchangée,
            puis comparée au `el.value` vivant et non à l'ancienne propriété.
            Une clé de secours a été essayée puis retirée : elle ne défendait
            rien, et deux listes d'options distinctes pouvaient la partager.
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
          Dans le `fieldset` : rétablir est un réglage comme un autre, et il suit
          donc la même règle que les curseurs quand l'effet ne tourne pas.
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

/*
 * Pas de `min-width: 0` sur les enfants d'une colonne flexible : la largeur y
 * est l'axe secondaire, où `min-width: auto` vaut déjà zéro. Seuls le `fieldset`
 * — qui porte sa propre largeur minimale — et le libellé, élément flexible d'une
 * ligne, en ont besoin.
 */
.field {
  display: flex;
  flex-direction: column;
  gap: var(--gap-1);
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
}

label {
  min-width: 0;
  color: var(--text-muted);
  font-size: 12px;
  overflow-wrap: anywhere;
}

/*
 * Même traitement que le libellé, et pour la même raison : sur un `choice`, la
 * valeur affichée est l'option **telle que l'effet la déclare**. Une option d'un
 * seul tenant un peu longue pousserait sinon la ligne hors de la colonne, et
 * `.detail` gagnerait une barre de défilement horizontale.
 */
.shown {
  min-width: 0;
  color: var(--text);
  font-size: 12px;
  text-align: right;
  overflow-wrap: anywhere;
}

/* Les contrôles suivent l'accent, comme le reste de l'application : c'est le
   navigateur qui dessine curseur, case et pastille, `accent-color` suffit. */
.fields input,
.fields select {
  accent-color: var(--accent);
}

/* `width: 100%` et non la largeur intrinsèque d'un `input`, qui vaut environ
   150 px : c'est ce qui laisse le curseur suivre la colonne quand elle se
   resserre. */
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
