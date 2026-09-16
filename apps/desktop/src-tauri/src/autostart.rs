//! Launching Candeo at login, hidden in the notification area (#103).
//!
//! A firmware effect survives a reboot; an effect Candeo runs only comes back
//! once Candeo runs again.
//!
//! # The system's entry is the only record
//!
//! The Windows `Run` registry value, or an XDG autostart file. Nothing is kept in
//! `settings.json`: the system's own tools change the same entry (Task Manager's
//! startup apps, a desktop's session settings), and a copy would disagree with
//! them.
//!
//! No `tauri-plugin-autostart`: an entry is one registry value or one short
//! file, and the registry is already read for the diagnostic.
//!
//! # Not from a development build
//!
//! It would register a binary under `target/`, which the next build replaces
//! and `cargo clean` deletes.

use std::path::PathBuf;

use serde::Serialize;

use crate::{CmdResult, Failure};

/// The argument the entry passes: start without a window.
pub const HIDDEN: &str = "--hidden";

/// The name of the entry, in the registry and as a file name.
const NAME: &str = "candeo";

/// True when this process was launched by the entry.
///
/// The `Run` value and the XDG entry pass [`HIDDEN`]. A packaged application's
/// startup task passes **nothing** — the extension takes no arguments — so
/// there the question is asked of Windows: what activated this process.
pub fn launched_hidden() -> bool {
    std::env::args().skip(1).any(|arg| arg == HIDDEN) || platform::started_at_login()
}

/// Why no entry can be written, when none can.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Refused {
    /// A development build: the entry would name a binary under `target/`.
    Development,
    /// A system Candeo writes no entry for.
    Unsupported,
}

/// What went wrong turning the entry on.
pub enum Trouble {
    /// Windows holds the answer: someone turned the startup task off in Task
    /// Manager, or a policy did, and only they can turn it back on. Carries the
    /// code the catalogs translate.
    Held(&'static str),
    /// Anything else, in English, for the log and the copied diagnostic.
    Unexpected(String),
}

impl From<String> for Trouble {
    fn from(detail: String) -> Self {
        Trouble::Unexpected(detail)
    }
}

/// What a startup task's state means for the setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Meaning {
    /// It runs at login.
    On,
    /// It does not, and turning it on is this application's to do.
    Off,
    /// Turned off in Task Manager: only the person who did it can undo it.
    HeldByUser,
    /// Turned off by a policy on this computer.
    HeldByPolicy,
}

/// What Windows means by a `StartupTaskState`.
///
/// The values are the WinRT enumeration's, and they are read rather than
/// trusted: anything unknown is treated as off, never as running.
fn meaning(state: i32) -> Meaning {
    match state {
        // Disabled
        0 => Meaning::Off,
        // DisabledByUser
        1 => Meaning::HeldByUser,
        // Enabled, EnabledByPolicy
        2 | 4 => Meaning::On,
        // DisabledByPolicy
        3 => Meaning::HeldByPolicy,
        _ => Meaning::Off,
    }
}

/// Why an entry cannot be written here, or `None` when it can.
fn refused(development: bool, supported: bool) -> Option<Refused> {
    match (development, supported) {
        (true, _) => Some(Refused::Development),
        (_, false) => Some(Refused::Unsupported),
        _ => None,
    }
}

fn why() -> Option<Refused> {
    refused(
        cfg!(debug_assertions),
        cfg!(any(windows, target_os = "linux")),
    )
}

/// Whether this build may write an entry.
fn available() -> bool {
    why().is_none()
}

/// The entry's state, as Settings reads it.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LaunchAtLogin {
    pub enabled: bool,
    /// False in a development build, from an MSIX package, and on a system
    /// Candeo writes no entry for.
    pub available: bool,
    /// What `available: false` is about; `None` when it is true.
    pub refused: Option<Refused>,
}

fn status() -> LaunchAtLogin {
    LaunchAtLogin {
        enabled: platform::enabled(),
        available: available(),
        refused: why(),
    }
}

#[tauri::command]
pub fn get_launch_at_login() -> LaunchAtLogin {
    status()
}

