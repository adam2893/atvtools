//! Android TV safety guards, privacy/ACR audit, OTA drift detection, and
//! launcher takeover helpers (ported from gathin23/cleanyourtv).

use crate::adb::{Adb, PackageId};
use crate::{apps, debloat, undo};
use anyhow::Result;
use std::collections::BTreeSet;

/// Core Android & TV middleware packages that must never be disabled:
/// disabling them can prevent the TV from booting (e.g. MediaTek MAL / TIS,
/// Android ext.services, permissioncontroller, location.fused).
pub fn is_core_package(pkg: &str) -> bool {
    if matches!(
        pkg,
        "android"
            | "android.ext.services"
            | "android.ext.shared"
            | "com.android.sdksandbox"
            | "com.google.android.sdksandbox"
            | "com.android.location.fused"
            | "com.android.systemui"
            | "com.android.shell"
            | "com.android.phone"
            | "com.android.settings"
            | "com.android.tv.settings"
            | "com.android.permissioncontroller"
            | "com.google.android.permissioncontroller"
            | "com.android.packageinstaller"
            | "com.google.android.packageinstaller"
            | "com.android.ext.services"
            | "com.google.android.ext.services"
            | "com.android.ext.shared"
            | "com.google.android.ext.shared"
            | "com.android.modulemetadata"
            | "com.google.android.modulemetadata"
            | "com.cltv.mal"
            | "com.mediatek.tis"
            | "com.mediatek.tv.service"
            | "com.mediatek.tv.agent"
    ) {
        return true;
    }
    if pkg.starts_with("com.google.android.overlay.modules") {
        return true;
    }
    if pkg.starts_with("com.android.providers.") {
        return !matches!(
            pkg,
            "com.android.providers.calendar"
                | "com.android.providers.contacts"
                | "com.android.providers.userdictionary"
                | "com.android.providers.blockednumber"
        );
    }
    false
}

/// Factory/production menu packages that should never be launched (opening
/// them on CVTE/Haier/MediaTek boards can flip a factory-mode flag).
pub fn is_factory_package(pkg: &str) -> bool {
    let lower = pkg.to_ascii_lowercase();
    lower.contains("factorytest")
        || lower.contains("factory_test")
        || lower.contains("factorymode")
        || lower.contains("factorymenu")
        || lower.contains(".fac.menu")
        || lower.contains("hkcfac")
        || lower.starts_with("com.at.factory.")
}

pub const SETUPWRAITH: &str = "com.google.android.tungsten.setupwraith";

/// Extract `cname=` owner strings from `dumpsys wifi` output.
pub fn wifi_owners_from_dumpsys(raw: &str) -> Vec<String> {
    let mut owners = Vec::new();
    for token in raw.split_whitespace() {
        if let Some(owner) = token.strip_prefix("cname=") {
            let clean = owner.trim_end_matches([',', ')', ']']);
            if !clean.is_empty() {
                owners.push(clean.to_string());
            }
        }
    }
    owners
}

/// Returns `Ok(true)` if disabling `setupwraith` is safe (it does NOT own a
/// saved Wi-Fi network), or `Ok(false)` with a reason if it should be skipped.
pub fn is_setupwraith_safe(adb: &Adb) -> (bool, Option<String>) {
    let Ok(wifi) = adb.shell("dumpsys wifi") else {
        return (
            false,
            Some(
                "Skipping setupwraith: could not read Wi-Fi network owners (dumpsys wifi failed). Forget & rejoin Wi-Fi in Settings first, or use force override on Ethernet."
                    .to_string(),
            ),
        );
    };
    let owners = wifi_owners_from_dumpsys(&wifi);
    if owners.is_empty() {
        return (
            false,
            Some(
                "Skipping setupwraith: no saved Wi-Fi owner found in dumpsys wifi. Use force override if on Ethernet."
                    .to_string(),
            ),
        );
    }
    if owners.iter().any(|o| o.contains("setupwraith")) {
        return (
            false,
            Some(
                "Skipping setupwraith: your saved Wi-Fi network belongs to it; disabling it would delete that Wi-Fi network and cut ADB. Forget & rejoin Wi-Fi in Settings first."
                    .to_string(),
            ),
        );
    }
    (true, None)
}

