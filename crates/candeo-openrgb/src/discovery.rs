//! Finding the OpenRGB the user installed, and its version
//! (`openrgb-output.md` §6.2).
//!
//! Where it is looked for, in order: a path the user picked; on Windows the
//! MSI's folder, `%ProgramFiles%\OpenRGB`, which winget's `OpenRGB.OpenRGB`
//! installs to, then `PATH`; on Linux `PATH`, then the Flatpak
//! `org.openrgb.OpenRGB`. Nothing is run to find it: the file system is
//! looked at, through [`Probe`], so the search is tested on a fake one.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The Flatpak's id, from OpenRGB's README.
pub const FLATPAK_ID: &str = "org.openrgb.OpenRGB";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Install {
    /// The executable itself.
    Executable(PathBuf),
    /// Started as `flatpak run org.openrgb.OpenRGB`, through this `flatpak`.
    Flatpak { flatpak: PathBuf },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Linux,
}

impl Platform {
    pub fn current() -> Option<Self> {
        if cfg!(windows) {
            Some(Self::Windows)
        } else if cfg!(target_os = "linux") {
            Some(Self::Linux)
        } else {
            None
        }
    }

    fn path_separator(self) -> char {
        match self {
            Self::Windows => ';',
            Self::Linux => ':',
        }
    }
}

/// What the search may look at.
pub struct Probe<'a> {
    pub var: &'a dyn Fn(&str) -> Option<OsString>,
    pub is_file: &'a dyn Fn(&Path) -> bool,
    pub is_dir: &'a dyn Fn(&Path) -> bool,
}

pub fn find(platform: Platform, chosen: Option<&Path>, probe: &Probe<'_>) -> Option<Install> {
    if let Some(path) = chosen.filter(|p| (probe.is_file)(p)) {
        return Some(Install::Executable(path.to_path_buf()));
    }
    match platform {
        Platform::Windows => {
            let program_files = ["ProgramFiles", "ProgramW6432"]
                .iter()
                .filter_map(|v| (probe.var)(v))
                .map(|dir| PathBuf::from(dir).join("OpenRGB").join("OpenRGB.exe"));
            program_files
                .chain(on_path(platform, probe, "OpenRGB.exe"))
                .find(|p| (probe.is_file)(p))
                .map(Install::Executable)
        }
        Platform::Linux => {
            if let Some(p) = on_path(platform, probe, "openrgb").find(|p| (probe.is_file)(p)) {
                return Some(Install::Executable(p));
            }
            let flatpak = on_path(platform, probe, "flatpak").find(|p| (probe.is_file)(p))?;
            let app = Path::new("app").join(FLATPAK_ID).join("current");
            let system = Path::new("/var/lib/flatpak").join(&app);
            let user = (probe.var)("XDG_DATA_HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .or_else(|| {
                    (probe.var)("HOME").map(|h| PathBuf::from(h).join(".local").join("share"))
                })
                .map(|data| data.join("flatpak").join(&app));
            [Some(system), user]
                .into_iter()
                .flatten()
                .any(|p| (probe.is_dir)(&p))
                .then_some(Install::Flatpak { flatpak })
        }
    }
}

/// The search on this computer.
pub fn find_here(chosen: Option<&Path>) -> Option<Install> {
    let probe = Probe {
        var: &|name| std::env::var_os(name),
        is_file: &|p| p.is_file(),
        is_dir: &|p| p.is_dir(),
    };
    find(Platform::current()?, chosen, &probe)
}

/// `name` in each absolute folder of `PATH`. A relative entry is skipped: it
/// would resolve against whatever folder Candeo runs in.
fn on_path<'a>(
    platform: Platform,
    probe: &Probe<'_>,
    name: &'a str,
) -> impl Iterator<Item = PathBuf> + 'a {
    let path = (probe.var)("PATH").unwrap_or_default();
    let dirs: Vec<PathBuf> = path
        .to_string_lossy()
        .split(platform.path_separator())
        .map(|d| d.trim().trim_matches('"'))
        .filter(|d| is_absolute(platform, d))
        .map(PathBuf::from)
        .collect();
    dirs.into_iter().map(move |d| d.join(name))
}

/// Absolute for `platform`, whatever the platform the tests run on.
fn is_absolute(platform: Platform, dir: &str) -> bool {
    match platform {
        Platform::Linux => dir.starts_with('/'),
        Platform::Windows => {
            let b = dir.as_bytes();
            (b.len() >= 3
                && b[0].is_ascii_alphabetic()
                && b[1] == b':'
                && (b[2] == b'\\' || b[2] == b'/'))
                || dir.starts_with("\\\\")
        }
    }
}

/// An OpenRGB version: `1.1`, or `1.0rc2` with its release candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub candidate: Option<u32>,
}

