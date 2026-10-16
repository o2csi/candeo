//! The lighting devices plugged in that no definition describes (#327,
//! `docs/design/device-sdk.md` §9): shown in *Devices* as *Detected, not
//! supported yet*, with **Report** to say what they are.
//!
//! Read from the system's enumeration only. Nothing opens such a device and
//! nothing writes to it: Candeo knows neither its protocol nor what a write
//! would do there.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::{catalog, journal, CmdResult, DeviceRef, Failure};

/// The makers whose devices light, by vendor id, as #327 lists them. A device of
/// theirs is shown even when none of its collections says it lights: most
/// declare their lighting on a page of their own (`0xff00`…).
const MAKERS: &[(u16, &str)] = &[
    (0x187c, "Alienware"),
    (0x0d62, "Darfon"),
    (0x1532, "Razer"),
    (0x0b05, "ASUS"),
    (0x046d, "Logitech"),
    (0x1038, "SteelSeries"),
    (0x0951, "HyperX"),
    // HP's own id: HyperX devices made since HP bought the brand carry it, and
    // so does every other HP device, which HyperX would misname.
    (0x03f0, "HP"),
    (0x1b1c, "Corsair"),
];

/// HID's Lighting and Illumination usage page: whoever makes a device with a
/// collection there says it lights.
const LIGHTING_PAGE: u16 = 0x59;

/// One line of the enumeration: what the system says of a collection without
/// the device being opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Listed {
    pub vid: u16,
    pub pid: u16,
    /// Negative when the system does not say, over Bluetooth.
    pub interface: i32,
    pub usage_page: u16,
    pub usage: u16,
    /// As the device names its maker, for one outside [`MAKERS`].
    pub manufacturer: Option<String>,
}

/// A collection, as the card and the report show it.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    /// `None` when the system does not say.
    pub interface: Option<i32>,
    pub usage_page: u16,
    pub usage: u16,
}

/// A device plugged in that no definition describes: detected, not supported
/// yet. One per `vid:pid`, whatever its number of collections.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UnsupportedDevice {
    /// As [`MAKERS`] names it, or failing that as the device does; `None` when
    /// it says nothing.
    pub maker: Option<String>,
    pub vid: u16,
    pub pid: u16,
    /// Sorted by interface, then page and usage; no report lengths, which the
    /// enumeration does not give (`docs/design/device-sdk.md` §9).
    pub collections: Vec<Collection>,
}

fn maker(vid: u16) -> Option<&'static str> {
    MAKERS
        .iter()
        .find(|(v, _)| *v == vid)
        .map(|(_, name)| *name)
}

/// The devices of [`MAKERS`], or with a collection on [`LIGHTING_PAGE`], that
/// no definition describes: `known` holds every `vid:pid` one does. Sorted by
/// `vid:pid`.
pub(crate) fn select(listed: &[Listed], known: &HashSet<(u16, u16)>) -> Vec<UnsupportedDevice> {
    let mut models: BTreeMap<(u16, u16), Vec<&Listed>> = BTreeMap::new();
    for line in listed {
        let model = (line.vid, line.pid);
        if !known.contains(&model) {
            models.entry(model).or_default().push(line);
        }
    }
    models
        .into_iter()
        .filter_map(|((vid, pid), lines)| {
            let lights = lines.iter().any(|l| l.usage_page == LIGHTING_PAGE);
            let maker = match maker(vid) {
                Some(name) => Some(name.to_owned()),
                None if lights => lines
                    .iter()
                    .filter_map(|l| l.manufacturer.as_deref().map(str::trim))
                    .find(|m| !m.is_empty())
                    .map(str::to_owned),
                None => return None,
            };
            let mut collections: Vec<Collection> = lines
                .iter()
                .map(|l| Collection {
                    interface: (l.interface >= 0).then_some(l.interface),
                    usage_page: l.usage_page,
                    usage: l.usage,
                })
                .collect();
            collections.sort();
            collections.dedup();
            Some(UnsupportedDevice {
                maker,
                vid,
                pid,
                collections,
            })
        })
        .collect()
}

/// Every `vid:pid` a definition for this machine describes: built in, yours,
/// or unverified, whose devices *Devices* already lists to try. One naming
/// another machine describes nothing here (#326): its device is listed.
fn known() -> HashSet<(u16, u16)> {
    let unverified = catalog::unverified().map(|(_, l)| *l);
    catalog::current()
        .layouts
        .iter()
        .copied()
        .chain(unverified)
        .map(|l| (l.vid, l.pid))
        .collect()
}

