//! What an effect is told about the sound playing, from the latest samples
//! (`docs/design/inputs-and-automations.md` §2.2).
//!
//! Pure: a test feeds it a sine or a burst and reads back the bands and the
//! beat, with no sound card in the loop.

/// Bands an effect receives, logarithmically spaced.
pub const BANDS: usize = 16;

/// Samples analysed at once: 2048 at 48 kHz is about 43 ms, the window §2.2
/// asks for — short enough to follow a kick, long enough for bass notes.
pub const WINDOW: usize = 2048;

/// The band edges, in hertz: 40 Hz, below which speakers play little, to
/// 16 kHz, above which there is little music.
const LOWEST_HZ: f32 = 40.0;
const HIGHEST_HZ: f32 = 16_000.0;

/// Decibels mapped to 0..1: -60 dBFS reads as silence, full scale as 1. Music
/// mastered today sits between -20 and -8 dBFS, so it fills the upper half.
const FLOOR_DB: f32 = -60.0;

/// How fast bars and the peak fall, in full scale per second: half a second
/// from the top, so bars do not flicker between frames.
const FALL_PER_SECOND: f32 = 2.0;

/// The lowest bands, where a kick drum lands: a beat is a jump in them.
const BEAT_BANDS: usize = 6;
/// About 0.7 s of flux at 60 analyses a second: the "usual" a beat stands out
/// from.
const FLUX_HISTORY: usize = 43;
/// A beat is a jump this many deviations above the usual flux…
const BEAT_SENSITIVITY: f32 = 1.5;
/// …and at least this much, so a quiet flutter is not taken for one.
const BEAT_MIN_FLUX: f32 = 0.08;
/// No two beats closer than this: 500 beats a minute is more than music plays.
const BEAT_MIN_GAP_S: f32 = 0.12;

/// What one analysis gives: every value 0..1, `beat` true on the analysis an
/// onset is found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub level: f32,
    pub peak: f32,
    pub bands: [f32; BANDS],
    pub beat: bool,
    /// How strong the jump in the low bands is now, against the last moment:
    /// 0.5 is where `beat` starts, so an effect can set its own threshold.
    pub onset: f32,
}

impl Frame {
    /// Nothing playing, and nothing captured: an effect cannot tell them apart,
    /// and does not need to (§2.2).
    pub const SILENT: Frame = Frame {
        level: 0.0,
        peak: 0.0,
        bands: [0.0; BANDS],
        beat: false,
        onset: 0.0,
    };

    /// As the bootstrap reads it: numbers rounded to three decimals, which is
    /// finer than any LED shows.
    pub fn to_json(self) -> String {
        let round = |v: f32| (v * 1000.0).round() / 1000.0;
        let bands: Vec<String> = self.bands.iter().map(|b| round(*b).to_string()).collect();
        format!(
            r#"{{"level":{},"peak":{},"bands":[{}],"beat":{},"onset":{}}}"#,
            round(self.level),
            round(self.peak),
            bands.join(","),
            self.beat,
            round(self.onset)
        )
    }
}

/// Amplitude in 0..1 from a linear one, through decibels.
fn scaled(amplitude: f32) -> f32 {
    if amplitude <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * amplitude.log10();
    ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0)
}

/// The analysis, with what it keeps from one call to the next: bars that fall
/// rather than drop, and the flux a beat is measured against.
pub struct Analyzer {
    /// Each band's first and last FFT bin, half-open.
    bins: [(usize, usize); BANDS],
    window: Vec<f32>,
    /// Sum of the window's coefficients: an FFT bin of a full-scale sine reads
    /// half of it.
    window_sum: f32,
    bands: [f32; BANDS],
    peak: f32,
    previous: [f32; BANDS],
    flux: std::collections::VecDeque<f32>,
    since_beat: f32,
}

impl Analyzer {
    pub fn new(sample_rate: u32) -> Self {
        let resolution = sample_rate as f32 / WINDOW as f32;
        let mut bins = [(0, 0); BANDS];
        for (band, range) in bins.iter_mut().enumerate() {
            let edge =
                |i: usize| LOWEST_HZ * (HIGHEST_HZ / LOWEST_HZ).powf(i as f32 / BANDS as f32);
            let first = (edge(band) / resolution).floor() as usize;
            // At least one bin: the lowest bands are narrower than a bin at 48 kHz.
            let last = ((edge(band + 1) / resolution).floor() as usize).max(first + 1);
            *range = (first.max(1), last.clamp(2, WINDOW / 2));
        }
        // Hann: the window's edges fade to zero, so a note does not smear into
        // every band.
        let window: Vec<f32> = (0..WINDOW)
            .map(|i| {
                let x = std::f32::consts::PI * 2.0 * i as f32 / (WINDOW - 1) as f32;
                0.5 - 0.5 * x.cos()
            })
            .collect();
        let window_sum = window.iter().sum();
        Analyzer {
            bins,
            window,
            window_sum,
            bands: [0.0; BANDS],
            peak: 0.0,
            previous: [0.0; BANDS],
            flux: std::collections::VecDeque::with_capacity(FLUX_HISTORY),
            since_beat: f32::INFINITY,
        }
    }

