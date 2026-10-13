//! Preflight and environment diagnosis: is adb present and recent, is a device
//! reachable, is there internet for the catalog, and the region hint the
//! original printed (its "Countries List.txt" step existed because a VPN IP in
//! the wrong country makes Play Store installs fail).

use crate::adb::Adb;
use anyhow::Result;
use std::fmt::Write as _;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::process::Command;
use std::time::Duration;

/// Below this, wireless debugging and modern pm commands misbehave.
pub const MIN_ADB: (u32, u32, u32) = (33, 0, 0);

/// "Android Debug Bridge version 1.0.41" / "Version 35.0.2-11815563" -> the
/// most specific version in the output.
pub fn parse_adb_version(text: &str) -> Option<(u32, u32, u32)> {
    let mut best = None;
    for word in text.split_whitespace() {
        let w = word.trim_start_matches('v');
        let core = w.split(['-', '+']).next().unwrap_or(w);
        let parts: Vec<&str> = core.split('.').collect();
        if parts.len() < 2 || !parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty()) {
            continue;
        }
        let n = |i: usize| parts.get(i).and_then(|p| p.parse().ok()).unwrap_or(0);
        let v = (n(0), n(1), n(2));
        // platform-tools report 1.0.41 first, then the real 3x version
        if best.map(|b: (u32, u32, u32)| v.0 > b.0).unwrap_or(true) {
            best = Some(v);
        }
    }
    best
}

pub fn older_than(v: (u32, u32, u32), min: (u32, u32, u32)) -> bool {
    v < min
}

/// The whole report as text, so the CLI and the GUI can both show it.
pub fn run(adb: &Adb) -> Result<String> {
    let mut out = String::new();
    let _ = writeln!(out, "adb binary     : {}", adb.bin);
    match Command::new(&adb.bin).arg("version").output() {
        Ok(o) => {
            let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
            match parse_adb_version(&text) {
                Some(v) => {
                    let _ = writeln!(out, "adb version    : {}.{}.{}", v.0, v.1, v.2);
                    if older_than(v, MIN_ADB) {
                        let _ = writeln!(out, "                 -> older than {}.{}.{}; install current platform-tools", MIN_ADB.0, MIN_ADB.1, MIN_ADB.2);
                    }
                }
                None => {
                    let _ = writeln!(out, "adb version    : unrecognised output");
                }
            }
        }
        Err(e) => {
            let _ = writeln!(out, "adb version    : cannot run ({}); install platform-tools and put adb on PATH", e.kind());
        }
    }

    let devices = adb.devices().unwrap_or_default();
    let _ = writeln!(out, "devices        : {}", devices.len());
    for d in &devices {
        let _ = writeln!(out, "                 {} {} {}", d.serial, d.state, d.model.clone().unwrap_or_default());
    }
    if devices.is_empty() {
        let _ = writeln!(out, "                 -> enable developer options on the TV, then: atvtools connect <tv-ip>");
    }
    for d in devices.iter().filter(|d| d.state != "device") {
        let why = crate::adb::advice(&d.state);
        let _ = writeln!(out, "                 -> {} is {}{}", d.serial, d.state, if why.is_empty() { String::new() } else { format!(": {why}") });
    }

    let online = crate::download::fetch_text("https://ipinfo.io/country").ok();
    let _ = writeln!(out, "internet       : {}", if online.is_some() { "yes" } else { "no (/ catalogue installs will fail)" });
    if let Some(country) = online {
        let c = country.trim();
        let _ = writeln!(out, "egress country : {c}");
        let _ = writeln!(out, "                 -> Play Store installs fail when this is a VPN exit in the wrong region");
    }

    if !devices.is_empty() {
        if let Ok(ota) = crate::settings::ota_state(adb) {
            let _ = writeln!(out, "ota updates    : {ota}   (atvtools settings ota --off blocks them)");
        }
    }
    Ok(out)
}

/// Repair what this machine owns: restart the adb server, and put a USB-attached
/// TV into TCP mode so it can be reached over Wi-Fi. What only the TV can fix
/// (wireless debugging off, an unaccepted dialog) is named, not attempted.
pub fn fix(adb: &Adb) -> Result<String> {
    let mut out = String::new();

    let _ = write!(out, "adb server     : ");
    let _ = adb.server().output(&["kill-server"]); // non-zero when no server was running
    match adb.server().output(&["start-server"]) {
        Ok((true, _, _)) => {
            let _ = writeln!(out, "restarted");
        }
        Ok((false, o, e)) => {
            let _ = writeln!(out, "could not start: {}", first_line(&e, &o));
            let port: SocketAddr = (Ipv4Addr::LOCALHOST, 5037).into();
            if TcpStream::connect_timeout(&port, Duration::from_millis(300)).is_ok() {
                let _ = writeln!(
                    out,
                    "                 -> port 5037 answers, so another adb server owns it: quit that one and retry"
                );
            }
        }
        Err(e) => {
            let _ = writeln!(out, "cannot run it ({e}); install platform-tools and put adb on PATH");
            return Ok(out);
        }
    }

    let devices = adb.devices().unwrap_or_default();
    let usb: Vec<&crate::adb::Device> = devices
        .iter()
        .filter(|d| d.state == "device" && !d.serial.contains(':'))
        .collect();
    if usb.is_empty() {
        let _ = writeln!(out, "usb -> tcp     : no USB TV attached (plug one in to put it on port 5555)");
    }
    for d in usb {
        let one = Adb { serial: Some(d.serial.clone()), bin: adb.bin.clone() };
        match one.output(&["tcpip", "5555"]) {
            Ok((true, o, e)) => {
                let _ = writeln!(out, "usb -> tcp     : {} {}", d.serial, first_line(&e, &o));
                match wireless_ip(&one) {
                    Some(ip) => {
                        let _ = writeln!(out, "                 -> unplug and run: atvtools connect {ip}");
                    }
                    None => {
                        let _ = writeln!(
                            out,
                            "                 -> now on port 5555; its Wi-Fi address is in Settings > System > Status"
                        );
                    }
                }
            }
            Ok((false, o, e)) => {
                let _ = writeln!(out, "usb -> tcp     : {} refused it: {}", d.serial, first_line(&e, &o));
            }
            Err(e) => {
                let _ = writeln!(out, "usb -> tcp     : cannot run adb ({e})");
            }
        }
    }

    if let Some(serial) = &adb.serial {
        if serial.contains(':') && !devices.iter().any(|d| &d.serial == serial && d.state == "device") {
            // a wireless TV falls out of the list with the old server, so ask it back
            let _ = write!(out, "target         : reconnecting {serial}: ");
            let answer = adb.server().connect(serial).map(|t| t.trim().to_string()).unwrap_or_else(|e| e.to_string());
            let _ = writeln!(out, "{answer}");
            if !answer.to_ascii_lowercase().contains("connected to") {
                let _ = writeln!(
                    out,
                    "                 -> still down: adbd stops listening on TCP after a reboot, so plug in \
                     USB and rerun this, or re-enable Wireless debugging"
                );
            }
        }
    }
    Ok(out)
}

