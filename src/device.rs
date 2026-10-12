//! Device info, remote-control key events, power/reboot, intent launches and
//! the coarse performance tweaks the original batch script performed.

use crate::adb::Adb;
use anyhow::{bail, Result};

/// (friendly name, Android keycode) for the remote emulation.
pub const KEYS: &[(&str, u32)] = &[
    ("up", 19), ("down", 20), ("left", 21), ("right", 22), ("ok", 23), ("enter", 66),
    ("back", 4), ("home", 3), ("menu", 82), ("search", 84),
    ("play", 126), ("pause", 127), ("playpause", 85), ("stop", 86), ("next", 87), ("prev", 88),
    ("volup", 24), ("voldown", 25), ("mute", 164),
    ("power", 26), ("wakeup", 224), ("sleep", 223),
    ("settings", 176), ("info", 165), ("tv", 170), ("guide", 172), ("assistant", 219),
    ("del", 67), ("tab", 61),
];

pub fn keycode(name: &str) -> Option<u32> {
    let n = name.trim().to_ascii_lowercase();
    if let Ok(n) = n.parse::<u32>() {
        return Some(n);
    }
    KEYS.iter().find(|(k, _)| *k == n).map(|(_, c)| *c)
}

pub fn key(adb: &Adb, names: &[String]) -> Result<()> {
    for n in names {
        let code = keycode(n).ok_or_else(|| anyhow::anyhow!("unknown key '{n}' (numeric keycode also accepted)"))?;
        adb.shell(&format!("input keyevent {code}"))?;
    }
    Ok(())
}

/// Spaces must be escaped as %s for the device-side input command.
pub fn text(adb: &Adb, words: &[String]) -> Result<()> {
    let joined = words.join(" ");
    if joined.is_empty() {
        bail!("nothing to send");
    }
    adb.shell(&format!("input text {}", joined.replace(' ', "%s")))?;
    Ok(())
}

pub fn wake(adb: &Adb) -> Result<()> {
    adb.shell("input keyevent 224")?; // KEYCODE_WAKEUP
    Ok(())
}

pub fn sleep(adb: &Adb) -> Result<()> {
    adb.shell("input keyevent 223")?; // KEYCODE_SLEEP
    Ok(())
}

pub fn reboot(adb: &Adb, mode: Option<&str>) -> Result<()> {
    match mode.unwrap_or("") {
        "" => adb.try_out(&["reboot"]),
        "recovery" => adb.try_out(&["reboot", "recovery"]),
        "bootloader" | "fastboot" => adb.try_out(&["reboot", "bootloader"]),
        other => bail!("unknown reboot mode '{other}' (recovery | bootloader)"),
    };
    Ok(())
}

pub fn open_url(adb: &Adb, url: &str) -> Result<()> {
    adb.shell(&format!("am start -a android.intent.action.VIEW -d '{url}'"))?;
    Ok(())
}

/// Settings screens that can be opened directly, as the original shortcuts did.
pub const SCREENS: &[(&str, &str)] = &[
    ("wifi", "android.settings.WIFI_SETTINGS"),
    ("bluetooth", "android.settings.BLUETOOTH_SETTINGS"),
    ("display", "android.settings.DISPLAY_SETTINGS"),
    ("sound", "android.settings.SOUND_SETTINGS"),
    ("apps", "android.settings.APPLICATION_SETTINGS"),
    ("storage", "android.settings.INTERNAL_STORAGE_SETTINGS"),
    ("date", "android.settings.DATE_SETTINGS"),
    ("language", "android.settings.LOCALE_SETTINGS"),
    ("accessibility", "android.settings.ACCESSIBILITY_SETTINGS"),
    ("developer", "android.settings.APPLICATION_DEVELOPMENT_SETTINGS"),
    ("about", "android.settings.DEVICE_INFO_SETTINGS"),
    ("home", "android.settings.HOME_SETTINGS"),
    ("location", "android.settings.LOCATION_SOURCE_SETTINGS"),
    ("privacy", "android.settings.PRIVACY_SETTINGS"),
    ("screensaver", "android.settings.DREAM_SETTINGS"),
    ("network", "android.settings.NETWORK_OPERATOR_SETTINGS"),
    ("caption", "android.settings.CAPTIONING_SETTINGS"),
];

pub fn screen(adb: &Adb, page: &str) -> Result<()> {
    let p = page.trim().to_ascii_lowercase();
    let action = SCREENS
        .iter()
        .find(|(k, _)| *k == p)
        .map(|(_, a)| *a)
        .ok_or_else(|| anyhow::anyhow!("unknown settings page '{page}'. known: {}", screen_names()))?;
    adb.shell(&format!("am start -a {action}"))?;
    Ok(())
}

pub fn screen_names() -> String {
    SCREENS.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(", ")
}

pub fn expand_notifications(adb: &Adb) -> Result<()> {
    adb.shell("cmd statusbar expand-notifications")?;
    Ok(())
}

/// adb mDNS can suggest TVs around the network when the IP is unknown.
pub fn mdns(adb: &Adb) -> Result<String> {
    Ok(adb.try_out(&["mdns", "services"]))
}

/// Key/value device facts (getprop whitelist + display + battery).
pub fn info(adb: &Adb) -> Result<Vec<(String, String)>> {
    const PROPS: &[(&str, &str)] = &[
        ("serial", "ro.serialno"),
        ("model", "ro.product.model"),
        ("manufacturer", "ro.product.manufacturer"),
        ("android", "ro.build.version.release"),
        ("sdk", "ro.build.version.sdk"),
        ("build", "ro.build.display.id"),
        ("abi", "ro.product.cpu.abi"),
        ("ip", "dhcp.eth0.ipaddress"),
    ];
    let mut out = Vec::new();
    for (label, prop) in PROPS {
        let v = adb.try_out(&["shell", &format!("getprop {prop}")]).trim().to_string();
        if !v.is_empty() {
            out.push(((*label).to_string(), v));
        }
    }
    let wm = adb.try_out(&["shell", "wm size"]).trim().to_string();
    if !wm.is_empty() {
        out.push(("resolution".into(), wm.replace('\n', " ")));
    }
    let density = crate::settings::density(adb).unwrap_or_default();
    if !density.is_empty() {
        out.push(("density".into(), density));
    }
    let battery = adb.try_out(&["shell", "dumpsys battery"]);
    for key in ["level", "status", "temperature"] {
        if let Some(v) = battery.lines().map(str::trim).find_map(|l| l.strip_prefix(&format!("{key}: "))) {
            out.push((format!("battery_{key}"), v.to_string()));
        }
    }
    Ok(out)
}

/// Animation scales, cache trim, background app kill - the script's option 8.
pub fn optimize(adb: &Adb) -> Result<()> {
    for (k, v) in [
        ("window_animation_scale", "0.5"),
        ("transition_animation_scale", "0.5"),
        ("animator_duration_scale", "0.5"),
    ] {
        adb.shell(&format!("settings put global {k} {v}"))?;
    }
    println!("{}", adb.try_out(&["shell", "pm trim-caches 1000M"]).trim());
    adb.try_out(&["shell", "am kill-all"]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keycodes_resolve_names_and_numbers() {
        assert_eq!(keycode("Home"), Some(3));
        assert_eq!(keycode("224"), Some(224));
        assert_eq!(keycode("nope"), None);
    }

    #[test]
    fn settings_pages_resolve() {
        assert!(SCREENS.iter().any(|(k, a)| *k == "developer" && a.contains("DEVELOPMENT")));
        assert!(screen_names().contains("screensaver"));
    }
}
