//! Effects shipped with the application.
//!
//! They are ordinary effects: `.ts` files in `packages/effects/`, written against
//! the same API as the user's, copied into the effects folder at startup and
//! from then on listed, edited, renamed and deleted like any other. What the
//! application keeps of them is only what it needs to copy them once and update
//! them while nobody changed them — see [`crate::storage::Store::seed_shipped`]
//! and `docs/design/effects-library.md` §4.
//!
//! They are embedded with `include_str!`: the first launch happens with the
//! window closed and possibly offline.
//!
//! # No type annotations
//!
//! The Rust tests run these sources as they are, without a TypeScript compiler:
//! they check the geometry of the waves and the swatches on real effects. So
//! they declare no types — `defineEffect` already infers the parameters of
//! `render`, and a helper function gives its parameters default values
//! (`hash(n = 0)`), from which TypeScript infers them under `strict` — and
//! [`tests::every_shipped_source_runs_as_it_is`] fails the day one of them would
//! need stripping.

/// An effect shipped with the application.
pub struct Shipped {
    /// Its name, and so its file name.
    pub name: &'static str,
    /// The id it had when effects were compiled into the binary, still found in
    /// settings written by earlier versions. `None` for effects shipped since.
    pub former_id: Option<&'static str>,
    pub source: &'static str,
}

/// The shipped effects, in the order they are copied.
///
/// Named in English: a name is a file name and is not translated, and English
/// is the reference language of the interface.
pub static ALL: [Shipped; 23] = [
    Shipped {
        name: "Radial wave",
        former_id: Some("onde-radiale"),
        source: include_str!("../../../../packages/effects/Radial wave.ts"),
    },
    Shipped {
        name: "Diagonal wave",
        former_id: Some("onde-matricielle"),
        source: include_str!("../../../../packages/effects/Diagonal wave.ts"),
    },
    Shipped {
        name: "Breathing",
        former_id: Some("respiration"),
        source: include_str!("../../../../packages/effects/Breathing.ts"),
    },
    Shipped {
        name: "Sweep",
        former_id: Some("balayage"),
        source: include_str!("../../../../packages/effects/Sweep.ts"),
    },
    Shipped {
        name: "Fixed gradient",
        former_id: Some("degrade-fixe"),
        source: include_str!("../../../../packages/effects/Fixed gradient.ts"),
    },
    Shipped {
        name: "Color wheel",
        former_id: None,
        source: include_str!("../../../../packages/effects/Color wheel.ts"),
    },
    Shipped {
        name: "Noise map",
        former_id: None,
        source: include_str!("../../../../packages/effects/Noise map.ts"),
    },
    Shipped {
        name: "Rain",
        former_id: None,
        source: include_str!("../../../../packages/effects/Rain.ts"),
    },
    Shipped {
        name: "Starry night",
        former_id: None,
        source: include_str!("../../../../packages/effects/Starry night.ts"),
    },
    Shipped {
        name: "Bubbles",
        former_id: None,
        source: include_str!("../../../../packages/effects/Bubbles.ts"),
    },
    Shipped {
        name: "Lightning",
        former_id: None,
        source: include_str!("../../../../packages/effects/Lightning.ts"),
    },
    Shipped {
        name: "Crossing beams",
        former_id: None,
        source: include_str!("../../../../packages/effects/Crossing beams.ts"),
    },
    Shipped {
        name: "Swirl circles",
        former_id: None,
        source: include_str!("../../../../packages/effects/Swirl circles.ts"),
    },
    Shipped {
        name: "Ripples",
        former_id: None,
        source: include_str!("../../../../packages/effects/Ripples.ts"),
    },
    Shipped {
        name: "Clock",
        former_id: None,
        source: include_str!("../../../../packages/effects/Clock.ts"),
    },
    Shipped {
        name: "Status row",
        former_id: None,
        source: include_str!("../../../../packages/effects/Status row.ts"),
    },
    Shipped {
        name: "Scrolling text",
        former_id: None,
        source: include_str!("../../../../packages/effects/Scrolling text.ts"),
    },
    Shipped {
        name: "Equalizer",
        former_id: None,
        source: include_str!("../../../../packages/effects/Equalizer.ts"),
    },
    Shipped {
        name: "Beat pulse",
        former_id: None,
        source: include_str!("../../../../packages/effects/Beat pulse.ts"),
    },
    Shipped {
        name: "Waterfall",
        former_id: None,
        source: include_str!("../../../../packages/effects/Waterfall.ts"),
    },
    Shipped {
        name: "Beat ripples",
        former_id: None,
        source: include_str!("../../../../packages/effects/Beat ripples.ts"),
    },
    Shipped {
        name: "Fireworks",
        former_id: None,
        source: include_str!("../../../../packages/effects/Fireworks.ts"),
    },
    Shipped {
        name: "Aurora",
        former_id: None,
        source: include_str!("../../../../packages/effects/Aurora.ts"),
    },
];

/// The source of the shipped effect with this name: a readable handle for tests.
#[cfg(test)]
pub fn source(name: &str) -> &'static str {
    ALL.iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("\"{name}\" is not shipped"))
        .source
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_names_are_valid_and_unique() {
        for (i, s) in ALL.iter().enumerate() {
            crate::storage::validate_name(s.name).unwrap_or_else(|e| panic!("\"{}\": {e}", s.name));
            assert!(
                ALL[i + 1..].iter().all(|o| {
                    o.name.to_lowercase() != s.name.to_lowercase()
                        && (o.former_id.is_none() || o.former_id != s.former_id)
                }),
                "\"{}\" is shipped twice",
                s.name
            );
        }
    }

    /// **What keeps the Rust tests honest.** They run these sources without a
    /// compiler; a type annotation would make them fail to load here while the
    /// window, which strips types, still ran them.
    #[test]
    fn every_shipped_source_runs_as_it_is() {
        for s in &ALL {
            let declared = crate::runtime::declared_manifest(s.source)
                .unwrap_or_else(|e| panic!("\"{}\" does not load as JavaScript: {e}", s.name));
            let declared: serde_json::Value = serde_json::from_str(&declared).unwrap();
            assert!(
                declared["params"]
                    .as_object()
                    .is_some_and(|p| !p.is_empty()),
                "\"{}\" declares no parameters",
                s.name
            );
            // Shipped effects describe themselves in every language the
            // interface speaks, English first.
            for language in ["en", "fr"] {
                assert!(
                    declared["description"][language]
                        .as_str()
                        .is_some_and(|d| !d.is_empty()),
                    "\"{}\" has no {language} description",
                    s.name
                );
                for (id, spec) in declared["params"].as_object().unwrap() {
                    assert!(
                        spec["label"][language]
                            .as_str()
                            .is_some_and(|l| !l.is_empty()),
                        "\"{}\": parameter \"{id}\" has no {language} label",
                        s.name
                    );
                    for option in spec["options"].as_array().into_iter().flatten() {
                        assert!(
                            option["label"][language]
                                .as_str()
                                .is_some_and(|l| !l.is_empty()),
                            "\"{}\": an option of \"{id}\" has no {language} label",
                            s.name
                        );
                    }
                }
            }
        }
    }
}
