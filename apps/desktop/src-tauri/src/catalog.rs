//! The devices Candeo knows (`docs/design/device-sdk.md` §9): the built-in
//! definitions and yours, read from `Documents/candeo/devices`, and for each
//! device the one chosen.
//!
//! One list, rebuilt whole when the folder is read again or a choice changes,
//! and never changed in place: a device opened on a layout keeps it, and the
//! next opening takes the new one. Each version is kept for the process's life,
//! since every part of the application holds layouts as `&'static`; a reading
//! is a few hundred bytes.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::RwLock;

use candeo_device::{definition, Layout};
use serde::{Deserialize, Serialize};

/// Whose definition a device is known by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    /// Shipped as a definition: reviewed, and replayed by the tests.
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

/// A definition that loads, and where it comes from.
#[derive(Clone)]
pub struct Definition {
    pub file: String,
    pub origin: Origin,
    pub layout: &'static Layout,
}

/// The file of yours chosen per device, by `(vid, pid)`; a device absent from
/// it uses the default.
pub type Choices = BTreeMap<(u16, u16), String>;

pub struct Catalog {
    /// Every definition that loads: the built-in ones in their order, then
    /// yours by file name.
    definitions: Vec<Definition>,
    /// One per device, the definition chosen: what is listed and opened. The
    /// built-in devices first, then those only yours define.
    pub layouts: Vec<&'static Layout>,
    choices: Choices,
    pub problems: Vec<Problem>,
}

impl Catalog {
    /// The definition this layout comes from.
    pub fn definition(&self, layout: &Layout) -> Option<&Definition> {
        self.definitions
            .iter()
            .find(|d| std::ptr::eq(d.layout, layout))
    }

    pub fn origin(&self, layout: &Layout) -> Origin {
        self.definition(layout)
            .map_or(Origin::BuiltIn, |d| d.origin)
    }

    /// Every definition of this device, to choose from: the built-in one first.
    pub fn candidates(&self, vid: u16, pid: u16) -> impl Iterator<Item = &Definition> {
        self.definitions
            .iter()
            .filter(move |d| (d.layout.vid, d.layout.pid) == (vid, pid))
    }

    /// The file chosen for this device when another drives it: it does not
    /// load, or no longer defines this device.
    pub fn unloaded_choice(&self, layout: &Layout) -> Option<&str> {
        let chosen = self.choices.get(&(layout.vid, layout.pid))?;
        let used = self.definition(layout)?;
        (used.origin != Origin::Yours || used.file != *chosen).then_some(chosen.as_str())
    }
}

static CURRENT: RwLock<Option<&'static Catalog>> = RwLock::new(None);

/// The list read last; the built-in one until the folder has been read.
pub fn current() -> &'static Catalog {
    if let Some(catalog) = *CURRENT.read().unwrap() {
        return catalog;
    }
    reload(None, Choices::new())
}

fn install(catalog: Catalog) -> &'static Catalog {
    let catalog: &'static Catalog = Box::leak(Box::new(catalog));
    *CURRENT.write().unwrap() = Some(catalog);
    catalog
}

/// Reads the folder again, `None` for the built-in devices alone.
pub fn reload(yours: Option<&Path>, choices: Choices) -> &'static Catalog {
    install(build(yours, choices))
}

/// The same definitions with other choices: nothing is read again.
pub fn choose(choices: Choices) -> &'static Catalog {
    let now = current();
    install(Catalog {
        definitions: now.definitions.clone(),
        layouts: pick(&now.definitions, &choices),
        choices,
        problems: now.problems.clone(),
    })
}

fn build(folder: Option<&Path>, choices: Choices) -> Catalog {
    // Read once for the process: only yours are read again.
    let mut definitions: Vec<Definition> = definition::builtin()
        .iter()
        .zip(definition::BUILTIN)
        .map(|(&layout, &(file, _))| Definition {
            file: file.to_owned(),
            origin: Origin::BuiltIn,
            layout,
        })
        .collect();

    let mut problems = Vec::new();
    for (file, text) in folder.map(files).unwrap_or_default() {
        match text.and_then(|json| definition::load(&json)) {
            Ok(layout) => definitions.push(Definition {
                file,
                origin: Origin::Yours,
                layout,
            }),
            Err(reason) => problems.push(Problem { file, reason }),
        }
    }
    let yours = definitions.len() - definition::BUILTIN.len();
    if yours > 0 || !problems.is_empty() {
        tracing::info!(
            loaded = yours,
            refused = problems.len(),
            "your device definitions read"
        );
    }
    Catalog {
        layouts: pick(&definitions, &choices),
        definitions,
        choices,
        problems,
    }
}

