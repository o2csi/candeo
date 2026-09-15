/**
 * The effect the Effects screen selects for a device (#118): the one running on
 * it, else the one it remembers, as long as the library still holds it.
 *
 * `null` leaves the choice to the screen's fallback, the first effect.
 */
export function deviceEffect(
  running: string | null,
  remembered: string | null,
  available: readonly string[],
): string | null {
  return [running, remembered].find((id) => id !== null && available.includes(id)) ?? null
}
