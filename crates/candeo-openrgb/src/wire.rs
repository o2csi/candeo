//! The SDK's framing, the packet ids, and the only packets Candeo builds.
//!
//! From OpenRGBSDK.md at `release_1.1`, cited by the heading each fact sits
//! under. Neither it nor RGBControllerAPI.md states the byte order: little
//! endian here, the order of every platform OpenRGB is released for, and the
//! first thing to confirm against a real server (`openrgb-output.md` §9).
//!
//! # A fixed list
//!
//! [`Request`] is the only way to build a packet, and every request goes
//! through [`packet`], which refuses any id outside [`SENT`]. `SAVEMODE`,
//! profiles, settings, rescans, client flags (and with them local status)
//! cannot be encoded at all; a test tries every id the document lists
//! (§6.6).

use crate::controller::{encode_mode, Mode};
use crate::Rgb;

/// *NetPacketHeader structure*: "Always the literal `ORGB`".
pub const MAGIC: [u8; 4] = *b"ORGB";
pub const HEADER_LEN: usize = 16;
/// Above it the connection is dropped. A keyboard's controller data takes a
/// few kilobytes; a strip of a thousand LEDs, some tens.
pub const MAX_PAYLOAD: u32 = 8 * 1024 * 1024;
/// The highest version Candeo speaks, sent in `REQUEST_PROTOCOL_VERSION`.
pub const CLIENT_PROTOCOL: u32 = 6;
/// The lowest it accepts: OpenRGB 0.9. Below, zones have no segments and the
/// layouts of §6.6 were not written for them.
pub const MIN_PROTOCOL: u32 = 4;
/// What `SET_CLIENT_NAME` carries: the program's name, never the computer's.
pub const CLIENT_NAME: &str = "Candeo";

/// Every packet id of *Packet IDs*, with the name the document gives it
/// (`NET_PACKET_ID_` left out).
pub mod id {
    pub const REQUEST_CONTROLLER_COUNT: u32 = 0;
    pub const REQUEST_CONTROLLER_DATA: u32 = 1;
    pub const ACK: u32 = 10;
    pub const REQUEST_PROTOCOL_VERSION: u32 = 40;
    pub const SET_CLIENT_NAME: u32 = 50;
    pub const SET_SERVER_NAME: u32 = 51;
    pub const SET_CLIENT_FLAGS: u32 = 52;
    pub const SET_SERVER_FLAGS: u32 = 53;
    pub const DEVICE_LIST_UPDATED: u32 = 100;
    pub const DETECTION_STARTED: u32 = 101;
    pub const DETECTION_PROGRESS_CHANGED: u32 = 102;
    pub const DETECTION_COMPLETE: u32 = 103;
    pub const GET_I2C_BUS_INFO: u32 = 120;
    pub const GET_HID_DEVICE_INFO: u32 = 121;
    pub const GET_USB_DEVICE_INFO: u32 = 122;
    pub const GET_SERIAL_PORTS: u32 = 123;
    pub const GET_USB_SERIAL_PORTS: u32 = 124;
    pub const REQUEST_RESCAN_DEVICES: u32 = 140;
    pub const PROFILEMANAGER_GET_PROFILE_LIST: u32 = 150;
    pub const PROFILEMANAGER_SAVE_PROFILE: u32 = 151;
    pub const PROFILEMANAGER_LOAD_PROFILE: u32 = 152;
    pub const PROFILEMANAGER_DELETE_PROFILE: u32 = 153;
    pub const PROFILEMANAGER_UPLOAD_PROFILE: u32 = 154;
    pub const PROFILEMANAGER_DOWNLOAD_PROFILE: u32 = 155;
    pub const PROFILEMANAGER_GET_ACTIVE_PROFILE: u32 = 156;
    pub const PROFILEMANAGER_ACTIVE_PROFILE_CHANGED: u32 = 157;
    pub const PROFILEMANAGER_PROFILE_LOADED: u32 = 158;
    pub const PROFILEMANAGER_PROFILE_ABOUT_TO_LOAD: u32 = 159;
    pub const PROFILEMANAGER_PROFILE_LIST_UPDATED: u32 = 160;
    pub const PROFILEMANAGER_CLEAR_ACTIVE_PROFILE: u32 = 161;
    pub const PLUGINMANAGER_GET_PLUGIN_LIST: u32 = 200;
    pub const PLUGINMANAGER_PLUGIN_SPECIFIC: u32 = 201;
    pub const SETTINGSMANAGER_GET_SETTINGS: u32 = 250;
    pub const SETTINGSMANAGER_GET_SETTINGS_SCHEMA: u32 = 251;
    pub const SETTINGSMANAGER_MODIFY_SETTINGS: u32 = 252;
    pub const SETTINGSMANAGER_SET_SETTINGS: u32 = 253;
    pub const SETTINGSMANAGER_SAVE_SETTINGS: u32 = 254;
    pub const LOGMANAGER_CLEAR_LOG_BUFFER: u32 = 300;
    pub const LOGMANAGER_GET_LOG_BUFFER: u32 = 301;
    pub const LOGMANAGER_GET_LOG_LEVEL: u32 = 302;
    pub const LOGMANAGER_SET_LOG_LEVEL: u32 = 303;
    pub const LOGMANAGER_LOGGED_ENTRY: u32 = 304;
    pub const RGBCONTROLLER_RESIZEZONE: u32 = 1000;
    pub const RGBCONTROLLER_CLEARSEGMENTS: u32 = 1001;
    pub const RGBCONTROLLER_ADDSEGMENT: u32 = 1002;
    pub const RGBCONTROLLER_CONFIGUREZONE: u32 = 1003;
    pub const RGBCONTROLLER_CONFIGUREDEVICE: u32 = 1004;
    pub const RGBCONTROLLER_SETHIDDEN: u32 = 1005;
    pub const RGBCONTROLLER_UPDATELEDS: u32 = 1050;
    pub const RGBCONTROLLER_UPDATEZONELEDS: u32 = 1051;
    pub const RGBCONTROLLER_UPDATESINGLELED: u32 = 1052;
    pub const RGBCONTROLLER_SETCUSTOMMODE: u32 = 1100;
    pub const RGBCONTROLLER_UPDATEMODE: u32 = 1101;
    pub const RGBCONTROLLER_SAVEMODE: u32 = 1102;
    pub const RGBCONTROLLER_UPDATEZONEMODE: u32 = 1103;
    pub const RGBCONTROLLER_SETDEVICESPECIFICCONFIGURATION: u32 = 1130;
    pub const RGBCONTROLLER_SETDEVICESPECIFICZONECONFIGURATION: u32 = 1131;
    pub const RGBCONTROLLER_SIGNALUPDATE: u32 = 1150;

