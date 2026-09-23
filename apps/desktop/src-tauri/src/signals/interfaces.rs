//! Where the signals API listens: loopback, and the interfaces someone ticked
//! (§2.3).
//!
//! An interface is kept **by name**, never by address: an address changes with
//! DHCP, another Wi-Fi or a dock, and a choice kept as `192.0.2.23` would stop
//! answering the day it does. [`addresses`] turns the names into the addresses
//! they have now, and is pure, so that is a unit test.

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use serde::Serialize;

/// An interface that is up, as Settings lists it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NetworkInterface {
    /// Its name as the system shows it: *Wi-Fi*, *Ethernet*, `eth0`.
    pub name: String,
    pub addresses: Vec<IpAddr>,
}

/// The interfaces that are up and have an address other machines can reach,
/// loopback aside — it is always listened on and never offered.
pub fn list() -> Vec<NetworkInterface> {
    let found = match if_addrs::get_if_addrs() {
        Ok(found) => found,
        Err(e) => {
            tracing::warn!("network interfaces not listed: {e}");
            return Vec::new();
        }
    };
    group(
        found
            .into_iter()
            .filter(|i| i.is_oper_up() && !i.is_loopback())
            .map(|i| (i.name.clone(), i.ip()))
            .collect(),
    )
}

/// Addresses grouped by interface, in the order the system gave them, keeping
/// only those a listener can bind without a scope: a link-local IPv6 address
/// needs one, and nobody sends to it by hand.
fn group(found: Vec<(String, IpAddr)>) -> Vec<NetworkInterface> {
    let mut interfaces: Vec<NetworkInterface> = Vec::new();
    for (name, ip) in found {
        if is_link_local_v6(ip) {
            continue;
        }
        match interfaces.iter_mut().find(|i| i.name == name) {
            Some(interface) => interface.addresses.push(ip),
            None => interfaces.push(NetworkInterface {
                name,
                addresses: vec![ip],
            }),
        }
    }
    for interface in &mut interfaces {
        // IPv4 first: it is the address someone copies into Home Assistant.
        interface.addresses.sort_by_key(|ip| ip.is_ipv6());
    }
    interfaces
}

fn is_link_local_v6(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V6(v6) if (v6.segments()[0] & 0xffc0) == 0xfe80)
}

/// Every address to listen on: loopback, both families, and the current
/// addresses of the ticked interfaces. An interface ticked but down or gone adds
/// nothing, and comes back with its name.
///
/// Loopback in both families because `localhost` resolves to `::1` first on
/// some systems, and a client that does not fall back would find nobody there.
pub fn addresses(port: u16, ticked: &[String], up: &[NetworkInterface]) -> BTreeSet<SocketAddr> {
    let mut wanted: BTreeSet<SocketAddr> = [
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
    ]
    .into_iter()
    .map(|ip| SocketAddr::new(ip, port))
    .collect();
    for interface in up.iter().filter(|i| ticked.contains(&i.name)) {
        wanted.extend(
            interface
                .addresses
                .iter()
                .map(|&ip| SocketAddr::new(ip, port)),
        );
    }
    wanted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    fn up() -> Vec<NetworkInterface> {
        group(vec![
            ("Wi-Fi".into(), ip("fe80::1")),
            ("Wi-Fi".into(), ip("2001:db8::23")),
            ("Wi-Fi".into(), ip("192.0.2.23")),
            ("Ethernet".into(), ip("198.51.100.5")),
        ])
    }

    #[test]
    fn interfaces_are_grouped_ipv4_first_without_link_local() {
        assert_eq!(
            up(),
            vec![
                NetworkInterface {
                    name: "Wi-Fi".into(),
                    addresses: vec![ip("192.0.2.23"), ip("2001:db8::23")],
                },
                NetworkInterface {
                    name: "Ethernet".into(),
                    addresses: vec![ip("198.51.100.5")],
                },
            ]
        );
    }

    #[test]
    fn loopback_is_always_listened_on() {
        let wanted = addresses(7317, &[], &up());
        let expected: BTreeSet<SocketAddr> = ["127.0.0.1:7317", "[::1]:7317"]
            .into_iter()
            .map(|a| a.parse().unwrap())
            .collect();
        assert_eq!(wanted, expected);
    }

    #[test]
    fn a_ticked_interface_adds_its_current_addresses() {
        let wanted = addresses(7317, &["Wi-Fi".into()], &up());
        assert!(wanted.contains(&"192.0.2.23:7317".parse().unwrap()));
        assert!(wanted.contains(&"[2001:db8::23]:7317".parse().unwrap()));
        assert!(
            !wanted.contains(&"198.51.100.5:7317".parse().unwrap()),
            "Ethernet was not ticked"
        );
    }

    #[test]
    fn a_ticked_interface_that_is_gone_adds_nothing() {
        let wanted = addresses(7317, &["Docking station".into()], &up());
        assert_eq!(wanted.len(), 2, "loopback only");
    }
}
