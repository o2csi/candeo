/**
 * The editor's keyboard shortcuts worth showing: Monaco's own bindings for
 * the contributions `monaco.ts` loads, and Ctrl+S from `CodeEditor.vue`.
 *
 * Written as Windows and Linux show them: those are the platforms Candeo
 * ships on.
 */
export const SHORTCUTS = [
  { keys: 'Ctrl+S', action: 'save' },
  { keys: 'Shift+Alt+F', action: 'format' },
  { keys: 'Ctrl+Space', action: 'complete' },
  { keys: 'Ctrl+F', action: 'find' },
  { keys: 'Ctrl+H', action: 'replace' },
  { keys: 'Ctrl+/', action: 'comment' },
  { keys: 'Alt+↑ · Alt+↓', action: 'moveLine' },
  { keys: 'Ctrl+D', action: 'nextOccurrence' },
] as const
