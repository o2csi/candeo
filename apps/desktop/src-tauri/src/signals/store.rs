//! The signals held right now, and what a request may carry (#108,
//! `docs/design/inputs-and-automations.md` §2.3).
//!
//! **Pure**: the instant comes in as an argument, and nothing here opens a socket
//! or touches the disk, so every bound and every expiry is a unit test. Values
//! live in memory only: never written, never logged.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

/// The lifetime of a value sent without one: long enough for a watcher that
/// re-sends in a loop, short enough that a sender that died does not leave a
/// device lit for the rest of the day.
pub const DEFAULT_TTL_SECONDS: u32 = 60;

/// Bounds, so a broken sender cannot grow the process. Chosen, not measured
/// (§2.3): nothing had been written against them when they were set.
pub const MAX_SIGNALS: usize = 64;
pub const MAX_NAME: usize = 64;
pub const MAX_TEXT: usize = 256;

/// One value, flat: a rule compares one equality and an effect reads one scalar.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Scalar {
    Text(String),
    Number(f64),
    Flag(bool),
}

impl Scalar {
    /// The value as a rule compares it. Someone writing "equals 1" in a rule
    /// means the sender's `1` and `"1"` alike, and a rule stores what was typed.
    pub fn as_text(&self) -> String {
        match self {
            Scalar::Text(text) => text.clone(),
            Scalar::Flag(flag) => flag.to_string(),
            // `2.0` reads `2`, as it was most likely sent. The bound keeps the
            // conversion exact.
            Scalar::Number(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", *n as i64),
            Scalar::Number(n) => n.to_string(),
        }
    }
}

/// A value held, and the instants a rule reads from it.
#[derive(Clone, Debug, PartialEq)]
pub struct Held {
    pub value: Scalar,
    /// When this value was first set, unchanged since: where a rule that holds
    /// while it does starts. A watcher re-sending the same state keeps it.
    pub since: i64,
    /// When it was last received: where a flash starts, again at each send.
    pub received: i64,
    /// When it expires, in epoch milliseconds; `None` until erased.
    pub expires: Option<i64>,
}

impl Held {
    fn alive(&self, now: i64) -> bool {
        self.expires.is_none_or(|at| now < at)
    }
}

/// Why a request is refused. Its `Display` is what the API answers, in English:
/// the person reading it is writing a script.
#[derive(Debug, PartialEq)]
pub enum Refusal {
    NotAnObject,
    Name(String),
    NotScalar(String),
    TooLong(String),
    TooMany,
    Ttl(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::NotAnObject => write!(f, "the body must be a JSON object of names and values"),
            Refusal::Name(name) => write!(
                f,
                "\"{name}\" is not a signal name: 1 to {MAX_NAME} letters, digits, or _ - . :"
            ),
            Refusal::NotScalar(name) => {
                write!(f, "\"{name}\" must be a string, a number or a boolean")
            }
            Refusal::TooLong(name) => {
                write!(f, "\"{name}\" is longer than {MAX_TEXT} characters")
            }
            Refusal::TooMany => write!(f, "at most {MAX_SIGNALS} signals are held at once"),
            Refusal::Ttl(ttl) => {
                write!(f, "ttl must be a number of seconds, 0 for none: \"{ttl}\"")
            }
        }
    }
}

/// What a request changed.
#[derive(Debug, Default, PartialEq)]
pub struct Change {
    pub set: Vec<String>,
    pub erased: Vec<String>,
    /// When the values set expire; `None` until erased.
    pub expires: Option<i64>,
}

/// A held signal, as Settings lists it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalView {
    pub name: String,
    pub value: Scalar,
    pub received: i64,
    pub expires: Option<i64>,
}

/// Whether `name` can name a signal. It is what a person types into a rule:
/// letters, digits and `_ - . :`, so that no name hides a control character or
/// passes for another in a list.
pub fn valid_name(name: &str) -> bool {
    let length = name.chars().count();
    (1..=MAX_NAME).contains(&length)
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'))
}

