//! A command's failure, as it crosses to the window (`AGENTS.md`,
//! internationalisation): a catalog key and its parameters. The window says it
//! in its language, the log in English.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

use crate::i18n;
use crate::language::Language;

/// `code` names `errors.<code>` in the catalogs.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: &'static str,
    pub params: BTreeMap<&'static str, String>,
}

impl Failure {
    pub fn new(code: &'static str) -> Self {
        Self {
            code,
            params: BTreeMap::new(),
        }
    }

    pub fn with(mut self, name: &'static str, value: impl fmt::Display) -> Self {
        self.params.insert(name, value.to_string());
        self
    }

    /// What nobody can act on from the window — a disk refusing a write, a value
    /// that does not serialise: the technical detail, in English, is all there is
    /// to say.
    pub fn unexpected(detail: impl fmt::Display) -> Self {
        Self::new("unexpected").with("detail", detail)
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key = format!("errors.{}", self.code);
        f.write_str(&i18n::t(Language::En, &key, &self.params))
    }
}

/// For the paths that only log: they keep their own English sentences.
impl From<Failure> for String {
    fn from(failure: Failure) -> Self {
        failure.to_string()
    }
}

impl From<candeo_device::Error> for Failure {
    fn from(e: candeo_device::Error) -> Self {
        use candeo_device::Error;
        match e {
            Error::NotFound { .. } => Self::new("deviceNotFound"),
            Error::Refused { command } => Self::new("commandUnsupported").with("command", command),
            Error::Hid(e) => Self::new("deviceAccess").with("detail", e),
            Error::NoFirmwareEffect { device } => {
                Self::new("noFirmwareEffect").with("device", device)
            }
            // The screen offers neither a row write nor a slider to a device
            // whose family has neither: reaching these is a bug, not a gesture.
            e @ (Error::RowOutOfRange { .. }
            | Error::NoRowWrite { .. }
            | Error::NoBrightness { .. }) => Self::unexpected(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_crosses_as_a_code_and_logs_in_english() {
        let failure = Failure::new("effectNotFound").with("name", "Rain");
        assert_eq!(
            serde_json::to_value(&failure).unwrap(),
            serde_json::json!({ "code": "effectNotFound", "params": { "name": "Rain" } })
        );
        assert_eq!(failure.to_string(), "No effect is named “Rain”.");
    }
}
