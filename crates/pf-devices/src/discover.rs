//! Finding controllers on the local network.
//!
//! Four methods run together, because no single one finds everything (and a firewall may
//! block UDP replies): FPP's MultiSync discover ping, mDNS, an HTTP sweep of each local
//! subnet that recognizes controller web pages, and asking every FPP which controllers it
//! syncs with or sends data to. Every candidate is then identified over HTTP.

use crate::device::{Device, DeviceKind, FoundBy};
use crate::error::DeviceError;
use crate::fingerprint::classify_home_page;
use crate::fpp;
use crate::fpp_ping::{MULTISYNC_GROUP, MULTISYNC_PORT, discover_packet, kind_for_type, parse_ping};
use crate::http::Http;
use crate::identify::identify;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Parallel HTTP requests during the sweep and while identifying candidates.
const SWEEP_THREADS: usize = 48;
/// Most candidates identified per discovery pass (typed-in hosts are always kept).
const MAX_CANDIDATES: usize = 512;
/// Most peers taken from each FPP's lists (applied in [`usable_peers`]).
const MAX_PEERS_PER_FPP: usize = 64;

/// What discovery should do.
#[derive(Debug, Clone)]
pub struct DiscoverOptions {
    pub ping: bool,
    pub mdns: bool,
    pub sweep: bool,
    /// Addresses the user typed in; always checked.
    pub extra_hosts: Vec<String>,
    /// How long to listen for ping and mDNS answers.
    pub listen_for: Duration,
}

impl Default for DiscoverOptions {
    fn default() -> Self {
        Self {
            ping: true,
            mdns: true,
            sweep: true,
            extra_hosts: Vec::new(),
            listen_for: Duration::from_secs(3),
        }
    }
}

#[derive(Default)]
struct Candidates(BTreeMap<String, (Option<DeviceKind>, BTreeSet<FoundBy>)>);

impl Candidates {
    /// Adds a candidate; new addresses from sources other than `Manual` are dropped once
    /// `MAX_CANDIDATES` are held.
    fn add(&mut self, address: String, kind: Option<DeviceKind>, by: FoundBy) {
        if by != FoundBy::Manual && self.0.len() >= MAX_CANDIDATES && !self.0.contains_key(&address) {
            return;
        }
        let entry = self.0.entry(address).or_default();
        entry.0 = entry.0.or(kind);
        entry.1.insert(by);
    }
}

/// Typed-in hosts: trusted (hostnames allowed), trimmed, blanks skipped.
fn add_manual_hosts(candidates: &mut Candidates, hosts: &[String]) {
    for host in hosts.iter().map(|h| h.trim()).filter(|h| !h.is_empty()) {
        candidates.add(host.to_string(), None, FoundBy::Manual);
    }
}

/// Addresses heard on the network, de-duplicated and bounded.
struct Found<K>(BTreeMap<Ipv4Addr, K>);

impl<K> Default for Found<K> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<K> Found<K> {
    fn add(&mut self, address: Ipv4Addr, kind: K) {
        if self.0.len() < MAX_CANDIDATES || self.0.contains_key(&address) {
            self.0.entry(address).or_insert(kind);
        }
    }
}

/// Whether it makes sense to send a request to `ip`: not unspecified, loopback, link-local,
/// multicast, broadcast, or in 0.0.0.0/8 or 240.0.0.0/4.
pub fn probeable(ip: Ipv4Addr) -> bool {
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.octets()[0] == 0
        || ip.octets()[0] >= 240)
}

/// An address from an untrusted source (network reply or a device's lists): must be a plain,
/// probeable IPv4 address (no hostnames, ports, or garbage).
fn parse_untrusted(text: &str) -> Option<Ipv4Addr> {
    text.parse::<Ipv4Addr>().ok().filter(|ip| probeable(*ip))
}

/// Only private (RFC 1918) networks are swept; others are reached by typing an address.
fn sweepable_interface(ip: Ipv4Addr) -> bool {
    ip.is_private()
}

