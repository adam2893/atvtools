//! Preflight and environment diagnosis: is adb present and recent, is a device
//! reachable, is there internet for the catalog, and the region hint the
//! original printed (its "Countries List.txt" step existed because a VPN IP in
//! the wrong country makes Play Store installs fail).

use crate::adb::Adb;
use anyhow::Result;
use std::process::Command;

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

pub fn run(adb: &Adb) -> Result<()> {
    println!("adb binary     : {}", adb.bin);
    match Command::new(&adb.bin).arg("version").output() {
        Ok(o) => {
            let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
            match parse_adb_version(&text) {
                Some(v) => {
                    println!("adb version    : {}.{}.{}", v.0, v.1, v.2);
                    if older_than(v, MIN_ADB) {
                        println!("                 -> older than {}.{}.{}; install current platform-tools", MIN_ADB.0, MIN_ADB.1, MIN_ADB.2);
                    }
                }
                None => println!("adb version    : unrecognised output"),
            }
        }
        Err(e) => println!("adb version    : cannot run ({}); install platform-tools and put adb on PATH", e.kind()),
    }

    let devices = adb.devices().unwrap_or_default();
    println!("devices        : {}", devices.len());
    for d in &devices {
        println!("                 {} {} {}", d.serial, d.state, d.model.clone().unwrap_or_default());
    }
    if devices.is_empty() {
        println!("                 -> enable developer options on the TV, then: atvtools connect <tv-ip>");
    }

    let online = crate::download::fetch_text("https://ipinfo.io/country").ok();
    println!("internet       : {}", if online.is_some() { "yes" } else { "no (/ catalogue installs will fail)" });
    if let Some(country) = online {
        let c = country.trim();
        println!("egress country : {c}");
        println!("                 -> Play Store installs fail when this is a VPN exit in the wrong region");
    }

    if !devices.is_empty() {
        if let Ok(ota) = crate::settings::ota_state(adb) {
            println!("ota updates    : {ota}   (atvtools settings ota --off blocks them)");
        }
    }
    Ok(())
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