/// Writes or removes the entry.
#[tauri::command]
pub fn set_launch_at_login(on: bool) -> CmdResult<LaunchAtLogin> {
    if !available() {
        return Err(Failure::unexpected(
            "launch at login is written only by a release build on Windows or Linux",
        ));
    }
    let written = if on {
        executable()
            .map_err(Trouble::Unexpected)
            .and_then(|exe| platform::enable(&exe))
    } else {
        platform::disable().map_err(Trouble::Unexpected)
    };
    written.map_err(|trouble| match trouble {
        Trouble::Held(code) => {
            tracing::warn!(code, "launch at login is held by Windows");
            Failure::new(code)
        }
        Trouble::Unexpected(detail) => {
            tracing::warn!("launch at login not changed: {detail}");
            Failure::unexpected(detail)
        }
    })?;
    tracing::info!(on, "launch at login changed");
    Ok(status())
}

/// Rewrites the entry when the file it starts is gone.
///
/// The entry records a full path, and the file's name changed once (#143): an
/// entry written by an earlier version names a file that no longer exists, so
/// nothing starts at login while the setting still reads *on*. A copy moved
/// elsewhere leaves the same entry behind.
///
/// An entry naming another file that **does** exist is left alone: two
/// installations on one account is a decision, not a mistake.
pub fn repair() {
    if !available() || !platform::enabled() {
        return;
    }
    let Some(recorded) = platform::command().as_deref().and_then(starts) else {
        return;
    };
    if PathBuf::from(&recorded).exists() {
        return;
    }
    let written = executable()
        .map_err(Trouble::Unexpected)
        .and_then(|exe| platform::enable(&exe));
    match written {
        Ok(()) => tracing::info!(
            gone = %crate::paths::shown(&PathBuf::from(recorded)),
            "launch at login named a file that is gone, rewritten"
        ),
        Err(Trouble::Held(code)) => {
            tracing::warn!(code, "launch at login not rewritten: Windows holds it")
        }
        Err(Trouble::Unexpected(detail)) => {
            tracing::warn!("launch at login not rewritten: {detail}")
        }
    }
}

/// The file a recorded command starts, quoted or not.
fn starts(command: &str) -> Option<String> {
    let path = match command.strip_prefix('"') {
        Some(rest) => rest.split('"').next()?,
        None => command.split_whitespace().next()?,
    };
    (!path.is_empty()).then(|| path.to_string())
}

/// The file the entry starts.
///
/// An AppImage runs from a mount that changes at every launch: the entry must
/// start the image itself, which the runtime names in `APPIMAGE`.
fn executable() -> Result<PathBuf, String> {
    #[cfg(target_os = "linux")]
    if let Some(image) = std::env::var_os("APPIMAGE") {
        return Ok(PathBuf::from(image));
    }
    std::env::current_exe().map_err(|e| format!("executable path not read: {e}"))
}

#[cfg(windows)]
mod platform {
    use std::path::Path;