/// The signal a binding reads, from its source as a parameter stores it:
/// `signal:status`. A source names its kind so that another per-frame value — the
/// sound level of §2.2 — binds the same way; only signals exist so far.
pub fn bound_signal(source: &str) -> Option<&str> {
    source
        .strip_prefix("signal:")
        .filter(|name| valid_name(name))
}

/// The `ttl` a request asks for, in seconds, from its query string.
pub fn parse_ttl(raw: Option<&str>) -> Result<Option<u32>, Refusal> {
    match raw {
        None => Ok(None),
        Some(text) => text
            .parse::<u32>()
            .map(Some)
            .map_err(|_| Refusal::Ttl(text.to_string())),
    }
}

#[derive(Debug, Default)]
pub struct Store {
    held: BTreeMap<String, Held>,
}

impl Store {
    /// Takes a request's values: all of them or none, so a script never has to
    /// guess which half landed. An empty string erases its name.
    pub fn apply(&mut self, body: &Value, ttl: Option<u32>, now: i64) -> Result<Change, Refusal> {
        let object = body.as_object().ok_or(Refusal::NotAnObject)?;
        let mut asked = Vec::with_capacity(object.len());
        for (name, value) in object {
            if !valid_name(name) {
                return Err(Refusal::Name(name.clone()));
            }
            let scalar = match value {
                Value::String(text) if text.is_empty() => None,
                Value::String(text) if text.chars().count() > MAX_TEXT => {
                    return Err(Refusal::TooLong(name.clone()))
                }
                Value::String(text) => Some(Scalar::Text(text.clone())),
                Value::Number(n) => Some(Scalar::Number(
                    n.as_f64().ok_or_else(|| Refusal::NotScalar(name.clone()))?,
                )),
                Value::Bool(flag) => Some(Scalar::Flag(*flag)),
                _ => return Err(Refusal::NotScalar(name.clone())),
            };
            asked.push((name.clone(), scalar));
        }

        self.prune(now);
        let added = asked
            .iter()
            .filter(|(name, value)| value.is_some() && !self.held.contains_key(name))
            .count();
        if self.held.len() + added > MAX_SIGNALS {
            return Err(Refusal::TooMany);
        }

        let seconds = ttl.unwrap_or(DEFAULT_TTL_SECONDS);
        let expires = (seconds > 0).then(|| now + i64::from(seconds) * 1000);
        let mut change = Change {
            expires,
            ..Change::default()
        };
        for (name, value) in asked {
            match value {
                None => {
                    self.held.remove(&name);
                    change.erased.push(name);
                }
                Some(value) => {
                    let since = match self.held.get(&name) {
                        Some(old) if old.value.as_text() == value.as_text() => old.since,
                        _ => now,
                    };
                    self.held.insert(
                        name.clone(),
                        Held {
                            value,
                            since,
                            received: now,
                            expires,
                        },
                    );
                    change.set.push(name);
                }
            }
        }
        Ok(change)
    }

    /// Erases one signal, whatever its lifetime. Whether it was held.
    pub fn erase(&mut self, name: &str) -> bool {
        self.held.remove(name).is_some()
    }

    /// Drops what has expired. Whether anything went.
    pub fn prune(&mut self, now: i64) -> bool {
        let before = self.held.len();
        self.held.retain(|_, held| held.alive(now));
        self.held.len() != before
    }

    /// What is held, for the resolver.
    pub fn held(&self) -> &BTreeMap<String, Held> {
        &self.held
    }

    /// The value held under `name`, unless it expired: what a device's
    /// brightness following that signal reads at each frame (§2.2.2).
    pub fn value(&self, name: &str, now: i64) -> Option<&Scalar> {
        self.held
            .get(name)
            .filter(|held| held.alive(now))
            .map(|held| &held.value)
    }