/// Peers worth probing from one FPP's lists: plain IPv4 addresses only, at most
/// `MAX_PEERS_PER_FPP`.
fn usable_peers(peers: Vec<(String, String)>) -> Vec<(String, String)> {
    peers
        .into_iter()
        .filter(|(address, _)| parse_untrusted(address).is_some())
        .take(MAX_PEERS_PER_FPP)
        .collect()
}

/// The broadcast address for a ping, unless the subnet is too small for one (/31, /32).
fn broadcast_for(ip: Ipv4Addr, netmask: Ipv4Addr) -> Option<Ipv4Addr> {
    (u32::from(netmask).leading_ones() < 31).then(|| Ipv4Addr::from(u32::from(ip) | !u32::from(netmask)))
}

/// A ping reply's address is its packet source (the address in the payload is ignored).
fn ping_source(from: Ipv4Addr) -> Option<Ipv4Addr> {
    probeable(from).then_some(from)
}

fn sort_devices(devices: &mut [Device]) {
    devices.sort_by_cached_key(|d| {
        let ip = d.address.parse::<Ipv4Addr>().ok();
        (d.kind, ip.is_none(), ip, d.address.clone())
    });
}

/// Every other host address on the subnet of `ip`/`netmask` (at most a /24 around `ip`).
/// Never more than 254 hosts, never `ip` itself, never panics on odd masks.
pub fn sweep_hosts(ip: Ipv4Addr, netmask: Ipv4Addr) -> Vec<Ipv4Addr> {
    let prefix = u32::from(netmask).leading_ones().max(24);
    let mask = if prefix >= 32 {
        u32::MAX
    } else {
        !(u32::MAX >> prefix)
    };
    let network = u32::from(ip) & mask;
    let broadcast = network | !mask;
    (network.saturating_add(1)..broadcast)
        .map(Ipv4Addr::from)
        .filter(|host| *host != ip)
        .collect()
}

/// A controller another device told us about that didn't answer at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SilentPeer {
    pub address: String,
    /// How the listing device describes it (e.g. "Falcon_F16V5_B9F5").
    pub description: String,
    /// Name of the device that listed it.
    pub listed_by: String,
}

/// Everything discovery found.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Discovery {
    pub devices: Vec<Device>,
    /// Controllers listed by an FPP that didn't respond (powered off, unplugged, or moved).
    pub silent: Vec<SilentPeer>,
}

/// Finds controllers. `sweep_http` is used for the subnet sweep (give it a short connect
/// timeout); `http` for everything else. Never fails: unreachable or unrecognized hosts are
/// left out, except FPP-listed controllers that didn't answer at all, which are reported as
/// silent. Devices are sorted by kind, then address.
pub fn discover(http: &dyn Http, sweep_http: &dyn Http, options: &DiscoverOptions) -> Discovery {
    let interfaces = local_ipv4_interfaces();
    let mut candidates = Candidates::default();
    std::thread::scope(|scope| {
        let ping = options
            .ping
            .then(|| scope.spawn(|| ping_discovery(&interfaces, options.listen_for)));
        let mdns = options
            .mdns
            .then(|| scope.spawn(|| mdns_discovery(options.listen_for)));
        let sweep = options
            .sweep
            .then(|| scope.spawn(|| web_sweep(sweep_http, &interfaces)));
        for (address, kind) in ping.map(|h| h.join().unwrap_or_default()).unwrap_or_default() {
            candidates.add(address.to_string(), kind, FoundBy::Ping);
        }
        for (address, kind) in mdns.map(|h| h.join().unwrap_or_default()).unwrap_or_default() {
            candidates.add(address.to_string(), Some(kind), FoundBy::Mdns);
        }
        for (address, kind) in sweep.map(|h| h.join().unwrap_or_default()).unwrap_or_default() {
            candidates.add(address, Some(kind), FoundBy::WebSweep);
        }
    });
    add_manual_hosts(&mut candidates, &options.extra_hosts);

    let mut devices: BTreeMap<String, Device> = BTreeMap::new();
    identify_all(http, &candidates, &mut devices);
    // Controllers an FPP knows about (MultiSync peers and output destinations).
    let mut peers = Candidates::default();
    let mut listed: BTreeMap<String, (String, String)> = BTreeMap::new();
    for device in devices.values().filter(|d| d.kind == DeviceKind::Fpp) {
        for (address, description) in usable_peers(fpp::peers(http, &device.address)) {
            if !devices.contains_key(&address) {
                peers.add(address.clone(), None, FoundBy::FppPeer);
                listed
                    .entry(address)
                    .or_insert((description, device.name.clone()));
            }
        }
    }
    let answered = identify_all(http, &peers, &mut devices);
    let silent = listed
        .into_iter()
        .filter(|(address, _)| !devices.contains_key(address) && !answered.contains(address))
        .map(|(address, (description, listed_by))| SilentPeer {
            address,
            description,
            listed_by,
        })
        .collect();

    let mut found: Vec<Device> = devices.into_values().collect();
    sort_devices(&mut found);
    Discovery {
        devices: found,
        silent,
    }
}

