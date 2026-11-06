//! A connection to an OpenRGB server (`openrgb-output.md` §6.6, #368 steps
//! 1 and 4).
//!
//! Loopback only. A thread reads the socket, so packets nobody asked for — a
//! list update, an `ACK`, anything a newer server sends — never fill it while
//! frames go out; requests wait for their answer by packet id. Everything
//! written goes through [`Request::encode`], so through the fixed list.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, SocketAddrV4, TcpStream};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use crate::controller::{parse_controller, Controller, Mode, ParseError};
use crate::wire::{id, EncodeError, Header, Request, CLIENT_PROTOCOL, HEADER_LEN, MIN_PROTOCOL};
use crate::Rgb;

/// *Protocol Basics*: "The default TCP port is 6742". Where Candeo looks for
/// the user's server, and never where it starts its own (§6.3, §6.5).
pub const DEFAULT_PORT: u16 = 6742;

/// More controllers than this in one answer is taken for a malformed one.
const MAX_CONTROLLERS: u32 = 4096;

/// *NET_PACKET_ID_ACK*, status 4: "invalid ID".
pub const ACK_INVALID_ID: u32 = 4;

/// `127.0.0.1` and a port.
pub fn endpoint(port: u16) -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
}

#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub connect: Duration,
    /// For the version's answer: none means a version 0 server.
    pub version: Duration,
    pub reply: Duration,
}

impl Default for Timeouts {
    /// §6.3: half a second to connect, a second for the version.
    fn default() -> Self {
        Self {
            connect: Duration::from_millis(500),
            version: Duration::from_secs(1),
            reply: Duration::from_secs(5),
        }
    }
}

/// English, for the log: the interface shows a `Failure` (§6.10).
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("only a loopback address is accepted")]
    NotLoopback,
    #[error("could not connect: {0}")]
    Connect(#[source] std::io::Error),
    #[error("the server did not answer the protocol version")]
    NoVersionAnswer,
    #[error("the server speaks protocol {server}; {MIN_PROTOCOL} or later is needed")]
    TooOld { server: u32 },
    #[error("no answer to packet {0} in time")]
    Timeout(u32),
    #[error("the server refused packet {packet} with status {status}")]
    Nack { packet: u32, status: u32 },
    #[error("a malformed answer to packet {0}")]
    Malformed(u32),
    #[error("the connection closed: {0}")]
    Closed(String),
    #[error(transparent)]
    Encode(#[from] EncodeError),
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error("could not send: {0}")]
    Send(#[source] std::io::Error),
}

/// A failed `ACK` (protocol 6): the packet it answers and its status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ack {
    pub packet: u32,
    pub status: u32,
}

enum Event {
    Packet(Header, Vec<u8>),
    Closed(String),
}

pub struct Client {
    stream: TcpStream,
    events: Receiver<Event>,
    protocol: u32,
    server_protocol: u32,
    timeouts: Timeouts,
    list_changed: bool,
    failures: Vec<Ack>,
    dropped: usize,
    closed: Option<String>,
}

impl Client {
    /// Connects and negotiates: Candeo sends 6, the server its highest, the
    /// lower is spoken; below 4 the connection is refused, and no answer at
    /// all is a version 0 server (*NET_PACKET_ID_REQUEST_PROTOCOL_VERSION*).
    /// Then the client's name.
    pub fn connect(addr: SocketAddr, timeouts: Timeouts) -> Result<Self, ClientError> {
        if !addr.ip().is_loopback() {
            return Err(ClientError::NotLoopback);
        }
        let stream =
            TcpStream::connect_timeout(&addr, timeouts.connect).map_err(ClientError::Connect)?;
        // Frames are small and late ones useless: no Nagle.
        let _ = stream.set_nodelay(true);
        let reading = stream.try_clone().map_err(ClientError::Connect)?;
        let (tx, events) = mpsc::channel();
        thread::Builder::new()
            .name("openrgb-reader".into())
            .spawn(move || read_loop(reading, &tx))
            .map_err(ClientError::Connect)?;
        let mut client = Self {
            stream,
            events,
            protocol: 0,
            server_protocol: 0,
            timeouts,
            list_changed: false,
            failures: Vec::new(),
            dropped: 0,
            closed: None,
        };
        client.send(&Request::ProtocolVersion)?;
        let answer = match client.wait(id::REQUEST_PROTOCOL_VERSION, None, timeouts.version) {
            Err(ClientError::Timeout(_)) => return Err(ClientError::NoVersionAnswer),
            other => other?.1,
        };
        let server = (answer.len() == 4)
            .then(|| word(&answer, 0))
            .flatten()
            .ok_or(ClientError::Malformed(id::REQUEST_PROTOCOL_VERSION))?;
        if server < MIN_PROTOCOL {
            return Err(ClientError::TooOld { server });
        }
        client.server_protocol = server;
        client.protocol = server.min(CLIENT_PROTOCOL);
        client.send(&Request::ClientName)?;
        Ok(client)
    }