    use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows_sys::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ,
        RRF_RT_REG_BINARY, RRF_RT_REG_SZ,
    };

    use super::{HIDDEN, NAME};

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    /// Where Task Manager records a startup app turned off, without removing it
    /// from `Run`.
    const APPROVED: &str =
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    // Two ways of starting at login, and which one applies is not a preference:
    // a packaged application's `Run` value goes to a store nothing reads at
    // login, and only a package can declare a startup task.

    pub fn enabled() -> bool {
        if crate::msix::packaged() {
            task::enabled()
        } else {
            run_value_enabled()
        }
    }

    /// The command the entry runs, when the entry records one.
    ///
    /// A startup task records none: it names the executable of the package it
    /// belongs to, which no update can leave behind.
    pub fn command() -> Option<String> {
        if crate::msix::packaged() {
            None
        } else {
            run_value_command()
        }
    }

    pub fn enable(exe: &Path) -> Result<(), super::Trouble> {
        if crate::msix::packaged() {
            task::enable()
        } else {
            run_value_enable(exe).map_err(super::Trouble::Unexpected)
        }
    }

    pub fn disable() -> Result<(), String> {
        if crate::msix::packaged() {
            task::disable()
        } else {
            run_value_disable()
        }
    }

    /// Whether Windows started this process for the startup task.
    pub fn started_at_login() -> bool {
        crate::msix::packaged() && task::started_at_login()
    }

    fn run_value_enabled() -> bool {
        let (run, name) = (wide(RUN), wide(NAME));
        let mut size = 0u32;
        // SAFETY: both strings end in NUL; a null buffer only asks for the size.
        let present = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                run.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        } == 0;
        if !present {
            return false;
        }

        let approved = wide(APPROVED);
        let mut flags = [0u8; 12];
        let mut size = flags.len() as u32;
        // SAFETY: `size` is the buffer's size in bytes; both strings end in NUL.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                approved.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_BINARY,
                std::ptr::null_mut(),
                flags.as_mut_ptr().cast(),
                &mut size,
            )
        };
        status != 0 || super::approved(&flags[..size as usize])
    }

    /// The command the entry runs, as the registry holds it.
    fn run_value_command() -> Option<String> {
        let (run, name) = (wide(RUN), wide(NAME));
        let mut size = 0u32;
        // SAFETY: both strings end in NUL; a null buffer only asks for the size,
        // in bytes.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                run.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if status != 0 {
            return None;
        }

        let mut buffer = vec![0u16; size as usize / 2 + 1];
        let mut read = (buffer.len() * 2) as u32;
        // SAFETY: `read` is the buffer's size in bytes, and the call writes at
        // most that much into it.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                run.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut read,
            )
        };
        if status != 0 {
            return None;
        }
        // The value ends in NUL, which is not part of the command.
        let end = (read as usize / 2).saturating_sub(1).min(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..end]))
    }

    fn run_value_enable(exe: &Path) -> Result<(), String> {
        let command = wide(&format!("\"{}\" {HIDDEN}", exe.display()));
        let (run, name) = (wide(RUN), wide(NAME));
        // SAFETY: the data is `command`, NUL included, its size given in bytes.
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                run.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                command.as_ptr().cast(),
                (command.len() * 2) as u32,
            )
        };
        if status != 0 {
            return Err(format!("Run value not written: error {status}"));
        }
        // Turned on from Candeo is a decision: an earlier "off" from Task Manager
        // would otherwise keep the new entry from running.
        delete(APPROVED)
    }

    fn run_value_disable() -> Result<(), String> {
        delete(RUN)?;
        delete(APPROVED)
    }

    /// The startup task an MSIX package declares, which Windows runs at login.
    ///
    /// Declared in `packaging/windows/AppxManifest.xml`, off until someone turns
    /// the setting on. Windows keeps the last word: a task turned off in Task
    /// Manager cannot be turned back on from here, and
    /// [`StartupTask::RequestEnableAsync`] says so by returning the state it
    /// left it in.
    mod task {
        use windows::core::HSTRING;
        use windows::ApplicationModel::Activation::ActivationKind;
        use windows::ApplicationModel::{AppInstance, StartupTask};

        use super::super::{Meaning, Trouble};

        /// The `TaskId` of the manifest's `windows.startupTask` extension.
        const ID: &str = "CandeoStartup";

        fn task() -> Result<StartupTask, String> {
            StartupTask::GetAsync(&HSTRING::from(ID))
                .and_then(|pending| pending.get())
                .map_err(|e| format!("startup task not read: {e}"))
        }

        fn meaning(task: &StartupTask) -> Result<Meaning, String> {
            task.State()
                .map(|state| super::super::meaning(state.0))
                .map_err(|e| format!("startup task state not read: {e}"))
        }

        pub fn enabled() -> bool {
            task()
                .and_then(|task| meaning(&task))
                .is_ok_and(|meaning| meaning == Meaning::On)
        }

        pub fn enable() -> Result<(), Trouble> {
            let task = task()?;
            let state = task
                .RequestEnableAsync()
                .and_then(|pending| pending.get())
                .map_err(|e| format!("startup task not turned on: {e}"))?;
            match super::super::meaning(state.0) {
                Meaning::On => Ok(()),
                Meaning::HeldByUser => Err(Trouble::Held("startupHeldByUser")),
                Meaning::HeldByPolicy => Err(Trouble::Held("startupHeldByPolicy")),
                Meaning::Off => Err(Trouble::Unexpected(
                    "the startup task stayed off, and Windows gave no reason".into(),
                )),
            }
        }

        pub fn disable() -> Result<(), String> {
            task()?
                .Disable()
                .map_err(|e| format!("startup task not turned off: {e}"))
        }

        /// Whether Windows activated this process for the startup task.
        ///
        /// Asked once and remembered: the arguments come back on the **first**
        /// call only, and they are read at startup, before the window.
        pub fn started_at_login() -> bool {
            use std::sync::OnceLock;

            static AT_LOGIN: OnceLock<bool> = OnceLock::new();
            *AT_LOGIN.get_or_init(|| {
                AppInstance::GetActivatedEventArgs()
                    .and_then(|args| args.Kind())
                    .is_ok_and(|kind| kind == ActivationKind::StartupTask)
            })
        }
    }

    fn delete(key: &str) -> Result<(), String> {
        let (key_w, name) = (wide(key), wide(NAME));
        // SAFETY: both strings end in NUL.
        let status =
            unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key_w.as_ptr(), name.as_ptr()) };
        match status {
            0 | ERROR_FILE_NOT_FOUND => Ok(()),
            _ => Err(format!("value not removed from {key}: error {status}")),
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::path::{Path, PathBuf};

    use super::NAME;

    /// `$XDG_CONFIG_HOME/autostart`, or `~/.config/autostart`.
    fn entry() -> Option<PathBuf> {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
        Some(config.join("autostart").join(format!("{NAME}.desktop")))
    }

    pub fn enabled() -> bool {
        entry()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .is_some_and(|content| !super::entry_turned_off(&content))
    }

    /// The command the entry runs, as its `Exec` line holds it.
    pub fn command() -> Option<String> {
        let content = std::fs::read_to_string(entry()?).ok()?;
        super::exec_value(&content)
    }

    pub fn enable(exe: &Path) -> Result<(), super::Trouble> {
        write(exe).map_err(super::Trouble::Unexpected)
    }

    fn write(exe: &Path) -> Result<(), String> {
        let path = entry().ok_or("no configuration folder: neither XDG_CONFIG_HOME nor HOME")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("autostart folder not created: {e}"))?;
        }
        std::fs::write(&path, super::desktop_entry(&exe.to_string_lossy()))
            .map_err(|e| format!("autostart entry not written: {e}"))
    }

    /// The entry passes `--hidden`, which [`super::launched_hidden`] reads.
    pub fn started_at_login() -> bool {
        false
    }

    pub fn disable() -> Result<(), String> {
        let Some(path) = entry() else { return Ok(()) };
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(format!("autostart entry not removed: {e}"))
            }
            _ => Ok(()),
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use std::path::Path;

    pub fn enabled() -> bool {
        false
    }

    pub fn command() -> Option<String> {
        None
    }

    pub fn started_at_login() -> bool {
        false
    }

    pub fn enable(_exe: &Path) -> Result<(), super::Trouble> {
        Err("launch at login is not written on this system".into())
    }

    pub fn disable() -> Result<(), String> {
        Ok(())
    }
}

