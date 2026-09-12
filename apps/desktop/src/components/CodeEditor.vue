<script setup lang="ts">
/**
 * L'éditeur de code — une enveloppe Vue autour de Monaco, et rien de plus.
 *
 * Monaco gère lui-même son DOM : ce composant ne fait que le monter, tenir sa
 * valeur en phase avec `v-model`, et le démonter proprement. Tout ce qui
 * concerne le **service de langage** vit dans `editor/monaco.ts` — c'est un
 * réglage global de Monaco, pas une propriété d'un composant.
 */

import { onBeforeUnmount, onMounted, ref, watch } from 'vue'

import { effectModel, monaco, setupMonaco } from '../editor/monaco'

const props = defineProps<{
  modelValue: string
  /** Pendant le chargement d'un effet installé, on n'édite pas un texte vide. */
  disabled?: boolean
}>()

const emit = defineEmits<{ 'update:modelValue': [value: string] }>()

const host = ref<HTMLElement | null>(null)

let editor: monaco.editor.IStandaloneCodeEditor | null = null
let model: monaco.editor.ITextModel | null = null

onMounted(() => {
  setupMonaco()
  model = effectModel(props.modelValue)

  editor = monaco.editor.create(host.value as HTMLElement, {
    model,
    // Suit la taille du conteneur sans qu'on ait à l'observer nous-mêmes : le
    // panneau se redimensionne avec la fenêtre et avec l'autre volet.
    automaticLayout: true,
    readOnly: props.disabled === true,
    // La police de l'application, par son jeton : Monaco écrit la valeur telle
    // quelle dans le style, la variable CSS y est donc résolue normalement.
    fontFamily: 'var(--font-mono)',
    fontSize: 13,
    lineHeight: 20,
    tabSize: 2,
    insertSpaces: true,
    minimap: { enabled: false },
    // Un effet tient en un écran : la vue d'ensemble ne compense pas la place
    // qu'elle prend sur un panneau déjà partagé avec le simulateur.
    scrollBeyondLastLine: false,
    padding: { top: 12, bottom: 12 },
    // Les menus du système d'exploitation n'ont pas cours dans une fenêtre
    // d'application qui n'offre ni copier-coller de fichiers ni navigation.
    contextmenu: false,
    smoothScrolling: false,
    renderLineHighlight: 'line',
    scrollbar: { verticalScrollbarSize: 10, horizontalScrollbarSize: 10 },
  })

  model.onDidChangeContent(() => emit('update:modelValue', model?.getValue() ?? ''))
})

/**
 * Le texte peut aussi changer de l'extérieur : brouillon restauré, source relue
 * au disque.
 *
 * La comparaison n'est pas une optimisation, c'est ce qui empêche la boucle :
 * chaque frappe remonte par `update:modelValue` et redescend ici, et un
 * `setValue` inconditionnel replacerait le curseur au début à chaque caractère.
 */
watch(
  () => props.modelValue,
  (value) => {
    if (model && model.getValue() !== value) model.setValue(value)
  },
)

watch(
  () => props.disabled,
  (value) => editor?.updateOptions({ readOnly: value === true }),
)

onBeforeUnmount(() => {
  editor?.dispose()
  editor = null
  // Le modèle est disposé avec l'éditeur : son URI doit redevenir libre, sinon
  // rouvrir l'éditeur échouerait à en créer un second sous la même.
  model?.dispose()
  model = null
})
</script>

<template>
  <div ref="host" class="code" />
</template>

<style scoped>
.code {
  width: 100%;
  height: 100%;
  min-height: 0;
}

/*
 * `base.css` interdit la sélection dans toute l'application — une fenêtre
 * d'application ne se sélectionne pas comme une page. L'éditeur est l'exception
 * qui le justifie : on y sélectionne du texte pour le déplacer.
 */
.code :deep(.monaco-editor),
.code :deep(.monaco-editor) * {
  user-select: text;
}
</style>
