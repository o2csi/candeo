//! Whether this process runs from an MSIX package (#126).
//!
//! Inside one, the rules change: what the application writes under `AppData`
//! and in the registry goes to a location private to the package, the installed
//! files are read-only, and the Store — not the application — updates it. Code
//! that would write outside its own folder, or replace the executable, asks
//! here first.
//!
//! Everywhere else this is simply false: the question only exists on Windows.

/// True when Windows gave this process a package identity.
#[cfg(windows)]
pub fn packaged() -> bool {
    use std::sync::OnceLock;

    use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;

    /// `APPMODEL_ERROR_NO_PACKAGE`: this process has no package identity.
    const NO_PACKAGE: u32 = 15700;

    static PACKAGED: OnceLock<bool> = OnceLock::new();
    *PACKAGED.get_or_init(|| {
        let mut length = 0u32;
        // SAFETY: a null buffer asks for the length only, which the call writes
        // into `length`; it returns a code either way and reads nothing else.
        let status = unsafe { GetCurrentPackageFullName(&mut length, std::ptr::null_mut()) };
        status != NO_PACKAGE
    })
}

#[cfg(not(windows))]
pub fn packaged() -> bool {
    false
}