fn plugged_in(api: &hidapi::HidApi) -> Vec<UnsupportedDevice> {
    let listed: Vec<Listed> = api
        .device_list()
        .map(|d| Listed {
            vid: d.vendor_id(),
            pid: d.product_id(),
            interface: d.interface_number(),
            usage_page: d.usage_page(),
            usage: d.usage(),
            manufacturer: d.manufacturer_string().map(str::to_owned),
        })
        .collect();
    select(&listed, &known())
}

/// The lighting devices plugged in that Candeo cannot drive yet.
#[tauri::command]
pub fn unsupported_devices() -> CmdResult<Vec<UnsupportedDevice>> {
    Ok(plugged_in(&crate::hid()?))
}

/// The collections as the form lists them, one per line.
fn collections_text(collections: &[Collection]) -> String {
    collections
        .iter()
        .map(|c| {
            let interface = c
                .interface
                .map(|i| format!("interface {i}: "))
                .unwrap_or_default();
            let lighting = if c.usage_page == LIGHTING_PAGE {
                " (Lighting and Illumination)"
            } else {
                ""
            };
            format!(
                "- {interface}usage page {:#06x}{lighting}, usage {:#06x}",
                c.usage_page, c.usage
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The form for a device not supported yet, filled with what the enumeration
/// says of it, the machine's model when the system gives it (#326), Candeo's
/// version and system. What lights on it is the person's to say. A model, and
/// no serial and no path: nothing that names this computer or its user.
fn report_url(device: &UnsupportedDevice, machine: Option<&str>, candeo: &str) -> String {
    let ids = format!("{:04x}:{:04x}", device.vid, device.pid);
    let named = match &device.maker {
        Some(maker) => format!("{maker} {ids}"),
        None => ids.clone(),
    };
    let mut url = format!(
        "{}/issues/new?template=unsupported-device.yml&title={}&device={}&collections={}&candeo={}",
        env!("CARGO_PKG_REPOSITORY"),
        crate::query(&format!("Device not supported yet: {named}")),
        crate::query(&ids),
        crate::query(&collections_text(&device.collections)),
        crate::query(candeo),
    );
    if let Some(maker) = &device.maker {
        url.push_str(&format!("&maker={}", crate::query(maker)));
    }
    if let Some(machine) = machine {
        url.push_str(&format!("&machine={}", crate::query(machine)));
    }
    url
}

/// Opens the form in the browser for a device not supported yet: the person
/// says what it is and sends it. Candeo sends nothing.
#[tauri::command]
pub fn report_unsupported_device(app: AppHandle, vid: u16, pid: u16) -> CmdResult<()> {
    let device = plugged_in(&crate::hid()?)
        .into_iter()
        .find(|d| (d.vid, d.pid) == (vid, pid))
        .ok_or_else(|| Failure::new("deviceNotFound"))?;
    let candeo = format!("{}, {}", app.package_info().version, journal::os_version());
    tracing::info!(device = %DeviceRef { vid, pid }, "report form opened for a device not supported yet");
    app.opener()
        .open_url(
            report_url(&device, crate::machine::model(), &candeo),
            None::<&str>,
        )
        .map_err(|e| Failure::unexpected(format!("cannot open the report form: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(vid: u16, pid: u16, interface: i32, usage_page: u16, usage: u16) -> Listed {
        Listed {
            vid,
            pid,
            interface,
            usage_page,
            usage,
            manufacturer: None,
        }
    }

    #[test]
    fn a_lighting_makers_device_nobody_describes_is_one_entry_with_its_collections() {
        let listed = [
            line(0x187c, 0x0550, 0, 0xff00, 0x0001),
            line(0x187c, 0x0550, 0, 0x0001, 0x0006),
            // Windows lists a collection again for each of its reports.
            line(0x187c, 0x0550, 0, 0xff00, 0x0001),
            line(0x1532, 0x0099, -1, 0x000c, 0x0001),
        ];
        let found = select(&listed, &HashSet::new());
        assert_eq!(
            found,
            vec![
                UnsupportedDevice {
                    maker: Some("Razer".into()),
                    vid: 0x1532,
                    pid: 0x0099,
                    collections: vec![Collection {
                        interface: None,
                        usage_page: 0x000c,
                        usage: 0x0001,
                    }],
                },
                UnsupportedDevice {
                    maker: Some("Alienware".into()),
                    vid: 0x187c,
                    pid: 0x0550,
                    collections: vec![
                        Collection {
                            interface: Some(0),
                            usage_page: 0x0001,
                            usage: 0x0006,
                        },
                        Collection {
                            interface: Some(0),
                            usage_page: 0xff00,
                            usage: 0x0001,
                        },
                    ],
                },
            ]
        );
    }

    #[test]
    fn a_device_a_definition_describes_is_not_listed() {
        let listed = [
            line(0x1532, 0x0292, 3, 0xff00, 0x0001),
            line(0x187c, 0x0551, 0, 0xff00, 0x0001),
        ];
        let known = HashSet::from([(0x1532, 0x0292)]);
        let found = select(&listed, &known);
        assert_eq!(
            found.iter().map(|d| (d.vid, d.pid)).collect::<Vec<_>>(),
            vec![(0x187c, 0x0551)]
        );
    }

    #[test]
    fn another_makers_device_is_listed_only_when_it_lights() {
        let mut lamp = line(0x1234, 0x5678, 1, LIGHTING_PAGE, 0x0001);
        lamp.manufacturer = Some("  Lamp Maker ".into());
        let mut silent = line(0x1234, 0x5678, 0, 0x0001, 0x0006);
        silent.manufacturer = Some(String::new());
        let listed = [
            silent,
            lamp,
            line(0x4321, 0x0001, 0, 0x0001, 0x0002),
            line(0x4321, 0x0002, 0, LIGHTING_PAGE, 0x0001),
        ];
        let found = select(&listed, &HashSet::new());
        assert_eq!(
            found
                .iter()
                .map(|d| (d.maker.as_deref(), d.pid, d.collections.len()))
                .collect::<Vec<_>>(),
            vec![(Some("Lamp Maker"), 0x5678, 2), (None, 0x0002, 1)]
        );
    }

    #[test]
    fn every_maker_of_the_issue_is_listed() {
        for vid in [
            0x187c, 0x0d62, 0x1532, 0x0b05, 0x046d, 0x1038, 0x0951, 0x03f0, 0x1b1c,
        ] {
            assert!(maker(vid).is_some(), "{vid:#06x}");
        }
    }

    /// The form comes filled and escaped, on this repository, with nothing but
    /// what the enumeration and the machine's model say, and the person would
    /// type.
    #[test]
    fn the_report_form_is_filled_and_escaped() {
        let device = UnsupportedDevice {
            maker: Some("Alienware".into()),
            vid: 0x187c,
            pid: 0x0550,
            collections: vec![
                Collection {
                    interface: Some(0),
                    usage_page: 0x0059,
                    usage: 0x0001,
                },
                Collection {
                    interface: None,
                    usage_page: 0xff00,
                    usage: 0x0002,
                },
            ],
        };
        let url = report_url(
            &device,
            Some("Alienware 17 R5"),
            "0.21.0, Windows 11 24H2 (26100.1) · x86_64",
        );
        assert!(url.starts_with(
            "https://github.com/o2csi/candeo/issues/new?template=unsupported-device.yml&"
        ));
        assert!(url.contains("&title=Device%20not%20supported%20yet%3A%20Alienware%20187c%3A0550&"));
        assert!(url.contains("&device=187c%3A0550&"));
        assert!(url.contains(
            "&collections=-%20interface%200%3A%20usage%20page%200x0059%20%28Lighting%20and%20Illumination%29%2C%20usage%200x0001%0A-%20usage%20page%200xff00%2C%20usage%200x0002&"
        ));
        assert!(url.contains("&candeo=0.21.0%2C%20Windows%2011"));
        assert!(url.ends_with("&maker=Alienware&machine=Alienware%2017%20R5"));

        let nameless = UnsupportedDevice {
            maker: None,
            ..device
        };
        // Neither maker nor model read: the person types them.
        let url = report_url(&nameless, None, "0.21.0");
        assert!(url.contains("&title=Device%20not%20supported%20yet%3A%20187c%3A0550&"));
        assert!(!url.contains("maker"));
        assert!(!url.contains("machine"));
    }
}
