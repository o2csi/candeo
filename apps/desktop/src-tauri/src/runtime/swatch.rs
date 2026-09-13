//! Color swatch of an effect — sampled **by running it**.
//!
//! In the library, each effect carries a few colors that help find it again
//! without launching it. They are sampled from the render, never declared in the
//! manifest nor drawn by hand, for two reasons:
//!
//! 1. the author has nothing to provide — you write your effect, it has its
//!    swatch;
//! 2. above all, **the swatch cannot lie**. Declared by hand, it would drift
//!    from the first change to the code: an effect turned blue would keep its
//!    red thumbnail. Here, it *comes* from the effect.
//!
//! The engine can already produce a frame without touching the hardware —
//! [`super::prepare_bounded`] then [`super::render_once`]. Sampling is only a
//! few more calls, on the same path as production: what the list shows is
//! rendered by the code that will light the keyboard.
//!
//! # Where we sample, and why
//!
//! [`SAMPLES`] frames, at different instants, and one color per frame — but
//! **not at the same place on the keyboard** from one frame to the next.
//!
//! Always taking a key at the same place would not tell a still gradient from a
//! flat fill: both would render the same color four times. Averaging the whole
//! frame would not tell them apart either — the average reduces any frame to a
//! single hue, and an averaged rainbow is grey.
//!
//! Each sample is therefore the average of a **diagonal band** of the keyboard,
//! and the bands move on from one frame to the next. Diagonal, and not a row
//! or a column: a horizontal gradient only varies by column, a vertical sweep
//! only by row. Slicing along either one would make the other perfectly
//! uniform — and so invisible in the swatch.
//!
//! A band, and not a key: an effect may leave most of the keyboard dark —
//! "Balayage" (Sweep) is exactly that —, and a single key would land on black
//! by chance. The average of a quarter of the LEDs does say something true: a
//! mostly dark effect gives a dark swatch, and that is precisely what sets it
//! apart from an effect that fills everything.
//!
//! The instants are **irregularly spaced**. Regularly spaced, they would lock
//! onto the period of a cyclic effect and render the same color four times —
//! the very failure we are trying to avoid.
//!
//! # What the format does not freeze
//!
//! A swatch is a **list** of colors, not a quadruplet. The day the gallery wants
//! animated thumbnails, it will be enough not to stop at a few frames: neither
//! the storage nor the exposed type has to change.

use std::time::{Duration, Instant};

use candeo_device::Layout;
use rquickjs::Context;

/// Number of colors in a swatch.
///
/// Four: enough for a gradient to read as a gradient and a cycle to read as a
/// cycle, few enough for sampling to stay imperceptible at install time.
pub const SAMPLES: usize = 4;

/// Render instants, in seconds.
///
/// Irregularly spaced: see the module header. They cover a little over two
/// seconds, which gives a slow effect — "Respiration" (Breathing) breathes in
/// five seconds — time to show something other than its first frame.
const INSTANTS: [f64; SAMPLES] = [0.0, 0.37, 1.13, 2.61];

/// Maximum time allowed for a complete sampling, loading included.
///
/// This is user code: it may throw, but it may also loop forever. Without this
/// bound, a looping effect **would block its own installation** forever — the
/// opposite of what we want from a swatch, which is only a nicety. A few
/// milliseconds are enough in practice; two seconds is two orders of magnitude
/// above that.
const BUDGET: Duration = Duration::from_secs(2);

/// The swatch of an effect: a few `#rrggbb` colors.
///
/// Hexadecimal strings rather than triplets: the interface puts them into CSS as
/// they are, and the file stays readable when you open it.
pub type Swatch = Vec<String>;

/// Samples the swatch of an effect.
///
/// **Never fails.** An effect that does not load, throws or loops gives an
/// empty swatch; it is up to the caller to fall back on something neutral. A
/// missing swatch is no reason to reject an otherwise valid effect.
///
/// An effect that renders black everywhere, on the other hand, is **not** a
/// failure: its swatch is black, and that is the truth about what it does.
pub fn sample(js: &str, layout: &'static Layout) -> Swatch {
    let deadline = Instant::now() + BUDGET;

    let Ok((_rt, ctx)) = super::prepare_bounded(js, layout, Some(deadline)) else {
        return Swatch::new();
    };

    let params = default_params(&ctx);
    let bands = bands(layout);
    let frame_len = layout.led_count();

    let mut swatch = Swatch::with_capacity(SAMPLES);
    for (&time, band) in INSTANTS.iter().zip(&bands) {
        // The frame index is the one the loop would have at this instant: an
        // effect that counts frames rather than seconds must move on too.
        let frame_index = (time * f64::from(super::FPS)).round() as u32;

        // A frame that throws is skipped, not fatal: an effect that only trips
        // at one instant keeps the colors we could sample elsewhere.
        let Ok(bytes) = super::render_once(&ctx, time, frame_index, &params, frame_len) else {
            continue;
        };
        swatch.push(hex(average(&bytes, band)));
    }
    swatch
}

