//! Finding the TV on the local network: the original's "IP connection screen".
//!
//! Two sources, cheapest first: adb's own mDNS discovery, then the host's
//! neighbour table (the script used `arp -a`) probed with `adb connect`.

use crate::adb::Adb;
use std::net::{SocketAddr, TcpStream};
use std::process::Command;
use std::time::Duration;

pub const ADB_PORT: u16 = 5555;

/// Upper bound on adb connect attempts per scan.
pub const MAX_PROBES: usize = 64;

/// How long a single TCP reachability check may take.
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(400);

/// Cheap reachability check: is anything listening on that port? adb connect
/// itself can block for minutes on a firewalled host, so it is only tried here.
pub fn port_open(host: &str, port: u16) -> bool {
    let Ok(addr) = format!("{host}:{port}").parse::<SocketAddr>() else { return false };
    TcpStream::connect_timeout(&addr, PROBE_TIMEOUT).is_ok()
}

fn split_host(entry: &str) -> (String, u16) {
    match entry.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().unwrap_or(ADB_PORT)),
        None => (entry.to_string(), ADB_PORT),
    }
}

/// "192.168.1.50" or "192.168.1.50:5555" -> "192.168.1.50:5555".
pub fn normalize_target(host: &str, port: u16) -> String {
    let host = host.trim();
    if host.contains(':') { host.to_string() } else { format!("{host}:{port}") }
}

/// Candidate addresses from `arp -an` (macOS/BSD), `ip neigh` (Linux) or
/// `arp -a` (Windows). Anything that is not a usable unicast IPv4 is dropped.
pub fn parse_hosts(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for token in text.split_whitespace() {
        let t = token.trim_matches(|c: char| !c.is_ascii_digit() && c != '.');
        if !is_usable_ipv4(t) {
            continue;
        }
        if !out.iter().any(|h| h == t) {
            out.push(t.to_string());
        }
    }
    out.sort_by_key(|ip| {
        let mut parts = ip.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
        (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0))
    });
    out
}

fn is_usable_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    let mut nums = [0u32; 4];
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty() || p.len() > 3 || !p.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        nums[i] = p.parse().unwrap_or(999);
        if nums[i] > 255 {
            return false;
        }
    }
    let first = nums[0];
    if first == 0 || first >= 224 || first == 127 {
        return false;      // unspecified, multicast/reserved, loopback
    }
    if nums == [255, 255, 255, 255] || (first == 169 && nums[1] == 254) {
        return false;      // broadcast, link-local self-assigned
    }
    true
}

/// Hosts the machine has recently talked to on the local network.
pub fn neighbours() -> Vec<String> {
    let attempts: &[&[&str]] = if cfg!(windows) {
        &[&["arp", "-a"]]
    } else {
        &[&["arp", "-an"], &["ip", "neigh"]]
    };
    for args in attempts {
        if let Ok(out) = Command::new(args[0]).args(&args[1..]).output() {
            let text = String::from_utf8_lossy(&out.stdout);
            let hosts = parse_hosts(&text);
            if !hosts.is_empty() {
                return hosts;
            }
        }
    }
    Vec::new()
}

/// `adb mdns services` rows that are connect endpoints, as "host:port".
pub fn parse_mdns(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        if !(line.contains("_adb-tls-connect._tcp") || line.contains("_adb._tcp")) {
            continue;
        }
        let Some(last) = line.split_whitespace().last() else { continue };
        let Some((host, port)) = last.rsplit_once(':') else { continue };
        if host.is_empty() || port.parse::<u16>().is_err() {
            continue;
        }
        let entry = format!("{host}:{port}");
        if !out.contains(&entry) {
            out.push(entry);
        }
    }
    out
}

/// Every address worth trying, mDNS first, then the neighbour table.
pub fn candidates(adb: &Adb) -> Vec<String> {
    let mut out = parse_mdns(&adb.try_out(&["mdns", "services"]));
    for host in neighbours() {
        let entry = format!("{host}:{ADB_PORT}");
        if !out.contains(&entry) {
            out.push(entry);
        }
    }
    out
}

fn connected(out: &str) -> bool {
    let lower = out.to_ascii_lowercase();
    lower.contains("connected to") && !lower.contains("cannot") && !lower.contains("failed")
}

/// Tries each candidate; returns the ones that answered as "connected to".
/// Capped so a big /24 does not turn into hundreds of adb connections.
pub fn probe(adb: &Adb, hosts: &[String]) -> Vec<String> {
    let mut hits = Vec::new();
    for host in hosts.iter().take(MAX_PROBES) {
        let (ip, port) = split_host(host);
        if !port_open(&ip, port) {
            continue;
        }
        if let Ok(out) = adb.connect(host) {
            if connected(&out) {
                hits.push(host.clone());
            }
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arp_output_is_parsed_in_all_three_formats() {
        let macos = "? (192.168.1.24) at 3c:5a:b4:1:2:3 on en0 ifscope [ethernet]\n? (192.168.1.1) at 4:5:6:7:8:9 on en0";
        assert_eq!(parse_hosts(macos), vec!["192.168.1.1", "192.168.1.24"]);
        let linux = "192.168.1.24 dev wlan0 lladdr 3c:5a:b4:1:2:3 REACHABLE";
        assert_eq!(parse_hosts(linux), vec!["192.168.1.24"]);
        let windows = "  192.168.1.31          3c-5a-b4-01-02-03     dynamic";
        assert_eq!(parse_hosts(windows), vec!["192.168.1.31"]);
    }

    #[test]
    fn junk_addresses_are_rejected() {
        let text = "255.255.255.255 224.0.0.251 127.0.0.1 169.254.3.4 0.0.0.0 1.2.3 1.2.3.999 300.1.1.1";
        assert!(parse_hosts(text).is_empty());
    }

    #[test]
    fn mdns_rows_become_host_port_pairs() {
        let out = "List of discovered mdns services\nadb-1234\t_adb-tls-connect._tcp\t192.168.1.42:39115\nadb-5678\t_adb._tcp\t192.168.1.42:5555\npair\t_adb-tls-pairing._tcp\t192.168.1.42:37061";
        assert_eq!(parse_mdns(out), vec!["192.168.1.42:39115", "192.168.1.42:5555"]);
    }

    #[test]
    fn only_hosts_with_an_open_port_are_probed() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        assert!(port_open("127.0.0.1", port));
        let closed = std::net::TcpListener::bind("127.0.0.1:0").expect("bind").local_addr().expect("addr").port();
        assert!(!port_open("127.0.0.1", closed) || closed == port);
        assert_eq!(split_host("192.168.1.5"), ("192.168.1.5".to_string(), ADB_PORT));
        assert_eq!(split_host("192.168.1.5:39115"), ("192.168.1.5".to_string(), 39115));
    }

    #[test]
    fn connect_output_decides_whether_a_host_answered() {
        assert!(connected("connected to 192.168.1.42:5555"));
        assert!(!connected("cannot connect to 192.168.1.42:5555: Connection refused"));
        assert!(!connected("failed to connect to '1.2.3.4:5555'"));
    }
}
