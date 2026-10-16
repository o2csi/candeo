//! The machine Candeo runs on, by its model: what a definition naming one is
//! matched against (`docs/design/device-sdk.md` §9, #326).
//!
//! A model, not a unit: `Alienware 17 R5` names thousands of machines. It goes
//! to the log and the copied diagnostic, where whoever writes a definition reads
//! it.

use std::sync::OnceLock;

/// The system's product name as SMBIOS gives it, read on first use — at
/// startup, by the log — and kept for the process: the machine does not change
/// under it. `None` when the system does not say.
pub(crate) fn model() -> Option<&'static str> {
    static MODEL: OnceLock<Option<String>> = OnceLock::new();
    MODEL
        .get_or_init(|| imp::product_name().and_then(|raw| product_name(&raw)))
        .as_deref()
}

/// The name as definitions compare it: without the spaces and NULs a firmware
/// pads it with. An empty one names nothing.
fn product_name(raw: &str) -> Option<String> {
    let name = raw.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    (!name.is_empty()).then(|| name.to_owned())
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    /// What Windows copies from SMBIOS at boot, readable without administrator
    /// rights and without WMI.
    pub fn product_name() -> Option<String> {
        let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(Some(0)).collect() };
        let key = wide(r"HARDWARE\DESCRIPTION\System\BIOS");
        let name = wide("SystemProductName");
        let mut buffer = [0u16; 256];
        let mut size = std::mem::size_of_val(&buffer) as u32;
        // SAFETY: `size` is the buffer's size in bytes; both strings end in NUL.
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        let chars = (size as usize / 2).saturating_sub(1);
        (status == 0).then(|| String::from_utf16_lossy(&buffer[..chars]))
    }
}

#[cfg(target_os = "linux")]
mod imp {
    /// The kernel's copy of SMBIOS, readable by every user.
    pub fn product_name() -> Option<String> {
        std::fs::read_to_string("/sys/class/dmi/id/product_name").ok()
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    pub fn product_name() -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// sysfs ends the name with a newline, and some firmwares pad it: a
    /// definition naming `Alienware 17 R5` must still find it.
    #[test]
    fn the_model_is_the_name_without_its_padding() {
        assert_eq!(
            product_name("Alienware 17 R5\n").as_deref(),
            Some("Alienware 17 R5")
        );
        assert_eq!(
            product_name("  Alienware 17 R5\0\0").as_deref(),
            Some("Alienware 17 R5")
        );
        assert_eq!(product_name(" \n"), None);
        assert_eq!(product_name(""), None);
    }
}
