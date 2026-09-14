//! Effects shipped with the application.
//!
//! They are **JavaScript modules**, loaded by the same engine, against the same
//! API and with the same `export default` as effects written by the user.
//! Writing them in native Rust would make them faster and prove nothing: the
//! first example you open must be exactly what you could write yourself.
//!
//! They have no folder: `include_str!` compiles them into the binary. Their
//! JavaScript is therefore also their source — there is no `.ts` to transpile,
//! and that is what lets you read them as they run.
//!
//! ## Two declarations, one content
//!
//! The manifest is written here, in Rust, so that listing the library does not
//! cost instantiating five QuickJS contexts; the module also declares it in its
//! `export default`, because that is the API contract. The test
//! `runtime::tests::built_in_manifests_match_their_modules` forbids them from
//! diverging: the module is authoritative.

use std::sync::OnceLock;

use crate::runtime::swatch::{self, Swatch};

/// An effect compiled into the binary.
pub struct Builtin {
    /// Stable identifier, written by hand and not derived from the name: it is
    /// saved in `settings.json` as the active effect, so renaming the effect
    /// must not change it.
    pub id: &'static str,
    /// The module, as the engine loads it.
    pub js: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// Declared parameters, as JSON, in the shape of `ParamSpec` on the
    /// TypeScript side.
    pub params: &'static str,
}

/// The shipped effects, in the order the gallery presents them.
///
/// Five, and the rule has not moved: they are there to be read, and two
/// variants of the same motion would teach nothing more. The two waves are not
/// one — they measure **two different spaces**: one the physical distance
/// between keycaps, drawing circles from the center; the other the number of
/// steps through the matrix, crossing it in diagonals from a corner.
pub static ALL: [Builtin; 5] = [
    Builtin {
        id: "onde-radiale",
        js: include_str!("onde-radiale.js"),
        name: "Onde radiale",
        description: "Une onde de teinte se propage en cercles, à la distance physique des touches",
        params: r#"{
            "speed": { "kind": "number", "label": "Vitesse", "min": 0, "max": 400, "default": 120 },
            "scale": { "kind": "number", "label": "Échelle", "min": 1, "max": 60, "default": 18 }
        }"#,
    },
    Builtin {
        id: "onde-matricielle",
        js: include_str!("onde-matricielle.js"),
        name: "Onde diagonale",
        description:
            "Une onde de teinte part du coin supérieur gauche et traverse le clavier en diagonale",
        params: r#"{
            "speed": { "kind": "number", "label": "Vitesse", "min": 0, "max": 400, "default": 120 },
            "scale": { "kind": "number", "label": "Échelle", "min": 1, "max": 60, "default": 18 }
        }"#,
    },
    Builtin {
        id: "respiration",
        js: include_str!("respiration.js"),
        name: "Respiration",
        description: "Tout le clavier respire, d'une seule couleur",
        params: r#"{
            "color": { "kind": "color", "label": "Couleur", "default": { "r": 255, "g": 96, "b": 0 } },
            "period": { "kind": "number", "label": "Période (s)", "min": 1, "max": 20, "step": 0.5, "default": 5 }
        }"#,
    },
    Builtin {
        id: "balayage",
        js: include_str!("balayage.js"),
        name: "Balayage",
        description: "Une rangée éclairée descend le clavier en laissant une traînée",
        params: r#"{
            "color": { "kind": "color", "label": "Couleur", "default": { "r": 0, "g": 180, "b": 255 } },
            "speed": { "kind": "number", "label": "Rangées par seconde", "min": 0.5, "max": 12, "step": 0.5, "default": 3 },
            "trail": { "kind": "number", "label": "Traînée (rangées)", "min": 0.5, "max": 6, "step": 0.5, "default": 2 },
            "bounce": { "kind": "boolean", "label": "Rebond", "default": false }
        }"#,
    },
    Builtin {
        id: "degrade-fixe",
        js: include_str!("degrade-fixe.js"),
        name: "Dégradé fixe",
        description: "Un dégradé entre deux couleurs, immobile",
        params: r#"{
            "from": { "kind": "color", "label": "Couleur de départ", "default": { "r": 255, "g": 0, "b": 128 } },
            "to": { "kind": "color", "label": "Couleur d'arrivée", "default": { "r": 0, "g": 128, "b": 255 } },
            "axis": { "kind": "choice", "label": "Sens", "options": ["horizontal", "vertical"], "default": "horizontal" }
        }"#,
    },
];

