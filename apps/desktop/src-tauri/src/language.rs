//! The interface language: what `settings.json` keeps, and what "system" means.
//!
//! Two languages, English as the reference. Log messages and the copied
//! diagnostic are not concerned: they serve bug reports (`AGENTS.md`).

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::CmdResult;

/// What someone chose. [`LanguageSetting::System`] is the default, and is not
/// written to the file.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LanguageSetting {
    #[default]
    System,
    En,
    Fr,
}

impl LanguageSetting {
    pub fn is_system(&self) -> bool {
        *self == Self::System
    }

    /// The language the interface uses for this setting, `system` being the
    /// system's language when it is one of ours.
    pub fn resolve(self, system: Language) -> Language {
        match self {
            Self::System => system,
            Self::En => Language::En,
            Self::Fr => Language::Fr,
        }
    }
}

/// A language the interface is written in.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    En,
    Fr,
}

impl Language {
    /// As a text's languages are keyed: `en`, `fr`.
    pub fn code(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Fr => "fr",
        }
    }
}

/// The system's display language, English when it is neither of ours.
///
/// The display language and not the regional format: someone reading Windows in
/// English with French dates expects an English interface.
pub fn system_language() -> Language {
    imp::system_language()
}

/// A locale tag as environments write it — `fr_FR.UTF-8`, `fr-CA`, `fr` — to one
/// of our languages.
#[cfg_attr(windows, allow(dead_code))]
fn from_tag(tag: &str) -> Option<Language> {
    let primary = tag.split(['_', '-', '.', '@']).next()?.to_ascii_lowercase();
    match primary.as_str() {
        "fr" => Some(Language::Fr),
        "en" => Some(Language::En),
        _ => None,
    }
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;

    use super::Language;

    /// `LANG_FRENCH`, the primary language of every French LANGID.
    const FRENCH: u16 = 0x0C;

    pub fn system_language() -> Language {
        // SAFETY: no arguments, returns a LANGID.
        let langid = unsafe { GetUserDefaultUILanguage() };
        if langid & 0x3FF == FRENCH {
            Language::Fr
        } else {
            Language::En
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{from_tag, Language};

    /// The variables that decide messages' language, in their order of priority.
    pub fn system_language() -> Language {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|v| std::env::var(v).ok())
            .find(|v| !v.is_empty())
            .and_then(|tag| from_tag(&tag))
            .unwrap_or(Language::En)
    }
}

/// The setting, the language it resolves to, and the system's, as the interface
/// reads them: the "System" choice names the system's language, whatever is
/// chosen now.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LanguageStatus {
    pub setting: LanguageSetting,
    pub language: Language,
    pub system: Language,
}

fn status(setting: LanguageSetting) -> LanguageStatus {
    let system = system_language();
    LanguageStatus {
        setting,
        language: setting.resolve(system),
        system,
    }
}

/// The language setting, the default when the settings cannot be read: whatever
/// shows text must still be able to say what went wrong.
fn setting(app: &AppHandle) -> LanguageSetting {
    crate::storage::store(app)
        .and_then(|s| s.read_settings())
        .map(|s| s.preferences.language)
        .unwrap_or_default()
}

/// The language the interface shows, for text Rust renders (the tray).
pub fn current(app: &AppHandle) -> Language {
    setting(app).resolve(system_language())
}

/// The interface language.
#[tauri::command]
pub fn get_language(app: AppHandle) -> LanguageStatus {
    status(setting(&app))
}

/// Changes the interface language and saves it, and rebuilds the tray menu in
/// it.
#[tauri::command]
pub fn set_language(app: AppHandle, setting: LanguageSetting) -> CmdResult<LanguageStatus> {
    let store = crate::storage::store(&app)?;
    let mut settings = store.read_settings()?;
    if settings.preferences.language != setting {
        settings.preferences.language = setting;
        store.write_settings(&settings)?;
    }
    crate::tray::refresh(&app);
    Ok(status(setting))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_tags_give_our_languages() {
        assert_eq!(from_tag("fr_FR.UTF-8"), Some(Language::Fr));
        assert_eq!(from_tag("fr-CA"), Some(Language::Fr));
        assert_eq!(from_tag("en_GB"), Some(Language::En));
        assert_eq!(from_tag("de_DE.UTF-8"), None);
        assert_eq!(from_tag(""), None);
    }

    #[test]
    fn a_setting_resolves_to_a_language() {
        assert_eq!(LanguageSetting::System.resolve(Language::Fr), Language::Fr);
        assert_eq!(LanguageSetting::En.resolve(Language::Fr), Language::En);
        assert_eq!(LanguageSetting::Fr.resolve(Language::En), Language::Fr);
    }
}