    /// The version spoken: the lower of Candeo's and the server's.
    pub fn protocol(&self) -> u32 {
        self.protocol
    }

    /// The server's own highest version, for the log.
    pub fn server_protocol(&self) -> u32 {
        self.server_protocol
    }

    /// The controllers' ids (protocol 6) or indexes (below), as
    /// *NET_PACKET_ID_REQUEST_CONTROLLER_COUNT* answers them.
    pub fn controller_ids(&mut self) -> Result<Vec<u32>, ClientError> {
        const ID: u32 = id::REQUEST_CONTROLLER_COUNT;
        self.send(&Request::ControllerCount)?;
        let (_, p) = self.wait(ID, None, self.timeouts.reply)?;
        let count = word(&p, 0)
            .filter(|n| *n <= MAX_CONTROLLERS)
            .ok_or(ClientError::Malformed(ID))?;
        if self.protocol >= 6 {
            // Protocol 6+: "4 + (4 * number of controllers)", the ids in the
            // order of the server's list.
            let n = count as usize;
            if p.len() != 4 + 4 * n {
                return Err(ClientError::Malformed(ID));
            }
            Ok((0..n).filter_map(|i| word(&p, 4 + 4 * i)).collect())
        } else {
            if p.len() != 4 {
                return Err(ClientError::Malformed(ID));
            }
            Ok((0..count).collect())
        }
    }

    /// One controller, read for the version spoken.
    pub fn controller(&mut self, dev: u32) -> Result<Controller, ClientError> {
        self.send(&Request::ControllerData {
            dev,
            protocol: self.protocol,
        })?;
        let (_, p) = self.wait(id::REQUEST_CONTROLLER_DATA, Some(dev), self.timeouts.reply)?;
        Ok(parse_controller(&p, self.protocol)?)
    }

    /// Selects a mode; [`Request::UpdateMode`] refuses one that saves itself.
    pub fn update_mode(&mut self, dev: u32, index: u16, mode: &Mode) -> Result<(), ClientError> {
        self.send(&Request::UpdateMode {
            dev,
            index,
            mode,
            protocol: self.protocol,
        })
    }

    /// A frame: every LED's colour.
    pub fn update_leds(&mut self, dev: u32, colors: &[Rgb]) -> Result<(), ClientError> {
        self.send(&Request::UpdateLeds { dev, colors })
    }

    /// Whether `DEVICE_LIST_UPDATED` came since the last call.
    pub fn take_list_changed(&mut self) -> Result<bool, ClientError> {
        self.pump()?;
        Ok(std::mem::take(&mut self.list_changed))
    }

    /// The `ACK`s that reported a failure since the last call.
    pub fn take_failures(&mut self) -> Result<Vec<Ack>, ClientError> {
        self.pump()?;
        Ok(std::mem::take(&mut self.failures))
    }

    /// How many packets came that Candeo does not act on.
    pub fn dropped(&self) -> usize {
        self.dropped
    }

    fn send(&mut self, request: &Request<'_>) -> Result<(), ClientError> {
        if let Some(reason) = &self.closed {
            return Err(ClientError::Closed(reason.clone()));
        }
        let bytes = request.encode()?;
        self.stream.write_all(&bytes).map_err(ClientError::Send)
    }

    /// Handles what came without waiting.
    fn pump(&mut self) -> Result<(), ClientError> {
        loop {
            match self.events.try_recv() {
                Ok(Event::Packet(h, p)) => self.unasked(&h, &p),
                Ok(Event::Closed(reason)) => return Err(self.close(reason)),
                Err(TryRecvError::Empty) => return Ok(()),
                Err(TryRecvError::Disconnected) => {
                    return Err(self.close("the reader stopped".into()))
                }
            }
        }
    }

