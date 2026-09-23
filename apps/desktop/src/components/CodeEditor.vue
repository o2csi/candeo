<script setup lang="ts">
/**
 * The code editor: a Vue wrapper around Monaco, and nothing more.
 *
 * Monaco manages its own DOM: this component only mounts it, keeps its value in
 * step with `v-model`, and unmounts it cleanly. Everything about the **language
 * service** lives in `editor/monaco.ts`: it is a global Monaco setting, not a
 * property of a component.
 */

import { onBeforeUnmount, onMounted, ref, watch } from 'vue'

import { effectModel, monaco, setupMonaco } from '../editor/monaco'

const props = defineProps<{
  modelValue: string
  /** While an installed effect is loading, an empty text is not edited. */
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
    // Follows the container's size without us having to observe it: the panel
    // resizes with the window and with the other pane.
    automaticLayout: true,
    readOnly: props.disabled === true,
    // The application's font, by its token: Monaco writes the value as is into
    // the style, so the CSS variable resolves there normally.
    fontFamily: 'var(--font-mono)',
    fontSize: 13,
    lineHeight: 20,
    tabSize: 2,
    insertSpaces: true,
    minimap: { enabled: false },
    // An effect fits on one screen: the overview does not make up for the room
    // it takes on a panel already shared with the simulator.
    scrollBeyondLastLine: false,
    padding: { top: 12, bottom: 12 },
    // The operating system's menus have no place in an application window that
    // offers neither file copy and paste nor navigation.
    contextmenu: false,
    smoothScrolling: false,
    renderLineHighlight: 'line',
    scrollbar: { verticalScrollbarSize: 10, horizontalScrollbarSize: 10 },
  })

  model.onDidChangeContent(() => emit('update:modelValue', model?.getValue() ?? ''))
})

/**
 * The text can also change from outside: a restored draft, a source read back
 * from disk.
 *
 * The comparison is not an optimization, it is what prevents the loop: every
 * keystroke goes up through `update:modelValue` and comes back down here, and an
 * unconditional `setValue` would put the cursor back at the start on every
 * character.
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
  // The model is disposed with the editor: its URI must be free again, or
  // reopening the editor would fail to create a second one under the same URI.
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
 * `base.css` forbids selection across the whole application: an application
 * window is not selected like a page. The editor is the exception that warrants
 * it: text is selected there to move it.
 */
.code :deep(.monaco-editor),
.code :deep(.monaco-editor) * {
  user-select: text;
}
</style>
