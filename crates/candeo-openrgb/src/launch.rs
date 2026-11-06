//! Whether Candeo starts an OpenRGB, and how (`openrgb-output.md` §6.3,
//! §6.5).

use std::ffi::OsString;
use std::io;
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use crate::client::{endpoint, DEFAULT_PORT};
use crate::discovery::{Install, Version, FLATPAK_ID};

/// What turning the option on leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// A server answers on the default port: the user's, used as it is and
    /// never stopped. Its detectors stay as the user set them.
    Reuse,
    /// Candeo starts this OpenRGB.
    Start(Install),
    /// Nothing is started; the switch says why (§6.10).
    Hold(Hold),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    /// An OpenRGB runs without its server: a second one would detect the
    /// same devices.
    OpenWithoutServer,
    NotFound,
    TooOld(Version),
    UnreadableVersion,
}

/// What Candeo found, in the order it looks (§6.3, §6.2).
#[derive(Debug, Clone, Default)]
pub struct Facts {
    /// The handshake succeeded on `127.0.0.1:6742`.
    pub server_answers: bool,
    /// A process named `OpenRGB.exe` or `openrgb` runs.
    pub openrgb_running: bool,
    pub install: Option<Install>,
    /// `None` when `--version` could not be read.
    pub version: Option<Version>,
}

pub fn plan(facts: &Facts) -> Plan {
    if facts.server_answers {
        return Plan::Reuse;
    }
    if facts.openrgb_running {
        return Plan::Hold(Hold::OpenWithoutServer);
    }
    let Some(install) = &facts.install else {
        return Plan::Hold(Hold::NotFound);
    };
    match facts.version {
        None => Plan::Hold(Hold::UnreadableVersion),
        Some(v) if !v.may_start() => Plan::Hold(Hold::TooOld(v)),
        Some(_) => Plan::Start(install.clone()),
    }
}

/// A program and its arguments: what [`crate::process::ManagedChild`] starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCommand {
    pub program: PathBuf,
    pub args: Vec<OsString>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LaunchError {
    #[error("port {0} is not one Candeo starts OpenRGB on")]
    Port(u16),
    #[error("the configuration folder cannot be given to flatpak")]
    FlatpakFolder,
}

/// §6.5's arguments, and nothing else:
///
/// ```text
/// OpenRGB --server --server-host 127.0.0.1 --server-port <port>
///         --noautoconnect --config <folder> --loglevel 2
/// ```
///
/// `--server-host` is loopback, OpenRGB's default since 1.0, passed anyway so
/// no setting can widen it; `--noautoconnect` keeps OpenRGB from first
/// connecting to a local server, which may be the user's; `--loglevel 2`
/// stays below *info*, where device paths are logged. Through Flatpak, the
/// folder is shared with `--filesystem`, which takes no `:` in it.
pub fn command(install: &Install, port: u16, config: &Path) -> Result<LaunchCommand, LaunchError> {
    if port == 0 || port == DEFAULT_PORT {
        return Err(LaunchError::Port(port));
    }
    let openrgb_args = [
        OsString::from("--server"),
        "--server-host".into(),
        "127.0.0.1".into(),
        "--server-port".into(),
        port.to_string().into(),
        "--noautoconnect".into(),
        "--config".into(),
        config.as_os_str().to_owned(),
        "--loglevel".into(),
        "2".into(),
    ];
    match install {
        Install::Executable(program) => Ok(LaunchCommand {
            program: program.clone(),
            args: openrgb_args.to_vec(),
        }),
        Install::Flatpak { flatpak } => {
            let folder = config
                .to_str()
                .filter(|f| !f.contains(':'))
                .ok_or(LaunchError::FlatpakFolder)?;
            let mut args = vec![
                OsString::from("run"),
                format!("--filesystem={folder}").into(),
                FLATPAK_ID.into(),
            ];
            args.extend(openrgb_args);
            Ok(LaunchCommand {
                program: flatpak.clone(),
                args,
            })
        }
    }
}

