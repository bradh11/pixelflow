//! Is a controller there? A quick, read-only look: open a TCP connection to its web port and
//! close it again. Nothing is sent or read, so nothing on the device changes (and no endpoint that
//! could return credentials is touched). A controller that refuses the connection still answered:
//! it's on the network, it just has no web page there (a plain sACN receiver).

use crate::discover::{local_ipv4_interfaces, probeable};
use serde::Serialize;
use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Most addresses looked at in one check.
const MAX_ADDRESSES: usize = 64;
/// Most resolved addresses tried for one host name.
const MAX_TRIES: usize = 4;

/// How the app sees whether a controller answers (a fake in tests).
pub trait Reach: Send + Sync {
    /// Whether something answers at `address` (an IP address or host name). Read-only.
    fn answers(&self, address: &str) -> bool;
}

/// Looks with a TCP connection to port 80 (or the port in the address), given up after `timeout`.
pub struct TcpReach {
    timeout: Duration,
}

impl TcpReach {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl Reach for TcpReach {
    fn answers(&self, address: &str) -> bool {
        socket_addrs(address).iter().take(MAX_TRIES).any(|to| {
            match TcpStream::connect_timeout(to, self.timeout) {
                Ok(_) => true,
                // Turned away: the host is there, with nothing listening on that port.
                Err(e) => e.kind() == ErrorKind::ConnectionRefused,
            }
        })
    }
}

/// Where `address` can be reached: itself when it has a port, else its port 80. Addresses no
/// request should go to (broadcast, multicast, loopback, …) give none.
fn socket_addrs(address: &str) -> Vec<SocketAddr> {
    let address = address.trim();
    let all: Vec<SocketAddr> = if address.is_empty() {
        Vec::new()
    } else if let Ok(to) = address.parse::<SocketAddr>() {
        vec![to]
    } else if let Ok(ip) = address.parse::<IpAddr>() {
        vec![SocketAddr::new(ip, 80)]
    } else {
        (address, 80)
            .to_socket_addrs()
            .map(Iterator::collect)
            .unwrap_or_default()
    };
    all.into_iter()
        .filter(|to| match to.ip() {
            IpAddr::V4(ip) => probeable(ip),
            IpAddr::V6(ip) => !(ip.is_unspecified() || ip.is_loopback() || ip.is_multicast()),
        })
        .collect()
}

/// Answers from a list (for tests): only the listed addresses answer.
#[derive(Debug, Default)]
pub struct FakeReach {
    answering: BTreeSet<String>,
}

impl FakeReach {
    pub fn new<S: Into<String>>(answering: impl IntoIterator<Item = S>) -> Self {
        Self {
            answering: answering.into_iter().map(Into::into).collect(),
        }
    }
}

impl Reach for FakeReach {
    fn answers(&self, address: &str) -> bool {
        self.answering.contains(address.trim())
    }
}

/// Whether one controller answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReachCheck {
    pub address: String,
    pub answering: bool,
    /// Whether the address is on one of this computer's networks; null when that isn't known (a
    /// host name, or no network found).
    pub on_local_network: Option<bool>,
}

/// This computer's IPv4 networks (address and netmask), loopback and link-local left out.
pub fn local_networks() -> Vec<(Ipv4Addr, Ipv4Addr)> {
    local_ipv4_interfaces()
}

/// Whether `address` is on one of `networks`; None for a host name or when there are none.
pub fn on_local_network(address: &str, networks: &[(Ipv4Addr, Ipv4Addr)]) -> Option<bool> {
    let ip = address.trim().parse::<Ipv4Addr>().ok()?;
    if networks.is_empty() {
        return None;
    }
    Some(networks.iter().any(|(own, mask)| {
        let mask = u32::from(*mask);
        u32::from(ip) & mask == u32::from(*own) & mask
    }))
}

/// Looks at each address at once (each distinct address once, at most 64), in the order given.
pub fn check_reach(
    reach: &dyn Reach,
    addresses: &[String],
    networks: &[(Ipv4Addr, Ipv4Addr)],
) -> Vec<ReachCheck> {
    let mut seen = BTreeSet::new();
    let unique: Vec<&str> = addresses
        .iter()
        .map(|a| a.trim())
        .filter(|a| !a.is_empty() && seen.insert(*a))
        .take(MAX_ADDRESSES)
        .collect();
    std::thread::scope(|scope| {
        let looks: Vec<_> = unique
            .iter()
            .map(|address| scope.spawn(move || reach.answers(address)))
            .collect();
        unique
            .iter()
            .zip(looks)
            .map(|(address, look)| ReachCheck {
                address: (*address).to_string(),
                answering: look.join().unwrap_or(false),
                on_local_network: on_local_network(address, networks),
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: (Ipv4Addr, Ipv4Addr) = (Ipv4Addr::new(10, 28, 128, 20), Ipv4Addr::new(255, 255, 255, 0));

    #[test]
    fn says_which_controllers_answer_and_which_are_on_this_network() {
        let reach = FakeReach::new(["10.28.128.177"]);
        let addresses = [
            "10.28.128.177",
            "10.28.128.175",
            "192.168.1.50",
            "porch.local",
            " 10.28.128.177 ",
            "",
        ]
        .map(String::from);
        let checks = check_reach(&reach, &addresses, &[HOME]);
        let got: Vec<_> = checks
            .iter()
            .map(|c| (c.address.as_str(), c.answering, c.on_local_network))
            .collect();
        assert_eq!(
            got,
            [
                ("10.28.128.177", true, Some(true)),
                ("10.28.128.175", false, Some(true)),
                ("192.168.1.50", false, Some(false)),
                ("porch.local", false, None),
            ]
        );
        // With no network found, nothing can be said about where an address is.
        assert_eq!(on_local_network("192.168.1.50", &[]), None);
    }

    #[test]
    fn addresses_no_request_should_go_to_are_never_tried() {
        let reach = TcpReach::new(Duration::from_millis(200));
        for address in ["255.255.255.255", "224.0.0.1", "127.0.0.1", "0.0.0.0", "", "  "] {
            assert!(socket_addrs(address).is_empty(), "{address}");
            assert!(!reach.answers(address), "{address}");
        }
        let to = |s: &str| s.parse::<SocketAddr>().unwrap();
        assert_eq!(socket_addrs("10.28.128.177"), [to("10.28.128.177:80")]);
        assert_eq!(socket_addrs(" 10.28.128.177:8080 "), [to("10.28.128.177:8080")]);
    }
}