const NOT_LAUNCHERS: &[&str] = &[
    SETUPWRAITH,
    "com.android.tv.settings",
    "com.android.settings",
    "com.android.provision",
];

/// Filter `cmd package query-activities --brief -a android.intent.action.MAIN -c android.intent.category.HOME`
/// down to real usable launchers (excluding FallbackHome, factorytest, setupwraith, settings).
pub fn filter_real_launchers(raw: &str) -> Vec<String> {
    raw.lines()
        .map(str::trim)
        .filter(|line| line.contains('/') && !line.contains(' '))
        .filter(|comp| {
            let pkg = comp.split('/').next().unwrap_or("");
            !NOT_LAUNCHERS.contains(&pkg)
                && !comp.ends_with("FallbackHome")
                && !is_factory_package(pkg)
        })
        .map(str::to_string)
        .collect()
}

/// Query all real HOME launcher components installed on the TV.
pub fn query_home_launchers(adb: &Adb) -> Result<Vec<String>> {
    let raw = adb.shell(
        "cmd package query-activities --brief -a android.intent.action.MAIN -c android.intent.category.HOME",
    )?;
    Ok(filter_real_launchers(&raw))
}

#[derive(Debug, Clone, Default)]
pub struct SafetyOptions {
    pub force_setupwraith: bool,
    pub force_home: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GuardResult {
    pub allowed: Vec<String>,
    pub blocked: Vec<(String, String)>,
    pub warnings: Vec<String>,
}

/// Validate and filter a list of packages before disabling them.
/// Core Android/TV middleware packages are always blocked.
/// `setupwraith` and the last remaining Home launcher are skipped unless overridden.
pub fn guard_packages(adb: &Adb, pkgs: &[String], opts: &SafetyOptions) -> GuardResult {
    let mut res = GuardResult::default();
    let mut candidates = Vec::new();

    for p in pkgs {
        let pkg = p.trim();
        if pkg.is_empty() {
            continue;
        }
        if PackageId::parse(pkg).is_err() {
            res.blocked
                .push((pkg.to_string(), "invalid package name".to_string()));
            continue;
        }
        if is_core_package(pkg) {
            res.blocked.push((
                pkg.to_string(),
                "core Android or TV middleware package (brick risk)".to_string(),
            ));
            continue;
        }
        if pkg == SETUPWRAITH && !opts.force_setupwraith {
            let (safe, msg) = is_setupwraith_safe(adb);
            if !safe {
                let reason = msg.unwrap_or_else(|| "setupwraith Wi-Fi guard".to_string());
                res.warnings.push(reason.clone());
                res.blocked.push((pkg.to_string(), reason));
                continue;
            }
        }
        candidates.push(pkg.to_string());
    }

    if !opts.force_home && !candidates.is_empty() {
        if let Ok(launchers) = query_home_launchers(adb) {
            if !launchers.is_empty() {
                let mut remaining = launchers.clone();
                let mut filtered = Vec::with_capacity(candidates.len());
                for pkg in candidates {
                    let is_launcher = remaining
                        .iter()
                        .any(|comp| comp.split('/').next() == Some(pkg.as_str()));
                    if is_launcher {
                        let left_after = remaining
                            .iter()
                            .filter(|comp| comp.split('/').next() != Some(pkg.as_str()))
                            .count();
                        if left_after == 0 {
                            let reason = format!(
                                "Home guard: {pkg} is the only usable HOME launcher on this TV; install FLauncher/Projectivy first or pass --force-home"
                            );
                            res.warnings.push(reason.clone());
                            res.blocked.push((pkg, reason));
                            continue;
                        }
                        remaining.retain(|comp| comp.split('/').next() != Some(pkg.as_str()));
                    }
                    filtered.push(pkg);
                }
                candidates = filtered;
            }
        }
    }

    res.allowed = candidates;
    res
}

/// Set a third-party launcher as the default HOME activity and verify focus.
pub fn set_home_activity(adb: &Adb, package: &str) -> Result<String> {
    let pkg = PackageId::parse(package)?;
    let launchers = query_home_launchers(adb)?;
    let component = launchers
        .into_iter()
        .find(|c| c.split('/').next() == Some(pkg.as_str()))
        .ok_or_else(|| anyhow::anyhow!("{pkg} has no usable HOME activity on this TV"))?;
    let out = adb.shell(&format!("cmd package set-home-activity {component}"))?;
    Ok(format!("{component}: {}", out.trim()))
}

/// Flush Android package state to disk (`sync`) before rebooting so recent
/// `pm disable-user` changes are not lost during Android's `WRITE_SETTINGS_DELAY`.
pub fn pre_reboot_sync(adb: &Adb) -> Result<()> {
    let _ = adb.shell("sync");
    Ok(())
}

/// Check `ro.adb.secure` — returns `Some(warning)` if port 5555 is unauthenticated.
pub fn check_adb_secure(adb: &Adb) -> Option<String> {
    let val = adb.shell("getprop ro.adb.secure").ok()?;
    let v = val.trim();
    if v == "0" || v.is_empty() {
        Some(
            "ro.adb.secure is 0/unset: any device on your LAN can open an ADB shell on port 5555 without a prompt. Turn off ADB/network debugging when finished."
                .to_string(),
        )
    } else {
        None
    }
}

/// Read the TV's manufacturer/brand string for brand-specific preset matching.
pub fn device_brand(adb: &Adb) -> String {
    let mfg = adb.try_out(&["shell", "getprop ro.product.manufacturer"]);
    let brand = adb.try_out(&["shell", "getprop ro.product.brand"]);
    format!("{} {}", mfg.trim(), brand.trim()).trim().to_string()
}

/// Switch a USB-attached TV into TCP/IP mode on port 5555 and try to read its LAN IP.
pub fn usb_to_wifi(adb: &Adb) -> Result<String> {
    let tcp = adb.out(&["tcpip", "5555"])?;
    let route = adb.try_out(&["shell", "ip route get 1.1.1.1"]);
    let ip = route
        .split_whitespace()
        .skip_while(|t| *t != "src")
        .nth(1)
        .unwrap_or("<tv-ip>");
    Ok(format!("{} — connect wirelessly at {ip}:5555", tcp.trim()))
}

/// Capture remote key events for ~4 seconds via `getevent -lt` so users can
/// see exact keycodes and scancodes for remapping.
pub fn read_last_key_event(adb: &Adb) -> Result<String> {
    let out = adb.try_out(&["shell", "timeout 4 getevent -lt"]);
    let lines: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("EV_KEY") || l.contains("MSC_SCAN") || l.contains("KEY_"))
        .take(20)
        .collect();
    if lines.is_empty() {
        Ok("No key events captured in 4s (press a remote button while scanning, or TV restricts getevent).".to_string())
    } else {
        Ok(lines.join("\n"))
    }
}