/// A free loopback port, never the default one. Taken by someone else
/// before OpenRGB binds it, the handshake fails, and Candeo tries once more
/// (§6.5).
pub fn free_port() -> io::Result<u16> {
    for _ in 0..8 {
        let port = TcpListener::bind(endpoint(0))?.local_addr()?.port();
        if port != DEFAULT_PORT {
            return Ok(port);
        }
    }
    Err(io::Error::new(io::ErrorKind::AddrInUse, "no free port"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe() -> Install {
        Install::Executable(PathBuf::from("OpenRGB.exe"))
    }

    fn v(major: u32, minor: u32, candidate: Option<u32>) -> Version {
        Version {
            major,
            minor,
            candidate,
        }
    }

    #[test]
    fn a_running_server_is_reused_before_anything_else() {
        let facts = Facts {
            server_answers: true,
            openrgb_running: true,
            ..Facts::default()
        };
        assert_eq!(plan(&facts), Plan::Reuse);
    }

    #[test]
    fn an_openrgb_open_without_its_server_holds_the_start() {
        let facts = Facts {
            openrgb_running: true,
            install: Some(exe()),
            version: Some(v(1, 1, None)),
            ..Facts::default()
        };
        assert_eq!(plan(&facts), Plan::Hold(Hold::OpenWithoutServer));
    }

    #[test]
    fn not_found_too_old_or_unreadable_is_held() {
        assert_eq!(plan(&Facts::default()), Plan::Hold(Hold::NotFound));
        let with = |version| Facts {
            install: Some(exe()),
            version,
            ..Facts::default()
        };
        assert_eq!(plan(&with(None)), Plan::Hold(Hold::UnreadableVersion));
        let old = v(0, 9, None);
        assert_eq!(plan(&with(Some(old))), Plan::Hold(Hold::TooOld(old)));
        let rc = v(1, 0, Some(2));
        assert_eq!(plan(&with(Some(rc))), Plan::Hold(Hold::TooOld(rc)));
        assert_eq!(plan(&with(Some(v(1, 1, None)))), Plan::Start(exe()));
    }

    #[test]
    fn the_arguments_are_these_and_nothing_else() {
        let config = PathBuf::from("data").join("openrgb");
        let c = command(&exe(), 51234, &config).unwrap();
        assert_eq!(c.program, PathBuf::from("OpenRGB.exe"));
        let expected: Vec<OsString> = [
            "--server",
            "--server-host",
            "127.0.0.1",
            "--server-port",
            "51234",
            "--noautoconnect",
            "--config",
        ]
        .iter()
        .map(OsString::from)
        .chain([
            config.as_os_str().to_owned(),
            "--loglevel".into(),
            "2".into(),
        ])
        .collect();
        assert_eq!(c.args, expected);
    }

    #[test]
    fn through_flatpak_the_folder_is_shared_first() {
        let install = Install::Flatpak {
            flatpak: PathBuf::from("/usr/bin/flatpak"),
        };
        let c = command(&install, 51234, Path::new("/data/candeo/openrgb")).unwrap();
        assert_eq!(c.program, PathBuf::from("/usr/bin/flatpak"));
        assert_eq!(
            &c.args[..3],
            &[
                "run",
                "--filesystem=/data/candeo/openrgb",
                "org.openrgb.OpenRGB"
            ]
        );
        assert_eq!(c.args[3], "--server");
        assert_eq!(c.args.len(), 13);
        assert_eq!(
            command(&install, 51234, Path::new("/data/odd:name")),
            Err(LaunchError::FlatpakFolder)
        );
    }

    #[test]
    fn never_on_the_default_port() {
        let p = Path::new("x");
        assert_eq!(
            command(&exe(), DEFAULT_PORT, p),
            Err(LaunchError::Port(DEFAULT_PORT))
        );
        assert_eq!(command(&exe(), 0, p), Err(LaunchError::Port(0)));
        let port = free_port().unwrap();
        assert_ne!(port, DEFAULT_PORT);
        assert_ne!(port, 0);
    }
}
