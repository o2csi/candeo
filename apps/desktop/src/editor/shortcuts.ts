/**
 * The editor's keyboard shortcuts worth showing: Monaco's own bindings for
 * the contributions `monaco.ts` loads, Ctrl+S from `CodeEditor.vue`, and F1
 * from `EditorShortcuts.vue`.
 *
 * ## The keyboard's layout, not the interface's language
 *
 * Monaco listens to keys, not to the characters they print (Windows' virtual
 * key codes), so every binding works on any layout by itself. What changes is
 * the label: a letter is the key printed with it everywhere, but Ctrl+/ is
 * the key Windows calls `VK_OEM_2`, which prints `:` on AZERTY — and Monaco
 * labels everything as on a US keyboard. {@link commentKey} gives that one key
 * its name, from the layout the web view reads.
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
  { keys: 'F1', action: 'help' },
] as const

export type Shortcut = (typeof SHORTCUTS)[number]

/** What the layout prints, by physical key (`KeyQ`, `Period`), as the web view reads it. */
export type LayoutMap = ReadonlyMap<string, string>

/**
 * What `VK_OEM_2` prints: `:` on AZERTY (French, Belgian), `#` on German
 * QWERTZ — as VS Code labels Ctrl+/ there — and `/` elsewhere, or when the
 * layout cannot be read.
 */
export function commentKey(layout: LayoutMap | null): string {
  if (layout?.get('Period') === ':') return ':'
  if (layout?.get('KeyY') === 'z' && layout.get('Backquote') === '^') return '#'
  return '/'
}

/** A shortcut's keys as this keyboard prints them. */
export function keysOf(shortcut: Shortcut, layout: LayoutMap | null): string {
  return shortcut.action === 'comment' ? `Ctrl+${commentKey(layout)}` : shortcut.keys
}

/**
 * The layout, read once: the Keyboard API of Chromium's web view. `null` where
 * there is none (WebKitGTK) or it refuses; the labels are then US ones.
 */
let reading: Promise<LayoutMap | null> | null = null

export function readLayout(): Promise<LayoutMap | null> {
  const keyboard = (
    navigator as Navigator & { keyboard?: { getLayoutMap(): Promise<LayoutMap> } }
  ).keyboard
  reading ??= keyboard ? keyboard.getLayoutMap().catch(() => null) : Promise.resolve(null)
  return reading
}
