/**
 * Putting a device definition in shape, for Ctrl+S and Shift+Alt+F.
 *
 * Not the usual one value per line: a keyboard's lights are a hundred small
 * objects, and the Razer's definition would grow from 166 lines to about 800.
 * Anything that fits in {@link WIDTH} columns stays on one line, as the
 * built-in definitions are written; the Alienware keyboard's comes back
 * unchanged.
 */

/** Wide enough for a light on one line, with its drawing. */
export const WIDTH = 120

function inline(value: unknown): string {
  if (Array.isArray(value)) return value.length ? `[${value.map(inline).join(', ')}]` : '[]'
  if (value !== null && typeof value === 'object') {
    const entries = Object.entries(value)
    if (!entries.length) return '{}'
    return `{ ${entries.map(([k, v]) => `${JSON.stringify(k)}: ${inline(v)}`).join(', ')} }`
  }
  return JSON.stringify(value)
}

/** `lead` is what precedes the value on its line: its key, for a property. */
function print(value: unknown, indent: string, lead: number): string {
  const one = inline(value)
  if (value === null || typeof value !== 'object' || indent.length + lead + one.length <= WIDTH) {
    return one
  }
  const inner = indent + '  '
  if (Array.isArray(value)) {
    return `[\n${value.map((v) => inner + print(v, inner, 0)).join(',\n')}\n${indent}]`
  }
  const lines = Object.entries(value).map(([k, v]) => {
    const key = `${JSON.stringify(k)}: `
    return inner + key + print(v, inner, key.length)
  })
  return `{\n${lines.join(',\n')}\n${indent}}`
}

/** Whether the text holds an object key JavaScript would move: `"2"` goes before `"a"`. */
function reorders(value: unknown): boolean {
  if (Array.isArray(value)) return value.some(reorders)
  if (value === null || typeof value !== 'object') return false
  return Object.entries(value).some(([k, v]) => /^\d+$/.test(k) || reorders(v))
}

/**
 * The text in shape, or `null` when it is not JSON — nothing to put in shape,
 * the editor underlines why — or when printing it again would reorder keys.
 */
export function formatJson(text: string): string | null {
  let value: unknown
  try {
    value = JSON.parse(text)
  } catch {
    return null
  }
  if (reorders(value)) return null
  return `${print(value, '', 0)}\n`
}