/// Whether Task Manager leaves a startup app on: the first byte of its record is
/// even when on (`02`, `06`), odd when turned off (`03`, `07`).
#[cfg_attr(not(windows), allow(dead_code))]
fn approved(record: &[u8]) -> bool {
    !matches!(record.first(), Some(flag) if flag & 1 == 1)
}

/// The XDG autostart entry starting `exe` hidden.
///
/// `Exec` quotes the path, escaping what the specification reserves inside
/// quotes; a backslash is then escaped once more, as any string value is.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn desktop_entry(exe: &str) -> String {
    let mut quoted = String::from('"');
    for c in exe.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    let exec = quoted.replace('\\', "\\\\");
    format!(
        "[Desktop Entry]\nType=Application\nName={NAME}\nExec={exec} {HIDDEN}\nX-GNOME-Autostart-enabled=true\n"
    )
}

/// The `Exec` value of an entry, as [`desktop_entry`] wrote it.
///
/// Twice unescaped, since the escaping is applied twice: once inside the
/// quotes, once for the string value that carries them.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn exec_value(content: &str) -> Option<String> {
    let line = content
        .lines()
        .find_map(|line| line.trim().strip_prefix("Exec="))?;
    Some(unescape(&unescape(line)))
}

/// Drops one backslash before the character it escapes.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.extend(chars.next()),
            _ => out.push(c),
        }
    }
    out
}

