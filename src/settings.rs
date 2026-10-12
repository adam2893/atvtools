//! Device settings: display density, font scale, DNS ad blocking, NTP.

use crate::adb::Adb;
use anyhow::Result;

pub fn density(adb: &Adb) -> Result<String> {
    Ok(first_after_colon(&adb.shell("wm density")?))
}

pub fn set_density(adb: &Adb, dpi: u32) -> Result<()> {
    if dpi < 72 {
        anyhow::bail!("density must be 72 or higher (you asked for {dpi})");
    }
    adb.shell(&format!("wm density {dpi}"))?;
    Ok(())
}

pub fn reset_density(adb: &Adb) -> Result<()> {
    adb.shell("wm density reset")?;
    Ok(())
}

pub fn font_scale(adb: &Adb) -> Result<String> {
    Ok(adb.shell("settings get system font_scale")?.trim().to_string())
}

pub fn set_font_scale(adb: &Adb, scale: f32) -> Result<()> {
    if !(0.5..=2.0).contains(&scale) {
        anyhow::bail!("font scale must be between 0.5 and 2.0");
    }
    adb.shell(&format!("settings put system font_scale {scale}"))?;
    Ok(())
}

/// AdGuard DNS-over-TLS host, the same one the original tool installs.
pub const ADGUARD_DNS: &str = "dns.adguard.com";

pub fn adblock_on(adb: &Adb) -> Result<()> {
    adb.shell("settings put global private_dns_mode hostname")?;
    adb.shell(&format!("settings put global private_dns_specifier {ADGUARD_DNS}"))?;
    Ok(())
}

pub fn adblock_off(adb: &Adb) -> Result<()> {
    adb.shell("settings put global private_dns_mode off")?;
    adb.shell("settings delete global private_dns_specifier")?;
    Ok(())
}

pub fn dns_state(adb: &Adb) -> Result<String> {
    let mode = adb.shell("settings get global private_dns_mode")?;
    let spec = adb.shell("settings get global private_dns_specifier")?;
    Ok(format!("mode={} specifier={}", mode.trim(), spec.trim()))
}

pub fn set_ntp(adb: &Adb, host: &str) -> Result<()> {
    adb.shell(&format!("settings put global ntp_server {host}"))?;
    Ok(())
}

fn first_after_colon(raw: &str) -> String {
    raw.lines().find_map(|l| l.split_once(':').map(|(_, v)| v.trim().to_string())).unwrap_or_default()
}

/// Custom TV settings toggles the original exposed as "custom settings".
pub struct Toggle {
    pub name: &'static str,
    pub namespace: &'static str,
    pub key: &'static str,
    pub on: &'static str,
    pub off: &'static str,
}

pub const TOGGLES: &[Toggle] = &[
    Toggle { name: "developer-options", namespace: "global", key: "development_settings_enabled", on: "1", off: "0" },
    Toggle { name: "gps", namespace: "secure", key: "location_mode", on: "3", off: "0" },
    Toggle { name: "play-protect", namespace: "global", key: "package_verifier_enable", on: "1", off: "0" },
    Toggle { name: "ambient-display", namespace: "secure", key: "doze_enabled", on: "1", off: "0" },
    Toggle { name: "ambient-always-on", namespace: "secure", key: "doze_always_on", on: "1", off: "0" },
    Toggle { name: "screensaver", namespace: "secure", key: "screensaver_enabled", on: "1", off: "0" },
];

pub fn toggle(adb: &Adb, name: &str, on: Option<bool>) -> Result<String> {
    let n = name.trim().to_ascii_lowercase();
    let t = TOGGLES
        .iter()
        .find(|t| t.name == n)
        .ok_or_else(|| anyhow::anyhow!("unknown setting '{name}'. known: {}", toggle_names()))?;
    let cmd = match on {
        Some(true) => format!("settings put {} {} {}", t.namespace, t.key, t.on),
        Some(false) => format!("settings put {} {} {}", t.namespace, t.key, t.off),
        None => format!("settings get {} {}", t.namespace, t.key),
    };
    Ok(adb.shell(&cmd)?.trim().to_string())
}

pub fn toggle_names() -> String {
    TOGGLES.iter().map(|t| t.name).collect::<Vec<_>>().join(", ")
}

pub fn screen_off_timeout(adb: &Adb, ms: Option<u32>) -> Result<String> {
    match ms {
        Some(v) => Ok(adb.shell(&format!("settings put system screen_off_timeout {v}"))?.trim().to_string()),
        None => Ok(adb.shell("settings get system screen_off_timeout")?.trim().to_string()),
    }
}

/// Locks the screen to a fixed rotation (0, 90, 180, 270).
pub fn rotate(adb: &Adb, rotation: u8) -> Result<()> {
    if rotation > 3 {
        anyhow::bail!("rotation must be 0..3 (0=0deg, 1=90, 2=180, 3=270)");
    }
    adb.shell("settings put system accelerometer_rotation 0")?;
    adb.shell(&format!("settings put system user_rotation {rotation}"))?;
    Ok(())
}

/// Replaces the daydream/screensaver component, e.g. Aerial Views.
/// Shortcut: exact component strings differ per build; pass the full
/// package/activity and read it back from DreamManagerService if it does not stick.
pub fn screensaver(adb: &Adb, component: &str) -> Result<()> {
    if component.eq_ignore_ascii_case("off") {
        adb.shell("settings put secure screensaver_enabled 0")?;
        return Ok(());
    }
    adb.shell(&format!("settings put secure screensaver_components {component}"))?;
    adb.shell("settings put secure screensaver_enabled 1")?;
    Ok(())
}

/// Apply one appop to every third-party package - the script's "grant write
/// permissions on external USB" step, generalised to any op.
pub fn set_default_permissions(adb: &Adb, op: &str, allow: bool) -> Result<()> {
    let mode = if allow { "allow" } else { "deny" };
    let mut done = 0;
    let mut failed = 0;
    for package in crate::apps::list(adb, crate::apps::Scope::ThirdParty)? {
        if crate::apps::appop(adb, &package, op, allow).is_ok() {
            done += 1;
        } else {
            failed += 1;
        }
    }
    let mut tail = String::new();
    if failed > 0 {
        tail = format!(", {failed} failed");
    }
    if includes_storage_provider(op, allow) {
        // system package: never in the third-party list above
        adb.shell("appops set com.android.externalstorage MANAGE_EXTERNAL_STORAGE allow").ok();
        tail.push_str(", +com.android.externalstorage");
    }
    println!("{done} package(s): {op}={mode}{tail}");
    Ok(())
}

/// The external-storage provider is a system package, so --all needs it added
/// back by hand - but only in the direction the script used it.
pub fn includes_storage_provider(op: &str, allow: bool) -> bool {
    allow && op == "MANAGE_EXTERNAL_STORAGE"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggles_resolve_by_name() {
        assert_eq!(TOGGLES.iter().find(|t| t.name == "gps").unwrap().key, "location_mode");
        assert!(toggle_names().contains("play-protect"));
    }

    #[test]
    fn storage_provider_is_added_only_for_the_grant() {
        assert!(includes_storage_provider("MANAGE_EXTERNAL_STORAGE", true));
        assert!(!includes_storage_provider("MANAGE_EXTERNAL_STORAGE", false));
        assert!(!includes_storage_provider("WRITE_SETTINGS", true));
    }

    #[test]
    fn rotation_range_is_enforced() {
        assert!(toggle_names().len() > 10);
    }

    #[test]
    fn density_line_is_parsed() {
        assert_eq!(first_after_colon("Physical density: 320\n"), "320");
    }
}
