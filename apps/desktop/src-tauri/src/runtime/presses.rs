//! Key presses, for the effects that declare them — `docs/design/key-input.md`.
//!
//! Reading keys system-wide is what a keylogger does. What keeps this from being
//! one is held here, not in the callers:
//!
//! - presses are captured only while a [`Reading`] exists, that is while a loop
//!   runs an effect declaring `inputs: ['keys']`;
//! - only scancodes, instants and the keyboard's VID/PID are kept, never a
//!   character;
//! - nothing about a press is logged, at any level;
//! - at most [`KEEP_AT_MOST`] presses younger than [`KEEP_FOR`], and none once
//!   the last reader is gone.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use candeo_device::Layout;

/// How long a press stays available to effects.
pub const KEEP_FOR: Duration = Duration::from_secs(10);

/// How many presses are kept at most: more than any trail an effect draws.
#[cfg_attr(not(windows), allow(dead_code))]
pub const KEEP_AT_MOST: usize = 32;

/// A key going down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Press {
    pub at: Instant,
    /// As `candeo_device::Key::scancode` writes it.
    pub scancode: u16,
    /// The keyboard's VID and PID, or `None` when the system does not say —
    /// input injected by software, for one.
    pub device: Option<(u16, u16)>,
}

/// Stops a capture when dropped.
#[cfg_attr(not(windows), allow(dead_code))]
pub struct Capture(Option<Box<dyn FnOnce() + Send>>);

impl Drop for Capture {
    fn drop(&mut self) {
        if let Some(stop) = self.0.take() {
            stop();
        }
    }
}

#[derive(Default)]
struct State {
    ring: VecDeque<Press>,
    readers: usize,
    capture: Option<Capture>,
}

/// The presses captured for the loops that read them.
pub struct Presses {
    state: Mutex<State>,
    /// Starts what feeds [`Presses::push`]. The system's capture in the
    /// application; nothing in the tests, which push presses themselves.
    start: fn(Weak<Presses>) -> Option<Capture>,
}

impl Default for Presses {
    fn default() -> Self {
        Self {
            state: Mutex::default(),
            start: system_capture,
        }
    }
}

impl Presses {
    /// No capture: presses come from [`Presses::push`] only.
    #[cfg(test)]
    pub fn manual() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::default(),
            start: |_| None,
        })
    }

    /// Starts reading presses, if nobody was, for as long as the guard lives.
    pub fn read(self: &Arc<Self>) -> Reading {
        let mut state = self.state.lock().unwrap();
        state.readers += 1;
        if state.readers == 1 {
            tracing::info!("key presses read");
            state.capture = (self.start)(Arc::downgrade(self));
        }
        Reading(Arc::clone(self))
    }

    /// Records a press, unless nobody reads them any more. Fed by the system's
    /// capture, which does not exist everywhere yet.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn push(&self, press: Press) {
        let mut state = self.state.lock().unwrap();
        if state.readers == 0 {
            return;
        }
        state.ring.push_back(press);
        while state.ring.len() > KEEP_AT_MOST {
            state.ring.pop_front();
        }
    }

    /// How many loops read presses.
    #[cfg(test)]
    pub fn readers(&self) -> usize {
        self.state.lock().unwrap().readers
    }
}

/// Presses are read while this lives. See [`Presses::read`].
pub struct Reading(Arc<Presses>);

impl Reading {
    /// The presses since `start`, younger than [`KEEP_FOR`] at `now`, oldest
    /// first; from `device` only when one is given.
    pub fn since(&self, start: Instant, now: Instant, device: Option<(u16, u16)>) -> Vec<Press> {
        let mut state = self.0.state.lock().unwrap();
        while state
            .ring
            .front()
            .is_some_and(|p| now.saturating_duration_since(p.at) > KEEP_FOR)
        {
            state.ring.pop_front();
        }
        state
            .ring
            .iter()
            .filter(|p| p.at >= start && device.is_none_or(|d| p.device == Some(d)))
            .copied()
            .collect()
    }
}

