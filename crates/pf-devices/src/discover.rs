//! Finding controllers on the local network.
//!
//! Four methods run together, because no single one finds everything (and a firewall may
//! block UDP replies): FPP's MultiSync discover ping, mDNS, an HTTP sweep of each local
//! subnet that recognizes controller web pages, and asking every FPP which controllers it
//! syncs with or sends data to. Every candidate is then identified over HTTP.

use crate::device::{Device, DeviceKind, FoundBy};
use crate::fingerprint::classify_home_page;
use crate::fpp;
use crate::fpp_ping::{MULTISYNC_GROUP, MULTISYNC_PORT, discover_packet, kind_for_type, parse_ping};
use crate::http::Http;
use crate::identify::identify;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

/// Parallel HTTP requests during the sweep.
const SWEEP_THREADS: usize = 48;

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
    fn add(&mut self, address: String, kind: Option<DeviceKind>, by: FoundBy) {
        let entry = self.0.entry(address).or_default();
        entry.0 = entry.0.or(kind);
        entry.1.insert(by);
    }
}

/// Every other host address on the subnet of `ip`/`netmask` (at most a /24 around `ip`).
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

/// A controller another device told us about that didn't answer.
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
/// left out, except FPP-listed controllers, which are reported as silent.
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
            candidates.add(address, kind, FoundBy::Ping);
        }
        for (address, kind) in mdns.map(|h| h.join().unwrap_or_default()).unwrap_or_default() {
            candidates.add(address, Some(kind), FoundBy::Mdns);
        }
        for (address, kind) in sweep.map(|h| h.join().unwrap_or_default()).unwrap_or_default() {
            candidates.add(address, Some(kind), FoundBy::WebSweep);
        }
    });
    for host in &options.extra_hosts {
        candidates.add(host.trim().to_string(), None, FoundBy::Manual);
    }

    let mut devices: BTreeMap<String, Device> = BTreeMap::new();
    identify_all(http, &candidates, &mut devices);
    // Controllers an FPP knows about (MultiSync peers and output destinations).
    let mut peers = Candidates::default();
    let mut listed: BTreeMap<String, (String, String)> = BTreeMap::new();
    for device in devices.values().filter(|d| d.kind == DeviceKind::Fpp) {
        for (address, description) in fpp::peers(http, &device.address) {
            if !devices.contains_key(&address) {
                peers.add(address.clone(), None, FoundBy::FppPeer);
                listed
                    .entry(address)
                    .or_insert((description, device.name.clone()));
            }
        }
    }
    identify_all(http, &peers, &mut devices);
    let silent = listed
        .into_iter()
        .filter(|(address, _)| !devices.contains_key(address))
        .map(|(address, (description, listed_by))| SilentPeer {
            address,
            description,
            listed_by,
        })
        .collect();

    let mut found: Vec<Device> = devices.into_values().collect();
    found.sort_by(|a, b| (a.kind, &a.address).cmp(&(b.kind, &b.address)));
    Discovery {
        devices: found,
        silent,
    }
}

fn identify_all(http: &dyn Http, candidates: &Candidates, devices: &mut BTreeMap<String, Device>) {
    let results: Vec<(String, BTreeSet<FoundBy>, Option<Device>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = candidates
            .0
            .iter()
            .map(|(address, (kind, by))| {
                scope.spawn(move || (address.clone(), by.clone(), identify(http, address, *kind).ok()))
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    for (address, by, device) in results {
        if let Some(mut device) = device {
            device.found_by = by.into_iter().collect();
            devices.insert(address, device);
        }
    }
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
    let hosts: Vec<Ipv4Addr> = interfaces
        .iter()
        .flat_map(|(ip, mask)| sweep_hosts(*ip, *mask))
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
) -> Vec<(String, Option<DeviceKind>)> {
    let Ok(socket) = multisync_socket(interfaces) else {
        return Vec::new();
    };
    let packet = discover_packet();
    for (ip, mask) in interfaces {
        let broadcast = Ipv4Addr::from(u32::from(*ip) | !u32::from(*mask));
        let _ = socket.send_to(&packet, SocketAddrV4::new(broadcast, MULTISYNC_PORT));
    }
    let _ = socket.send_to(&packet, SocketAddrV4::new(MULTISYNC_GROUP, MULTISYNC_PORT));
    let mut found = Vec::new();
    let deadline = Instant::now() + listen_for;
    let mut buf = [0u8; 1500];
    while Instant::now() < deadline {
        let Ok((len, SocketAddr::V4(from))) = socket.recv_from(&mut buf) else {
            continue;
        };
        if let Some(ping) = parse_ping(&buf[..len]).filter(|p| !p.discover) {
            let address = if ping.address.is_unspecified() {
                *from.ip()
            } else {
                ping.address
            };
            found.push((address.to_string(), kind_for_type(ping.type_id)));
        }
    }
    found
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

fn mdns_discovery(listen_for: Duration) -> Vec<(String, DeviceKind)> {
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
    let mut found = Vec::new();
    let deadline = Instant::now() + listen_for;
    while Instant::now() < deadline {
        for (rx, kind) in &receivers {
            while let Ok(event) = rx.try_recv() {
                if let mdns_sd::ServiceEvent::ServiceResolved(info) = event {
                    for address in info.get_addresses_v4() {
                        found.push((address.to_string(), *kind));
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = daemon.shutdown();
    found
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
}
