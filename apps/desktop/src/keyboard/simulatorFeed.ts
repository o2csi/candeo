/**
 * What the simulator draws — a device's frames or the preview loop's — and how
 * long the preview loop lives.
 *
 * Shared by the gallery and the editor so that both screens follow one rule
 * (`docs/design/studio.md` §8): the device frames when the effect on screen is
 * the one running on that device, the preview otherwise. Two copies would
 * drift apart, and a simulator fed by the wrong source looks exactly like a
 * working one (issue #63).
 *
 * ## A region's preview is laid over its device
 *
 * On a split device the preview runs on one region, and the drawing stays the
 * whole device: the region from the preview, the others with the frames the
 * device shows now (`docs/design/regions.md` §3). Both channels are then open,
 * one owner each: the device's channel is opened by one owner only, whether it
 * is drawn alone or under a preview, so that closing one use never closes the
 * other's — an unsubscription landing after a subscription erases it.
 */

import { computed, onBeforeUnmount, ref, shallowRef, watch } from 'vue'

import { startPreview, stopPreview, type Bindings, type EffectParams } from '../api/candeo'
import { deviceOf, targetKey, type Target } from '../api/target'
import type { Rgb } from '../api/types'
import { overlay } from '../composables/regions'
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

const BLACK: Rgb = [0, 0, 0]

export interface SimulatorFeedOptions {
  /** The layout black frames are drawn with until the first frame arrives. */
  layout: () => LayoutView | null
  /**
   * The device whose frames can be shown, and whose layout the preview
   * borrows: one of its regions on a split device, whose own layout the preview
   * then runs on. `null` borrows the default layout: previewing needs no
   * keyboard.
   */
  device: () => Target | null
  /** True when the effect on screen is the one the device runs. */
  showsDevice: () => boolean
  /**
   * The frame positions of the region looked at on a split device, `null` for
   * a whole one. While the preview runs, those come from it and the others from
   * the device.
   */
  focus?: () => readonly number[] | null
  /**
   * What the device's loop runs, as a key that changes when it is started
   * again — its channel goes with its state — or `null` while it runs nothing:
   * there is then no frame to lay a region's preview over, and the other
   * regions are black.
   */
  deviceLoop?: () => string | null
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
  /**
   * The parameters reading a signal, read when the preview starts like
   * `params`. Without them the preview would draw the values the keyboard does
   * not show.
   */
  bindings?: () => Bindings
  onError: (e: unknown) => void
}

export function useSimulatorFeed(options: SimulatorFeedOptions) {
  // One owner per channel: see the header.
  const onDevice = useEngineFrames(options.layout)
  const onPreview = useEngineFrames(options.layout)

  // Only comparable values are watched. `engine_status` is re-read every second
  // and returns fresh objects: watching those would reopen a channel per second.
  const modelKey = computed(() => {
    const d = options.device()
    return d ? `${d.vid}:${d.pid}` : null
  })
  /** The device, or the region: the preview starts again on another region. */
  const previewKey = computed(() => {
    const d = options.device()
    return d ? targetKey(d) : null
  })
  const showsDevice = computed(() => options.showsDevice())
  const focus = computed(() => options.focus?.() ?? null)
  const deviceLoop = computed(() => options.deviceLoop?.() ?? null)
  // Doubling the device's real frames in a second engine would cost a context
  // for nothing.
  const previewed = computed(() => (showsDevice.value ? null : options.previewed()))
  /** The device's frames are wanted under a region's preview: they exist only while its loop runs. */
  const under = computed(() => !showsDevice.value && focus.value !== null && deviceLoop.value !== null)

  /** Bumped when the previewed id stays the same but its code on disk changed. */
  const revision = ref(0)

  // The device's channel: drawn alone, or under a region's preview. A loop
  // started again comes with a new state, where the old channel is not: the
  // loop's key subscribes again, on the same source, which replaces it.
  watch(
    [modelKey, showsDevice, under, deviceLoop],
    ([, shown, beneath]) => {
      const d = options.device()
      if (d && (shown || beneath)) void onDevice.listen(deviceOf(d))
      // Forgotten: listened to again, its last frame would be minutes old.
      else onDevice.stop(true)
    },
    { immediate: true },
  )

  let timer = 0

  // The settle time covers browsing. The stop goes out at once: a preview left
  // running 180 ms after an apply would make the simulator flicker between two
  // sources.
  watch(
    [previewed, previewKey, revision],
    ([id]) => {
      window.clearTimeout(timer)
      if (id === null) {
        onPreview.stop(true)
        void stopPreview()
        return
      }
      timer = window.setTimeout(() => {
        const d = options.device()
        startPreview(d, id, options.params(), options.bindings?.() ?? {})
          // Resubscribing after every start is mandatory. The channel lives in
          // the loop state and `start_preview` builds a new one: without this,
          // the simulator would freeze on the previous preview's last frame and
          // no error would say so.
          .then(() => onPreview.listenPreview())
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

  /**
   * What the chosen source has to show, `null` until it has received a frame:
   * the device's frames, the preview's, or the preview's laid over the
   * device's. A region's preview is black outside the region (§7), which is
   * what its neighbours show while the device runs nothing.
   */
  const live = computed<readonly Rgb[] | null>(() => {
    if (showsDevice.value) return onDevice.last.value
    const shown = onPreview.last.value
    const positions = focus.value
    const around = under.value ? onDevice.last.value : null
    return shown !== null && positions !== null && around !== null
      ? overlay(around, shown, positions)
      : shown
  })

  /**
   * The last frame drawn, held while the next source has sent nothing yet: going
   * from the device's frames to a preview keeps the device's on screen for the
   * length of the start, rather than a black board or another effect's last
   * frame.
   */
  const held = shallowRef<readonly Rgb[] | null>(null)
  watch(live, (f) => {
    if (f !== null) held.value = f
  })

  /** The illustration when one is drawn, the engine's frames otherwise. */
  const frame = computed<readonly Rgb[]>(() => {
    if (drawn.value) return drawn.value
    const length = options.layout()?.frameLen ?? 0
    const f = live.value ?? held.value
    // A frame of another length is another device's: a black one instead, of
    // the right length, until this device's first arrives.
    return f !== null && f.length === length ? f : new Array<Rgb>(length).fill(BLACK)
  })

  return { frame, restartPreview }
}