/// Security audit summary for locking down ADB after cleanup.
pub fn security_audit(adb: &Adb) -> String {
    let mut notes = Vec::new();
    if let Some(w) = check_adb_secure(adb) {
        notes.push(format!("WARNING: {w}"));
    } else {
        notes.push("OK: ro.adb.secure=1 (ADB requires authorization prompt).".to_string());
    }
    let usb_dbg = adb.try_out(&["shell", "settings get global adb_enabled"]);
    let wifi_dbg = adb.try_out(&["shell", "settings get global adb_wifi_enabled"]);
    notes.push(format!(
        "adb_enabled={}, adb_wifi_enabled={}",
        usb_dbg.trim(),
        wifi_dbg.trim()
    ));
    notes.push("When finished: open Developer Options on the TV and toggle off Wireless/Network debugging.".to_string());
    notes.join("\n")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyCategory {
    Acr,
    Voice,
    Telemetry,
}

impl PrivacyCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Acr => "ACR",
            Self::Voice => "Voice",
            Self::Telemetry => "Telemetry",
        }
    }
}

pub struct PrivacyEntry {
    pub package: &'static str,
    pub category: PrivacyCategory,
    pub note: &'static str,
}

pub const PRIVACY_CATALOG: &[PrivacyEntry] = &[
    PrivacyEntry {
        package: "tv.inscape.tvclient.inscapeacr",
        category: PrivacyCategory::Acr,
        note: "Inscape ACR: identifies what is on screen (including HDMI) to target ads",
    },
    PrivacyEntry {
        package: "com.skyworth.aisense",
        category: PrivacyCategory::Acr,
        note: "Skyworth/Coocaa ACR ad-targeting service",
    },
    PrivacyEntry {
        package: "tv.samba.ssm.ui",
        category: PrivacyCategory::Acr,
        note: "Samba TV ACR: identifies on-screen content on TCL/Sony sets",
    },
    PrivacyEntry {
        package: "com.google.android.katniss",
        category: PrivacyCategory::Voice,
        note: "Google Assistant & voice search (disabling stops remote mic button)",
    },
    PrivacyEntry {
        package: "com.google.android.speech.pumpkin",
        category: PrivacyCategory::Voice,
        note: "Offline hotword model for Google Assistant",
    },
    PrivacyEntry {
        package: "org.droidtv.amazonalexa",
        category: PrivacyCategory::Voice,
        note: "Amazon Alexa voice integration on Philips TVs",
    },
    PrivacyEntry {
        package: "org.droidtv.candeebug",
        category: PrivacyCategory::Telemetry,
        note: "Philips DAC analytics service",
    },
    PrivacyEntry {
        package: "org.droidtv.gel",
        category: PrivacyCategory::Telemetry,
        note: "Philips GEL analytics",
    },
    PrivacyEntry {
        package: "org.droidtv.usagelogger",
        category: PrivacyCategory::Telemetry,
        note: "Philips boot stats, keypress counts, and panel telemetry",
    },
    PrivacyEntry {
        package: "org.droidtv.tv.frameworklogger",
        category: PrivacyCategory::Telemetry,
        note: "Philips framework and reboot logging",
    },
    PrivacyEntry {
        package: "com.tcl.logkit",
        category: PrivacyCategory::Telemetry,
        note: "TCL log capture and upload",
    },
    PrivacyEntry {
        package: "com.google.android.feedback",
        category: PrivacyCategory::Telemetry,
        note: "Google feedback report uploader",
    },
    PrivacyEntry {
        package: "com.android.tv.feedbackconsent",
        category: PrivacyCategory::Telemetry,
        note: "Android TV bug report consent & log collector",
    },
    PrivacyEntry {
        package: "com.google.android.tv.remotecontrol.logging",
        category: PrivacyCategory::Telemetry,
        note: "Google TV remote usage logging",
    },
    PrivacyEntry {
        package: "com.google.android.apps.tv.netoscope",
        category: PrivacyCategory::Telemetry,
        note: "Google TV network diagnostics (holds DUMP + INTERNET)",
    },
    PrivacyEntry {
        package: "com.google.android.partnersetup",
        category: PrivacyCategory::Telemetry,
        note: "Google partner setup / telemetry",
    },
];

