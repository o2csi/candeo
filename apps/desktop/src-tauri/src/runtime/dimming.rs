//! A device's brightness following the sound or a signal
//! (`docs/design/inputs-and-automations.md` §2.2.2, #239).
//!
//! The slider stays the hardware brightness; what follows is a factor on the
//! colours of each frame, from a floor to 1. Scaling the frame costs nothing on
//! the bus, where writing the hardware brightness thirty times a second would be
//! one more report per frame, three on the Alienware.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::audio::analysis::{Frame, Source};
use crate::failure::Failure;
use crate::signals::store::{bound_signal, Scalar};
use crate::DeviceRef;

/// The floor unless moved: an effect stays visible between two beats.
pub const DEFAULT_FLOOR: u8 = 20;

/// What a device's brightness follows: a source, named as a setting names one
/// (`sound:bass`, `signal:lux`), and the floor the factor never goes under, in
/// percent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dimming {
    pub source: String,
    #[serde(default = "default_floor")]
    pub floor: u8,
}

fn default_floor() -> u8 {
    DEFAULT_FLOOR
}

/// Each device's, shared by its loop, the preview borrowing its layout, and the
/// commands. Kept by the engine rather than by a loop: it belongs to the device,
/// and holds when the effect changes or a rule takes over.
pub type Table = Arc<Mutex<HashMap<DeviceRef, Dimming>>>;

impl Dimming {
    /// As the interface or the file gives it, checked: a source Rust reads, as a
    /// setting's binding is, and a floor within 0..=100.
    pub fn checked(self) -> Result<Self, Failure> {
        if !super::is_source(&self.source) {
            return Err(Failure::new("dimmingInvalid").with("source", &self.source));
        }
        Ok(Self {
            floor: self.floor.min(100),
            ..self
        })
    }

    fn floor(&self) -> f32 {
        f32::from(self.floor.min(100)) / 100.0
    }

    /// The sound source it follows, or `None` when it follows a signal.
    pub fn sound(&self) -> Option<Source> {
        Source::parse(&self.source)
    }

    /// The signal it follows, or `None` when it follows the sound.
    pub fn signal(&self) -> Option<&str> {
        bound_signal(&self.source)
    }

    /// The factor the sound gives: the floor in silence, 1 at full, as a number
    /// setting goes from its minimum to its maximum (§2.2.1).
    pub fn sound_factor(&self, frame: &Frame) -> f32 {
        let Some(source) = self.sound() else {
            return 1.0;
        };
        let floor = self.floor();
        floor + (1.0 - floor) * frame.source(source).clamp(0.0, 1.0)
    }

    /// The factor a signal's value gives: a number, or a number as text, is a
    /// percent held between the floor and 100, as a number setting takes a
    /// signal. Absent or anything else, 1: the brightness the slider sets.
    pub fn signal_factor(&self, value: Option<&Scalar>) -> f32 {
        let percent = match value {
            Some(Scalar::Number(n)) => *n,
            Some(Scalar::Text(text)) => match text.trim().parse::<f64>() {
                Ok(n) => n,
                Err(_) => return 1.0,
            },
            _ => return 1.0,
        };
        if !percent.is_finite() {
            return 1.0;
        }
        ((percent / 100.0) as f32).clamp(self.floor(), 1.0)
    }
}

/// Scales every colour of a frame by `factor`, 0..1. Nothing to do at 1, which
/// is every frame of a device whose brightness follows nothing.
pub fn dim(bytes: &mut [u8], factor: f32) {
    if factor >= 1.0 {
        return;
    }
    let factor = factor.max(0.0);
    for b in bytes {
        *b = (f32::from(*b) * factor).round() as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn following(source: &str, floor: u8) -> Dimming {
        Dimming {
            source: source.into(),
            floor,
        }
    }

    fn bass(value: f32) -> Frame {
        Frame {
            bass: value,
            ..Frame::SILENT
        }
    }

    #[test]
    fn the_sound_goes_from_the_floor_to_full() {
        let d = following("sound:bass", 20);
        assert_eq!(
            d.sound_factor(&Frame::SILENT),
            0.2,
            "silence rests on the floor"
        );
        assert_eq!(d.sound_factor(&bass(1.0)), 1.0);
        assert!((d.sound_factor(&bass(0.5)) - 0.6).abs() < 1e-6);
    }

    #[test]
    fn a_floor_at_zero_lets_silence_go_dark() {
        assert_eq!(
            following("sound:volume", 0).sound_factor(&Frame::SILENT),
            0.0
        );
    }

    #[test]
    fn a_signal_is_a_percent_held_above_the_floor() {
        let d = following("signal:lux", 20);
        assert_eq!(d.signal_factor(Some(&Scalar::Number(50.0))), 0.5);
        assert_eq!(d.signal_factor(Some(&Scalar::Text(" 75 ".into()))), 0.75);
        assert_eq!(
            d.signal_factor(Some(&Scalar::Number(5.0))),
            0.2,
            "under the floor"
        );
        assert_eq!(
            d.signal_factor(Some(&Scalar::Number(250.0))),
            1.0,
            "over 100"
        );
    }

    #[test]
    fn a_signal_that_is_not_a_number_leaves_the_slider_s_brightness() {
        let d = following("signal:lux", 20);
        assert_eq!(d.signal_factor(None), 1.0, "absent");
        assert_eq!(d.signal_factor(Some(&Scalar::Text("dim".into()))), 1.0);
        assert_eq!(d.signal_factor(Some(&Scalar::Flag(true))), 1.0);
    }

    #[test]
    fn only_a_source_a_setting_could_follow_is_taken() {
        assert!(following("sound:beat", 20).checked().is_ok());
        assert!(following("signal:lux", 20).checked().is_ok());
        assert!(following("sound:pitch", 20).checked().is_err());
        assert!(following("lux", 20).checked().is_err());
        assert_eq!(following("signal:lux", 180).checked().unwrap().floor, 100);
    }

    #[test]
    fn dimming_scales_every_channel() {
        let mut frame = [255, 128, 0, 10, 20, 30];
        dim(&mut frame, 0.5);
        assert_eq!(frame, [128, 64, 0, 5, 10, 15]);
    }

    #[test]
    fn full_brightness_leaves_the_frame_untouched() {
        let mut frame = [255, 1, 2];
        dim(&mut frame, 1.0);
        assert_eq!(frame, [255, 1, 2]);
    }
}
