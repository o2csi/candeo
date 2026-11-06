//! Windows' *Controlled folder access* (#394), which refuses writes to
//! `Documents` from the programs it has not allowed: the effects people write and
//! their device definitions live there (`docs/design/effects-sources.md` §7).
//!
//! Candeo reads the setting, only once a write there has failed, and says what
//! to do. It never changes it: adding itself to the allowed programs takes
//! administrator rights, and a program doing it would behave like the malware
//! the protection is for.

use std::io;
use std::path::{Component, Path};
use std::sync::Mutex;

use crate::failure::Failure;

/// The codes a refused write comes back with, measured on 2026-11-06 (Windows 11
/// build 26300, Defender 4.18.26100.4): creating a file or a folder, or copying
/// one in, fails with `ERROR_FILE_NOT_FOUND` (2), the "file not found" of #394;
/// renaming, overwriting or deleting one fails with `ERROR_ACCESS_DENIED` (5).
const REFUSALS: [i32; 2] = [2, 5];

/// `EnableControlledFolderAccess` when it blocks. 2 only audits, 3 and 4 are about
/// the disk's sectors, not files.
const BLOCKS: u8 = 1;

/// The setting, as a standard user reads it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Protection {
    /// `EnableControlledFolderAccess`.
    pub mode: u8,
    /// `ControlledFolderAccessAllowedApplications`. Only an administrator reads
    /// the programs; anyone else reads one line saying so, which names none.
    pub allowed: Vec<String>,
}

/// Whether Controlled folder access is what refused a write to `path`.
///
/// `code` is the system's error, `documents` the protected folder Candeo writes
/// in, `program` this executable. `protection` is read last, and only when the
/// rest points to it: it asks Windows Security, which takes up to a second and a
/// half.
pub fn refused(
    code: Option<i32>,
    path: &Path,
    documents: Option<&Path>,
    program: Option<&Path>,
    protection: impl FnOnce() -> Option<Protection>,
) -> bool {
    let suspect = code.is_some_and(|c| REFUSALS.contains(&c))
        && documents.is_some_and(|documents| under(path, documents));
    suspect
        && protection().is_some_and(|p| {
            let allowed = program
                .is_some_and(|program| p.allowed.iter().any(|a| same(Path::new(a), program)));
            p.mode == BLOCKS && !allowed
        })
}

/// Whether `path` is `folder` or inside it. Windows compares paths without case,
/// and this only runs there.
fn under(path: &Path, folder: &Path) -> bool {
    let mut parts = path.components();
    folder
        .components()
        .all(|f| parts.next().is_some_and(|p| alike(p, f)))
}

fn same(a: &Path, b: &Path) -> bool {
    a.components().count() == b.components().count() && under(a, b)
}

fn alike(a: Component<'_>, b: Component<'_>) -> bool {
    a.as_os_str().to_string_lossy().to_lowercase() == b.as_os_str().to_string_lossy().to_lowercase()
}

/// The system's words for the last write the protection refused, for the copied
/// diagnostic.
static LAST_REFUSAL: Mutex<Option<String>> = Mutex::new(None);

/// A write to `path` that failed with `error`: Controlled folder access when it
/// is what refused it, `detail` as an unexpected failure otherwise.
///
/// `detail` names the path through [`crate::paths::shown`]. When the protection
/// refused the write, it goes to the log and the copied diagnostic only: the
/// window says what to do (`AGENTS.md`, interface text).
pub fn failure(path: &Path, error: &io::Error, detail: String) -> Failure {
    let program = std::env::current_exe().ok();
    let documents = documents();
    if !refused(
        error.raw_os_error(),
        path,
        documents.as_deref(),
        program.as_deref(),
        read_protection,
    ) {
        return Failure::unexpected(detail);
    }
    tracing::warn!("Controlled folder access refused a write: {detail}");
    *LAST_REFUSAL.lock().unwrap_or_else(|e| e.into_inner()) = Some(detail);
    Failure::new("controlledFolderAccess")
}

