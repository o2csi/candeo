//! Accès matériel et description des périphériques.
//!
//! La couche transport est isolée ici pour que [`candeo_protocol`] reste pur,
//! testable sans matériel, et sans dépendance système.

use candeo_protocol::{CommandId, Effect, Report, Rgb};

pub mod inspection;
pub mod layout;

pub use inspection::{Check, Inspection, Verdict};
pub use layout::{Key, Layout, DEATHSTALKER_V2_PRO};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("périphérique introuvable (VID {vid:#06x}, PID {pid:#06x})")]
    NotFound { vid: u16, pid: u16 },
    #[error("accès HID : {0}")]
    Hid(#[from] hidapi::HidError),
    #[error("la rangée {row} dépasse la matrice ({rows} rangées)")]
    RowOutOfRange { row: u8, rows: u8 },
    #[error(
        "commande {command} non envoyée : ce micrologiciel a répondu à l'ouverture qu'il ne la \
         connaît pas"
    )]
    Refused { command: CommandId },
}

/// Un clavier ouvert, prêt à recevoir des commandes.
pub struct Keyboard {
    device: hidapi::HidDevice,
    layout: &'static Layout,
    inspection: Inspection,
}

impl Keyboard {
    /// Ouvre le premier périphérique correspondant au gabarit fourni, puis
    /// l'inspecte.
    ///
    /// Sur Windows, l'éclairage passe par une interface précise du composite
    /// (`interface_number`) : ouvrir la mauvaise donne un handle valide sur
    /// lequel toute écriture échoue silencieusement. Voir
    /// [`Layout::is_lighting_interface`].
    ///
    /// # L'inspection est ici, et nulle part ailleurs
    ///
    /// Trois chemins ouvrent un appareil — l'adoption au démarrage, l'adoption
    /// à la demande, l'ouverture ponctuelle — et un quatrième viendra. Laisser à
    /// chacun le soin d'inspecter, c'est attendre celui qui l'oubliera, et
    /// retrouver sur ce chemin-là un clavier dont personne ne sait ce qu'il
    /// comprend. Ici, un `Keyboard` sans inspection ne peut pas exister.
    ///
    /// Elle ne fait jamais échouer l'ouverture : un appareil qui ne répond pas
    /// aux lectures reste un appareil qui reçoit nos écritures, et le refuser
    /// pour ça bloquerait l'éclairage sur une question restée sans réponse.
    /// Voir [`inspection`].
    pub fn open(api: &hidapi::HidApi, layout: &'static Layout) -> Result<Self, Error> {
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
        let inspection = inspection::inspect(&device);
        Ok(Self {
            device,
            layout,
            inspection,
        })
    }

    pub fn layout(&self) -> &'static Layout {
        self.layout
    }

    /// Ce que l'appareil a dit de lui-même à l'ouverture.
    pub fn inspection(&self) -> &Inspection {
        &self.inspection
    }

    /// Envoie un rapport — **sauf** si l'appareil a déclaré ne pas le connaître.
    ///
    /// Le refus est ce qui rend l'inspection utile au-delà d'un avertissement :
    /// sans lui, une commande rendue `0x05` partirait encore à chaque image,
    /// `hidapi` l'accepterait, et la boucle se dirait saine au-dessus d'un
    /// clavier qui jette tout. Avec lui, l'échec remonte par le chemin
    /// d'erreur d'écriture que l'interface affiche déjà. Le coût est une
    /// recherche dans trois entrées : rien n'est relu.
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