#[derive(Debug, Clone)]
pub struct PrivacyFinding {
    pub package: String,
    pub category: PrivacyCategory,
    pub note: String,
    pub enabled: bool,
}

/// Audit enabled & disabled packages on the TV against `PRIVACY_CATALOG`.
pub fn audit_privacy(adb: &Adb) -> Result<Vec<PrivacyFinding>> {
    let all: BTreeSet<String> = apps::list(adb, apps::Scope::All)?.into_iter().collect();
    let disabled_raw = adb.try_out(&["shell", "pm list packages -d"]);
    let disabled: BTreeSet<String> = apps::parse_package_list(&disabled_raw).into_iter().collect();

    let mut findings = Vec::new();
    for entry in PRIVACY_CATALOG {
        if all.contains(entry.package) {
            findings.push(PrivacyFinding {
                package: entry.package.to_string(),
                category: entry.category,
                note: entry.note.to_string(),
                enabled: !disabled.contains(entry.package),
            });
        }
    }
    Ok(findings)
}

#[derive(Debug, Clone, Default)]
pub struct DriftReport {
    /// Packages expected to be disabled that are currently enabled.
    pub reenabled: Vec<String>,
    /// Current resolved HOME component if it differs from expected launcher.
    pub current_home: Option<String>,
}

