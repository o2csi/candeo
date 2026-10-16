//! The devices Candeo knows (`docs/design/device-sdk.md` §9): the built-in
//! definitions and yours, read from `Documents/candeo/devices`, and for each
//! device the one chosen.
//!
//! Only the definitions for this machine: one naming another model is not
//! there, and one of yours says why (#326).
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
    /// Embedded, never tried on the device: it only says a device plugged in is
    /// one it describes, until someone tries it (#300).
    Unverified,
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
    install(build(yours, choices, crate::machine::model()))
}

/// The unverified definitions that may describe a device of this machine: one
/// naming another model is left out, as a built-in one is (#326).
pub fn unverified() -> impl Iterator<Item = &'static (&'static str, &'static Layout)> {
    fitting(definition::unverified(), crate::machine::model())
}

fn fitting<'a>(
    all: &'a [(&'static str, &'static Layout)],
    machine: Option<&'a str>,
) -> impl Iterator<Item = &'a (&'static str, &'static Layout)> {
    all.iter().filter(move |(_, l)| l.fits_machine(machine))
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

/// The catalog on `machine`, the system's product name.
fn build(folder: Option<&Path>, choices: Choices, machine: Option<&str>) -> Catalog {
    // Read once for the process: only yours are read again.
    let mut definitions: Vec<Definition> = definition::builtin()
        .iter()
        .zip(definition::BUILTIN)
        .filter(|(layout, _)| layout.fits_machine(machine))
        .map(|(&layout, &(file, _))| Definition {
            file: file.to_owned(),
            origin: Origin::BuiltIn,
            layout,
        })
        .collect();
    let built_in = definitions.len();

    let mut problems = Vec::new();
    for (file, text) in folder.map(files).unwrap_or_default() {
        let loaded = text
            .and_then(|json| definition::load(&json))
            .and_then(|layout| for_machine(layout, machine));
        match loaded {
            Ok(layout) => definitions.push(Definition {
                file,
                origin: Origin::Yours,
                layout,
            }),
            Err(reason) => problems.push(Problem { file, reason }),
        }
    }
    let yours = definitions.len() - built_in;
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

/// A file of yours for another machine drives nothing, and says why with both
/// models: its author may have copied it from that machine, or mistyped one.
fn for_machine(layout: &'static Layout, machine: Option<&str>) -> Result<&'static Layout, String> {
    match layout.machine {
        Some(wanted) if !layout.fits_machine(machine) => Err(match machine {
            Some(model) => format!("match.machine “{wanted}”: this machine is “{model}”"),
            None => format!("match.machine “{wanted}”: this machine's model could not be read"),
        }),
        _ => Ok(layout),
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
    /// The AW-ELC the 17 R5's built-in definition drives, on that machine only.
    const AW_ELC: (u16, u16) = (0x187c, 0x0550);
    /// A machine every built-in definition applies on.
    const R5: Option<&str> = Some("Alienware 17 R5");
    /// Another machine with an AW-ELC, wired otherwise.
    const M15: Option<&str> = Some("Alienware m15 R1");

    /// The m18 R1's zones, named for `machine`, under another product id.
    fn zones_for(machine: &str) -> String {
        let named = ZONES.replace(
            "\"usage\": \"0001\" }",
            &format!("\"usage\": \"0001\", \"machine\": \"{machine}\" }}"),
        );
        assert_ne!(named, ZONES);
        named.replace("\"0551\"", "\"0999\"")
    }

    fn knows(c: &Catalog, id: (u16, u16)) -> bool {
        c.layouts.iter().any(|l| (l.vid, l.pid) == id)
    }

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
        let c = build(None, Choices::new(), R5);
        assert_eq!(c.layouts.len(), definition::BUILTIN.len());
        assert!(c.layouts.iter().all(|l| c.origin(l) == Origin::BuiltIn));
        assert!(c.problems.is_empty());
    }

    #[test]
    fn a_file_of_yours_adds_a_device() {
        let other = ZONES
            .replace("\"0551\"", "\"0999\"")
            .replace("m18 R1 zones", "test zones");
        let dir = folder(&[("test.json", &other), ("notes.txt", "not a definition")]);
        let c = build(Some(dir.path()), Choices::new(), R5);
        let added = c.layouts.last().unwrap();
        assert_eq!((added.name, added.pid), ("Alienware test zones", 0x0999));
        assert_eq!(c.origin(added), Origin::Yours);
        assert_eq!(
            c.layouts.len(),
            definition::BUILTIN.len() + 1,
            "the text file is not read"
        );
    }

    #[test]
    fn a_file_of_yours_drives_a_built_in_device_only_once_chosen() {
        let dir = folder(&[("a.json", ZONES), ("b.json", ZONES)]);

        let c = build(Some(dir.path()), Choices::new(), R5);
        assert_eq!(c.layouts.len(), definition::BUILTIN.len());
        assert_eq!(zones(&c).origin, Origin::BuiltIn);
        let files: Vec<&str> = c
            .candidates(ZONES_ID.0, ZONES_ID.1)
            .map(|d| d.file.as_str())
            .collect();
        assert_eq!(files, ["alienware-m18-r1-zones.json", "a.json", "b.json"]);

        let c = build(
            Some(dir.path()),
            Choices::from([(ZONES_ID, "b.json".into())]),
            R5,
        );
        assert_eq!(
            (zones(&c).origin, zones(&c).file.as_str()),
            (Origin::Yours, "b.json")
        );
        assert_eq!(c.unloaded_choice(zones(&c).layout), None);
        assert_eq!(c.layouts.len(), definition::BUILTIN.len());
    }

    #[test]
    fn a_chosen_file_that_does_not_load_leaves_the_built_in_one() {
        let broken = ZONES.replace("{count}", "{counts}");
        let dir = folder(&[("mine.json", &broken)]);
        let c = build(
            Some(dir.path()),
            Choices::from([(ZONES_ID, "mine.json".into())]),
            R5,
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
        let c = build(Some(dir.path()), Choices::new(), R5);
        let added = c.layouts.last().unwrap();
        assert_eq!(c.definition(added).unwrap().file, "a.json");
    }

    /// The issue's own case (#326): an AW-ELC on another machine is not taken
    /// for the 17 R5's, nor where the model could not be read, and the 17 R5
    /// still is. The m18 R1's definitions name no machine and stay everywhere.
    #[test]
    fn the_17_r5_is_known_on_the_17_r5_only() {
        let c = build(None, Choices::new(), R5);
        assert!(knows(&c, AW_ELC));
        let r5 = c.candidates(AW_ELC.0, AW_ELC.1).next().unwrap();
        assert_eq!(
            (r5.file.as_str(), r5.origin),
            ("alienware-17-r5-zones.json", Origin::BuiltIn)
        );

        for elsewhere in [M15, None] {
            let c = build(None, Choices::new(), elsewhere);
            assert!(!knows(&c, AW_ELC), "{elsewhere:?}");
            assert_eq!(c.candidates(AW_ELC.0, AW_ELC.1).count(), 0);
            assert!(knows(&c, ZONES_ID), "{elsewhere:?}");
            assert_eq!(c.layouts.len(), definition::BUILTIN.len() - 1);
            assert!(c.problems.is_empty(), "a built-in one is no problem");
        }
    }

    /// A file of yours naming a machine drives its device there, and elsewhere
    /// nothing, saying which machine it wants and which this one is.
    #[test]
    fn a_file_of_yours_for_another_machine_drives_nothing_and_says_why() {
        let dir = folder(&[("m15.json", &zones_for("Alienware m15 R1"))]);

        let c = build(Some(dir.path()), Choices::new(), M15);
        let added = c.layouts.last().unwrap();
        assert_eq!((added.pid, c.origin(added)), (0x0999, Origin::Yours));
        assert!(c.problems.is_empty());

        let c = build(Some(dir.path()), Choices::new(), R5);
        assert!(!knows(&c, (0x187c, 0x0999)));
        assert_eq!(c.problems.len(), 1);
        assert_eq!(c.problems[0].file, "m15.json");
        assert_eq!(
            c.problems[0].reason,
            "match.machine “Alienware m15 R1”: this machine is “Alienware 17 R5”"
        );

        let c = build(Some(dir.path()), Choices::new(), None);
        assert!(c.problems[0].reason.contains("could not be read"));
    }

    /// A copy of the 17 R5's definition chosen for the AW-ELC: on the 17 R5 it
    /// drives it; on another machine, the choice stays but nothing takes it.
    #[test]
    fn a_chosen_copy_for_another_machine_leaves_the_device_unknown() {
        let r5 = definition::builtin_text("alienware-17-r5-zones.json").unwrap();
        let dir = folder(&[("mine.json", r5)]);
        let chosen = || Choices::from([(AW_ELC, "mine.json".to_string())]);

        let c = build(Some(dir.path()), chosen(), R5);
        let layout = c.layouts.iter().find(|l| (l.vid, l.pid) == AW_ELC);
        let used = c.definition(layout.unwrap()).unwrap();
        assert_eq!(
            (used.origin, used.file.as_str()),
            (Origin::Yours, "mine.json")
        );

        let c = build(Some(dir.path()), chosen(), M15);
        assert!(!knows(&c, AW_ELC));
        assert_eq!(c.problems.len(), 1);
    }

    /// The unverified definitions are filtered as the built-in ones are: the
    /// keyboards' stay on any machine, the AW-ELC's (#328) on theirs alone.
    #[test]
    fn an_unverified_definition_naming_another_machine_is_left_out() {
        let named = definition::load(&zones_for("Alienware m15 R1")).unwrap();
        let any = definition::load(ZONES).unwrap();
        let all = [("m15.json", named), ("any.json", any)];
        let files = |machine| -> Vec<&str> { fitting(&all, machine).map(|(f, _)| *f).collect() };
        assert_eq!(files(M15), ["m15.json", "any.json"]);
        assert_eq!(files(R5), ["any.json"]);
        assert_eq!(files(None), ["any.json"]);

        let everything = definition::unverified();
        let anywhere = everything.iter().filter(|(_, l)| l.machine.is_none());
        assert_eq!(fitting(everything, None).count(), anywhere.count());
        let machines: Vec<&(&str, &Layout)> = everything
            .iter()
            .filter(|(_, l)| l.machine.is_some())
            .collect();
        assert!(machines.len() >= 17, "{} for a machine", machines.len());
        for (file, layout) in machines {
            let only: Vec<&str> = fitting(everything, layout.machine)
                .filter(|(_, l)| l.machine.is_some())
                .map(|(f, _)| *f)
                .collect();
            assert_eq!(only, [*file], "one definition for {:?}", layout.machine);
        }
        // The 17 R5 has its own, built in: none of these claims it.
        assert_eq!(
            fitting(everything, R5).count(),
            fitting(everything, None).count()
        );
    }
}