    /// Every id of *Packet IDs*, for the tests that try them all.
    #[cfg(test)]
    pub(crate) const ALL: [u32; 58] = [
        0, 1, 10, 40, 50, 51, 52, 53, 100, 101, 102, 103, 120, 121, 122, 123, 124, 140, 150, 151,
        152, 153, 154, 155, 156, 157, 158, 159, 160, 161, 200, 201, 250, 251, 252, 253, 254, 300,
        301, 302, 303, 304, 1000, 1001, 1002, 1003, 1004, 1005, 1050, 1051, 1052, 1100, 1101, 1102,
        1103, 1130, 1131, 1150,
    ];
}

/// The packets Candeo sends, and no other (§6.6).
pub const SENT: [u32; 6] = [
    id::REQUEST_PROTOCOL_VERSION,
    id::SET_CLIENT_NAME,
    id::REQUEST_CONTROLLER_COUNT,
    id::REQUEST_CONTROLLER_DATA,
    id::RGBCONTROLLER_UPDATEMODE,
    id::RGBCONTROLLER_UPDATELEDS,
];

pub fn is_sent(packet_id: u32) -> bool {
    SENT.contains(&packet_id)
}

/// *NetPacketHeader structure*: `pkt_magic`, `pkt_dev_id`, `pkt_id`,
/// `pkt_size`, then `pkt_size` bytes of payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// The controller's index (protocol 5 and below) or id (6), for the
    /// `RGBCONTROLLER_*` packets and their answers (*Device IDs*).
    pub dev_id: u32,
    pub id: u32,
    pub size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    #[error("a packet did not start with ORGB")]
    Magic,
    #[error("a {0}-byte payload is above the limit")]
    TooLarge(u32),
}