/// The parameters to sample with: **the default values** declared by the effect
/// itself.
///
/// Not an empty object: nothing forces an effect to fall back on a value when a
/// parameter is missing, and `params.couleur.r` on `undefined` gives black. The
/// swatch would then show an effect nobody will ever see — the gallery launches
/// the effect with its defaults.
///
/// They are read from the manifest the **module** declares, not from the one the
/// Rust side announces: the module is authoritative, and this lets the function
/// need nothing but the JavaScript.
fn default_params(ctx: &Context) -> String {
    let declared: Option<String> = ctx.with(|ctx| ctx.globals().get("__candeo_manifest").ok());
    let Some(declared) = declared else {
        return "{}".into();
    };
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&declared) else {
        return "{}".into();
    };
    let Some(specs) = manifest.get("params").and_then(|p| p.as_object()) else {
        return "{}".into();
    };

    let values: serde_json::Map<String, serde_json::Value> = specs
        .iter()
        .filter_map(|(name, spec)| Some((name.clone(), spec.get("default")?.clone())))
        .collect();
    serde_json::Value::Object(values).to_string()
}

/// Splits the lit positions into [`SAMPLES`] diagonal bands.
///
/// The diagonal is a single coordinate, `row + column`, each normalized: it
/// therefore moves on when going down **and** when going right. Positions are
/// sorted along this coordinate then cut into groups of **equal size**, not
/// slices of equal width — no band can then end up empty on a layout whose LEDs
/// are unevenly spread, and every sample weighs the same number of LEDs.
///
/// Ties are many (a whole anti-diagonal shares its coordinate): the sort is
/// stable and the walk goes row by row, so the split is deterministic. It has
/// to be — the same effect must give the same swatch at every installation.
fn bands(layout: &'static Layout) -> Vec<Vec<u16>> {
    // A single-row or single-column layout does not divide by zero: the
    // matching term simply stays zero.
    let last_row = f32::from(layout.rows.saturating_sub(1).max(1));
    let last_col = f32::from(layout.cols.saturating_sub(1).max(1));

    let mut lit: Vec<(f32, u16)> = Vec::with_capacity(layout.lit_count());
    for row in 0..layout.rows {
        for col in 0..layout.cols {
            let Some(index) = layout.at(row, col) else {
                continue;
            };
            lit.push((f32::from(row) / last_row + f32::from(col) / last_col, index));
        }
    }
    lit.sort_by(|a, b| a.0.total_cmp(&b.0));

    (0..SAMPLES)
        .map(|k| {
            let start = k * lit.len() / SAMPLES;
            let end = (k + 1) * lit.len() / SAMPLES;
            lit[start..end].iter().map(|&(_, index)| index).collect()
        })
        .collect()
}

/// Average color of a band, over a raw frame.
///
/// Arithmetic mean of the bytes, without gamma correction: what we show is a
/// dot a few pixels wide, not a frame to reproduce faithfully, and a black band
/// must stay black.
fn average(bytes: &[u8], band: &[u16]) -> [u8; 3] {
    let mut sum = [0u32; 3];
    let mut counted = 0u32;

    for &index in band {
        let at = index as usize * 3;
        // A frame shorter than the layout is already rejected by
        // `render_once`; the guard protects against a layout changed under us.
        let Some(color) = bytes.get(at..at + 3) else {
            continue;
        };
        for (s, &c) in sum.iter_mut().zip(color) {
            *s += u32::from(c);
        }
        counted += 1;
    }

    if counted == 0 {
        return [0, 0, 0];
    }
    [
        (sum[0] / counted) as u8,
        (sum[1] / counted) as u8,
        (sum[2] / counted) as u8,
    ]
}