/// The last write Controlled folder access refused since startup, as the system
/// said it.
pub fn last_refusal() -> Option<String> {
    LAST_REFUSAL
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// Opens Windows Security on *Ransomware protection*, the page holding
/// Controlled folder access and its allowed programs. A fixed address: the
/// window names no page.
#[tauri::command]
pub fn open_controlled_folder_access(app: tauri::AppHandle) -> crate::CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url("windowsdefender://ransomwareprotection", None::<&str>)
        .map_err(|e| Failure::unexpected(format!("cannot open Windows Security: {e}")))
}

/// `Documents`, which the protection covers by default. Elsewhere nothing
/// refuses Candeo's writes this way: macOS asks, see the design note.
#[cfg(windows)]
fn documents() -> Option<std::path::PathBuf> {
    dirs::document_dir()
}

#[cfg(not(windows))]
fn documents() -> Option<std::path::PathBuf> {
    None
}

#[cfg(not(windows))]
fn read_protection() -> Option<Protection> {
    None
}

/// How long a failed write waits for Windows Security: the query took 0.5 to
/// 1.6 s on a Windows 11 machine with the protection on (2026-11-06). Past this,
/// the failure is said as the system gave it, rather than holding the window.
#[cfg(windows)]
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(3);

/// The setting through WMI, in a thread of its own: commands run on the main
/// thread, whose COM apartment is the webview's, and a query that does not
/// answer is left behind there instead of holding a save.
#[cfg(windows)]
fn read_protection() -> Option<Protection> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("controlled-folders".into())
        .spawn(move || {
            let _ = sender.send(query());
        });
    if let Err(e) = spawned {
        tracing::warn!("Controlled folder access not read: {e}");
        return None;
    }
    match receiver.recv_timeout(PATIENCE) {
        Ok(Ok(protection)) => Some(protection),
        Ok(Err(e)) => {
            tracing::warn!("Controlled folder access not read: {e}");
            None
        }
        Err(_) => {
            tracing::warn!(
                "Controlled folder access not read within {} s",
                PATIENCE.as_secs()
            );
            None
        }
    }
}