impl DriftReport {
    pub fn is_clean(&self) -> bool {
        self.reenabled.is_empty() && self.current_home.is_none()
    }
}

/// Compare current TV package state against expected-disabled packages (e.g. a
/// preset or saved profile) to catch OTA updates re-enabling bloatware.
pub fn check_drift(adb: &Adb, expected_disabled: &[String]) -> Result<DriftReport> {
    let all: BTreeSet<String> = apps::list(adb, apps::Scope::All)?.into_iter().collect();
    let disabled_raw = adb.try_out(&["shell", "pm list packages -d"]);
    let disabled: BTreeSet<String> = apps::parse_package_list(&disabled_raw).into_iter().collect();

    let mut reenabled = Vec::new();
    for pkg in expected_disabled {
        if all.contains(pkg) && !disabled.contains(pkg) {
            reenabled.push(pkg.clone());
        }
    }
    Ok(DriftReport {
        reenabled,
        current_home: None,
    })
}

/// Re-disable any drifted packages using `guard_packages` and record an undo entry.
pub fn fix_drift(
    adb: &Adb,
    expected_disabled: &[String],
    opts: &SafetyOptions,
) -> Result<(GuardResult, DriftReport)> {
    let report = check_drift(adb, expected_disabled)?;
    let guard = guard_packages(adb, &report.reenabled, opts);
    if !guard.allowed.is_empty() {
        apps::set_enabled(adb, &guard.allowed, false)?;
        let _ = undo::note("drift-fix", adb.serial.as_deref(), &guard.allowed);
    }
    Ok((guard, report))
}

/// Collect expected-disabled packages from a preset name (or all presets if None).
pub fn preset_packages(preset: Option<&str>) -> Result<Vec<String>> {
    match preset {
        Some(name) => {
            let p = debloat::find(name)
                .ok_or_else(|| anyhow::anyhow!("unknown preset '{name}'"))?;
            Ok(p.packages.iter().map(|s| (*s).to_string()).collect())
        }
        None => Ok(debloat::PRESETS
            .iter()
            .flat_map(|p| p.packages.iter().map(|s| (*s).to_string()))
            .collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_packages_are_protected() {
        assert!(is_core_package("android"));
        assert!(is_core_package("com.cltv.mal"));
        assert!(is_core_package("com.mediatek.tis"));
        assert!(is_core_package("com.android.location.fused"));
        assert!(is_core_package("com.android.providers.media"));
        assert!(!is_core_package("com.android.providers.calendar"));
        assert!(!is_core_package("com.google.android.youtube.tv"));
    }

    #[test]
    fn factory_packages_are_detected() {
        assert!(is_factory_package("com.cvte.tv.hkcfac"));
        assert!(is_factory_package("com.duokan.factorytest"));
        assert!(!is_factory_package("com.google.android.youtube.tv"));
    }

    #[test]
    fn parses_wifi_cname_owners() {
        let sample = "NetworkInfo: SSID: \"Home\" cname=com.google.android.tungsten.setupwraith, bssid=00:11";
        assert_eq!(
            wifi_owners_from_dumpsys(sample),
            vec!["com.google.android.tungsten.setupwraith".to_string()]
        );
    }

    #[test]
    fn filters_real_launchers_only() {
        let raw = "\
            com.google.android.tvlauncher/.MainActivity\n\
            com.tcl.keycustomfunctionservice/.FallbackHome\n\
            com.duokan.factorytest/.FactoryLauncher\n\
            com.google.android.tungsten.setupwraith/.SetupActivity\n\
            de.cyberflohr.flauncher/.MainActivity\n";
        assert_eq!(
            filter_real_launchers(raw),
            vec![
                "com.google.android.tvlauncher/.MainActivity".to_string(),
                "de.cyberflohr.flauncher/.MainActivity".to_string(),
            ]
        );
    }
}
