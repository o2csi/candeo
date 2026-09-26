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
 * its name: the character it was last seen typing, or else the layout the web
 * view reads — WebView2 does not give it.
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
 * What `VK_OEM_2` prints: the character it typed last, when it has been seen;
 * else `:` on AZERTY (French, Belgian), `#` on German QWERTZ — as VS Code
 * labels Ctrl+/ there — and `/` elsewhere, or when the layout cannot be read.
 */
export function commentKey(layout: LayoutMap | null, learned: string | null = null): string {
  if (learned) return learned
  if (layout?.get('Period') === ':') return ':'
  if (layout?.get('KeyY') === 'z' && layout.get('Backquote') === '^') return '#'
  return '/'
}

/** A shortcut's keys as this keyboard prints them. */
export function keysOf(
  shortcut: Shortcut,
  layout: LayoutMap | null,
  learned: string | null = null,
): string {
  return shortcut.action === 'comment' ? `Ctrl+${commentKey(layout, learned)}` : shortcut.keys
}

/** Windows' `VK_OEM_2`, as `KeyboardEvent.keyCode` gives it: the key Monaco's Ctrl+/ is. */
const OEM_2 = 191

/**
 * The character a key press teaches about the comment key, or `null`. Only
 * the key alone, or with Ctrl: Shift or AltGr would print another character.
 */
export function taught(
  e: Pick<KeyboardEvent, 'keyCode' | 'key' | 'shiftKey' | 'altKey'>,
): string | null {
  return e.keyCode === OEM_2 && !e.shiftKey && !e.altKey && e.key.length === 1 ? e.key : null
}

/** The layout from the web view's Keyboard API, or why not. */
export function readLayout(): Promise<LayoutMap> {
  const keyboard = (
    navigator as Navigator & { keyboard?: { getLayoutMap(): Promise<LayoutMap> } }
  ).keyboard
  return keyboard ? keyboard.getLayoutMap() : Promise.reject(new Error('no Keyboard API'))
}