    /// The answer to `packet` (for controller `dev` when given), what came in
    /// between handled on the way.
    fn wait(
        &mut self,
        packet: u32,
        dev: Option<u32>,
        timeout: Duration,
    ) -> Result<(Header, Vec<u8>), ClientError> {
        if let Some(reason) = &self.closed {
            return Err(ClientError::Closed(reason.clone()));
        }
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(Event::Packet(h, p)) => {
                    let for_us = dev.is_none_or(|d| d == h.dev_id);
                    if h.id == packet && for_us {
                        return Ok((h, p));
                    }
                    // *NET_PACKET_ID_ACK*: its `pkt_dev_id` is the
                    // acknowledged packet's; a failure ends the wait.
                    if let (id::ACK, Some(ack)) = (h.id, ack(&p)) {
                        if ack.packet == packet && for_us && ack.status != 0 {
                            return Err(ClientError::Nack {
                                packet,
                                status: ack.status,
                            });
                        }
                    }
                    self.unasked(&h, &p);
                }
                Ok(Event::Closed(reason)) => return Err(self.close(reason)),
                Err(RecvTimeoutError::Timeout) => return Err(ClientError::Timeout(packet)),
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(self.close("the reader stopped".into()))
                }
            }
        }
    }

    fn unasked(&mut self, h: &Header, p: &[u8]) {
        match h.id {
            id::DEVICE_LIST_UPDATED => self.list_changed = true,
            id::ACK => match ack(p) {
                Some(a) if a.status != 0 => self.failures.push(a),
                Some(_) => {}
                None => self.dropped += 1,
            },
            _ => self.dropped += 1,
        }
    }

    fn close(&mut self, reason: String) -> ClientError {
        self.closed = Some(reason.clone());
        ClientError::Closed(reason)
    }
}

