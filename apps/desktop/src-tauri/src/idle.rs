//! How long since someone last used the computer, for the `idle` trigger of
//! automations (#179, `docs/design/inputs-and-automations.md` §3.6).
//!
//! The system's own count of the last input — a key, the mouse — and never key
//! capture: Candeo reads keys only while an effect asks for them
//! (`docs/design/key-input.md`), an idle rule needs no key at all, and someone
//! using only the mouse is not idle.

/// Milliseconds since the last input in this session, or `None` where the system
/// does not say — every system but Windows, for now (#183).
pub fn idle_ms() -> Option<i64> {
    imp::idle_ms()
}

/// Whether the idle trigger can apply here: the Automations tab says when not.
#[tauri::command]
pub fn idle_available() -> bool {
    idle_ms().is_some()
}

/// Milliseconds between two readings of a 32-bit millisecond count, which wraps
/// around every 49.7 days.
#[cfg_attr(not(windows), allow(dead_code))]
fn elapsed(now: u32, then: u32) -> i64 {
    i64::from(now.wrapping_sub(then))
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    pub fn idle_ms() -> Option<i64> {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        // SAFETY: `info` is a LASTINPUTINFO with `cbSize` set, as the call requires.
        if unsafe { GetLastInputInfo(&mut info) } == 0 {
            return None;
        }
        // SAFETY: no arguments; the same 32-bit count `dwTime` comes from.
        let now = unsafe { GetTickCount() };
        Some(super::elapsed(now, info.dwTime))
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn idle_ms() -> Option<i64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::elapsed;

    #[test]
    fn the_count_wrapping_around_still_gives_the_time_elapsed() {
        assert_eq!(elapsed(5_000, 2_000), 3_000);
        assert_eq!(elapsed(1_000, u32::MAX - 999), 2_000);
    }
}