#[cfg(windows)]
fn query() -> Result<Protection, String> {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

    #[derive(serde::Deserialize)]
    #[serde(rename = "MSFT_MpPreference", rename_all = "PascalCase")]
    struct MpPreference {
        enable_controlled_folder_access: Option<u8>,
        controlled_folder_access_allowed_applications: Option<Vec<String>>,
    }

    // Initialised here rather than by `wmi`, which would otherwise set the
    // process's COM security for everything else in it.
    // SAFETY: paired with `CoUninitialize` below, on this thread, once the
    // connection is dropped.
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .map_err(|e| format!("COM could not start: {e}"))?;
    let read = || -> Result<Protection, String> {
        let connection =
            wmi::WMIConnection::with_namespace_path(r"root\Microsoft\Windows\Defender")
                .map_err(|e| e.to_string())?;
        let preferences: Vec<MpPreference> = connection.query().map_err(|e| e.to_string())?;
        let preference = preferences
            .into_iter()
            .next()
            .ok_or("no MSFT_MpPreference instance")?;
        Ok(Protection {
            mode: preference.enable_controlled_folder_access.unwrap_or(0),
            allowed: preference
                .controlled_folder_access_allowed_applications
                .unwrap_or_default(),
        })
    };
    let protection = read();
    // SAFETY: `CoInitializeEx` succeeded on this thread, and what `read` made
    // is dropped.
    unsafe { CoUninitialize() };
    protection
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::PathBuf;

    use super::*;

    fn documents() -> PathBuf {
        Path::new("home").join("someone").join("Documents")
    }

    fn effect() -> PathBuf {
        documents()
            .join("candeo")
            .join("effects")
            .join("Rain.ts.tmp")
    }

    fn program() -> PathBuf {
        Path::new("apps").join("Candeo").join("candeo.exe")
    }

    fn blocking() -> Option<Protection> {
        Some(Protection {
            mode: BLOCKS,
            allowed: vec!["N/A: Must be an administrator to view exclusions".into()],
        })
    }

    fn verdict(code: i32, path: &Path, protection: Option<Protection>) -> bool {
        refused(
            Some(code),
            path,
            Some(&documents()),
            Some(&program()),
            || protection,
        )
    }

    #[test]
    fn a_refused_write_in_documents_is_the_protection_when_it_blocks() {
        // Creating the temporary file of a save, or the folder.
        assert!(verdict(2, &effect(), blocking()));
        // Renaming, overwriting or deleting.
        assert!(verdict(5, &effect(), blocking()));
        // Paths compare without case, as Windows does.
        let shouting = Path::new("HOME")
            .join("Someone")
            .join("DOCUMENTS")
            .join("x.ts");
        assert!(verdict(2, &shouting, blocking()));
    }

    #[test]
    fn the_protection_off_or_auditing_refuses_nothing() {
        for mode in [0, 2, 3, 4] {
            let protection = Some(Protection {
                mode,
                ..Protection::default()
            });
            assert!(!verdict(2, &effect(), protection), "mode {mode}");
        }
        // Windows Security did not answer: the system's error is said.
        assert!(!verdict(2, &effect(), None));
    }

    #[test]
    fn an_allowed_program_is_not_refused() {
        // What an administrator reads: the programs themselves.
        let protection = Some(Protection {
            mode: BLOCKS,
            allowed: vec![Path::new("APPS")
                .join("candeo")
                .join("Candeo.exe")
                .display()
                .to_string()],
        });
        assert!(!verdict(2, &effect(), protection));
        // Another program allowed is not this one.
        let other = Some(Protection {
            mode: BLOCKS,
            allowed: vec![program().join("elsewhere").display().to_string()],
        });
        assert!(verdict(2, &effect(), other));
    }

    #[test]
    fn windows_security_is_asked_only_for_a_refusal_in_documents() {
        let asked = Cell::new(false);
        let ask = || {
            asked.set(true);
            blocking()
        };
        // A full disk, a name Windows refuses: not the protection.
        let full = 112;
        assert!(!refused(
            Some(full),
            &effect(),
            Some(&documents()),
            None,
            ask
        ));
        // The settings, the cache and the shipped effects are in AppData.
        let settings = Path::new("home")
            .join("someone")
            .join("AppData")
            .join("settings.json");
        assert!(!refused(Some(2), &settings, Some(&documents()), None, ask));
        // A folder that only starts like Documents is another folder.
        let beside = Path::new("home")
            .join("someone")
            .join("Documents2")
            .join("x");
        assert!(!refused(Some(2), &beside, Some(&documents()), None, ask));
        // No documents folder, or an error without a system code.
        assert!(!refused(Some(2), &effect(), None, None, ask));
        assert!(!refused(None, &effect(), Some(&documents()), None, ask));
        assert!(!asked.get());

        assert!(refused(Some(2), &effect(), Some(&documents()), None, ask));
        assert!(asked.get());
    }

    #[test]
    fn a_refusal_crosses_as_a_code_and_logs_in_english() {
        let failure = Failure::new("controlledFolderAccess");
        assert_eq!(
            serde_json::to_value(&failure).unwrap(),
            serde_json::json!({ "code": "controlledFolderAccess", "params": {} })
        );
        assert!(failure.to_string().contains("Controlled folder access"));
    }

    /// Reads this machine's setting through WMI, as a failed write does. Ignored:
    /// it asks Windows Security, and only Defender answers.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn windows_security_answers_a_standard_user() {
        let protection = read_protection().expect("is Defender the antivirus?");
        eprintln!("EnableControlledFolderAccess = {}", protection.mode);
    }
}