impl Drop for Client {
    /// Ends the reader thread with the connection.
    fn drop(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

/// *NET_PACKET_ID_ACK*: size 8, the acknowledged packet id and a status.
fn ack(p: &[u8]) -> Option<Ack> {
    (p.len() == 8).then(|| Ack {
        packet: word(p, 0).unwrap_or(0),
        status: word(p, 4).unwrap_or(0),
    })
}

fn word(p: &[u8], at: usize) -> Option<u32> {
    let b = p.get(at..at + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_loop(mut stream: TcpStream, tx: &Sender<Event>) {
    loop {
        let mut head = [0u8; HEADER_LEN];
        if let Err(e) = stream.read_exact(&mut head) {
            let _ = tx.send(Event::Closed(e.to_string()));
            return;
        }
        let header = match Header::decode(&head) {
            Ok(h) => h,
            Err(e) => {
                let _ = tx.send(Event::Closed(e.to_string()));
                return;
            }
        };
        // `Header::decode` bounds the size.
        let mut payload = vec![0; header.size as usize];
        if let Err(e) = stream.read_exact(&mut payload) {
            let _ = tx.send(Event::Closed(e.to_string()));
            return;
        }
        if tx.send(Event::Packet(header, payload)).is_err() {
            return;
        }
    }
}

/// The controllers a server lists, kept in step with it.
#[derive(Debug, Default)]
pub struct Roster {
    pub controllers: BTreeMap<u32, Controller>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Changes {
    pub added: Vec<u32>,
    pub removed: Vec<u32>,
}

impl Roster {
    /// Reads the list again, as *NET_PACKET_ID_DEVICE_LIST_UPDATED* asks for
    /// the version spoken: from protocol 6, ids that are gone are dropped and
    /// only new ones read; below, every controller is read again by index,
    /// and what follows binds them again by fingerprint (§6.9).
    pub fn refresh(&mut self, client: &mut Client) -> Result<Changes, ClientError> {
        let ids = client.controller_ids()?;
        let mut changes = Changes::default();
        if client.protocol() >= 6 {
            let listed: BTreeSet<u32> = ids.iter().copied().collect();
            changes.removed = self
                .controllers
                .keys()
                .filter(|i| !listed.contains(i))
                .copied()
                .collect();
            for i in &changes.removed {
                self.controllers.remove(i);
            }
        } else {
            changes.removed = self.controllers.keys().copied().collect();
            self.controllers.clear();
        }
        for i in ids {
            if self.controllers.contains_key(&i) {
                continue;
            }
            match client.controller(i) {
                Ok(c) => {
                    self.controllers.insert(i, c);
                    changes.added.push(i);
                }
                // Gone between the count and the request: the next list
                // update says so.
                Err(ClientError::Nack {
                    status: ACK_INVALID_ID,
                    ..
                }) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::model;
    use crate::wire::{is_sent, MAX_PAYLOAD};
    use std::net::{Ipv6Addr, TcpListener};
    use std::thread::JoinHandle;

    /// The far end of a fake server, in this process.
    struct Peer {
        stream: TcpStream,
        received: Vec<u32>,
    }

    impl Peer {
        /// The next packet the client sent, which must be one of the six.
        fn next(&mut self) -> Option<(Header, Vec<u8>)> {
            let mut head = [0u8; HEADER_LEN];
            self.stream.read_exact(&mut head).ok()?;
            let h = Header::decode(&head).expect("the client wrote a valid header");
            let mut p = vec![0; h.size as usize];
            self.stream.read_exact(&mut p).ok()?;
            assert!(is_sent(h.id), "the client sent packet {}", h.id);
            self.received.push(h.id);
            Some((h, p))
        }

        fn expect(&mut self, packet: u32) -> (Header, Vec<u8>) {
            let (h, p) = self.next().expect("a packet");
            assert_eq!(h.id, packet);
            (h, p)
        }

        fn send(&mut self, packet: u32, dev: u32, payload: &[u8]) {
            let h = Header {
                dev_id: dev,
                id: packet,
                size: u32::try_from(payload.len()).unwrap(),
            };
            self.stream.write_all(&h.encode()).unwrap();
            self.stream.write_all(payload).unwrap();
        }

        /// Answers the handshake with `version` and reads the client's name.
        fn handshake(&mut self, version: u32) {
            let (_, p) = self.expect(id::REQUEST_PROTOCOL_VERSION);
            assert_eq!(p, 6u32.to_le_bytes(), "Candeo asks for 6");
            self.send(id::REQUEST_PROTOCOL_VERSION, 0, &version.to_le_bytes());
            if version >= MIN_PROTOCOL {
                let (_, name) = self.expect(id::SET_CLIENT_NAME);
                assert_eq!(name, b"Candeo\0");
            }
        }

        fn count(&mut self, payload: &[u32]) {
            self.expect(id::REQUEST_CONTROLLER_COUNT);
            let bytes: Vec<u8> = payload.iter().flat_map(|w| w.to_le_bytes()).collect();
            self.send(id::REQUEST_CONTROLLER_COUNT, 0, &bytes);
        }

        /// Answers a data request for `dev`, read for `version`.
        fn data(&mut self, dev: u32, version: u32, payload: &[u8]) {
            let (h, p) = self.expect(id::REQUEST_CONTROLLER_DATA);
            assert_eq!(h.dev_id, dev);
            assert_eq!(p, version.to_le_bytes());
            self.send(id::REQUEST_CONTROLLER_DATA, dev, payload);
        }

        /// Until the client closes.
        fn drain(&mut self) {
            while self.next().is_some() {}
        }
    }

    /// A server answering one connection with `script`; its thread returns
    /// every packet id the client sent.
    fn serve(
        script: impl FnOnce(&mut Peer) + Send + 'static,
    ) -> (SocketAddr, JoinHandle<Vec<u32>>) {
        let listener = TcpListener::bind(endpoint(0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut peer = Peer {
                stream,
                received: Vec::new(),
            };
            script(&mut peer);
            peer.received
        });
        (addr, server)
    }

    fn quick() -> Timeouts {
        Timeouts {
            connect: Duration::from_secs(2),
            version: Duration::from_millis(300),
            reply: Duration::from_secs(5),
        }
    }

    #[test]
    fn the_lower_version_is_spoken() {
        for (server, spoken) in [(6, 6), (5, 5), (4, 4), (9, 6)] {
            let (addr, done) = serve(move |p| {
                p.handshake(server);
                p.drain();
            });
            let client = Client::connect(addr, quick()).unwrap();
            assert_eq!(
                (client.protocol(), client.server_protocol()),
                (spoken, server)
            );
            drop(client);
            assert_eq!(done.join().unwrap(), vec![40, 50]);
        }
    }

    #[test]
    fn a_server_below_protocol_4_is_refused() {
        let (addr, done) = serve(|p| {
            p.handshake(3);
            p.drain();
        });
        let e = Client::connect(addr, quick()).err().unwrap();
        assert!(matches!(e, ClientError::TooOld { server: 3 }), "{e}");
        assert_eq!(done.join().unwrap(), vec![40], "not even the name is sent");
    }

    /// A version 0 server does not answer `REQUEST_PROTOCOL_VERSION`.
    #[test]
    fn no_answer_to_the_version_is_a_version_0_server() {
        let (addr, done) = serve(|p| {
            p.expect(id::REQUEST_PROTOCOL_VERSION);
            p.drain();
        });
        let e = Client::connect(addr, quick()).err().unwrap();
        assert!(matches!(e, ClientError::NoVersionAnswer), "{e}");
        done.join().unwrap();
    }

    #[test]
    fn an_address_that_is_not_loopback_is_refused_before_connecting() {
        for addr in [
            SocketAddr::from((Ipv4Addr::UNSPECIFIED, DEFAULT_PORT)),
            SocketAddr::from((Ipv6Addr::UNSPECIFIED, DEFAULT_PORT)),
        ] {
            assert!(matches!(
                Client::connect(addr, quick()),
                Err(ClientError::NotLoopback)
            ));
        }
    }

    #[test]
    fn protocol_6_lists_ids_and_below_lists_indexes() {
        let (addr, done) = serve(|p| {
            p.handshake(6);
            p.count(&[2, 7, 9]);
            p.drain();
        });
        let mut c = Client::connect(addr, quick()).unwrap();
        assert_eq!(c.controller_ids().unwrap(), vec![7, 9]);
        drop(c);
        done.join().unwrap();

        let (addr, done) = serve(|p| {
            p.handshake(4);
            p.count(&[3]);
            p.drain();
        });
        let mut c = Client::connect(addr, quick()).unwrap();
        assert_eq!(c.controller_ids().unwrap(), vec![0, 1, 2]);
        drop(c);
        done.join().unwrap();
    }

    #[test]
    fn a_count_that_disagrees_with_its_ids_is_malformed() {
        let (addr, done) = serve(|p| {
            p.handshake(6);
            p.count(&[3, 7]);
            p.drain();
        });
        let mut c = Client::connect(addr, quick()).unwrap();
        assert!(matches!(c.controller_ids(), Err(ClientError::Malformed(0))));
        drop(c);
        done.join().unwrap();
    }

    /// The round trip of #368's first steps against a fake server: the
    /// handshake, the list, a controller, its direct mode, one frame. The
    /// server checks each packet's bytes, and that nothing outside the six
    /// was sent.
    #[test]
    fn read_take_over_and_light_a_controller() {
        let (addr, done) = serve(|p| {
            p.handshake(6);
            p.count(&[1, 42]);
            p.data(42, 6, &fixtures::keyboard(6));
            let (h, mode) = p.expect(id::RGBCONTROLLER_UPDATEMODE);
            assert_eq!(h.dev_id, 42);
            assert_eq!(&mode[4..8], &[0, 0, 0, 0], "mode_idx: Direct");
            assert_eq!(&mode[10..17], b"Direct\0");
            let (h, leds) = p.expect(id::RGBCONTROLLER_UPDATELEDS);
            assert_eq!(h.dev_id, 42);
            assert_eq!(&leds[4..6], &[5, 0], "num_colors: every LED");
            assert_eq!(&leds[6..10], &[0xff, 0x80, 0x00, 0x00]);
            p.drain();
        });
        let mut c = Client::connect(addr, quick()).unwrap();
        let ids = c.controller_ids().unwrap();
        let controller = c.controller(ids[0]).unwrap();
        let device = model::describe(&controller);
        let direct = device.drive.unwrap();
        let mode = model::take_over(&controller.modes[usize::from(direct)]);
        c.update_mode(42, direct, &mode).unwrap();
        let frame = model::frame(&device, |_, _| Rgb::new(0xff, 0x80, 0));
        c.update_leds(42, &frame).unwrap();
        drop(c);
        let received = done.join().unwrap();
        assert_eq!(received, vec![40, 50, 0, 1, 1101, 1050]);
    }

    /// A packet nobody asked for — a log entry, a list update — does not
    /// derail the answer being waited for.
    #[test]
    fn unasked_packets_are_dropped_and_list_updates_noted() {
        let (addr, done) = serve(|p| {
            p.handshake(6);
            p.expect(id::REQUEST_CONTROLLER_COUNT);
            p.send(id::LOGMANAGER_LOGGED_ENTRY, 0, b"anything at all");
            p.send(id::DEVICE_LIST_UPDATED, 0, &[]);
            p.send(id::ACK, 0, &[52, 0, 0, 0, 3, 0, 0, 0]);
            p.send(id::REQUEST_CONTROLLER_COUNT, 0, &[0, 0, 0, 0]);
            p.drain();
        });
        let mut c = Client::connect(addr, quick()).unwrap();
        assert_eq!(c.controller_ids().unwrap(), Vec::<u32>::new());
        assert!(c.take_list_changed().unwrap());
        assert!(!c.take_list_changed().unwrap(), "taken once");
        assert_eq!(
            c.take_failures().unwrap(),
            vec![Ack {
                packet: 52,
                status: 3
            }]
        );
        assert_eq!(c.dropped(), 1);
        drop(c);
        done.join().unwrap();
    }

    #[test]
    fn an_oversized_packet_closes_the_connection() {
        let (addr, done) = serve(|p| {
            p.handshake(6);
            p.expect(id::REQUEST_CONTROLLER_COUNT);
            let h = Header {
                dev_id: 0,
                id: id::REQUEST_CONTROLLER_COUNT,
                size: MAX_PAYLOAD + 1,
            };
            p.stream.write_all(&h.encode()).unwrap();
            p.drain();
        });
        let mut c = Client::connect(addr, quick()).unwrap();
        assert!(matches!(c.controller_ids(), Err(ClientError::Closed(_))));
        assert!(
            matches!(c.update_leds(0, &[]), Err(ClientError::Closed(_))),
            "closed for good"
        );
        drop(c);
        done.join().unwrap();
    }

    /// Protocol 6: ids that are gone are dropped and only new ones read.
    #[test]
    fn a_protocol_6_refresh_reads_only_new_controllers() {
        let (addr, done) = serve(|p| {
            p.handshake(6);
            p.count(&[2, 1, 2]);
            p.data(1, 6, &fixtures::strip("One", 3, "I2C: bus 1", ""));
            p.data(2, 6, &fixtures::strip("Two", 3, "I2C: bus 2", ""));
            p.send(id::DEVICE_LIST_UPDATED, 0, &[]);
            p.count(&[2, 2, 3]);
            p.data(3, 6, &fixtures::strip("Three", 3, "I2C: bus 3", ""));
            p.drain();
        });
        let mut c = Client::connect(addr, quick()).unwrap();
        let mut roster = Roster::default();
        let first = roster.refresh(&mut c).unwrap();
        assert_eq!(first.added, vec![1, 2]);
        let second = roster.refresh(&mut c).unwrap();
        assert_eq!(
            second,
            Changes {
                added: vec![3],
                removed: vec![1]
            }
        );
        // The update came before the count's answer, on the same stream.
        assert!(c.take_list_changed().unwrap());
        assert_eq!(roster.controllers[&2].name, "Two");
        drop(c);
        let received = done.join().unwrap();
        assert_eq!(
            received.iter().filter(|i| **i == 1).count(),
            3,
            "each controller read once"
        );
    }

    /// Below protocol 6, indexes mean nothing after a change: all are read again.
    #[test]
    fn a_protocol_4_refresh_reads_everything_again() {
        let (addr, done) = serve(|p| {
            p.handshake(4);
            p.count(&[1]);
            p.data(0, 4, &fixtures::keyboard(4));
            p.count(&[1]);
            p.data(0, 4, &fixtures::keyboard(4));
            p.drain();
        });
        let mut c = Client::connect(addr, quick()).unwrap();
        let mut roster = Roster::default();
        assert_eq!(roster.refresh(&mut c).unwrap().added, vec![0]);
        assert_eq!(
            roster.refresh(&mut c).unwrap(),
            Changes {
                added: vec![0],
                removed: vec![0]
            }
        );
        drop(c);
        done.join().unwrap();
    }

    /// Protocol 6 answers an unknown id with a failed `ACK`: the wait ends at
    /// once, and a refresh skips the controller that went.
    #[test]
    fn a_controller_gone_between_count_and_request_is_skipped() {
        let (addr, done) = serve(|p| {
            p.handshake(6);
            p.count(&[2, 5, 6]);
            let (h, _) = p.expect(id::REQUEST_CONTROLLER_DATA);
            assert_eq!(h.dev_id, 5);
            let mut ack = 1u32.to_le_bytes().to_vec();
            ack.extend_from_slice(&ACK_INVALID_ID.to_le_bytes());
            p.send(id::ACK, 5, &ack);
            p.data(6, 6, &fixtures::strip("Six", 3, "I2C: bus 6", ""));
            p.drain();
        });
        let mut c = Client::connect(addr, quick()).unwrap();
        let mut roster = Roster::default();
        assert_eq!(roster.refresh(&mut c).unwrap().added, vec![6]);
        drop(c);
        done.join().unwrap();
    }
}