fn hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> &'static Layout {
        &candeo_device::DEATHSTALKER_V2_PRO
    }

    fn is_rrggbb(c: &str) -> bool {
        c.len() == 7
            && c.starts_with('#')
            && c[1..]
                .chars()
                .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
    }

    /// Every shipped effect must produce a complete swatch: that is the promise
    /// of the gallery, and a built-in without a swatch would show at first
    /// launch.
    #[test]
    fn every_builtin_effect_produces_a_swatch() {
        for b in &crate::builtins::ALL {
            let swatch = sample(b.js, layout());
            assert_eq!(swatch.len(), SAMPLES, "\"{}\": incomplete swatch", b.slug);
            for c in &swatch {
                assert!(is_rrggbb(c), "\"{}\": color \"{c}\"", b.slug);
            }
        }
    }

    /// An effect that throws gives an empty swatch rather than failing the
    /// caller. This is user code: it is allowed to be broken.
    #[test]
    fn an_effect_that_throws_gives_no_swatch() {
        let js = "export default { name: 'X', render() { throw new Error('boum') } }";
        assert!(sample(js, layout()).is_empty());
    }

    /// Loading can fail before the first render even happens.
    #[test]
    fn an_effect_that_does_not_load_gives_no_swatch() {
        assert!(sample("this is not JavaScript {{{", layout()).is_empty());
    }

    /// An effect that loops forever is **interrupted**, not waited for. Without
    /// this bound, a single `while (true)` would be enough for an installation
    /// never to return.
    #[test]
    fn an_effect_that_loops_is_interrupted() {
        let js = "export default { name: 'X', render() { for (;;) {} } }";

        let start = Instant::now();
        let swatch = sample(js, layout());

        assert!(swatch.is_empty());
        assert!(
            start.elapsed() < BUDGET * 3,
            "sampling took {:?}",
            start.elapsed()
        );
    }

    /// Black is not a failure. An "Éteint" (Off) effect has a swatch, and it is
    /// black — which sets it apart from an effect whose swatch could not be
    /// computed, which has none.
    #[test]
    fn an_all_black_effect_has_a_black_swatch() {
        let js = r#"
            import { BLACK } from '@candeo/effects-api'
            export default { name: 'Éteint', render({ frame }) { frame.fill(BLACK) } }
        "#;
        assert_eq!(sample(js, layout()), vec!["#000000"; SAMPLES]);
    }

    /// The heart of the matter: a uniform effect and a spatial effect must not
    /// look alike. The uniform one renders the same color everywhere and at every
    /// instant, so the same one four times; the gradient varies by column, so
    /// four different colors. Always sampling at the same place would confuse
    /// them.
    #[test]
    fn a_uniform_effect_and_a_spatial_effect_have_distinct_swatches() {
        let uniform = r#"
            import { rgb } from '@candeo/effects-api'
            export default {
              name: 'Uni',
              render({ layout, frame }) {
                for (const key of layout.keys) frame.set(key, rgb(200, 40, 40))
              },
            }
        "#;
        let spatial = r#"
            import { mix } from '@candeo/effects-api'
            export default {
              name: 'Dégradé',
              render({ layout, frame }) {
                for (const key of layout.keys) {
                  frame.set(key, mix({ r: 200, g: 40, b: 40 }, { r: 40, g: 40, b: 200 }, key.col / (layout.cols - 1)))
                }
              },
            }
        "#;

        let flat = sample(uniform, layout());
        let gradient = sample(spatial, layout());

        assert_eq!(
            flat,
            vec!["#c82828"; SAMPLES],
            "a flat fill must stay a flat fill"
        );
        assert_eq!(gradient.len(), SAMPLES);
        assert_eq!(
            gradient
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            SAMPLES,
            "the gradient must give four different colors: {gradient:?}"
        );
        assert_ne!(flat, gradient);
    }

    /// Two visually distinct shipped effects give distinct swatches. This is the
    /// same property as above, checked on what we ship.
    #[test]
    fn two_distinct_builtin_effects_have_distinct_swatches() {
        let breathing = sample(
            crate::builtins::by_slug("respiration")
                .expect("built-in")
                .js,
            layout(),
        );
        let fixed_gradient = sample(
            crate::builtins::by_slug("degrade-fixe")
                .expect("built-in")
                .js,
            layout(),
        );
        assert_ne!(breathing, fixed_gradient);
    }

    /// The swatch is computed once and stored: it must therefore be
    /// reproducible, or saving an unchanged effect again would change its
    /// thumbnail.
    #[test]
    fn the_same_effect_always_gives_the_same_swatch() {
        let js = crate::builtins::by_slug("onde-radiale")
            .expect("built-in")
            .js;
        assert_eq!(sample(js, layout()), sample(js, layout()));
    }

    /// The module's default values are used: without them, an effect that falls
    /// back on nothing would render black, and its swatch would lie.
    #[test]
    fn parameters_are_taken_at_their_default_value() {
        let js = r#"
            export default {
              name: 'X',
              params: { color: { kind: 'color', label: 'Couleur', default: { r: 0, g: 255, b: 0 } } },
              render({ layout, frame, params }) {
                for (const key of layout.keys) frame.set(key, params.color)
              },
            }
        "#;
        assert_eq!(sample(js, layout()), vec!["#00ff00"; SAMPLES]);
    }

    /// The bands cover every LED, once each, in parts equal to within one. An
    /// empty band would give black with nothing to say so.
    #[test]
    fn the_bands_share_out_every_led() {
        let bands = bands(layout());
        let total: usize = bands.iter().map(Vec::len).sum();
        assert_eq!(total, layout().lit_count());

        let mut seen: Vec<u16> = bands.concat();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), total, "an LED appears in two bands");

        let smallest = bands.iter().map(Vec::len).min().unwrap();
        let largest = bands.iter().map(Vec::len).max().unwrap();
        assert!(largest - smallest <= 1, "unbalanced bands");
    }
}