impl Header {
    pub fn encode(&self) -> [u8; HEADER_LEN] {
        let mut b = [0; HEADER_LEN];
        b[..4].copy_from_slice(&MAGIC);
        b[4..8].copy_from_slice(&self.dev_id.to_le_bytes());
        b[8..12].copy_from_slice(&self.id.to_le_bytes());
        b[12..].copy_from_slice(&self.size.to_le_bytes());
        b
    }

    pub fn decode(b: &[u8; HEADER_LEN]) -> Result<Self, WireError> {
        if b[..4] != MAGIC {
            return Err(WireError::Magic);
        }
        let word = |at: usize| u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
        let header = Self {
            dev_id: word(4),
            id: word(8),
            size: word(12),
        };
        if header.size > MAX_PAYLOAD {
            return Err(WireError::TooLarge(header.size));
        }
        Ok(header)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    #[error("packet {0} is not one Candeo sends")]
    Refused(u32),
    #[error("the mode saves itself to the device and is never selected")]
    SavesItself,
    #[error("{0} colours do not fit in one packet")]
    TooManyColors(usize),
    #[error("a {0}-byte string does not fit")]
    TooLong(usize),
    #[error("protocol {0} needs a value this mode does not have")]
    Protocol(u32),
}

/// A header and its payload, for an id of [`SENT`] only.
pub(crate) fn packet(packet_id: u32, dev_id: u32, payload: &[u8]) -> Result<Vec<u8>, EncodeError> {
    if !is_sent(packet_id) {
        return Err(EncodeError::Refused(packet_id));
    }
    let size = u32::try_from(payload.len())
        .ok()
        .filter(|size| *size <= MAX_PAYLOAD)
        .ok_or(EncodeError::TooLong(payload.len()))?;
    let header = Header {
        dev_id,
        id: packet_id,
        size,
    };
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&header.encode());
    out.extend_from_slice(payload);
    Ok(out)
}

/// What Candeo can ask of an OpenRGB server. Nothing else can be built.
#[derive(Debug, Clone, Copy)]
pub enum Request<'a> {
    /// *NET_PACKET_ID_REQUEST_PROTOCOL_VERSION*: the highest version Candeo
    /// speaks.
    ProtocolVersion,
    /// *NET_PACKET_ID_SET_CLIENT_NAME*: [`CLIENT_NAME`], null-terminated.
    ClientName,
    /// *NET_PACKET_ID_REQUEST_CONTROLLER_COUNT*: no data.
    ControllerCount,
    /// *NET_PACKET_ID_REQUEST_CONTROLLER_DATA*: the negotiated version, which
    /// the answer is written for.
    ControllerData { dev: u32, protocol: u32 },
    /// *NET_PACKET_ID_RGBCONTROLLER_UPDATEMODE*: `data_size`, `mode_idx`,
    /// then *Mode Data*. Refused for a mode that saves itself (§6.7).
    UpdateMode {
        dev: u32,
        /// Its place in the controller's modes, of which there are at most
        /// `u16::MAX` (*Device Data*, `num_modes`).
        index: u16,
        mode: &'a Mode,
        protocol: u32,
    },
    /// *NET_PACKET_ID_RGBCONTROLLER_UPDATELEDS*: `data_size`, `num_colors`,
    /// one colour per LED.
    UpdateLeds { dev: u32, colors: &'a [Rgb] },
}

impl Request<'_> {
    pub fn id(&self) -> u32 {
        match self {
            Self::ProtocolVersion => id::REQUEST_PROTOCOL_VERSION,
            Self::ClientName => id::SET_CLIENT_NAME,
            Self::ControllerCount => id::REQUEST_CONTROLLER_COUNT,
            Self::ControllerData { .. } => id::REQUEST_CONTROLLER_DATA,
            Self::UpdateMode { .. } => id::RGBCONTROLLER_UPDATEMODE,
            Self::UpdateLeds { .. } => id::RGBCONTROLLER_UPDATELEDS,
        }
    }

    /// The bytes to write. The document defines no `pkt_dev_id` for the
    /// handshake and the count: zero there.
    pub fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        match *self {
            Self::ProtocolVersion => packet(self.id(), 0, &CLIENT_PROTOCOL.to_le_bytes()),
            Self::ClientName => {
                let mut name = CLIENT_NAME.as_bytes().to_vec();
                name.push(0);
                packet(self.id(), 0, &name)
            }
            Self::ControllerCount => packet(self.id(), 0, &[]),
            Self::ControllerData { dev, protocol } => {
                packet(self.id(), dev, &protocol.to_le_bytes())
            }
            Self::UpdateMode {
                dev,
                index,
                mode,
                protocol,
            } => {
                if mode.saves_itself() {
                    return Err(EncodeError::SavesItself);
                }
                let mut body = i32::from(index).to_le_bytes().to_vec();
                encode_mode(mode, protocol, &mut body)?;
                packet(self.id(), dev, &with_size(&body))
            }
            Self::UpdateLeds { dev, colors } => {
                let n = u16::try_from(colors.len())
                    .map_err(|_| EncodeError::TooManyColors(colors.len()))?;
                let mut body = n.to_le_bytes().to_vec();
                for c in colors {
                    body.extend_from_slice(&c.to_wire());
                }
                packet(self.id(), dev, &with_size(&body))
            }
        }
    }
}

