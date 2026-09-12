//! Accès matériel et description des périphériques.
//!
//! La couche transport est isolée ici pour que [`candeo_protocol`] reste pur,
//! testable sans matériel, et sans dépendance système.

use candeo_protocol::{Effect, Report, Rgb};

pub mod layout;

pub use layout::{Key, Layout, DEATHSTALKER_V2_PRO};

/// Identifiant fabricant Razer.
pub const VID_RAZER: u16 = 0x1532;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("périphérique introuvable (VID {vid:#06x}, PID {pid:#06x})")]
    NotFound { vid: u16, pid: u16 },
    #[error("accès HID : {0}")]
    Hid(#[from] hidapi::HidError),
    #[error("la rangée {row} dépasse la matrice ({rows} rangées)")]
    RowOutOfRange { row: u8, rows: u8 },
}

/// Un clavier ouvert, prêt à recevoir des commandes.
pub struct Keyboard {
    device: hidapi::HidDevice,
    layout: &'static Layout,
}

impl Keyboard {
    /// Ouvre le premier périphérique correspondant au gabarit fourni.
    ///
    /// Sur Windows, l'éclairage passe par une interface précise du composite
    /// (`interface_number`) : ouvrir la mauvaise donne un handle valide sur
    /// lequel toute écriture échoue silencieusement.
    pub fn open(api: &hidapi::HidApi, layout: &'static Layout) -> Result<Self, Error> {
        let info = api
            .device_list()
            .find(|d| {
                d.vendor_id() == layout.vid
                    && d.product_id() == layout.pid
                    && d.interface_number() == layout.interface as i32
            })
            .ok_or(Error::NotFound {
                vid: layout.vid,
                pid: layout.pid,
            })?;

        let device = info.open_device(api)?;
        Ok(Self { device, layout })
    }

    pub fn layout(&self) -> &'static Layout {
        self.layout
    }

    fn send(&self, report: Report) -> Result<(), Error> {
        self.device
            .send_feature_report(&report.to_feature_buffer())?;
        Ok(())
    }

    pub fn set_effect(&self, effect: Effect) -> Result<(), Error> {
        self.send(Report::set_effect(effect))
    }

    pub fn set_brightness(&self, level: u8) -> Result<(), Error> {
        self.send(Report::set_brightness(level))
    }

    /// Écrit un segment de rangée. L'écriture partielle est prise en charge
    /// par l'appareil — vérifié sur le matériel.
    pub fn write_row(&self, row: u8, col_start: u8, colors: &[Rgb]) -> Result<(), Error> {
        if row >= self.layout.rows {
            return Err(Error::RowOutOfRange {
                row,
                rows: self.layout.rows,
            });
        }
        self.send(Report::write_row(row, col_start, colors))
    }

    /// Pousse une image complète : une rangée par transfert, puis bascule en
    /// mode piloté par l'hôte.
    ///
    /// `frame` doit couvrir **toute** la matrice, y compris les positions sans
    /// LED physique. En envoyer moins laisse les dernières rangées figées sur
    /// leur valeur précédente.
    pub fn present(&self, frame: &[Rgb]) -> Result<(), Error> {
        let expected = self.layout.led_count();
        assert_eq!(
            frame.len(),
            expected,
            "l'image doit couvrir les {expected} positions de la matrice"
        );
        let cols = self.layout.cols as usize;
        for row in 0..self.layout.rows {
            let start = row as usize * cols;
            self.write_row(row, 0, &frame[start..start + cols])?;
        }
        self.set_effect(Effect::Custom)
    }
}