/// The first non-empty line of a command's output, trimmed.
fn first_line(stderr: &str, stdout: &str) -> String {
    stderr
        .lines()
        .chain(stdout.lines())
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("no output")
        .to_string()
}

/// The TV's own Wi-Fi address: the routing table's "src", else the old DHCP property.
fn wireless_ip(adb: &Adb) -> Option<String> {
    let route = adb.try_out(&["shell", "ip", "route"]);
    for line in route.lines() {
        let mut it = line.split_whitespace();
        while let Some(tok) = it.next() {
            if tok == "src" {
                if let Some(ip) = it.next().filter(|ip| ip.parse::<Ipv4Addr>().is_ok()) {
                    return Some(ip.to_string());
                }
            }
        }
    }
    let prop = adb.try_out(&["shell", "getprop", "dhcp.wlan0.ipaddress"]).trim().to_string();
    (prop != "0.0.0.0" && prop.parse::<Ipv4Addr>().is_ok()).then_some(prop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adb_version_reads_the_real_number() {
        let modern = "Android Debug Bridge version 1.0.41\nVersion 35.0.2-11815563\n";
        assert_eq!(parse_adb_version(modern), Some((35, 0, 2)));
        assert_eq!(parse_adb_version("Android Debug Bridge version 1.0.41"), Some((1, 0, 41)));
        assert!(parse_adb_version("no version here").is_none());
    }

    #[test]
    fn old_adb_is_flagged() {
        assert!(older_than((1, 0, 41), MIN_ADB));
        assert!(!older_than((35, 0, 2), MIN_ADB));
    }
}

#[cfg(all(test, unix))]
mod fix_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    /// A fake adb: records every call and answers the four a repair makes.
    fn fake_adb(dir: &Path, devices: &str) -> Adb {
        let log = dir.join("calls");
        let script = dir.join("adb");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\necho \"$*\" >> {log}\ncase \"$*\" in\n  *kill-server*) exit 0 ;;\n  *start-server*) echo starting ; exit 0 ;;\n  *devices*) printf 'List of devices attached\\n{devices}' ; exit 0 ;;\n  *tcpip*) echo 'restarting in TCP mode port: 5555' ; exit 0 ;;\n  *route*) echo '192.168.1.0/24 dev wlan0 proto kernel scope link src 192.168.1.42' ; exit 0 ;;\nesac\nexit 0\n",
                log = log.display(),
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        Adb { serial: None, bin: script.to_string_lossy().to_string() }
    }

    fn calls(adb: &Adb) -> Vec<String> {
        let path = Path::new(&adb.bin).with_file_name("calls");
        std::fs::read_to_string(path).unwrap_or_default().lines().map(str::to_string).collect()
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("atvtools-doctor-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn fix_restarts_the_server_and_puts_a_usb_tv_on_port_5555() {
        let d = tmp("usb");
        let adb = fake_adb(&d, "USB123\tdevice model:TV\n");
        let out = fix(&adb).unwrap();
        assert!(out.contains("restarted"), "{out}");
        assert!(out.contains("atvtools connect 192.168.1.42"), "{out}");
        let calls = calls(&adb);
        let kill = calls.iter().position(|c| c.ends_with("kill-server")).expect("kill-server");
        let start = calls.iter().position(|c| c.ends_with("start-server")).expect("start-server");
        assert!(kill < start, "the server goes down before it comes up: {calls:?}");
        assert!(calls.iter().any(|c| c.contains("-s USB123 tcpip 5555")), "{calls:?}");
    }

    #[test]
    fn a_wireless_target_that_stopped_listening_is_named_as_a_tv_side_problem() {
        let d = tmp("wireless");
        let mut adb = fake_adb(&d, "");
        adb.serial = Some("192.168.1.105:5555".to_string());
        let out = fix(&adb).unwrap();
        assert!(out.contains("reconnecting 192.168.1.105:5555"), "{out}");
        assert!(out.contains("still down"), "{out}");
        assert!(!out.contains("atvtools connect"), "{out}");
    }
}
