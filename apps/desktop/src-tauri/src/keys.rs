//! What a key is called, in the keyboard layout the system uses.
//!
//! Layouts carry scancodes, not legends: the legend depends on the layout
//! variant, and the system already knows the one in use. Windows names a scancode
//! through `GetKeyNameTextW`, which reads the layout without touching its state —
//! unlike `ToUnicodeEx`, which consumes a pending dead key and would turn the "ê"
//! someone is typing elsewhere into "^e".
//!
//! Elsewhere there is no label yet: it is optional for effects and the window.

/// The name the system gives this scancode (see `candeo_device::Key::scancode`),
/// or `None` when it has none or when the platform cannot say.
pub fn label(scancode: u16) -> Option<String> {
    if scancode == candeo_device::NO_SCANCODE {
        return None;
    }
    imp::label(scancode)
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyNameTextW;

    pub fn label(scancode: u16) -> Option<String> {
        // `GetKeyNameTextW` swaps two keys against what the keyboard sends: it
        // wants Num Lock (0x45) flagged extended and Pause (E1 1D 45) as a plain
        // 0x45. Taken literally, Pause read "CTRL" and Num Lock "Pause" (checked
        // on Windows 11, French layout).
        let (make, extended) = match scancode {
            0x45 => (0x45, 1),
            0xE11D => (0x45, 0),
            _ => (i32::from(scancode & 0xFF), i32::from(scancode >> 8 == 0xE0)),
        };
        let lparam = (make << 16) | (extended << 24);
        let mut name = [0u16; 64];
        // SAFETY: the buffer is valid for its length, which is what is passed.
        let len = unsafe { GetKeyNameTextW(lparam, name.as_mut_ptr(), name.len() as i32) };
        (len > 0).then(|| String::from_utf16_lossy(&name[..len as usize]))
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn label(_scancode: u16) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key that sends something has a name, whatever the layout the machine
    /// running the test uses — which is why no name is compared to a string.
    #[test]
    #[cfg(windows)]
    fn every_key_that_sends_something_is_named() {
        for key in candeo_device::DEATHSTALKER_V2_PRO.keys {
            let name = label(key.scancode);
            if key.scancode == candeo_device::NO_SCANCODE {
                assert_eq!(name, None, "index {}", key.index);
            } else {
                assert!(name.is_some_and(|n| !n.is_empty()), "index {}", key.index);
            }
        }
    }

    /// The two keys `GetKeyNameTextW` swaps: Pause is not named like left Ctrl, nor
    /// like Num Lock.
    #[test]
    #[cfg(windows)]
    fn pause_and_num_lock_keep_their_own_names() {
        let (pause, num_lock, ctrl) = (label(0xE11D), label(0x45), label(0x1D));
        assert_ne!(pause, ctrl);
        assert_ne!(pause, num_lock);
    }

    #[test]
    #[cfg(not(windows))]
    fn no_label_without_a_system_to_ask() {
        assert_eq!(label(0x11), None);
    }
}
