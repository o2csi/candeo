//! Hardware access and device descriptions.
//!
//! The transport layer is isolated here so that [`candeo_protocol`] stays pure,
//! testable without hardware, and free of system dependencies.

use candeo_protocol::{CommandId, Rgb};

pub mod definition;
pub mod inspection;
pub mod layout;
pub mod lighting;

pub use inspection::{Check, Inspection, Verdict, Warning};
pub use layout::{Key, Layout, Lights, Outline, Port, Shape, DEATHSTALKER_V2_PRO, NO_SCANCODE};
pub use lighting::{Lighting, Outgoing, Wire};

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
    /// Said rather than approximated: a device that draws nothing by itself must
    /// not look like one running an effect.
    #[error("{device} has no firmware effect of its own")]
    NoFirmwareEffect { device: &'static str },
    #[error("{device} is addressed key by key, not by rows")]
    NoRowWrite { device: &'static str },
    #[error("{device} does not dim: its firmware has no brightness")]
    NoBrightness { device: &'static str },
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
        mut accept: impl FnMut(Option<&str>) -> bool,
    ) -> Result<Option<Self>, Error> {
        let info = api
            .device_list()
            .find(|d| {
                layout.is_lighting_interface(
                    d.vendor_id(),
                    d.product_id(),
                    d.interface_number(),
                    d.usage_page(),
                    d.usage(),
                )
            })
            .ok_or(Error::NotFound {
                vid: layout.vid,
                pid: layout.pid,
            })?;

        let device = info.open_device(api)?;
        // What a device is asked on opening belongs to its family: a command of
        // another maker's protocol is a guess, not a question.
        Ok(layout
            .lighting
            .inspect(&device, &mut accept)
            .map(|inspection| Self {
                device,
                layout,
                inspection,
            }))
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
    fn send(&self, report: &Outgoing) -> Result<(), Error> {
        if let Some(command) = report.command {
            if self.inspection.refuses(command) {
                return Err(Error::Refused { command });
            }
        }
        match report.wire {
            Wire::Feature => self.device.send_feature_report(&report.bytes)?,
            Wire::Output => self.device.send_output_report(&report.bytes)?,
        }
        Ok(())
    }

    /// Hands the lighting back to the firmware, where the firmware draws.
    ///
    /// A family whose firmware draws nothing has no such thing: *Off* is then a
    /// black frame, and anything else is refused rather than approximated, so
    /// that nobody believes the device runs an effect it does not.
    pub fn set_effect(&self, id: &str, colours: &[Rgb]) -> Result<(), Error> {
        match self.layout.lighting.firmware_effect(id, colours) {
            Some(report) => self.send(&report),
            None if id == "hardware:off" => {
                self.present(&vec![Rgb::default(); self.layout.led_count()])
            }
            None => Err(Error::NoFirmwareEffect {
                device: self.layout.name,
            }),
        }
    }

    /// The effect the firmware runs now, read back — the same read the inspection
    /// makes on opening. An automation reads it before interrupting a device no
    /// host loop drives, to give that effect back afterwards: nothing else
    /// remembers a firmware effect. `None` for one it cannot describe, and for a
    /// device that runs none.
    pub fn current_effect(&self) -> Result<Option<String>, String> {
        self.layout.lighting.current_effect(&self.device)
    }

    /// Dims the whole device, in whatever reports its family takes.
    ///
    /// A family with no brightness is refused rather than written to: the screen
    /// offers no slider for it, so reaching this is a bug, not a gesture.
    pub fn set_brightness(&self, level: u8) -> Result<(), Error> {
        let reports = self
            .layout
            .lighting
            .brightness(level)
            .ok_or(Error::NoBrightness {
                device: self.layout.name,
            })?;
        for report in reports {
            self.send(&report)?;
        }
        Ok(())
    }

    /// Writes a row segment, for a device addressed that way. The Razer handles
    /// partial writes — verified on hardware.
    pub fn write_row(&self, row: u8, col_start: u8, colors: &[Rgb]) -> Result<(), Error> {
        if row >= self.layout.rows {
            return Err(Error::RowOutOfRange {
                row,
                rows: self.layout.rows,
            });
        }
        match self.layout.lighting.row(row, col_start, colors) {
            Some(report) => self.send(&report),
            None => Err(Error::NoRowWrite {
                device: self.layout.name,
            }),
        }
    }

    /// Pushes a full frame, in whatever reports the device's family takes.
    ///
    /// `frame` must cover the **whole** matrix, including positions without a
    /// physical LED. Sending less leaves a Razer's last rows frozen on their
    /// previous value, and says nothing about the cells a per-key device skips.
    pub fn present(&self, frame: &[Rgb]) -> Result<(), Error> {
        let expected = self.layout.led_count();
        assert_eq!(
            frame.len(),
            expected,
            "the frame must cover all {expected} positions of the matrix"
        );
        for report in self.layout.lighting.frame(self.layout, frame) {
            self.send(&report)?;
        }
        Ok(())
    }
}
