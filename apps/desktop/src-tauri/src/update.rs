//! Whether a newer version exists — asked, never applied (#139).
//!
//! The window asks GitHub for the latest release and compares it with what is
//! running; nothing is downloaded and nothing is installed. This module says
//! what the window may do: which version runs, whether asking makes sense here,
//! and whether someone left the check on.
//!
//! # Why not in a package
//!
//! The Store updates what it installed, on its own schedule: right after a
//! release the package would announce a version the Store has not shipped yet,
//! and the only thing to act on would be an installer that puts a second,
//! unpackaged copy beside it. The setting stays visible and says so.

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::{CmdResult, Failure};

/// Where the releases live. A page this application opens is one of these.
///
/// From `repository` in `Cargo.toml`, so moving the repository — another
/// account, another organisation — is one line there and not a hunt through
/// the sources. GitHub redirects a repository that moved, but a redirect is
/// not something to build on: it stops the day someone takes the old name.
const RELEASES: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases/");

/// The repository, as the window's request needs it.
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// What Settings needs to show the version and its check.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    /// The version running, as the bundle declares it.
    pub version: String,
    /// False in an MSIX package, where the Store does the updating.
    pub available: bool,
    /// Whether the check runs once per launch. Read from `settings.json`, and
    /// meaningless when `available` is false.
    pub enabled: bool,
    /// What the window asks for the latest release: GitHub's API, for the
    /// repository this build came from.
    pub latest: String,
}

/// GitHub's address for the latest release of a repository.
///
/// `https://github.com/owner/name` becomes
/// `https://api.github.com/repos/owner/name/releases/latest`. Anything that is
/// not a GitHub repository address gives `None`, and the window then has
/// nothing to ask — which is what a fork published elsewhere should get, rather
/// than this repository's releases.
fn latest_release(repository: &str) -> Option<String> {
    let path = repository
        .trim_end_matches('/')
        .strip_prefix("https://github.com/")?;
    let (owner, name) = path.split_once('/')?;
    if owner.is_empty() || name.is_empty() || name.contains('/') {
        return None;
    }
    Some(format!(
        "https://api.github.com/repos/{owner}/{name}/releases/latest"
    ))
}

/// Opens a release's page in the browser.
///
/// The address comes back from GitHub through the window, so it is checked here
/// rather than trusted: this opens a page of this repository's releases, and
/// nothing else. A window that has been made to say something else — an
/// answer tampered with on the way — cannot turn this into "open anything".
#[tauri::command]
pub fn open_release(app: AppHandle, url: String) -> CmdResult<()> {
    if !is_release_page(&url) {
        return Err(Failure::unexpected(format!(
            "not a release page of this repository: {url}"
        )));
    }
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| Failure::unexpected(format!("cannot open {url}: {e}")))
}

/// Whether an address is a page of this repository's releases.
fn is_release_page(url: &str) -> bool {
    url.starts_with(RELEASES) && !url.contains(['"', '\'', ' ', '\\'])
}

#[tauri::command]
pub fn get_update_check(app: AppHandle) -> CmdResult<UpdateCheck> {
    let enabled = crate::storage::store(&app)?
        .read_settings()?
        .preferences
        .check_for_updates;
    let latest = latest_release(REPOSITORY);
    Ok(UpdateCheck {
        version: app.package_info().version.to_string(),
        // Nothing to ask, nothing to offer: a build whose repository is not on
        // GitHub has no releases to compare against.
        available: !crate::msix::packaged() && latest.is_some(),
        enabled,
        latest: latest.unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The address is checked, not trusted: it comes back from GitHub, through
    /// the window, and this command is the one thing that opens a browser.
    #[test]
    fn only_a_release_page_of_this_repository_opens() {
        assert!(is_release_page(
            "https://github.com/oorabona/candeo/releases/tag/v0.5.0"
        ));
        for elsewhere in [
            "https://github.com/someone/else/releases/tag/v1",
            "http://github.com/oorabona/candeo/releases/tag/v1",
            "https://github.com/oorabona/candeo/issues/139",
            "file:///C:/Windows/System32/cmd.exe",
            "",
        ] {
            assert!(!is_release_page(elsewhere), "{elsewhere}");
        }
        // Nothing that could carry a second argument along.
        assert!(!is_release_page(
            "https://github.com/oorabona/candeo/releases/tag/v1 --flag"
        ));
    }

    /// Both addresses come from `repository` in `Cargo.toml`: a repository that
    /// moves is one line there, and the allowed pages move with it.
    #[test]
    fn the_addresses_follow_the_repository() {
        assert_eq!(
            latest_release("https://github.com/someone/candeo").as_deref(),
            Some("https://api.github.com/repos/someone/candeo/releases/latest")
        );
        // A trailing slash, as a manifest may carry it.
        assert_eq!(
            latest_release("https://github.com/an-org/candeo/").as_deref(),
            Some("https://api.github.com/repos/an-org/candeo/releases/latest")
        );
        // Not GitHub, or not a repository: nothing to ask.
        for elsewhere in [
            "https://gitlab.com/someone/candeo",
            "https://github.com/someone",
            "https://github.com/someone/candeo/tree/main",
            "",
        ] {
            assert_eq!(latest_release(elsewhere), None, "{elsewhere}");
        }
    }

    /// What this build was compiled with: the allowed pages and the address
    /// asked belong to the same repository, whichever it is.
    #[test]
    fn this_build_asks_about_its_own_repository() {
        let latest = latest_release(REPOSITORY).expect("a GitHub repository");
        let path = RELEASES
            .strip_prefix("https://github.com/")
            .and_then(|rest| rest.strip_suffix("/releases/"))
            .expect("the releases of a GitHub repository");
        assert!(latest.contains(&format!("/repos/{path}/")), "{latest}");
    }
}