/// Identifies every candidate with a bounded pool of `SWEEP_THREADS` workers. Returns the
/// addresses that answered HTTP but aren't a recognized controller (not "silent").
fn identify_all(
    http: &dyn Http,
    candidates: &Candidates,
    devices: &mut BTreeMap<String, Device>,
) -> BTreeSet<String> {
    let work: Vec<_> = candidates.0.iter().collect();
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..SWEEP_THREADS.min(work.len()) {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some((address, (kind, by))) = work.get(index) else {
                        break;
                    };
                    let outcome = identify(http, address, *kind);
                    if let Ok(mut results) = results.lock() {
                        results.push(((*address).clone(), (*by).clone(), outcome));
                    }
                }
            });
        }
    });
    let mut answered = BTreeSet::new();
    for (address, by, outcome) in results.into_inner().unwrap_or_default() {
        match outcome {
            Ok(mut device) => {
                device.found_by = by.into_iter().collect();
                devices.insert(address, device);
            }
            Err(DeviceError::Unreachable { .. }) => {}
            Err(_) => {
                answered.insert(address);
            }
        }
    }
    answered
}

/// Local IPv4 interfaces (not loopback) with their netmasks.
fn local_ipv4_interfaces() -> Vec<(Ipv4Addr, Ipv4Addr)> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|iface| match iface.addr {
            if_addrs::IfAddr::V4(v4) if !v4.ip.is_loopback() && !v4.ip.is_link_local() => {
                Some((v4.ip, v4.netmask))
            }
            _ => None,
        })
        .collect()
}