    /// What one frame of an effect reads of the signals, as the render entry point
    /// takes it: the raw values bound to its parameters, by parameter, and — when
    /// it declares the `signals` input — every value held, by name. An empty
    /// string stands for none, and costs the effect no parsing.
    ///
    /// Raw on purpose: converting `"#ff0000"` into a colour needs the parameter's
    /// spec, which only the bootstrap holds (§2.3.1).
    pub fn frame_inputs(
        &self,
        bindings: &BTreeMap<String, String>,
        bag: bool,
        now: i64,
    ) -> (String, String) {
        let bound: serde_json::Map<String, Value> = bindings
            .iter()
            .filter_map(|(param, source)| {
                let held = self.held.get(bound_signal(source)?)?;
                held.alive(now)
                    .then(|| (param.clone(), scalar_json(&held.value)))
            })
            .collect();
        let all: serde_json::Map<String, Value> = if bag {
            self.held
                .iter()
                .filter(|(_, held)| held.alive(now))
                .map(|(name, held)| (name.clone(), scalar_json(&held.value)))
                .collect()
        } else {
            serde_json::Map::new()
        };
        let text = |map: serde_json::Map<String, Value>| {
            if map.is_empty() {
                String::new()
            } else {
                Value::Object(map).to_string()
            }
        };
        (text(bound), text(all))
    }

    /// What is held, for Settings and `GET /signals`, by name.
    pub fn views(&self, now: i64) -> Vec<SignalView> {
        self.held
            .iter()
            .filter(|(_, held)| held.alive(now))
            .map(|(name, held)| SignalView {
                name: name.clone(),
                value: held.value.clone(),
                received: held.received,
                expires: held.expires,
            })
            .collect()
    }
}

