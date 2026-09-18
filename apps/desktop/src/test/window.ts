import { vi } from 'vitest'

/**
 * The webview's `window`, as much of it as the modules under test use.
 *
 * Timers are looked up at each call, so that fake timers apply to them.
 */
export function stubWindow(): void {
  vi.stubGlobal('window', {
    setTimeout: (run: () => void, ms?: number) => setTimeout(run, ms),
    clearTimeout: (timer?: ReturnType<typeof setTimeout>) => clearTimeout(timer),
    setInterval: (run: () => void, ms?: number) => setInterval(run, ms),
    clearInterval: (timer?: ReturnType<typeof setInterval>) => clearInterval(timer),
    addEventListener: () => {},
  })
}
