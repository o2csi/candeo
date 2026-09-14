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

/// The text for `key` in `language`, then in English, with its `{name}`
/// placeholders filled. A key missing from both gives the key itself, which a
/// test keeps from happening.
pub fn t(language: Language, key: &str, params: &BTreeMap<&str, String>) -> String {
    let template = lookup(language, key)
        .or_else(|| lookup(Language::En, key))
        .unwrap_or(key);
    params
        .iter()
        .fold(template.to_owned(), |text, (name, value)| {
            text.replace(&format!("{{{name}}}"), value)
        })
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

    /// Every key the Rust sources translate exists in both catalogs: read from the
    /// sources themselves, so that no list has to be kept in step.
    #[test]
    fn every_key_used_from_rust_exists() {
        let sources = [include_str!("tray.rs")];
        let mut keys = Vec::new();
        for source in sources {
            for (i, _) in source.match_indices("\"tray.") {
                keys.push(&source[i + 1..i + 1 + source[i + 1..].find('"').unwrap()]);
            }
        }
        assert!(!keys.is_empty());
        for key in keys {
            assert!(exists(Language::En, key), "{key} missing from en.json");
            assert!(exists(Language::Fr, key), "{key} missing from fr.json");
        }
    }
}