/// An entry a desktop's session settings turned off without deleting it.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn entry_turned_off(content: &str) -> bool {
    content.lines().any(|line| {
        matches!(
            line.trim(),
            "Hidden=true" | "X-GNOME-Autostart-enabled=false"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_manager_turning_a_startup_app_off_is_read() {
        assert!(approved(&[0x02, 0, 0, 0]));
        assert!(approved(&[0x06, 0, 0, 0]));
        assert!(!approved(&[0x03, 0, 0, 0]));
        assert!(!approved(&[0x07, 0, 0, 0]));
        // No record: nobody turned it off.
        assert!(approved(&[]));
    }

    #[test]
    fn the_autostart_entry_quotes_the_executable() {
        let entry = desktop_entry("/opt/can deo/candeo");
        assert!(
            entry.contains("\nExec=\"/opt/can deo/candeo\" --hidden\n"),
            "{entry}"
        );
        assert!(entry.starts_with("[Desktop Entry]\n"));

        // `$` is escaped inside the quotes, and that backslash once more as a string.
        let entry = desktop_entry("/home/someone/$bin/candeo");
        assert!(
            entry.contains(r#"Exec="/home/someone/\\$bin/candeo" --hidden"#),
            "{entry}"
        );
    }

    #[test]
    fn what_refuses_an_entry_is_told_apart() {
        // A development build first: it is why nothing is written, whatever the
        // system says.
        assert_eq!(refused(true, true), Some(Refused::Development));
        assert_eq!(refused(true, false), Some(Refused::Development));
        assert_eq!(refused(false, false), Some(Refused::Unsupported));
        // An installed build on a system with an entry, packaged or not.
        assert_eq!(refused(false, true), None);
    }

    /// Windows keeps the last word on a startup task, and the two states that
    /// say so must not read as a plain "off": one asks the person to undo what
    /// they did in Task Manager, the other says a policy decided.
    #[test]
    fn a_startup_task_state_says_who_holds_it() {
        assert_eq!(meaning(2), Meaning::On);
        assert_eq!(meaning(4), Meaning::On);
        assert_eq!(meaning(0), Meaning::Off);
        assert_eq!(meaning(1), Meaning::HeldByUser);
        assert_eq!(meaning(3), Meaning::HeldByPolicy);
        // A state this version does not know is off, never running.
        assert_eq!(meaning(42), Meaning::Off);
    }

    #[test]
    fn the_file_a_recorded_command_starts_is_read() {
        // What Windows holds: the path quoted, the argument outside.
        assert_eq!(
            starts(r#""C:\Program Files\Candeo\candeo.exe" --hidden"#).as_deref(),
            Some(r"C:\Program Files\Candeo\candeo.exe")
        );
        // Unquoted, as someone may have written it by hand.
        assert_eq!(
            starts("/usr/bin/candeo --hidden").as_deref(),
            Some("/usr/bin/candeo")
        );
        assert_eq!(starts(""), None);
    }

    #[test]
    fn the_file_an_autostart_entry_starts_is_read_back() {
        for exe in [
            "/opt/can deo/candeo",
            "/home/someone/$bin/candeo",
            r"/home/someone/back\slash/candeo",
        ] {
            let read = exec_value(&desktop_entry(exe)).and_then(|exec| starts(&exec));
            assert_eq!(read.as_deref(), Some(exe));
        }
    }

    #[test]
    fn an_entry_turned_off_by_the_desktop_is_read() {
        assert!(entry_turned_off("[Desktop Entry]\nHidden=true\n"));
        assert!(entry_turned_off(
            "[Desktop Entry]\nX-GNOME-Autostart-enabled=false\n"
        ));
        assert!(!entry_turned_off(&desktop_entry("/usr/bin/candeo")));
    }
}
