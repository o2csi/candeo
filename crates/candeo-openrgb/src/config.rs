//! The private configuration of the OpenRGB Candeo starts
//! (`openrgb-output.md` §6.4).
//!
//! Its `OpenRGB.json` is rewritten before each start. Two things are set,
//! every other key kept where OpenRGB put it:
//!
//! - `Detectors.detectors.<name>`: `false` for every detector claiming a USB
//!   id one of Candeo's definitions has, and for every generic HID detector;
//!   the other names the map knows are **removed**, not set to `true`, so that
//!   OpenRGB puts its own default back rather than Candeo turning on a
//!   detector OpenRGB ships off. OpenRGB skips a detector turned off before
//!   its function runs, so before the device is opened; it adds the names it
//!   lacks and drops those it does not register (`DetectionManager.cpp`).
//! - `LogManager.log_file`: `false`. OpenRGB logs device paths at *info* on
//!   hot plug, and its log files would land in Candeo's folder
//!   (`LogManager.cpp`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;

use serde_json::{Map, Value};

use crate::guard::UsbId;

/// The file OpenRGB reads in the folder `--config` names.
pub const FILE_NAME: &str = "OpenRGB.json";

/// From each OpenRGB detector name to the USB ids it claims, for one
/// OpenRGB release, generated from that release's `60-openrgb.rules` read as
/// data; and the generic HID detectors, registered for any VID and PID
/// (§6.4). The proof of concept ships no map: its generator comes with the
/// wiring.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DetectorMap {
    /// The OpenRGB release it was generated for, `1.1`.
    pub release: String,
    pub detectors: BTreeMap<String, BTreeSet<UsbId>>,
    pub generic: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MapError {
    #[error("the detector map is not valid JSON")]
    Json,
    #[error("the detector map has no \"{0}\"")]
    Missing(&'static str),
    #[error("detector \"{name}\" claims \"{id}\", which is not a VID:PID")]
    BadId { name: String, id: String },
}

impl DetectorMap {
    /// `{"release": "1.1", "detectors": {"<name>": ["1532:0292", …]},
    /// "generic": ["<name>", …]}`.
    pub fn from_json(text: &str) -> Result<Self, MapError> {
        let root: Value = serde_json::from_str(text).map_err(|_| MapError::Json)?;
        let release = root
            .get("release")
            .and_then(Value::as_str)
            .ok_or(MapError::Missing("release"))?
            .to_string();
        let mut detectors = BTreeMap::new();
        let listed = root
            .get("detectors")
            .and_then(Value::as_object)
            .ok_or(MapError::Missing("detectors"))?;
        for (name, ids) in listed {
            let ids = ids.as_array().ok_or(MapError::Missing("detectors"))?;
            let mut set = BTreeSet::new();
            for id in ids {
                let text = id.as_str().unwrap_or_default();
                let parsed = UsbId::parse(text).ok_or_else(|| MapError::BadId {
                    name: name.clone(),
                    id: text.to_string(),
                })?;
                set.insert(parsed);
            }
            detectors.insert(name.clone(), set);
        }
        let generic = root
            .get("generic")
            .and_then(Value::as_array)
            .ok_or(MapError::Missing("generic"))?
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        Ok(Self {
            release,
            detectors,
            generic,
        })
    }

    /// The detectors to turn off: those claiming one of `native`, and every
    /// generic one.
    pub fn turned_off(&self, native: &BTreeSet<UsbId>) -> BTreeSet<String> {
        self.detectors
            .iter()
            .filter(|(_, ids)| !ids.is_disjoint(native))
            .map(|(name, _)| name.clone())
            .chain(self.generic.iter().cloned())
            .collect()
    }

    fn names(&self) -> impl Iterator<Item = &String> {
        self.detectors.keys().chain(self.generic.iter())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewritten {
    pub text: String,
    /// The file there could not be read as a JSON object, and was replaced.
    pub was_unreadable: bool,
}

/// `existing`, the file's text if there is one, with §6.4's two settings.
pub fn rewrite(existing: Option<&str>, map: &DetectorMap, off: &BTreeSet<String>) -> Rewritten {
    let parsed = existing.map(|text| serde_json::from_str::<Value>(text).ok().and_then(object));
    let was_unreadable = matches!(parsed, Some(None));
    let mut root = parsed.flatten().unwrap_or_default();

    let detectors = section(&mut root, "Detectors");
    let names = section(detectors, "detectors");
    for name in map.names() {
        if off.contains(name) {
            names.insert(name.clone(), Value::Bool(false));
        } else {
            names.shift_remove(name);
        }
    }
    for name in off {
        names.insert(name.clone(), Value::Bool(false));
    }
    section(&mut root, "LogManager").insert("log_file".into(), Value::Bool(false));

    let mut text = serde_json::to_string_pretty(&Value::Object(root)).unwrap_or_default();
    text.push('\n');
    Rewritten {
        text,
        was_unreadable,
    }
}

fn object(v: Value) -> Option<Map<String, Value>> {
    match v {
        Value::Object(m) => Some(m),
        _ => None,
    }
}

/// `parent[key]` as an object, replacing whatever else was there: the file
/// is Candeo's own.
fn section<'a>(parent: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    let slot = parent
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !slot.is_object() {
        *slot = Value::Object(Map::new());
    }
    match slot {
        Value::Object(m) => m,
        _ => unreachable!("just made an object"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub turned_off: usize,
    pub was_unreadable: bool,
}

/// Writes `folder/OpenRGB.json` for `native`, through a temporary file and a
/// rename so OpenRGB never reads half a file.
pub fn write(folder: &Path, map: &DetectorMap, native: &BTreeSet<UsbId>) -> io::Result<Written> {
    fs::create_dir_all(folder)?;
    let path = folder.join(FILE_NAME);
    let existing = match fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        // Not text: replaced like any unreadable file.
        Err(e) if e.kind() == io::ErrorKind::InvalidData => Some(String::new()),
        Err(e) => return Err(e),
    };
    let off = map.turned_off(native);
    let rewritten = rewrite(existing.as_deref(), map, &off);
    let temporary = folder.join(format!("{FILE_NAME}.candeo-tmp"));
    fs::write(&temporary, rewritten.text.as_bytes())?;
    fs::rename(&temporary, &path)?;
    Ok(Written {
        turned_off: off.len(),
        was_unreadable: rewritten.was_unreadable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Detector names in OpenRGB's style, for made-up ids.
    const MAP: &str = r#"{
        "release": "1.1",
        "detectors": {
            "Fixture Keyboard (Wired)": ["1234:5678"],
            "Fixture Keyboard (Wireless)": ["1234:5679", "1234:567a"],
            "Fixture Mouse": ["1234:9999"],
            "Other Brand Strip": ["abcd:0001"]
        },
        "generic": ["Fixture Generic HID"]
    }"#;

    /// What a run of OpenRGB might leave in Candeo's folder: its own
    /// settings beside the detectors, made up.
    const SAMPLE: &str = r#"{
    "Detectors": {
        "detectors": {
            "Fixture Keyboard (Wired)": true,
            "Fixture Mouse": false,
            "Other Brand Strip": true,
            "Not In The Map": false
        },
        "hid_safe_mode": false
    },
    "LogManager": {
        "file_count_limit": 10,
        "log_file": true
    },
    "Server": {
        "default_host": "127.0.0.1"
    }
}"#;

    fn map() -> DetectorMap {
        DetectorMap::from_json(MAP).unwrap()
    }

    fn native() -> BTreeSet<UsbId> {
        [UsbId::new(0x1234, 0x5678), UsbId::new(0x1234, 0x567a)].into()
    }

    #[test]
    fn the_detectors_claiming_a_native_id_and_the_generic_ones_are_off() {
        let off = map().turned_off(&native());
        let off: Vec<&str> = off.iter().map(String::as_str).collect();
        assert_eq!(
            off,
            vec![
                "Fixture Generic HID",
                "Fixture Keyboard (Wired)",
                "Fixture Keyboard (Wireless)"
            ]
        );
    }

    #[test]
    fn a_rewrite_sets_two_things_and_keeps_the_rest() {
        let m = map();
        let r = rewrite(Some(SAMPLE), &m, &m.turned_off(&native()));
        assert!(!r.was_unreadable);
        let v: Value = serde_json::from_str(&r.text).unwrap();
        let names = &v["Detectors"]["detectors"];
        assert_eq!(names["Fixture Keyboard (Wired)"], false);
        assert_eq!(names["Fixture Keyboard (Wireless)"], false);
        assert_eq!(names["Fixture Generic HID"], false);
        // Known to the map and not Candeo's: removed, so OpenRGB's default
        // comes back, never forced to true.
        assert!(names.get("Fixture Mouse").is_none());
        assert!(names.get("Other Brand Strip").is_none());
        // Unknown to the map: left as it was.
        assert_eq!(names["Not In The Map"], false);
        assert_eq!(v["Detectors"]["hid_safe_mode"], false);
        assert_eq!(v["LogManager"]["log_file"], false);
        assert_eq!(v["LogManager"]["file_count_limit"], 10);
        assert_eq!(v["Server"]["default_host"], "127.0.0.1");
    }

    #[test]
    fn the_keys_keep_their_order() {
        let m = map();
        let r = rewrite(Some(SAMPLE), &m, &m.turned_off(&native()));
        let detectors = r.text.find("\"Detectors\"").unwrap();
        let log = r.text.find("\"LogManager\"").unwrap();
        let server = r.text.find("\"Server\"").unwrap();
        assert!(detectors < log && log < server, "{}", r.text);
    }

    #[test]
    fn without_a_file_one_is_made() {
        let m = map();
        let r = rewrite(None, &m, &m.turned_off(&native()));
        assert!(!r.was_unreadable);
        let v: Value = serde_json::from_str(&r.text).unwrap();
        assert_eq!(
            v["Detectors"]["detectors"]["Fixture Keyboard (Wired)"],
            false
        );
        assert_eq!(v["LogManager"]["log_file"], false);
    }

    #[test]
    fn an_unreadable_file_is_replaced_and_said_so() {
        let m = map();
        for bad in ["{ not json", "[1, 2]", "", "{\"Detectors\": 3}"] {
            let r = rewrite(Some(bad), &m, &m.turned_off(&native()));
            let v: Value = serde_json::from_str(&r.text).unwrap();
            assert_eq!(
                v["Detectors"]["detectors"]["Fixture Keyboard (Wired)"], false,
                "{bad}"
            );
            assert_eq!(r.was_unreadable, !bad.starts_with("{\""), "{bad}");
        }
    }

    #[test]
    fn written_to_a_folder_through_a_rename() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("openrgb");
        let first = write(&folder, &map(), &native()).unwrap();
        assert_eq!(first.turned_off, 3);
        fs::write(folder.join(FILE_NAME), SAMPLE).unwrap();
        let second = write(&folder, &map(), &BTreeSet::new()).unwrap();
        assert_eq!(second.turned_off, 1, "the generic detector stays off");
        let v: Value =
            serde_json::from_str(&fs::read_to_string(folder.join(FILE_NAME)).unwrap()).unwrap();
        assert!(v["Detectors"]["detectors"]
            .get("Fixture Keyboard (Wired)")
            .is_none());
        assert!(!folder.join(format!("{FILE_NAME}.candeo-tmp")).exists());
    }

    #[test]
    fn a_map_with_a_bad_id_is_refused() {
        let bad = r#"{"release": "1.1", "detectors": {"X": ["1234-5678"]}, "generic": []}"#;
        assert!(matches!(
            DetectorMap::from_json(bad),
            Err(MapError::BadId { .. })
        ));
        assert_eq!(
            DetectorMap::from_json("{}"),
            Err(MapError::Missing("release"))
        );
    }
}
