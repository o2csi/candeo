//! Hardware access and device descriptions.
//!
//! The transport layer is isolated here so that [`candeo_protocol`] stays pure,
//! testable without hardware, and free of system dependencies.

use candeo_protocol::{CommandId, Effect, Report, Rgb};

pub mod inspection;
pub mod layout;

pub use inspection::{Check, Inspection, Verdict, Warning};
pub use layout::{Key, Layout, DEATHSTALKER_V2_PRO, NO_SCANCODE};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("device not found (VID {vid:#06x}, PID {pid:#06x})")]
    NotFound { vid: u16, pid: u16 },
    #[error("HID access: {0}")]
    Hid(#[from] hidapi::HidError),
    #[error("row {row} is outside the matrix ({rows} rows)")]
    RowOutOfRange { row: u8, rows: u8 },
    #[error(
        "command {command} not sent: on open, this firmware answered that it does not know it"
    )]
    Refused { command: CommandId },
}

/// An open keyboard, ready to receive commands.
pub struct Keyboard {
    device: hidapi::HidDevice,
    layout: &'static Layout,
    inspection: Inspection,
}

impl Keyboard {
    /// Opens the first device matching the given layout, then inspects it.
    ///
    /// On Windows, lighting goes through one specific interface of the composite
    /// device (`interface_number`): opening the wrong one yields a valid handle on
    /// which every write fails silently. See [`Layout::is_lighting_interface`].
    ///
    /// # Inspection happens here, and nowhere else
    ///
    /// Three paths open a device — adoption at startup, adoption on demand, the
    /// one-off open — and a fourth is coming. Leaving each of them to inspect is
    /// waiting for the one that forgets, and ending up, on that path, with a
    /// keyboard nobody knows what it understands. Here, a `Keyboard` without an
    /// inspection cannot exist.
    ///
    /// It never makes the open fail: a device that does not answer reads is still
    /// a device that receives our writes, and refusing it for that would block the
    /// lighting on a question left unanswered. See [`inspection`].
    pub fn open(api: &hidapi::HidApi, layout: &'static Layout) -> Result<Self, Error> {
        Ok(Self::open_if(api, layout, |_| true)?.expect("a unit nobody refuses is opened"))
    }

    /// [`Self::open`], unless `accept` refuses the unit the device names: its
    /// serial, read over the protocol (`None` when it does not answer), is asked
    /// about **before** the inspection rewrites anything. A refused unit is closed
    /// having received reads only, and `None` is returned (#74).
    pub fn open_if(
        api: &hidapi::HidApi,
        layout: &'static Layout,
        accept: impl FnOnce(Option<&str>) -> bool,
    ) -> Result<Option<Self>, Error> {
        let info = api
            .device_list()
            .find(|d| {
                layout.is_lighting_interface(d.vendor_id(), d.product_id(), d.interface_number())
            })
            .ok_or(Error::NotFound {
                vid: layout.vid,
                pid: layout.pid,
            })?;

        let device = info.open_device(api)?;
        Ok(
            inspection::inspect_if(&device, accept).map(|inspection| Self {
                device,
                layout,
                inspection,
            }),
        )
    }

    pub fn layout(&self) -> &'static Layout {
        self.layout
    }

    /// What the device said about itself when it was opened.
    pub fn inspection(&self) -> &Inspection {
        &self.inspection
    }

    /// Sends a report — **unless** the device declared it does not know it.
    ///
    /// The refusal is what makes the inspection useful beyond a warning: without
    /// it, a command answered `0x05` would still go out on every frame, `hidapi`
    /// would accept it, and the loop would call itself healthy on top of a
    /// keyboard that discards everything. With it, the failure travels up the
    /// write error path the interface already displays. The cost is a lookup in
    /// three entries: nothing is read back.
    fn send(&self, report: Report) -> Result<(), Error> {
        let command = report.id();
        if self.inspection.refuses(command) {
            return Err(Error::Refused { command });
        }
        self.device
            .send_feature_report(&report.to_feature_buffer())?;
        Ok(())
    }

    pub fn set_effect(&self, effect: Effect) -> Result<(), Error> {
        self.send(Report::set_effect(effect))
    }

    /// The effect the firmware runs now, read back — the same read the inspection
    /// makes on opening. An automation reads it before interrupting a device no
    /// host loop drives, to give that effect back afterwards: nothing else
    /// remembers a firmware effect. `None` for one it cannot describe.
    pub fn current_effect(&self) -> Result<Option<Effect>, String> {
        inspection::read_effect(&self.device)
    }

    pub fn set_brightness(&self, level: u8) -> Result<(), Error> {
        self.send(Report::set_brightness(level))
    }

    /// Writes a row segment. The device handles partial writes — verified on
    /// hardware.
    pub fn write_row(&self, row: u8, col_start: u8, colors: &[Rgb]) -> Result<(), Error> {
        if row >= self.layout.rows {
            return Err(Error::RowOutOfRange {
                row,
                rows: self.layout.rows,
            });
        }
        self.send(Report::write_row(row, col_start, colors))
    }

    /// Pushes a full frame: one row per transfer, then switches to host-controlled
    /// mode.
    ///
    /// `frame` must cover the **whole** matrix, including positions without a
    /// physical LED. Sending less leaves the last rows frozen on their previous
    /// value.
    pub fn present(&self, frame: &[Rgb]) -> Result<(), Error> {
        let expected = self.layout.led_count();
        assert_eq!(
            frame.len(),
            expected,
            "the frame must cover all {expected} positions of the matrix"
        );
        let cols = self.layout.cols as usize;
        for row in 0..self.layout.rows {
            let start = row as usize * cols;
            self.write_row(row, 0, &frame[start..start + cols])?;
        }
        self.set_effect(Effect::Custom)
    }
}