/// `data_size` and the body: "Size of all data in packet", written as the
/// whole payload, itself included (§6.6; to confirm, §9).
fn with_size(body: &[u8]) -> Vec<u8> {
    let size = u32::try_from(body.len() + 4).unwrap_or(u32::MAX);
    let mut out = size.to_le_bytes().to_vec();
    out.extend_from_slice(body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::parse_controller;
    use crate::fixtures;

    /// *NetPacketHeader structure*, by hand: `ORGB`, then `pkt_dev_id`,
    /// `pkt_id` and `pkt_size`, each four bytes, little endian.
    #[test]
    fn a_header_is_magic_then_three_words() {
        let h = Header {
            dev_id: 0x0102_0304,
            id: 1050,
            size: 10,
        };
        let expected = [
            b'O', b'R', b'G', b'B', 0x04, 0x03, 0x02, 0x01, 0x1a, 0x04, 0, 0, 10, 0, 0, 0,
        ];
        assert_eq!(h.encode(), expected);
        assert_eq!(Header::decode(&expected), Ok(h));
    }

    #[test]
    fn a_header_without_the_magic_or_too_large_is_refused() {
        let mut b = Header {
            dev_id: 0,
            id: 0,
            size: 0,
        }
        .encode();
        b[0] = b'X';
        assert_eq!(Header::decode(&b), Err(WireError::Magic));
        let b = Header {
            dev_id: 0,
            id: 1,
            size: MAX_PAYLOAD + 1,
        }
        .encode();
        assert_eq!(
            Header::decode(&b),
            Err(WireError::TooLarge(MAX_PAYLOAD + 1))
        );
    }

    /// The fixed list: of every id *Packet IDs* lists, only the six of §6.6
    /// can be encoded. `SAVEMODE` and `SET_CLIENT_FLAGS` among the refused.
    #[test]
    fn every_documented_packet_outside_the_list_is_refused() {
        let mut accepted = Vec::new();
        for packet_id in id::ALL {
            match packet(packet_id, 0, &[]) {
                Ok(_) => accepted.push(packet_id),
                Err(e) => assert_eq!(e, EncodeError::Refused(packet_id)),
            }
        }
        accepted.sort_unstable();
        assert_eq!(accepted, vec![0, 1, 40, 50, 1050, 1101]);
        for refused in [
            id::RGBCONTROLLER_SAVEMODE,
            id::RGBCONTROLLER_SETCUSTOMMODE,
            id::SET_CLIENT_FLAGS,
            id::REQUEST_RESCAN_DEVICES,
            id::SETTINGSMANAGER_SET_SETTINGS,
            id::PROFILEMANAGER_LOAD_PROFILE,
            id::GET_HID_DEVICE_INFO,
        ] {
            assert!(!is_sent(refused), "{refused}");
        }
    }

    /// An id the document does not list is refused too.
    #[test]
    fn an_unknown_packet_id_is_refused() {
        for packet_id in [2, 41, 999, 1053, 2000, u32::MAX] {
            assert_eq!(
                packet(packet_id, 0, &[]),
                Err(EncodeError::Refused(packet_id))
            );
        }
    }

    /// *NET_PACKET_ID_REQUEST_PROTOCOL_VERSION*, request: size 4, the
    /// client's highest version.
    #[test]
    fn the_protocol_request_carries_version_6() {
        let bytes = Request::ProtocolVersion.encode().unwrap();
        assert_eq!(&bytes[..4], b"ORGB");
        assert_eq!(&bytes[8..], &[40, 0, 0, 0, 4, 0, 0, 0, 6, 0, 0, 0]);
    }

    /// *NET_PACKET_ID_SET_CLIENT_NAME*: the name and its terminating zero,
    /// "strlen + 1" bytes.
    #[test]
    fn the_client_name_is_candeo_and_nothing_about_the_computer() {
        let bytes = Request::ClientName.encode().unwrap();
        assert_eq!(&bytes[8..16], &[50, 0, 0, 0, 7, 0, 0, 0]);
        assert_eq!(&bytes[16..], b"Candeo\0");
    }

    /// *NET_PACKET_ID_REQUEST_CONTROLLER_DATA*, request, protocol 1+: size 4,
    /// the version; the controller in `pkt_dev_id`.
    #[test]
    fn the_controller_request_names_the_controller_and_the_version() {
        let bytes = Request::ControllerData {
            dev: 7,
            protocol: 5,
        }
        .encode()
        .unwrap();
        assert_eq!(
            &bytes[4..],
            &[7, 0, 0, 0, 1, 0, 0, 0, 4, 0, 0, 0, 5, 0, 0, 0]
        );
        let count = Request::ControllerCount.encode().unwrap();
        assert_eq!(&count[8..], &[0; 8]);
    }

    /// *NET_PACKET_ID_RGBCONTROLLER_UPDATELEDS*: `data_size`, `num_colors`,
    /// then each `RGBColor` as red, green, blue, zero.
    #[test]
    fn update_leds_by_hand() {
        let colors = [Rgb::new(0x11, 0x22, 0x33), Rgb::new(0xaa, 0xbb, 0xcc)];
        let bytes = Request::UpdateLeds {
            dev: 3,
            colors: &colors,
        }
        .encode()
        .unwrap();
        let expected_payload = [
            14, 0, 0, 0, // data_size: the whole payload
            2, 0, // num_colors
            0x11, 0x22, 0x33, 0, // led_color[0]
            0xaa, 0xbb, 0xcc, 0, // led_color[1]
        ];
        assert_eq!(&bytes[4..16], &[3, 0, 0, 0, 0x1a, 0x04, 0, 0, 14, 0, 0, 0]);
        assert_eq!(&bytes[16..], &expected_payload);
    }

    #[test]
    fn more_colours_than_a_u16_are_refused() {
        let colors = vec![Rgb::default(); usize::from(u16::MAX) + 1];
        assert!(matches!(
            Request::UpdateLeds {
                dev: 0,
                colors: &colors
            }
            .encode(),
            Err(EncodeError::TooManyColors(_))
        ));
    }

    /// *NET_PACKET_ID_RGBCONTROLLER_UPDATEMODE*: `data_size`, `mode_idx`, then
    /// *Mode Data*; below protocol 6 the `mode_value` goes back.
    #[test]
    fn update_mode_carries_the_index_and_the_mode() {
        let c = parse_controller(&fixtures::keyboard(5), 5).unwrap();
        let direct = &c.modes[0];
        let bytes = Request::UpdateMode {
            dev: 2,
            index: 0,
            mode: direct,
            protocol: 5,
        }
        .encode()
        .unwrap();
        let payload = &bytes[16..];
        let size = u32::from_le_bytes(payload[..4].try_into().unwrap());
        assert_eq!(size as usize, payload.len());
        assert_eq!(&payload[4..8], &[0, 0, 0, 0], "mode_idx");
        assert_eq!(&payload[8..10], &[7, 0], "mode_name_len, with its zero");
        assert_eq!(&payload[10..17], b"Direct\0");
        assert_eq!(&payload[17..21], &0x10i32.to_le_bytes(), "mode_value");
    }

    /// The one sign of a mode written to the device's memory: never selected.
    #[test]
    fn a_mode_that_saves_itself_cannot_be_selected() {
        let c = parse_controller(&fixtures::keyboard(6), 6).unwrap();
        let wave = &c.modes[2];
        assert!(wave.saves_itself());
        assert_eq!(
            Request::UpdateMode {
                dev: 0,
                index: 2,
                mode: wave,
                protocol: 6
            }
            .encode(),
            Err(EncodeError::SavesItself)
        );
    }
}
