//! Text rendered by Rust, from the window's catalogs (`AGENTS.md`,
//! internationalisation).
//!
//! The same JSON files as the window, embedded: one translation per string for
//! the whole application, and the tray speaks the language the window does.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::language::Language;

const EN: &str = include_str!("../../src/locales/en.json");
const FR: &str = include_str!("../../src/locales/fr.json");

fn catalog(language: Language) -> &'static serde_json::Value {
    static EN_CATALOG: OnceLock<serde_json::Value> = OnceLock::new();
    static FR_CATALOG: OnceLock<serde_json::Value> = OnceLock::new();
    let (cell, raw) = match language {
        Language::En => (&EN_CATALOG, EN),
        Language::Fr => (&FR_CATALOG, FR),
    };
    // The catalogs are part of the binary and checked by tests: they parse.
    cell.get_or_init(|| serde_json::from_str(raw).expect("catalog is valid JSON"))
}

fn lookup(language: Language, key: &str) -> Option<&'static str> {
    key.split('.')
        .try_fold(catalog(language), |node, part| node.get(part))?
        .as_str()
}

/// The template for `key` in `language`, then in English; the key itself when
/// both lack it, which a test keeps from happening.
fn template(language: Language, key: &str) -> &str {
    lookup(language, key)
        .or_else(|| lookup(Language::En, key))
        .unwrap_or(key)
}

fn fill(template: &str, params: &BTreeMap<&str, String>) -> String {
    params
        .iter()
        .fold(template.to_owned(), |text, (name, value)| {
            text.replace(&format!("{{{name}}}"), value)
        })
}

/// The text for `key` in `language`, then in English, with its `{name}`
/// placeholders filled. A key missing from both gives the key itself, which a
/// test keeps from happening.
pub fn t(language: Language, key: &str, params: &BTreeMap<&str, String>) -> String {
    fill(template(language, key), params)
}

/// [`t`] for a text with plural forms, `{n} layer | {n} layers` as the
/// catalogs write them for the window, the form chosen as vue-i18n chooses it
/// by default, so that the tray and the window say the same: of two forms the
/// first for one, the second otherwise; of three, none, one, many. `n` fills
/// `{n}`.
pub fn tn(language: Language, key: &str, n: usize, params: &BTreeMap<&str, String>) -> String {
    let forms: Vec<&str> = template(language, key).split('|').map(str::trim).collect();
    let form = match forms.len() {
        1 => forms[0],
        2 if n == 1 => forms[0],
        2 => forms[1],
        _ => forms[n.min(2)],
    };
    let mut params = params.clone();
    params.insert("n", n.to_string());
    fill(form, &params)
}

/// [`t`] for a text without placeholders.
pub fn text(language: Language, key: &str) -> String {
    t(language, key, &BTreeMap::new())
}

#[cfg(test)]
pub fn exists(language: Language, key: &str) -> bool {
    lookup(language, key).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_filled_and_english_is_the_fallback() {
        let params = BTreeMap::from([("name", "Rain".to_owned())]);
        assert_eq!(
            t(Language::En, "effects.missing", &params),
            "“Rain” is no longer in the folder."
        );
        assert_eq!(
            t(Language::Fr, "effects.missing", &params),
            "« Rain » n'est plus dans le dossier."
        );
        assert_eq!(text(Language::Fr, "no.such.key"), "no.such.key");
    }

    /// The tray counts as the window does: vue-i18n's default rule, one form
    /// for one, the other for every other count.
    #[test]
    fn a_plural_takes_the_form_vue_i18n_takes() {
        let effect = BTreeMap::from([("effect", "Wave".to_owned())]);
        assert_eq!(
            tn(Language::En, "tray.withLayers", 1, &effect),
            "Wave + 1 layer"
        );
        assert_eq!(
            tn(Language::En, "tray.withLayers", 2, &effect),
            "Wave + 2 layers"
        );
        assert_eq!(
            tn(Language::Fr, "tray.withLayers", 3, &effect),
            "Wave + 3 calques"
        );
        assert_eq!(
            tn(
                Language::En,
                "devices.summary.controlled",
                0,
                &BTreeMap::new()
            ),
            "no device controlled"
        );
        assert_eq!(
            tn(
                Language::En,
                "devices.summary.controlled",
                5,
                &BTreeMap::new()
            ),
            "5 devices controlled"
        );
    }

    /// Every key the Rust sources translate exists in both catalogs: read from the
    /// sources themselves, so that no list has to be kept in step.
    #[test]
    fn every_key_used_from_rust_exists() {
        let sources = [
            include_str!("controlled_folders.rs"),
            include_str!("failure.rs"),
            include_str!("journal.rs"),
            include_str!("lib.rs"),
            include_str!("mcp/mod.rs"),
            include_str!("mcp/server.rs"),
            include_str!("presets.rs"),
            include_str!("runtime/mod.rs"),
            include_str!("storage.rs"),
            include_str!("tray.rs"),
            include_str!("unsupported.rs"),
        ];
        let quoted = |source: &str, i: usize| {
            let start = i + source[i..].find('"').unwrap() + 1;
            source[start..start + source[start..].find('"').unwrap()].to_owned()
        };
        let mut keys = vec!["errors.unexpected".to_owned()];
        for source in sources {
            for prefix in ["\"tray.", "\"devices.warnings."] {
                keys.extend(source.match_indices(prefix).map(|(i, _)| quoted(source, i)));
            }
            keys.extend(
                source
                    .match_indices("Failure::new(\"")
                    .map(|(i, _)| format!("errors.{}", quoted(source, i))),
            );
        }
        assert!(keys.len() > 20, "{keys:?}");
        // A key built with `format!` names a family, not a key.
        let keys: Vec<&str> = keys
            .iter()
            .map(String::as_str)
            .filter(|k| !k.contains('{'))
            .collect();
        for key in keys {
            assert!(exists(Language::En, key), "{key} missing from en.json");
            assert!(exists(Language::Fr, key), "{key} missing from fr.json");
        }
    }
}
