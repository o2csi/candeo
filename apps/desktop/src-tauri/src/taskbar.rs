//! The big window icon, the one the Windows taskbar reads.
//!
//! A window carries two icons on Windows: a small one, which the title bar and
//! Alt+Tab show, and a big one, which the taskbar button shows. tao sets the
//! small one from the icon Tauri passes, and the big one only through its own
//! Windows extension, which Tauri never uses. With neither a big icon nor a
//! class icon, the taskbar falls back to what the shell cached for the
//! executable's path: on a machine that ran an earlier version, the icon that
//! version shipped, which outlives the update.
//!
//! The icon comes from the executable's own resources, where the bundler wrote
//! `icon.ico` under `IDI_APPLICATION`: the group holds every size, and Windows
//! picks the one that fits the size it is drawing.

/// Gives the window its big icon. Does nothing anywhere but Windows.
#[cfg(windows)]
pub(crate) fn place<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        LoadImageW, SendMessageW, ICON_BIG, IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED,
        WM_SETICON,
    };

    let hwnd = match window.hwnd() {
        Ok(hwnd) => hwnd.0,
        // The window exists, so this does not happen; if it ever did, the
        // taskbar would keep the icon it has and nothing else would break.
        Err(e) => {
            tracing::warn!(error = %e, "window handle unavailable, taskbar icon left alone");
            return;
        }
    };

    // SAFETY: `hwnd` comes from the window Tauri just built, and the icon is
    // read from this executable's resources. `LR_SHARED` leaves its lifetime to
    // the system, which is what a window icon needs: it outlives this call.
    let icon = unsafe {
        LoadImageW(
            GetModuleHandleW(std::ptr::null()),
            IDI_APPLICATION,
            IMAGE_ICON,
            // The size the system draws a big icon at, this display's scaling
            // included, and the group's closest image for it.
            0,
            0,
            LR_DEFAULTSIZE | LR_SHARED,
        )
    };
    if icon.is_null() {
        tracing::warn!("no icon in the executable, taskbar icon left alone");
        return;
    }

    unsafe { SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, icon as isize) };
}

/// Elsewhere a window has one icon, and Tauri already set it.
#[cfg(not(windows))]
pub(crate) fn place<R: tauri::Runtime>(_window: &tauri::WebviewWindow<R>) {}