/// The built-in effect with this identifier, if there is one.
///
/// This is the single entry point of the precedence described in
/// [`crate::storage`]: a built-in identifier is resolved here **before** any
/// disk access.
pub fn find(id: &str) -> Option<&'static Builtin> {
    ALL.iter().find(|b| b.id == id)
}

/// Color swatches of the shipped effects, **in the order of [`ALL`]**.
///
/// # Why they are not on disk
///
/// An installed effect stores its swatch next to its manifest; a built-in has
/// neither a folder nor a manifest on disk, so the question is entirely open
/// again.
///
/// The swatch of a built-in is a property of the **binary**, not of the user's
/// library: it changes when the application changes, never otherwise. Writing
/// it to the data folder would create a cache to invalidate on every update — a
/// version date to compare, a file to rewrite, and a chance to show the swatch
/// of the previous version. All that for five effects whose sampling costs a few
/// milliseconds.
///
/// Writing it by hand in this file is ruled out by the very principle of the
/// swatch: it must come from execution, or it would end up lying.
///
/// That leaves memory: computed on first request, kept for the lifetime of the
/// process. This is the only lazy initialization in the module — the manifests
/// are rebuilt on every call because they only cost five small JSON objects,
/// whereas this instantiates five QuickJS engines.
///
/// The layout is the default one, not that of the plugged-in keyboard: a swatch
/// that depended on the hardware present would not be comparable from one
/// machine to another.
pub fn swatches() -> &'static [Swatch] {
    static SWATCHES: OnceLock<Vec<Swatch>> = OnceLock::new();
    SWATCHES.get_or_init(|| {
        ALL.iter()
            .map(|b| swatch::sample(b.js, crate::default_layout()))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A built-in identifier lives in the **same** namespace as those of user
    /// effects: it is saved in the settings and displayed like the others. If
    /// it did not pass the same validation, the reservation made at install
    /// time would protect nothing.
    #[test]
    fn builtin_ids_are_valid_ids() {
        for b in &ALL {
            crate::storage::validate_name(b.id).unwrap_or_else(|e| panic!("\"{}\": {e}", b.id));
        }
    }

    #[test]
    fn builtin_ids_are_unique() {
        for (i, b) in ALL.iter().enumerate() {
            assert!(
                ALL[i + 1..].iter().all(|o| o.id != b.id),
                "\"{}\" is shipped twice",
                b.id
            );
        }
    }

    /// Every shipped effect has its swatch, and the order follows that of
    /// [`ALL`] — which is what lets the library pair them by position.
    #[test]
    fn every_builtin_effect_has_its_swatch() {
        let swatches = swatches();
        assert_eq!(swatches.len(), ALL.len());
        for (b, swatch) in ALL.iter().zip(swatches) {
            assert!(!swatch.is_empty(), "\"{}\" has no swatch", b.id);
        }
        // Kept, so returned identically: nothing is recomputed each time the
        // library is opened.
        assert_eq!(swatches, self::swatches());
    }

    /// A built-in manifest is written by hand: invalid JSON would pass
    /// compilation and only show when the gallery is opened.
    #[test]
    fn builtin_params_are_valid_json() {
        for b in &ALL {
            let params: serde_json::Value =
                serde_json::from_str(b.params).unwrap_or_else(|e| panic!("\"{}\": {e}", b.id));
            assert!(
                params.as_object().is_some_and(|o| !o.is_empty()),
                "\"{}\" declares no parameters",
                b.id
            );
        }
    }
}