impl Drop for Reading {
    fn drop(&mut self) {
        let capture = {
            let mut state = self.0.state.lock().unwrap();
            state.readers -= 1;
            if state.readers > 0 {
                return;
            }
            state.ring.clear();
            state.capture.take()
        };
        // Stopped outside the lock: the capture thread may be waiting for it to
        // push a last press, and stopping joins that thread.
        drop(capture);
        tracing::info!("key presses no longer read");
    }
}

/// Where each scancode sits in `layout.keys` as effects receive it.
pub struct Positions(HashMap<u16, Vec<usize>>);

impl Positions {
    /// `keys` in the order effects receive them. Two positions for one scancode is
    /// the ISO Enter: a press lights both arms.
    pub fn of<'a>(keys: impl Iterator<Item = &'a candeo_device::Key>) -> Self {
        let mut positions: HashMap<u16, Vec<usize>> = HashMap::new();
        for (position, key) in keys.enumerate() {
            if key.scancode != candeo_device::NO_SCANCODE {
                positions.entry(key.scancode).or_default().push(position);
            }
        }
        Self(positions)
    }

    /// `presses` as `__candeo_render` reads them: `[{"k":<position>,"at":<s>}]`,
    /// `at` on the clock that reads `time` at `now`. A press on a key the layout
    /// does not have is dropped.
    pub fn to_json(&self, presses: &[Press], now: Instant, time: f64) -> String {
        let entries: Vec<String> = presses
            .iter()
            .flat_map(|p| {
                let at = time - now.saturating_duration_since(p.at).as_secs_f64();
                self.0
                    .get(&p.scancode)
                    .into_iter()
                    .flatten()
                    .map(move |k| format!(r#"{{"k":{k},"at":{at}}}"#))
            })
            .collect();
        format!("[{}]", entries.join(","))
    }
}

/// The scancodes of a layout's keys, in the order effects receive them.
pub fn positions(layout: &'static Layout) -> Positions {
    Positions::of(super::keys_in_order(layout).map(|(_, _, key)| key))
}

// ---------------------------------------------------------------- decoding
//
// Written without the Windows types so that it is tested on every system; only
// the Windows capture uses it outside the tests.

#[cfg_attr(not(windows), allow(dead_code))]
const RI_KEY_BREAK: u16 = 1;
#[cfg_attr(not(windows), allow(dead_code))]
const RI_KEY_E0: u16 = 2;
#[cfg_attr(not(windows), allow(dead_code))]
const RI_KEY_E1: u16 = 4;

/// Turns what the system reports for each key event into presses.
#[derive(Default)]
#[cfg_attr(not(windows), allow(dead_code))]
struct Decoder {
    held: HashSet<u16>,
    /// Pause arrives as `E1 1D` followed by a `45` that is not Num Lock.
    after_pause: bool,
}

#[cfg_attr(not(windows), allow(dead_code))]
impl Decoder {
    /// The scancode of a key going down, or `None` for a release, an auto-repeat,
    /// or a code the system adds on its own.
    fn press(&mut self, make: u16, flags: u16) -> Option<u16> {
        let after_pause = std::mem::take(&mut self.after_pause);
        // 0xFF: keyboard overrun.
        if make == 0 || make == 0xFF {
            return None;
        }
        let scancode = if flags & RI_KEY_E1 != 0 {
            self.after_pause = make == 0x1D;
            0xE11D
        } else if after_pause && make == 0x45 {
            return None;
        } else if flags & RI_KEY_E0 != 0 {
            // The Shift the system wraps around extended keys, depending on the
            // Num Lock and Shift states: not a key anyone pressed.
            if make == 0x2A || make == 0x36 {
                return None;
            }
            0xE000 | make
        } else {
            make
        };
        if flags & RI_KEY_BREAK != 0 {
            self.held.remove(&scancode);
            None
        } else {
            self.held.insert(scancode).then_some(scancode)
        }
    }
}

/// The VID and PID in a device name as the system reports it:
/// `\\?\HID#VID_1532&PID_0292&MI_00#…`.
#[cfg_attr(not(windows), allow(dead_code))]
fn vid_pid(name: &str) -> Option<(u16, u16)> {
    let upper = name.to_ascii_uppercase();
    let hex = |tag: &str| {
        let start = upper.find(tag)? + tag.len();
        u16::from_str_radix(upper.get(start..start + 4)?, 16).ok()
    };
    Some((hex("VID_")?, hex("PID_")?))
}

// ---------------------------------------------------------------- capture

#[cfg(not(windows))]
fn system_capture(_presses: Weak<Presses>) -> Option<Capture> {
    tracing::info!("key presses cannot be read on this system yet");
    None
}

#[cfg(windows)]
use raw_input::start as system_capture;

/// Raw Input: a copy of keyboard input, delivered to a message-only window even
/// without focus. See `docs/design/key-input.md` §2 for why not a hook.
#[cfg(windows)]
mod raw_input {
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr::{null, null_mut};
    use std::sync::mpsc;
    use std::sync::Weak;
    use std::time::Instant;

    use windows_sys::Win32::Foundation::{HANDLE, HWND, LPARAM};
    use windows_sys::Win32::UI::Input::{
        GetRawInputData, GetRawInputDeviceInfoW, RegisterRawInputDevices, HRAWINPUT, RAWINPUT,
        RAWINPUTDEVICE, RAWINPUTHEADER, RAWKEYBOARD, RIDEV_INPUTSINK, RIDEV_REMOVE,
        RIDI_DEVICENAME, RID_INPUT, RIM_TYPEKEYBOARD,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, DispatchMessageW, GetMessageW, PostMessageW, HWND_MESSAGE,
        MSG, WM_APP, WM_INPUT,
    };

    use super::{vid_pid, Capture, Decoder, Press, Presses};

    /// Posted to the window to end the capture.
    const STOP: u32 = WM_APP + 1;

    /// Generic Desktop page, keyboard usage.
    const KEYBOARDS: (u16, u16) = (0x01, 0x06);

    pub fn start(presses: Weak<Presses>) -> Option<Capture> {
        let (ready_tx, ready_rx) = mpsc::channel::<Option<isize>>();
        let thread = std::thread::Builder::new()
            .name("candeo-keys".into())
            .spawn(move || run(presses, ready_tx))
            .ok()?;
        match ready_rx.recv() {
            Ok(Some(window)) => Some(Capture(Some(Box::new(move || {
                // SAFETY: posting to a window owned by the capture thread, which
                // destroys it only after receiving this message.
                unsafe { PostMessageW(window as HWND, STOP, 0, 0) };
                let _ = thread.join();
            })))),
            _ => {
                let _ = thread.join();
                None
            }
        }
    }

    fn register(window: HWND, flags: u32) -> bool {
        let device = RAWINPUTDEVICE {
            usUsagePage: KEYBOARDS.0,
            usUsage: KEYBOARDS.1,
            dwFlags: flags,
            hwndTarget: window,
        };
        // SAFETY: one valid structure, with its size.
        unsafe { RegisterRawInputDevices(&device, 1, size_of::<RAWINPUTDEVICE>() as u32) != 0 }
    }

    fn run(presses: Weak<Presses>, ready: mpsc::Sender<Option<isize>>) {
        // The predefined STATIC class needs no registration; a message-only
        // window is never shown.
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // SAFETY: a null-terminated class name, no creation data.
        let window = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                null_mut(),
                null_mut(),
                null(),
            )
        };
        if window.is_null() {
            tracing::warn!("key presses not read: no message window");
            let _ = ready.send(None);
            return;
        }
        if !register(window, RIDEV_INPUTSINK) {
            tracing::warn!("key presses not read: keyboards could not be registered");
            // SAFETY: the window this thread created.
            unsafe { DestroyWindow(window) };
            let _ = ready.send(None);
            return;
        }
        let _ = ready.send(Some(window as isize));

        let mut decoder = Decoder::default();
        let mut devices: HashMap<isize, Option<(u16, u16)>> = HashMap::new();
        let mut msg = MSG::default();
        // SAFETY: `msg` is a valid buffer; the loop ends on error (-1) or quit (0).
        while unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {
            if msg.message == STOP {
                break;
            }
            if msg.message == WM_INPUT {
                if let Some((keyboard, handle)) = keyboard(msg.lParam) {
                    if let Some(scancode) = decoder.press(keyboard.MakeCode, keyboard.Flags) {
                        let device = *devices
                            .entry(handle as isize)
                            .or_insert_with(|| device_name(handle).as_deref().and_then(vid_pid));
                        let Some(presses) = presses.upgrade() else {
                            break;
                        };
                        presses.push(Press {
                            at: Instant::now(),
                            scancode,
                            device,
                        });
                    }
                }
            }
            // SAFETY: a message this thread received; `WM_INPUT` needs its
            // default handling to release its buffer.
            unsafe { DispatchMessageW(&msg) };
        }

        register(null_mut(), RIDEV_REMOVE);
        // SAFETY: the window this thread created.
        unsafe { DestroyWindow(window) };
    }

    /// The keyboard event behind a `WM_INPUT`, and the device it came from.
    fn keyboard(lparam: LPARAM) -> Option<(RAWKEYBOARD, HANDLE)> {
        let mut input = RAWINPUT::default();
        let mut size = size_of::<RAWINPUT>() as u32;
        // SAFETY: `input` is large enough for a keyboard event, and its size is
        // passed; a larger event (a HID one) fails and is ignored.
        let read = unsafe {
            GetRawInputData(
                lparam as HRAWINPUT,
                RID_INPUT,
                &mut input as *mut RAWINPUT as *mut c_void,
                &mut size,
                size_of::<RAWINPUTHEADER>() as u32,
            )
        };
        if read == u32::MAX || input.header.dwType != RIM_TYPEKEYBOARD {
            return None;
        }
        // SAFETY: the header says this is a keyboard event.
        Some((unsafe { input.data.keyboard }, input.header.hDevice))
    }

    /// The system's name for a device, which carries its VID and PID.
    fn device_name(handle: HANDLE) -> Option<String> {
        if handle.is_null() {
            return None;
        }
        let mut len = 0u32;
        // SAFETY: a null buffer asks for the length only.
        unsafe { GetRawInputDeviceInfoW(handle, RIDI_DEVICENAME, null_mut(), &mut len) };
        if len == 0 {
            return None;
        }
        let mut name = vec![0u16; len as usize];
        // SAFETY: a buffer of the length the system asked for.
        let written = unsafe {
            GetRawInputDeviceInfoW(
                handle,
                RIDI_DEVICENAME,
                name.as_mut_ptr() as *mut c_void,
                &mut len,
            )
        };
        if written == u32::MAX {
            return None;
        }
        Some(
            String::from_utf16_lossy(&name)
                .trim_end_matches('\0')
                .to_owned(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(at: Instant, scancode: u16, device: Option<(u16, u16)>) -> Press {
        Press {
            at,
            scancode,
            device,
        }
    }

    #[test]
    fn a_held_key_is_one_press_until_released() {
        let mut d = Decoder::default();
        assert_eq!(d.press(0x11, 0), Some(0x11));
        assert_eq!(d.press(0x11, 0), None, "auto-repeat");
        assert_eq!(d.press(0x11, RI_KEY_BREAK), None);
        assert_eq!(d.press(0x11, 0), Some(0x11));
    }

    #[test]
    fn extended_keys_carry_e0_and_the_added_shifts_are_dropped() {
        let mut d = Decoder::default();
        assert_eq!(d.press(0x2A, RI_KEY_E0), None, "fake Shift around an arrow");
        assert_eq!(d.press(0x4B, RI_KEY_E0), Some(0xE04B));
        assert_eq!(d.press(0x1D, RI_KEY_E0), Some(0xE01D), "right Ctrl");
        assert_eq!(d.press(0x2A, 0), Some(0x2A), "the real left Shift");
        assert_eq!(d.press(0xFF, 0), None, "overrun");
    }

    #[test]
    fn pause_is_one_press_and_not_num_lock() {
        let mut d = Decoder::default();
        assert_eq!(d.press(0x1D, RI_KEY_E1), Some(0xE11D));
        assert_eq!(d.press(0x45, 0), None, "the 45 that follows Pause");
        assert_eq!(d.press(0x1D, RI_KEY_E1 | RI_KEY_BREAK), None);
        assert_eq!(d.press(0x45, RI_KEY_BREAK), None);
        assert_eq!(d.press(0x45, 0), Some(0x45), "Num Lock itself");
    }

    #[test]
    fn device_names_give_vid_and_pid() {
        let name = r"\\?\HID#VID_1532&PID_0292&MI_00#7&1a2b3c4d&0&0000#{884b96c3-56ef-11d1-bc8c-00a0c91405dd}";
        assert_eq!(vid_pid(name), Some((0x1532, 0x0292)));
        assert_eq!(
            vid_pid(r"\\?\hid#vid_046d&pid_c31c#x"),
            Some((0x046D, 0xC31C))
        );
        assert_eq!(vid_pid(r"\\?\ACPI#PNP0303#0"), None);
    }

    #[test]
    fn nothing_is_kept_without_readers() {
        let presses = Presses::manual();
        let now = Instant::now();
        presses.push(press(now, 0x11, None));

        let reading = presses.read();
        assert!(
            reading.since(now, now, None).is_empty(),
            "pushed before reading"
        );
        presses.push(press(now, 0x11, None));
        assert_eq!(reading.since(now, now, None).len(), 1);

        drop(reading);
        let reading = presses.read();
        assert!(
            reading.since(now, now, None).is_empty(),
            "kept after the last reader"
        );
    }

    #[test]
    fn only_recent_presses_are_kept_and_at_most_so_many() {
        let presses = Presses::manual();
        let reading = presses.read();
        let start = Instant::now();
        for i in 0..40 {
            presses.push(press(start, i + 1, None));
        }
        let kept = reading.since(start, start, None);
        assert_eq!(kept.len(), KEEP_AT_MOST);
        assert_eq!(
            kept[0].scancode,
            40 - KEEP_AT_MOST as u16 + 1,
            "the oldest go first"
        );

        let later = start + KEEP_FOR + Duration::from_millis(1);
        assert!(reading.since(start, later, None).is_empty());
    }

    #[test]
    fn a_device_reads_its_keyboard_and_the_preview_reads_all() {
        let presses = Presses::manual();
        let reading = presses.read();
        let now = Instant::now();
        presses.push(press(now, 0x11, Some((1, 2))));
        presses.push(press(now, 0x1E, Some((1, 3))));
        presses.push(press(now, 0x1F, None));

        let mine: Vec<u16> = reading
            .since(now, now, Some((1, 2)))
            .iter()
            .map(|p| p.scancode)
            .collect();
        assert_eq!(mine, [0x11]);
        assert_eq!(reading.since(now, now, None).len(), 3);
    }

    #[test]
    fn capture_runs_while_someone_reads() {
        let presses = Presses::manual();
        let first = presses.read();
        let second = presses.read();
        assert_eq!(presses.readers(), 2);
        drop(first);
        assert_eq!(presses.readers(), 1);
        drop(second);
        assert_eq!(presses.readers(), 0);
    }

    #[test]
    fn presses_become_positions_on_the_effect_clock() {
        let layout: &'static Layout = candeo_device::definition::builtin()[0];
        let positions = positions(layout);
        let now = Instant::now();
        let presses = [
            press(now - Duration::from_millis(500), 0x1C, None),
            press(now, 0xE046, None),
        ];

        let json: serde_json::Value =
            serde_json::from_str(&positions.to_json(&presses, now, 2.0)).unwrap();
        let entries = json.as_array().unwrap();
        assert_eq!(
            entries.len(),
            2,
            "both Enter arms, nothing for a key not on the layout"
        );
        let keys: Vec<serde_json::Value> =
            serde_json::from_str::<serde_json::Value>(&super::super::layout_json(layout)).unwrap()
                ["keys"]
                .as_array()
                .unwrap()
                .clone();
        for entry in entries {
            assert_eq!(
                keys[entry["k"].as_u64().unwrap() as usize]["scancode"],
                0x1C
            );
            assert!((entry["at"].as_f64().unwrap() - 1.5).abs() < 1e-9);
        }
    }
}
