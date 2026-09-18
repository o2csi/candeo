/**
 * What the simulator draws — a device's frames or the preview loop's — and how
 * long the preview loop lives.
 *
 * Shared by the gallery and the editor so that both screens follow one rule
 * (`docs/design/studio.md` §8): the device frames when the effect on screen is
 * the one running on that device, the preview otherwise. Two copies would
 * drift apart, and a simulator fed by the wrong source looks exactly like a
 * working one (issue #63).
 */

import { computed, onBeforeUnmount, ref, watch } from 'vue'

import { startPreview, stopPreview, type EffectParams } from '../api/candeo'
import type { DeviceRef, Rgb } from '../api/types'
import { useEngineFrames } from './engineFrames'
import { illustrate } from './illustration'
import type { LayoutView } from './layout'

/**
 * Settle time before the preview (re)starts, in milliseconds.
 *
 * **Every preview builds one QuickJS context and destroys another.** Browsing
 * the gallery with a held arrow key would produce several per second, none of
 * them alive long enough to render a frame anyone looks at.
 *
 * Bounded on the gesture side and not in the engine: Rust would have had to
 * choose between making the last request wait and dropping it, and the window
 * would then have had to reconcile what it asked for with what runs. The colour
 * swatch sampling had to solve the same problem (issue #29).
 *
 * 180 ms: above the repeat rate of a held arrow key, below the time it takes to
 * decide one is really looking at this effect.
 */
const PREVIEW_DELAY = 180

/** How often an illustration is redrawn, in milliseconds: 25 images a second. */
const ILLUSTRATION_PACE = 40

export interface SimulatorFeedOptions {
  /** The layout black frames are drawn with until the first frame arrives. */
  layout: () => LayoutView | null
  /**
   * The device whose frames can be shown, and whose layout the preview
   * borrows. `null` borrows the default layout: previewing needs no keyboard.
   */
  device: () => DeviceRef | null
  /** True when the effect on screen is the one the device runs. */
  showsDevice: () => boolean
  /** The effect to preview when the device is not shown, or `null`. */
  previewed: () => string | null
  /**
   * The firmware effect to **illustrate** instead, or `null`.
   *
   * Not a preview and never called one: the device draws it and shows us
   * nothing, so this is the application's own drawing of what it was seen
   * doing. It runs in the window, without an engine context: nothing of it is
   * ever sent to a keyboard. See [`illustration`].
   */
  illustrated?: () => string | null
  /** The colours that effect paints with, as its settings have them. */
  illustratedColours?: () => readonly Rgb[]
  /**
   * Read when the preview starts, never watched: a slider move adjusts the
   * running preview live instead of rebuilding a QuickJS context.
   */
  params: () => EffectParams
  onError: (e: unknown) => void
}

export function useSimulatorFeed(options: SimulatorFeedOptions) {
  const { frame: engineFrame, listen, listenPreview, stop } = useEngineFrames(options.layout)

  // Only comparable values are watched. `engine_status` is re-read every second
  // and returns fresh objects: watching those would reopen a channel per second.
  const deviceKey = computed(() => {
    const d = options.device()
    return d ? `${d.vid}:${d.pid}` : null
  })
  const showsDevice = computed(() => options.showsDevice())
  // Doubling the device's real frames in a second engine would cost a context
  // for nothing.
  const previewed = computed(() => (showsDevice.value ? null : options.previewed()))

  /** Bumped when the previewed id stays the same but its code on disk changed. */
  const revision = ref(0)

  // One channel at a time: `useEngineFrames` closes the previous source when it
  // switches, otherwise two streams would feed the same simulator. The preview
  // channel is opened by the preview start below, not here.
  watch(
    [deviceKey, showsDevice],
    ([, onDevice]) => {
      const d = options.device()
      if (d && onDevice) void listen({ vid: d.vid, pid: d.pid })
      else stop()
    },
    { immediate: true },
  )

  let timer = 0

  // The settle time covers browsing. The stop goes out at once: a preview left
  // running 180 ms after an apply would make the simulator flicker between two
  // sources.
  watch(
    [previewed, deviceKey, revision],
    ([id]) => {
      window.clearTimeout(timer)
      if (id === null) {
        void stopPreview()
        return
      }
      timer = window.setTimeout(() => {
        const d = options.device()
        startPreview(d ? { vid: d.vid, pid: d.pid } : null, id, options.params())
          // Resubscribing after every start is mandatory. The channel lives in
          // the loop state and `start_preview` builds a new one: without this,
          // the simulator would freeze on the previous preview's last frame and
          // no error would say so.
          .then(() => listenPreview())
          .catch(options.onError)
      }, PREVIEW_DELAY)
    },
    { immediate: true },
  )

  // ------------------------------------------------------------ illustration

  /** The firmware effect drawn here, when one is selected and no device shows. */
  const illustrated = computed(() =>
    showsDevice.value ? null : (options.illustrated?.() ?? null),
  )
  /** Ticks while an illustration runs: what makes the drawing below move. */
  const tick = ref(0)
  let animation = 0
  let since = 0

  watch(
    illustrated,
    (id) => {
      window.clearInterval(animation)
      if (id === null) return
      // Nothing is sent anywhere, so the pace is what an eye needs to read a
      // movement — a quarter of what a screen refreshes at, for a drawing of a
      // few lights.
      since = Date.now()
      tick.value = 0
      animation = window.setInterval(() => tick.value++, ILLUSTRATION_PACE)
    },
    { immediate: true },
  )

  /**
   * The drawing, computed when it is read.
   *
   * **Read, not stored**: it takes the layout as it is at that moment, so a
   * frame never carries the length of the device shown a moment ago — the
   * simulator would then draw a keyboard against another's matrix. And nothing
   * of it runs while this screen is not showing one.
   */
  const drawn = computed<readonly Rgb[] | null>(() => {
    const id = illustrated.value
    const layout = options.layout()
    if (id === null || !layout || layout.frameLen === 0) return null
    void tick.value
    return illustrate(
      id,
      (Date.now() - since) / 1000,
      layout.cols,
      layout.frameLen,
      options.illustratedColours?.() ?? [],
    )
  })

  /** Rebuilds the preview, for code that was just installed under the same id. */
  function restartPreview(): void {
    revision.value++
  }

  onBeforeUnmount(() => {
    window.clearTimeout(timer)
    window.clearInterval(animation)
    // The preview stops with the screen, the applied effect does not: one is
    // what the keyboard does, the other what someone watches, and nobody is
    // left watching. Rust does the same when the window is hidden, which does
    // not go through here.
    void stopPreview()
  })

  /** The illustration when one is drawn, the engine's frames otherwise. */
  const frame = computed(() => drawn.value ?? engineFrame.value)

  return { frame, restartPreview }
}
