/**
 * The engine's frames, as they go out to the keyboard.
 *
 * This module replaces the demo frames: nothing is computed here any more. The
 * simulator shows what the engine produces, byte for byte — it is what makes
 * the preview **be** the production, and not a likeness of it
 * (`docs/design/studio.md` §3).
 *
 * ## The channel only opens once the effect is started, and on **one** device
 *
 * `subscribe_frames` puts the channel into the state of the running loop, that
 * of the targeted device. Without a loop, there is no state to put it in, and
 * `start_effect` creates a new one at every start: one must therefore subscribe
 * again **after** every start, not once and for all when the editor opens.
 *
 * Each device has its loop and its channel: the simulator follows the one that
 * was selected, changing the selection closes one channel and opens another.
 *
 * ## Two sources, never both at once
 *
 * The engine produces two distinct streams: that of a **device**, which is
 * exactly what goes out to its LEDs, and that of the **preview**, which goes
 * nowhere. The simulator draws only one, and the screen says which — otherwise
 * one would look at a preview believing it to be one's keyboard, which issue
 * #63 refuses. It is what {@link Source} holds: switching from one to the other
 * closes the previous one, there are never two channels open on this component.
 *
 * ## Unsubscribing does not stop the effect
 *
 * Leaving the editor releases the channel and the stream stops; the loop, for
 * its part, keeps feeding the keyboard. It is the intended behaviour: an effect
 * runs with the window closed.
 *
 * ## And `prefers-reduced-motion`?
 *
 * Nothing is neutralised here, and it is deliberate: it is not a decoration
 * that moves on its own, it is the subject of the screen, and it only animates
 * after the user has started the effect. Interface animations, for their part,
 * remain covered by `styles/base.css`.
 */

import { computed, onBeforeUnmount, shallowRef } from 'vue'

import { subscribeFrames, subscribePreviewFrames } from '../api/candeo'
import type { DeviceRef, Rgb } from '../api/types'
import type { LayoutView } from './layout'

const BLACK: Rgb = [0, 0, 0]

/**
 * Where the displayed frames come from: a device, or the preview.
 *
 * `'preview'` rather than a second flag next to the `DeviceRef`: the two are
 * mutually exclusive, and a type that says so is better than a pair of variables
 * of which one combination in four makes no sense.
 */
type Source = DeviceRef | 'preview'

function sameSource(a: Source, b: Source): boolean {
  if (a === 'preview' || b === 'preview') return a === b
  return a.vid === b.vid && a.pid === b.pid
}

/**
 * A complete black frame.
 *
 * `frameLen` positions, not `keys.length`: a frame covers the whole matrix,
 * gaps included. An empty array would make a simulator just as black, but would
 * violate the invariant `layoutProblems` checks — might as well respect it from
 * the first frame.
 */
function dark(layout: LayoutView | null): readonly Rgb[] {
  return layout ? new Array<Rgb>(layout.frameLen).fill(BLACK) : []
}

/**
 * A raw frame into the triplets the simulator expects.
 *
 * `shallowRef` on the caller's side: the frame is replaced as a whole, never
 * modified in place. A deep `ref` would wrap 132 triplets in as many reactive
 * proxies, thirty times per second.
 */
function colors(bytes: Uint8Array): Rgb[] {
  const out = new Array<Rgb>(Math.floor(bytes.length / 3))
  for (let i = 0; i < out.length; i++) {
    out[i] = [bytes[i * 3], bytes[i * 3 + 1], bytes[i * 3 + 2]]
  }
  return out
}

export function useEngineFrames(layout: () => LayoutView | null) {
  const received = shallowRef<readonly Rgb[] | null>(null)

  /**
   * As long as nothing has arrived, a black frame of the right layout — which
   * comes from Rust, so later than the first render.
   *
   * **A frame of another size than the layout is discarded**, not drawn: when
   * changing devices, the layout arrives before the new one's first frame, and
   * the previous one's last frame describes a matrix that is no longer there —
   * 132 colors for a keyboard that expects 140. Drawing it would leave keys
   * without a color and make `layoutProblems` cry out at every switch.
   */
  const frame = computed<readonly Rgb[]>(() => {
    const black = dark(layout())
    const last = received.value
    return last && last.length === black.length ? last : black
  })

  /** What closes the channel. `null` when nobody listens. */
  let release: (() => void) | null = null
  /** What this channel is subscribed to, to know when it must be closed. */
  let source: Source | null = null
  /** False from destruction on: the subscription is asynchronous, it may complete after. */
  let alive = true

  /**
   * Subscribes to a source's stream. Re-subscribable: the engine keeps only one
   * channel **per loop**, the new one replaces the old one.
   *
   * On the same source, we do **not unsubscribe first**: that would be two
   * commands in flight whose order of arrival is not guaranteed, and an
   * unsubscription arriving second would erase the channel just opened — the
   * simulator would stay frozen without anything reporting it.
   *
   * Changing source, on the other hand, requires closing the old one: the engine
   * would not replace a channel set on another loop, and both streams would feed
   * the same simulator.
   */
  async function subscribe(wanted: Source): Promise<void> {
    if (source !== null && !sameSource(source, wanted)) stop()

    source = wanted
    const close =
      wanted === 'preview'
        ? await subscribePreviewFrames((bytes) => {
            received.value = colors(bytes)
          })
        : await subscribeFrames(wanted, (bytes) => {
            received.value = colors(bytes)
          })
    // The component may have disappeared during the round trip. Close right
    // away rather than let a channel feed a destroyed view.
    if (alive) release = close
    else close()
  }

  /** The frames going out to **this device**, byte for byte. */
  function listen(device: DeviceRef): Promise<void> {
    return subscribe(device)
  }

  /** The preview's frames, which go nowhere. */
  function listenPreview(): Promise<void> {
    return subscribe('preview')
  }

  /**
   * Closes the channel. The last frame stays displayed: it is still the one the
   * keyboard lights — stopping the loop does not turn the LEDs off.
   */
  function stop(): void {
    release?.()
    release = null
    source = null
  }

  onBeforeUnmount(() => {
    alive = false
    stop()
  })

  return { frame, listen, listenPreview, stop }
}