/// One definition per device: the file chosen if it loads, else the built-in
/// one, else your first file by name. None takes over by being in the folder.
fn pick(definitions: &[Definition], choices: &Choices) -> Vec<&'static Layout> {
    let mut devices: Vec<(u16, u16)> = Vec::new();
    for d in definitions {
        let id = (d.layout.vid, d.layout.pid);
        if !devices.contains(&id) {
            devices.push(id);
        }
    }
    devices
        .into_iter()
        .filter_map(|id| {
            let of = || {
                definitions
                    .iter()
                    .filter(move |d| (d.layout.vid, d.layout.pid) == id)
            };
            let chosen = choices.get(&id);
            of().find(|d| d.origin == Origin::Yours && Some(&d.file) == chosen)
                .or_else(|| of().find(|d| d.origin == Origin::BuiltIn))
                .or_else(|| of().next())
                .map(|d| d.layout)
        })
        .collect()
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
    const ZONES_ID: (u16, u16) = (0x187c, 0x0551);

    fn folder(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in files {
            std::fs::write(dir.path().join(name), text).unwrap();
        }
        dir
    }

    fn zones(c: &Catalog) -> &Definition {
        let layout = c
            .layouts
            .iter()
            .find(|l| (l.vid, l.pid) == ZONES_ID)
            .unwrap();
        c.definition(layout).unwrap()
    }

    #[test]
    fn without_a_folder_the_built_in_devices_are_known() {
        let c = build(None, Choices::new());
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
        let c = build(Some(dir.path()), Choices::new());
        let added = c.layouts.last().unwrap();
        assert_eq!((added.name, added.pid), ("Alienware test zones", 0x0999));
        assert_eq!(c.origin(added), Origin::Yours);
        assert_eq!(c.layouts.len(), 4, "the text file is not read");
    }

    #[test]
    fn a_file_of_yours_drives_a_built_in_device_only_once_chosen() {
        let dir = folder(&[("a.json", ZONES), ("b.json", ZONES)]);

        let c = build(Some(dir.path()), Choices::new());
        assert_eq!(c.layouts.len(), 3);
        assert_eq!(zones(&c).origin, Origin::BuiltIn);
        let files: Vec<&str> = c
            .candidates(ZONES_ID.0, ZONES_ID.1)
            .map(|d| d.file.as_str())
            .collect();
        assert_eq!(files, ["alienware-m18-r1-zones.json", "a.json", "b.json"]);

        let c = build(
            Some(dir.path()),
            Choices::from([(ZONES_ID, "b.json".into())]),
        );
        assert_eq!(
            (zones(&c).origin, zones(&c).file.as_str()),
            (Origin::Yours, "b.json")
        );
        assert_eq!(c.unloaded_choice(zones(&c).layout), None);
        assert_eq!(c.layouts.len(), 3);
    }

    #[test]
    fn a_chosen_file_that_does_not_load_leaves_the_built_in_one() {
        let broken = ZONES.replace("{count}", "{counts}");
        let dir = folder(&[("mine.json", &broken)]);
        let c = build(
            Some(dir.path()),
            Choices::from([(ZONES_ID, "mine.json".into())]),
        );
        assert_eq!(zones(&c).origin, Origin::BuiltIn);
        assert_eq!(c.unloaded_choice(zones(&c).layout), Some("mine.json"));
        assert_eq!(c.problems.len(), 1);
        assert!(c.problems[0].reason.contains("{counts}"));
    }

    #[test]
    fn a_device_nobody_built_in_uses_your_first_file_by_name() {
        let other = ZONES.replace("\"0551\"", "\"0999\"");
        let dir = folder(&[("b.json", &other), ("a.json", &other)]);
        let c = build(Some(dir.path()), Choices::new());
        let added = c.layouts.last().unwrap();
        assert_eq!(c.definition(added).unwrap().file, "a.json");
    }
}