    /// Analyses the latest samples, mono, oldest first: [`WINDOW`] of them, or
    /// fewer, the missing ones counted as silence before. `dt` is the time since
    /// the previous analysis, in seconds.
    pub fn analyse(&mut self, samples: &[f32], dt: f32) -> Frame {
        let recent = &samples[samples.len().saturating_sub(WINDOW)..];
        let offset = WINDOW - recent.len();

        let rms = (recent.iter().map(|s| s * s).sum::<f32>() / WINDOW as f32).sqrt();
        let level = scaled(rms);
        let loudest = recent.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let fall = FALL_PER_SECOND * dt;
        self.peak = scaled(loudest).max(self.peak - fall);

        let mut re = vec![0.0f32; WINDOW];
        let mut im = vec![0.0f32; WINDOW];
        for (i, s) in recent.iter().enumerate() {
            re[offset + i] = s * self.window[offset + i];
        }
        fft(&mut re, &mut im);

        let mut now = [0.0f32; BANDS];
        for (band, &(first, last)) in self.bins.iter().enumerate() {
            // The band's whole energy, not its average: a note spreads over a
            // few bins, and averaging them over a wide band would hide it.
            let power: f32 = (first..last).map(|k| re[k] * re[k] + im[k] * im[k]).sum();
            now[band] = scaled(2.0 * power.sqrt() / self.window_sum);
        }

        // Bars rise at once and fall at a set pace.
        for (bar, value) in self.bands.iter_mut().zip(now) {
            *bar = value.max(*bar - fall);
        }

        // A beat is a jump in the low bands, well above the flux of the last
        // moment and not too soon after the previous one.
        let flux: f32 = (0..BEAT_BANDS)
            .map(|b| (now[b] - self.previous[b]).max(0.0))
            .sum::<f32>()
            / BEAT_BANDS as f32;
        self.previous = now;
        let (mean, deviation) = mean_and_deviation(&self.flux);
        self.since_beat += dt;
        // How many deviations above the usual the jump is, as 0..1 with the
        // beat's own threshold at half.
        let lift = if deviation > 1e-6 {
            (flux - mean) / deviation
        } else if flux > mean {
            2.0 * BEAT_SENSITIVITY
        } else {
            0.0
        };
        let onset = if flux > BEAT_MIN_FLUX {
            (lift / (2.0 * BEAT_SENSITIVITY)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let beat = flux > BEAT_MIN_FLUX
            && flux > mean + BEAT_SENSITIVITY * deviation
            && self.since_beat >= BEAT_MIN_GAP_S;
        if beat {
            self.since_beat = 0.0;
        }
        if self.flux.len() == FLUX_HISTORY {
            self.flux.pop_front();
        }
        self.flux.push_back(flux);

        Frame {
            level,
            peak: self.peak,
            bands: self.bands,
            beat,
            onset,
        }
    }
}

fn mean_and_deviation(values: &std::collections::VecDeque<f32>) -> (f32, f32) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    let n = values.len() as f32;
    let mean = values.iter().sum::<f32>() / n;
    let variance = values.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n;
    (mean, variance.sqrt())
}

/// In-place radix-2 FFT, `re` and `im` of the same power-of-two length.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    debug_assert!(n.is_power_of_two() && im.len() == n);
    // Bit-reversed order.
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -2.0 * std::f32::consts::PI / len as f32;
        let (w_re, w_im) = (angle.cos(), angle.sin());
        for start in (0..n).step_by(len) {
            let (mut t_re, mut t_im) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let a = start + k;
                let b = a + len / 2;
                let u_re = re[b] * t_re - im[b] * t_im;
                let u_im = re[b] * t_im + im[b] * t_re;
                re[b] = re[a] - u_re;
                im[b] = im[a] - u_im;
                re[a] += u_re;
                im[a] += u_im;
                let next = t_re * w_re - t_im * w_im;
                t_im = t_re * w_im + t_im * w_re;
                t_re = next;
            }
        }
        len <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn sine(hz: f32, amplitude: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| amplitude * (2.0 * std::f32::consts::PI * hz * i as f32 / RATE as f32).sin())
            .collect()
    }

    /// The band a frequency falls in, by the edges the analyzer uses.
    fn band_of(hz: f32) -> usize {
        ((hz / LOWEST_HZ).ln() / (HIGHEST_HZ / LOWEST_HZ).ln() * BANDS as f32).floor() as usize
    }

    #[test]
    fn the_fft_finds_a_sine_in_its_bin() {
        let n = 1024;
        let mut re: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 64.0 * i as f32 / n as f32).cos())
            .collect();
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im);
        let loudest = (0..n / 2)
            .max_by(|&a, &b| (re[a].hypot(im[a])).total_cmp(&re[b].hypot(im[b])))
            .unwrap();
        assert_eq!(loudest, 64);
        assert!(
            (re[64].hypot(im[64]) - n as f32 / 2.0).abs() < 1.0,
            "half the length, for a unit cosine"
        );
    }

    #[test]
    fn silence_is_zero_everywhere() {
        let mut a = Analyzer::new(RATE);
        assert_eq!(a.analyse(&[], 1.0 / 60.0), Frame::SILENT);
        assert_eq!(a.analyse(&vec![0.0; WINDOW], 1.0 / 60.0), Frame::SILENT);
    }

    #[test]
    fn a_tone_lights_its_band_and_not_the_far_ones() {
        let mut a = Analyzer::new(RATE);
        let frame = a.analyse(&sine(1_000.0, 0.5, WINDOW), 1.0 / 60.0);
        let own = band_of(1_000.0);
        let loudest = (0..BANDS)
            .max_by(|&x, &y| frame.bands[x].total_cmp(&frame.bands[y]))
            .unwrap();
        assert_eq!(loudest, own, "{:?}", frame.bands);
        assert!(
            frame.bands[own] > 0.8,
            "a half-scale tone reads loud: {}",
            frame.bands[own]
        );
        assert!(
            frame.bands[0] < 0.2 && frame.bands[BANDS - 1] < 0.2,
            "{:?}",
            frame.bands
        );
        // A half-scale sine is -9 dBFS in RMS.
        assert!((frame.level - 0.85).abs() < 0.03, "level {}", frame.level);
        assert!(frame.peak > 0.85, "peak {}", frame.peak);
    }

    #[test]
    fn a_low_tone_lands_in_a_low_band() {
        let mut a = Analyzer::new(RATE);
        let frame = a.analyse(&sine(60.0, 0.5, WINDOW), 1.0 / 60.0);
        let loudest = (0..BANDS)
            .max_by(|&x, &y| frame.bands[x].total_cmp(&frame.bands[y]))
            .unwrap();
        // Bins are 23 Hz wide at 48 kHz: 60 Hz spreads over the first three bands.
        assert!(loudest <= 2, "60 Hz in band {loudest}: {:?}", frame.bands);
    }

    #[test]
    fn bars_fall_at_a_set_pace_when_the_sound_stops() {
        let mut a = Analyzer::new(RATE);
        let lit = a.analyse(&sine(1_000.0, 0.5, WINDOW), 1.0 / 60.0);
        let own = band_of(1_000.0);
        let after = a.analyse(&vec![0.0; WINDOW], 0.1);
        let expected = lit.bands[own] - FALL_PER_SECOND * 0.1;
        assert!(
            (after.bands[own] - expected).abs() < 1e-4,
            "{} then {}",
            lit.bands[own],
            after.bands[own]
        );
        assert_eq!(after.level, 0.0, "the level follows at once");
        let gone = a.analyse(&vec![0.0; WINDOW], 1.0);
        assert_eq!(gone.bands, [0.0; BANDS]);
    }

    /// A kick after a quiet stretch is a beat; the same sound held is not one
    /// again, nor is a second kick too soon.
    #[test]
    fn a_sudden_low_sound_is_a_beat_and_a_held_one_is_not() {
        let mut a = Analyzer::new(RATE);
        let dt = 1.0 / 60.0;
        for _ in 0..30 {
            assert!(!a.analyse(&vec![0.0; WINDOW], dt).beat);
        }
        let kick = sine(60.0, 0.8, WINDOW);
        let hit = a.analyse(&kick, dt);
        assert!(hit.beat, "the kick");
        assert!(
            hit.onset >= 0.5,
            "a beat's onset is past half: {}",
            hit.onset
        );
        for _ in 0..20 {
            let held = a.analyse(&kick, dt);
            assert!(!held.beat, "held, it is no new beat");
            assert_eq!(held.onset, 0.0, "and no onset");
        }
    }

    #[test]
    fn the_frame_reads_as_json() {
        let mut frame = Frame::SILENT;
        frame.level = 0.12345;
        frame.beat = true;
        let json: serde_json::Value = serde_json::from_str(&frame.to_json()).unwrap();
        assert_eq!(json["level"], 0.123);
        assert_eq!(json["bands"].as_array().unwrap().len(), BANDS);
        assert_eq!(json["beat"], true);
        assert_eq!(json["onset"], 0.0);
    }
}