fn scalar_json(value: &Scalar) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: i64 = 1_790_000_000_000;

    #[test]
    fn a_value_expires_after_its_lifetime_or_never() {
        let mut store = Store::default();
        store
            .apply(&json!({ "build": "failed" }), None, NOW)
            .unwrap();
        store
            .apply(&json!({ "status": "busy" }), Some(0), NOW)
            .unwrap();

        assert!(!store.prune(NOW + 59_999));
        assert!(store.prune(NOW + 60_000), "60 s by default");
        assert_eq!(store.held().keys().collect::<Vec<_>>(), ["status"]);
        assert!(!store.prune(NOW + 86_400_000), "ttl 0 holds until erased");
    }

    #[test]
    fn an_empty_string_erases() {
        let mut store = Store::default();
        store
            .apply(&json!({ "build": "failed" }), Some(0), NOW)
            .unwrap();
        let change = store.apply(&json!({ "build": "" }), None, NOW + 1).unwrap();
        assert_eq!(change.erased, ["build"]);
        assert!(store.held().is_empty());
    }

    #[test]
    fn the_same_value_resent_keeps_its_start_and_moves_its_receipt() {
        let mut store = Store::default();
        store
            .apply(&json!({ "build": "failed" }), None, NOW)
            .unwrap();
        store
            .apply(&json!({ "build": "failed" }), None, NOW + 5_000)
            .unwrap();
        let held = &store.held()["build"];
        assert_eq!((held.since, held.received), (NOW, NOW + 5_000));

        store
            .apply(&json!({ "build": "ok" }), None, NOW + 9_000)
            .unwrap();
        assert_eq!(
            store.held()["build"].since,
            NOW + 9_000,
            "a new value starts over"
        );
    }

    #[test]
    fn a_request_is_taken_whole_or_not_at_all() {
        let mut store = Store::default();
        let refused = store.apply(&json!({ "build": "failed", "bad name": 1 }), None, NOW);
        assert_eq!(refused, Err(Refusal::Name("bad name".into())));
        assert!(store.held().is_empty(), "the valid half did not land");
    }

    #[test]
    fn only_flat_values_within_bounds_are_taken() {
        let mut store = Store::default();
        assert_eq!(
            store.apply(&json!(["build"]), None, NOW),
            Err(Refusal::NotAnObject)
        );
        assert_eq!(
            store.apply(&json!({ "build": { "state": "failed" } }), None, NOW),
            Err(Refusal::NotScalar("build".into()))
        );
        assert_eq!(
            store.apply(&json!({ "build": "x".repeat(MAX_TEXT + 1) }), None, NOW),
            Err(Refusal::TooLong("build".into()))
        );
        assert!(
            store
                .apply(&json!({ "build": "é".repeat(MAX_TEXT) }), None, NOW)
                .is_ok(),
            "characters are counted, not bytes"
        );
        assert_eq!(
            store.apply(&json!({ "n".repeat(MAX_NAME + 1): 1 }), None, NOW),
            Err(Refusal::Name("n".repeat(MAX_NAME + 1)))
        );
    }

    #[test]
    fn at_most_so_many_signals_are_held() {
        let mut store = Store::default();
        let full: serde_json::Map<String, Value> = (0..MAX_SIGNALS)
            .map(|i| (format!("s{i}"), json!(i)))
            .collect();
        store.apply(&Value::Object(full), None, NOW).unwrap();

        assert_eq!(
            store.apply(&json!({ "one-more": 1 }), None, NOW),
            Err(Refusal::TooMany)
        );
        assert!(
            store.apply(&json!({ "s0": 2 }), None, NOW).is_ok(),
            "a held name is no more"
        );
        assert!(
            store
                .apply(&json!({ "one-more": 1 }), None, NOW + 60_000)
                .is_ok(),
            "expired values make room"
        );
    }

    #[test]
    fn a_number_and_its_text_compare_alike() {
        assert_eq!(Scalar::Number(1.0).as_text(), "1");
        assert_eq!(Scalar::Number(0.4).as_text(), "0.4");
        assert_eq!(Scalar::Flag(true).as_text(), "true");
        assert_eq!(Scalar::Text("failed".into()).as_text(), "failed");
    }

    #[test]
    fn names_are_what_a_person_can_type() {
        assert!(valid_name("build"));
        assert!(valid_name("ci.main:status_2-b"));
        assert!(!valid_name(""));
        assert!(!valid_name("two words"));
        assert!(!valid_name("line\nbreak"));
        assert!(!valid_name("naïve"));
    }

    #[test]
    fn a_frame_reads_the_values_bound_to_its_parameters() {
        let mut store = Store::default();
        store
            .apply(&json!({ "status": "#ff0000", "volume": 0.4 }), None, NOW)
            .unwrap();
        let bindings: BTreeMap<String, String> = [
            ("colour".to_string(), "signal:status".to_string()),
            ("speed".to_string(), "signal:volume".to_string()),
            ("width".to_string(), "signal:never-sent".to_string()),
            ("height".to_string(), "sound:level".to_string()),
        ]
        .into();

        let (bound, all) = store.frame_inputs(&bindings, false, NOW);
        assert_eq!(bound, r##"{"colour":"#ff0000","speed":0.4}"##);
        assert_eq!(all, "", "the bag only for an effect declaring it");

        let (_, all) = store.frame_inputs(&BTreeMap::new(), true, NOW);
        assert_eq!(all, r##"{"status":"#ff0000","volume":0.4}"##);
    }

    #[test]
    fn an_expired_value_reaches_no_frame() {
        let mut store = Store::default();
        store
            .apply(&json!({ "status": "busy" }), None, NOW)
            .unwrap();
        let bindings = [("colour".to_string(), "signal:status".to_string())].into();
        assert_eq!(
            store.frame_inputs(&bindings, true, NOW + 60_000),
            (String::new(), String::new())
        );
    }

    #[test]
    fn a_binding_names_its_source() {
        assert_eq!(bound_signal("signal:status"), Some("status"));
        assert_eq!(bound_signal("status"), None, "the kind is not optional");
        assert_eq!(bound_signal("signal:two words"), None);
        assert_eq!(bound_signal("sound:level"), None, "not a source yet");
    }

    #[test]
    fn a_ttl_is_whole_seconds() {
        assert_eq!(parse_ttl(None), Ok(None));
        assert_eq!(parse_ttl(Some("0")), Ok(Some(0)));
        assert_eq!(parse_ttl(Some("-1")), Err(Refusal::Ttl("-1".into())));
        assert_eq!(parse_ttl(Some("1.5")), Err(Refusal::Ttl("1.5".into())));
    }
}