impl Version {
    /// 1.0 or later, its release candidates refused: the first release whose
    /// help documents `--server-host` with a loopback default and whose
    /// default bind is loopback (§6.2).
    pub fn may_start(&self) -> bool {
        (self.major, self.minor) > (1, 0)
            || ((self.major, self.minor) == (1, 0) && self.candidate.is_none())
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)?;
        if let Some(rc) = self.candidate {
            write!(f, "rc{rc}")?;
        }
        Ok(())
    }
}

/// From `--version`'s output: its `Version:` line, otherwise its first line,
/// `OpenRGB <version>, for controlling RGB lighting.` (`cli.cpp`'s help at
/// `release_1.0` and `release_1.1`). A pipeline build reads `0.9+ (1.0rc3)`:
/// the version in parentheses is the one that counts.
pub fn parse_version(output: &str) -> Option<Version> {
    let line = output
        .lines()
        .find_map(|l| l.trim().strip_prefix("Version:"))
        .or_else(|| {
            output
                .lines()
                .find_map(|l| l.trim().strip_prefix("OpenRGB "))
        })?;
    let text = match (line.find('('), line.find(')')) {
        (Some(open), Some(close)) if open < close => &line[open + 1..close],
        _ => line,
    };
    version(text.trim())
}

fn version(text: &str) -> Option<Version> {
    let token = text
        .split(|c: char| c.is_whitespace() || c == ',' || c == '+')
        .next()?;
    let (major, rest) = token.split_once('.')?;
    let digits = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let (minor, suffix) = rest.split_at(digits);
    let candidate = match suffix {
        "" => None,
        s => Some(s.strip_prefix("rc")?.parse().ok()?),
    };
    Some(Version {
        major: major.parse().ok()?,
        minor: minor.parse().ok()?,
        candidate,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum VersionError {
    #[error("could not run OpenRGB to read its version: {0}")]
    Run(#[source] std::io::Error),
    #[error("OpenRGB did not print its version in time")]
    Timeout,
    #[error("OpenRGB's version could not be read")]
    Unreadable,
}

/// Runs `--version`, which prints and exits before any detection
/// (`cli.cpp`, 1.0 and 1.1), allowing three seconds and 4 KiB.
pub fn read_version(install: &Install) -> Result<Version, VersionError> {
    let mut command = match install {
        Install::Executable(path) => Command::new(path),
        Install::Flatpak { flatpak } => {
            let mut c = Command::new(flatpak);
            c.args(["run", FLATPAK_ID]);
            c
        }
    };
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    crate::process::no_window(&mut command);
    let mut child = command.spawn().map_err(VersionError::Run)?;
    let deadline = Instant::now() + Duration::from_secs(3);
    let finished = loop {
        match child.try_wait() {
            Ok(Some(_)) => break true,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) | Err(_) => break false,
        }
    };
    if !finished {
        let _ = child.kill();
        let _ = child.wait();
        return Err(VersionError::Timeout);
    }
    let mut out = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        let _ = stdout.take(4096).read_to_end(&mut out);
    }
    parse_version(&String::from_utf8_lossy(&out)).ok_or(VersionError::Unreadable)
}

/// For the log: the executable's name, never the folder, which may hold the
/// user's name (`AGENTS.md`, *Privacy*).
pub fn shown(install: &Install) -> String {
    match install {
        Install::Executable(p) => p.file_name().map_or_else(
            || "OpenRGB".into(),
            |n: &OsStr| n.to_string_lossy().into_owned(),
        ),
        Install::Flatpak { .. } => format!("flatpak {FLATPAK_ID}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    /// A made-up computer: variables, files and folders.
    struct Fake {
        vars: BTreeMap<&'static str, &'static str>,
        files: BTreeSet<PathBuf>,
        dirs: BTreeSet<PathBuf>,
    }

    impl Fake {
        fn new(vars: &[(&'static str, &'static str)]) -> Self {
            Self {
                vars: vars.iter().copied().collect(),
                files: BTreeSet::new(),
                dirs: BTreeSet::new(),
            }
        }

        fn find(&self, platform: Platform, chosen: Option<&Path>) -> Option<Install> {
            let var = |n: &str| self.vars.get(n).map(OsString::from);
            let is_file = |p: &Path| self.files.contains(p);
            let is_dir = |p: &Path| self.dirs.contains(p);
            find(
                platform,
                chosen,
                &Probe {
                    var: &var,
                    is_file: &is_file,
                    is_dir: &is_dir,
                },
            )
        }
    }

    fn msi() -> PathBuf {
        PathBuf::from("C:\\Program Files")
            .join("OpenRGB")
            .join("OpenRGB.exe")
    }

    #[test]
    fn on_windows_the_msi_folder_comes_before_the_path() {
        let mut fake = Fake::new(&[
            ("ProgramFiles", "C:\\Program Files"),
            ("PATH", "C:\\Tools;C:\\Other"),
        ]);
        let portable = PathBuf::from("C:\\Tools").join("OpenRGB.exe");
        fake.files.insert(portable.clone());
        assert_eq!(
            fake.find(Platform::Windows, None),
            Some(Install::Executable(portable))
        );
        fake.files.insert(msi());
        assert_eq!(
            fake.find(Platform::Windows, None),
            Some(Install::Executable(msi()))
        );
    }

    #[test]
    fn a_path_the_user_picked_comes_first_if_it_exists() {
        let mut fake = Fake::new(&[("ProgramFiles", "C:\\Program Files")]);
        fake.files.insert(msi());
        let picked = PathBuf::from("D:\\Portable\\OpenRGB.exe");
        assert_eq!(
            fake.find(Platform::Windows, Some(&picked)),
            Some(Install::Executable(msi()))
        );
        fake.files.insert(picked.clone());
        assert_eq!(
            fake.find(Platform::Windows, Some(&picked)),
            Some(Install::Executable(picked))
        );
    }

    /// A relative `PATH` entry would resolve against Candeo's working folder.
    #[test]
    fn relative_path_entries_are_skipped() {
        let mut fake = Fake::new(&[("PATH", ".;bin;\"C:\\Quoted\"")]);
        fake.files.insert(PathBuf::from(".").join("OpenRGB.exe"));
        fake.files.insert(PathBuf::from("bin").join("OpenRGB.exe"));
        assert_eq!(fake.find(Platform::Windows, None), None);
        let quoted = PathBuf::from("C:\\Quoted").join("OpenRGB.exe");
        fake.files.insert(quoted.clone());
        assert_eq!(
            fake.find(Platform::Windows, None),
            Some(Install::Executable(quoted))
        );
    }

    #[test]
    fn on_linux_the_path_comes_before_the_flatpak() {
        let mut fake = Fake::new(&[
            ("PATH", "/usr/local/bin:/usr/bin"),
            ("HOME", "/home/someone"),
        ]);
        assert_eq!(fake.find(Platform::Linux, None), None);
        fake.files.insert(PathBuf::from("/usr/bin/flatpak"));
        fake.dirs.insert(PathBuf::from(
            "/home/someone/.local/share/flatpak/app/org.openrgb.OpenRGB/current",
        ));
        let flatpak = Install::Flatpak {
            flatpak: PathBuf::from("/usr/bin/flatpak"),
        };
        assert_eq!(fake.find(Platform::Linux, None), Some(flatpak));
        fake.files.insert(PathBuf::from("/usr/bin/openrgb"));
        assert_eq!(
            fake.find(Platform::Linux, None),
            Some(Install::Executable(PathBuf::from("/usr/bin/openrgb")))
        );
    }

    #[test]
    fn a_system_flatpak_is_found_and_flatpak_itself_is_needed() {
        let mut fake = Fake::new(&[("PATH", "/usr/bin")]);
        fake.dirs.insert(PathBuf::from(
            "/var/lib/flatpak/app/org.openrgb.OpenRGB/current",
        ));
        assert_eq!(
            fake.find(Platform::Linux, None),
            None,
            "no flatpak on the path"
        );
        fake.files.insert(PathBuf::from("/usr/bin/flatpak"));
        assert!(matches!(
            fake.find(Platform::Linux, None),
            Some(Install::Flatpak { .. })
        ));
    }

    #[test]
    fn versions_from_the_version_line() {
        let banner =
            "OpenRGB 1.1, for controlling RGB lighting.\n  Version:\t\t 1.1\n  Build Date:\t\t x\n";
        assert_eq!(
            parse_version(banner).map(|v| v.to_string()).as_deref(),
            Some("1.1")
        );
        assert_eq!(
            parse_version("OpenRGB 1.0, for controlling RGB lighting.")
                .unwrap()
                .to_string(),
            "1.0"
        );
        assert_eq!(
            parse_version("Version: 0.9+ (1.0rc3)").unwrap().to_string(),
            "1.0rc3"
        );
        assert_eq!(parse_version("Version: 1.0rc2").unwrap().candidate, Some(2));
        assert_eq!(parse_version("nothing here"), None);
        assert_eq!(parse_version("Version: soon"), None);
    }

    #[test]
    fn only_1_0_and_later_may_be_started() {
        let v = |s: &str| version(s).unwrap();
        for ok in ["1.0", "1.1", "1.2rc1", "2.0"] {
            assert!(v(ok).may_start(), "{ok}");
        }
        for refused in ["0.9", "1.0rc2", "0.8"] {
            assert!(!v(refused).may_start(), "{refused}");
        }
    }

    #[test]
    fn the_log_names_the_executable_not_its_folder() {
        let p = Install::Executable(PathBuf::from("C:\\Users\\someone\\Tools").join("OpenRGB.exe"));
        assert_eq!(shown(&p), "OpenRGB.exe");
        let f = Install::Flatpak {
            flatpak: PathBuf::from("/usr/bin/flatpak"),
        };
        assert_eq!(shown(&f), "flatpak org.openrgb.OpenRGB");
    }
}