fn web_sweep(http: &dyn Http, interfaces: &[(Ipv4Addr, Ipv4Addr)]) -> Vec<(String, DeviceKind)> {
    let own: BTreeSet<Ipv4Addr> = interfaces.iter().map(|(ip, _)| *ip).collect();
    let hosts: Vec<Ipv4Addr> = interfaces
        .iter()
        .filter(|(ip, _)| sweepable_interface(*ip))
        .flat_map(|(ip, mask)| sweep_hosts(*ip, *mask))
        .filter(|host| !own.contains(host))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let chunk = hosts.len().div_ceil(SWEEP_THREADS).max(1);
    std::thread::scope(|scope| {
        let handles: Vec<_> = hosts
            .chunks(chunk)
            .map(|batch| {
                scope.spawn(move || {
                    batch
                        .iter()
                        .filter_map(|host| {
                            let host = host.to_string();
                            let page = http.get(&host, "/").ok()?;
                            classify_home_page(&page).map(|kind| (host, kind))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|h| h.join().ok())
            .flatten()
            .collect()
    })
}

fn ping_discovery(
    interfaces: &[(Ipv4Addr, Ipv4Addr)],
    listen_for: Duration,
) -> Vec<(Ipv4Addr, Option<DeviceKind>)> {
    let Ok(socket) = multisync_socket(interfaces) else {
        return Vec::new();
    };
    let packet = discover_packet();
    for (ip, mask) in interfaces {
        if let Some(broadcast) = broadcast_for(*ip, *mask) {
            let _ = socket.send_to(&packet, SocketAddrV4::new(broadcast, MULTISYNC_PORT));
        }
    }
    let _ = socket.send_to(&packet, SocketAddrV4::new(MULTISYNC_GROUP, MULTISYNC_PORT));
    let mut found: Found<Option<DeviceKind>> = Found::default();
    let deadline = Instant::now() + listen_for;
    let mut buf = [0u8; 1500];
    while Instant::now() < deadline {
        let (len, from) = match socket.recv_from(&mut buf) {
            Ok(received) => received,
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => continue,
            Err(_) => break,
        };
        let SocketAddr::V4(from) = from else {
            continue;
        };
        // The packet's source is trusted over the address the payload claims.
        let Some(source) = ping_source(*from.ip()) else {
            continue;
        };
        if let Some(ping) = parse_ping(&buf[..len]).filter(|p| !p.discover) {
            found.add(source, kind_for_type(ping.type_id));
        }
    }
    found.0.into_iter().collect()
}

fn multisync_socket(interfaces: &[(Ipv4Addr, Ipv4Addr)]) -> std::io::Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.set_broadcast(true)?;
    socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, MULTISYNC_PORT).into())?;
    for (ip, _) in interfaces {
        let _ = socket.join_multicast_v4(&MULTISYNC_GROUP, ip);
    }
    socket.set_read_timeout(Some(Duration::from_millis(200)))?;
    Ok(socket.into())
}

fn mdns_discovery(listen_for: Duration) -> Vec<(Ipv4Addr, DeviceKind)> {
    let Ok(daemon) = mdns_sd::ServiceDaemon::new() else {
        return Vec::new();
    };
    let services = [
        ("_wled._tcp.local.", DeviceKind::Wled),
        ("_fppd._udp.local.", DeviceKind::Fpp),
    ];
    let receivers: Vec<_> = services
        .iter()
        .filter_map(|(service, kind)| daemon.browse(service).ok().map(|rx| (rx, *kind)))
        .collect();
    let mut found: Found<DeviceKind> = Found::default();
    let deadline = Instant::now() + listen_for;
    while Instant::now() < deadline {
        for (rx, kind) in &receivers {
            while let Ok(event) = rx.try_recv() {
                if let mdns_sd::ServiceEvent::ServiceResolved(info) = event {
                    for address in info.get_addresses_v4() {
                        if probeable(address) {
                            found.add(address, *kind);
                        }
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = daemon.shutdown();
    found.0.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_covers_the_local_slash_24_without_ourselves() {
        let hosts = sweep_hosts(Ipv4Addr::new(10, 0, 0, 65), Ipv4Addr::new(255, 255, 255, 0));
        assert_eq!(hosts.len(), 253);
        assert_eq!(hosts.first(), Some(&Ipv4Addr::new(10, 0, 0, 1)));
        assert_eq!(hosts.last(), Some(&Ipv4Addr::new(10, 0, 0, 254)));
        assert!(!hosts.contains(&Ipv4Addr::new(10, 0, 0, 65)));
    }

    #[test]
    fn wide_subnets_are_limited_to_a_slash_24_and_narrow_ones_respected() {
        assert_eq!(
            sweep_hosts(Ipv4Addr::new(10, 0, 5, 9), Ipv4Addr::new(255, 255, 0, 0)).len(),
            253
        );
        let small = sweep_hosts(Ipv4Addr::new(10, 0, 0, 9), Ipv4Addr::new(255, 255, 255, 248));
        assert_eq!(
            small,
            (10..15).map(|h| Ipv4Addr::new(10, 0, 0, h)).collect::<Vec<_>>()
        );
    }

    fn ip(a: u8, b: u8, c: u8, d: u8) -> Ipv4Addr {
        Ipv4Addr::new(a, b, c, d)
    }

    #[test]
    fn probeable_rejects_addresses_that_make_no_sense() {
        for bad in [
            ip(0, 0, 0, 0),
            ip(0, 1, 2, 3),
            ip(127, 0, 0, 1),
            ip(169, 254, 1, 1),
            ip(224, 0, 0, 1),
            ip(239, 255, 0, 1),
            ip(255, 255, 255, 255),
            ip(240, 0, 0, 1),
            ip(250, 1, 1, 1),
        ] {
            assert!(!probeable(bad), "{bad}");
        }
        for good in [
            ip(192, 0, 2, 10),
            ip(10, 0, 0, 5),
            ip(172, 16, 3, 4),
            ip(8, 8, 8, 8),
        ] {
            assert!(probeable(good), "{good}");
        }
    }

    #[test]
    fn untrusted_strings_must_be_plain_probeable_addresses() {
        assert_eq!(parse_untrusted("192.0.2.5"), Some(ip(192, 0, 2, 5)));
        for bad in [
            "fpp.local",
            "10.0.0.5:80",
            "239.255.0.1",
            "",
            "garbage",
            "127.0.0.1",
            " 10.0.0.5",
        ] {
            assert_eq!(parse_untrusted(bad), None, "{bad}");
        }
    }

    #[test]
    fn only_rfc1918_interfaces_are_swept() {
        assert!(sweepable_interface(ip(10, 1, 2, 3)));
        assert!(sweepable_interface(ip(172, 16, 0, 1)));
        assert!(sweepable_interface(ip(172, 31, 255, 1)));
        assert!(sweepable_interface(ip(192, 168, 1, 1)));
        assert!(!sweepable_interface(ip(172, 32, 0, 1)));
        assert!(!sweepable_interface(ip(172, 15, 0, 1)));
        assert!(!sweepable_interface(ip(100, 64, 0, 1)));
        assert!(!sweepable_interface(ip(8, 8, 8, 8)));
        assert!(!sweepable_interface(ip(192, 0, 2, 1)));
    }

    #[test]
    fn sweep_hosts_survive_odd_masks_and_addresses() {
        let m = |a, b, c, d| ip(a, b, c, d);
        let cases = [
            (ip(10, 0, 0, 5), m(255, 255, 255, 254)),
            (ip(10, 0, 0, 5), m(255, 255, 255, 255)),
            (ip(10, 0, 0, 5), m(0, 0, 0, 0)),
            (ip(10, 0, 0, 5), m(255, 0, 255, 0)),
            (ip(10, 0, 0, 5), m(255, 255, 0, 255)),
            (ip(10, 0, 0, 0), m(255, 255, 255, 0)),
            (ip(10, 0, 0, 255), m(255, 255, 255, 0)),
            (ip(255, 255, 255, 255), m(0, 0, 0, 0)),
            (ip(0, 0, 0, 0), m(255, 255, 255, 255)),
        ];
        for (host, mask) in cases {
            let hosts = sweep_hosts(host, mask);
            assert!(hosts.len() <= 254, "{host} {mask}");
            assert!(!hosts.contains(&host), "{host} {mask}");
        }
        assert!(sweep_hosts(ip(10, 0, 0, 5), m(255, 255, 255, 255)).is_empty());
        assert!(sweep_hosts(ip(10, 0, 0, 4), m(255, 255, 255, 254)).is_empty());
    }

    #[test]
    fn broadcast_is_only_sent_on_real_subnets() {
        assert_eq!(
            broadcast_for(ip(10, 0, 0, 5), ip(255, 255, 255, 0)),
            Some(ip(10, 0, 0, 255))
        );
        assert_eq!(
            broadcast_for(ip(10, 0, 0, 5), ip(255, 255, 255, 252)),
            Some(ip(10, 0, 0, 7))
        );
        assert_eq!(broadcast_for(ip(10, 0, 0, 5), ip(255, 255, 255, 254)), None);
        assert_eq!(broadcast_for(ip(10, 0, 0, 5), ip(255, 255, 255, 255)), None);
    }

    #[test]
    fn ping_replies_use_the_packet_source_and_drop_bad_senders() {
        assert_eq!(ping_source(ip(192, 0, 2, 7)), Some(ip(192, 0, 2, 7)));
        assert_eq!(ping_source(ip(224, 0, 0, 5)), None);
        assert_eq!(ping_source(ip(127, 0, 0, 1)), None);
    }

    #[test]
    fn found_addresses_are_capped_and_deduplicated() {
        let mut found: Found<Option<DeviceKind>> = Found::default();
        for n in 0..(MAX_CANDIDATES + 100) {
            let n = u32::try_from(n).unwrap();
            found.add(Ipv4Addr::from(0x0a00_0000 + 256 + n), None);
        }
        found.add(ip(10, 0, 0, 1), None);
        found.add(ip(10, 0, 0, 1), None);
        assert_eq!(found.0.len(), MAX_CANDIDATES);
    }

    #[test]
    fn candidates_cap_other_sources_but_keep_manual_hosts() {
        let mut c = Candidates::default();
        for n in 0..(MAX_CANDIDATES + 50) {
            c.add(format!("10.1.{}.{}", n / 256, n % 256), None, FoundBy::WebSweep);
        }
        c.add("fpp.local".to_string(), None, FoundBy::Manual);
        assert_eq!(c.0.len(), MAX_CANDIDATES + 1);
        assert!(c.0.contains_key("fpp.local"));
    }

    #[test]
    fn manual_hosts_are_trimmed_and_blanks_skipped() {
        let mut c = Candidates::default();
        add_manual_hosts(
            &mut c,
            &["  ".to_string(), " fpp.local ".to_string(), String::new()],
        );
        assert_eq!(c.0.keys().collect::<Vec<_>>(), vec!["fpp.local"]);
    }

    #[test]
    fn peers_are_filtered_and_capped_per_fpp() {
        let mut listed: Vec<(String, String)> = vec![
            ("fpp.local".into(), String::new()),
            ("10.0.0.5:80".into(), String::new()),
            ("239.255.0.1".into(), String::new()),
        ];
        for n in 0..(MAX_PEERS_PER_FPP + 20) {
            listed.push((format!("10.2.0.{}", n + 1), String::new()));
        }
        let kept = usable_peers(listed);
        assert_eq!(kept.len(), MAX_PEERS_PER_FPP);
        assert!(kept.iter().all(|(a, _)| a.starts_with("10.2.0.")));
    }

    #[test]
    fn devices_sort_by_kind_then_numeric_address() {
        let mk = |kind, address: &str| Device {
            address: address.to_string(),
            kind,
            name: String::new(),
            model: String::new(),
            firmware: String::new(),
            mode: None,
            found_by: Vec::new(),
        };
        let mut devices = vec![
            mk(DeviceKind::Wled, "10.0.0.9"),
            mk(DeviceKind::Fpp, "10.0.0.100"),
            mk(DeviceKind::Fpp, "10.0.0.20"),
            mk(DeviceKind::Fpp, "zeta.local"),
            mk(DeviceKind::Fpp, "alpha.local"),
        ];
        sort_devices(&mut devices);
        let order: Vec<_> = devices.iter().map(|d| d.address.as_str()).collect();
        assert_eq!(
            order,
            vec!["10.0.0.20", "10.0.0.100", "alpha.local", "zeta.local", "10.0.0.9"]
        );
    }
}
