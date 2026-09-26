//! The devices Candeo knows (`docs/design/device-sdk.md` §9): the layouts
//! written in Rust, the built-in definitions, then yours, read from
//! `Documents/candeo/devices`.
//!
//! One list, rebuilt whole when the folder is read again, and never changed in
//! place: a device opened on a layout keeps it, and the next opening takes the
//! new one. Each version is kept for the process's life, since every part of
//! the application holds layouts as `&'static`; a reading is a few hundred
//! bytes.

use std::path::Path;
use std::sync::RwLock;

use candeo_device::{definition, Layout};
use serde::Serialize;

/// Whose definition a device is known by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    /// Written in Rust or shipped as a definition: reviewed, and replayed by
    /// the tests.
    BuiltIn,
    /// A file of yours: nobody reviewed it.
    Yours,
}

/// A file of yours that drives nothing, and why, in English: what its author
/// reads, like an effect's compile error.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub file: String,
    pub reason: String,
}

pub struct Catalog {
    /// Built-in first, in their order, then yours; a file of yours defining a
    /// device already known takes its place.
    pub layouts: Vec<&'static Layout>,
    yours: Vec<(u16, u16)>,
    pub problems: Vec<Problem>,
}

impl Catalog {
    pub fn origin(&self, layout: &Layout) -> Origin {
        if self.yours.contains(&(layout.vid, layout.pid)) {
            Origin::Yours
        } else {
            Origin::BuiltIn
        }
    }
}

static CURRENT: RwLock<Option<&'static Catalog>> = RwLock::new(None);

/// The list read last; the built-in one until the folder has been read.
pub fn current() -> &'static Catalog {
    if let Some(catalog) = *CURRENT.read().unwrap() {
        return catalog;
    }
    reload(None)
}

/// Reads the folder again, `None` for the built-in devices alone.
pub fn reload(yours: Option<&Path>) -> &'static Catalog {
    let catalog: &'static Catalog = Box::leak(Box::new(build(yours)));
    *CURRENT.write().unwrap() = Some(catalog);
    catalog
}

fn build(folder: Option<&Path>) -> Catalog {
    // Read once for the process: only yours are read again.
    let mut layouts: Vec<&'static Layout> = definition::builtin().to_vec();

    let mut yours = Vec::new();
    let mut problems = Vec::new();
    for (file, text) in folder.map(files).unwrap_or_default() {
        let loaded = text.and_then(|json| definition::load(&json));
        let layout = match loaded {
            Ok(layout) => layout,
            Err(reason) => {
                problems.push(Problem { file, reason });
                continue;
            }
        };
        let id = (layout.vid, layout.pid);
        if yours.contains(&id) {
            problems.push(Problem {
                file,
                reason: format!(
                    "another of your files already defines {:04x}:{:04x}",
                    layout.vid, layout.pid
                ),
            });
            continue;
        }
        match layouts.iter().position(|l| (l.vid, l.pid) == id) {
            Some(i) => layouts[i] = layout,
            None => layouts.push(layout),
        }
        yours.push(id);
    }
    if !yours.is_empty() || !problems.is_empty() {
        tracing::info!(
            loaded = yours.len(),
            refused = problems.len(),
            "your device definitions read"
        );
    }
    Catalog {
        layouts,
        yours,
        problems,
    }
}

/// The folder's `.json` files, by name, each read or why not. A folder that
/// does not exist yet holds none.
fn files(folder: &Path) -> Vec<(String, Result<String, String>)> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: Vec<(String, Result<String, String>)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("json"))
        })
        .map(|p| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            (name, std::fs::read_to_string(&p).map_err(|e| e.to_string()))
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZONES: &str = definition::BUILTIN[2].1;

    fn folder(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in files {
            std::fs::write(dir.path().join(name), text).unwrap();
        }
        dir
    }

    #[test]
    fn without_a_folder_the_built_in_devices_are_known() {
        let c = build(None);
        assert_eq!(c.layouts.len(), 3);
        assert!(c.layouts.iter().all(|l| c.origin(l) == Origin::BuiltIn));
        assert!(c.problems.is_empty());
    }

    #[test]
    fn a_file_of_yours_adds_a_device() {
        let other = ZONES
            .replace("\"0551\"", "\"0999\"")
            .replace("m18 R1 zones", "test zones");
        let dir = folder(&[("test.json", &other), ("notes.txt", "not a definition")]);
        let c = build(Some(dir.path()));
        let added = c.layouts.last().unwrap();
        assert_eq!((added.name, added.pid), ("Alienware test zones", 0x0999));
        assert_eq!(c.origin(added), Origin::Yours);
        assert_eq!(c.layouts.len(), 4, "the text file is not read");
    }

    #[test]
    fn a_file_of_yours_replaces_the_built_in_definition_of_its_device() {
        let dir = folder(&[("zones.json", ZONES)]);
        let c = build(Some(dir.path()));
        assert_eq!(c.layouts.len(), 3);
        assert_eq!(c.origin(c.layouts[2]), Origin::Yours);
        assert_eq!(c.origin(c.layouts[0]), Origin::BuiltIn);
    }

    #[test]
    fn a_file_that_drives_nothing_says_why() {
        let broken = ZONES.replace("{count}", "{counts}");
        let dir = folder(&[("a.json", ZONES), ("b.json", ZONES), ("c.json", &broken)]);
        let c = build(Some(dir.path()));
        assert_eq!(
            c.problems
                .iter()
                .map(|p| p.file.as_str())
                .collect::<Vec<_>>(),
            ["b.json", "c.json"]
        );
        assert!(c.problems[0].reason.contains("already defines 187c:0551"));
        assert!(c.problems[1].reason.contains("{counts}"));
    }
}
