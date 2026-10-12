//! Thin wrapper around the platform adb binary. No shell is used, so every
//! argument reaches adb verbatim (the original batch script had to fight
//! cmd.exe quoting; here that whole class of bug is gone).

use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Default)]
pub struct Adb {
    /// Device serial (usually "192.168.1.10:5555"); None = let adb pick.
    pub serial: Option<String>,
    /// adb binary; ADB env var wins, else "adb" from PATH.
    pub bin: String,
}

/// The step the user should take, given what adb complained about. Empty when
/// adb already said something self-explanatory.
pub fn advice(text: &str) -> &'static str {
    let t = text.to_ascii_lowercase();
    if t.contains("unauthorized") {
        "the TV has not allowed this computer: accept the \"Allow debugging?\" dialog on the TV (never appeared? Developer options > revoke authorisations, then reconnect)"
    } else if t.contains("more than one device") {
        "several TVs are attached: choose one with -s <serial>, or press T in the menu"
    } else if t.contains("device offline") || t.contains("not responding") {
        "the TV is offline: wake it and reconnect (press C in the menu)"
    } else if t.contains("no devices") || t.contains("device not found") || t.contains("not found") {
        "no TV attached: press C in the menu, or run: atvtools connect <tv-ip>"
    } else {
        ""
    }
}

/// One line of `adb devices -l`.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub serial: String,
    pub state: String,
    pub model: Option<String>,
}

impl Adb {
    pub fn new(serial: Option<String>) -> Self {
        let bin = std::env::var("ADB").unwrap_or_else(|_| "adb".to_string());
        Self { serial, bin }
    }

    pub fn args_for(&self, args: &[&str]) -> Vec<String> {
        let mut v = Vec::with_capacity(args.len() + 2);
        if let Some(s) = &self.serial {
            v.push("-s".into());
            v.push(s.clone());
        }
        v.extend(args.iter().map(|a| (*a).to_string()));
        v
    }

    pub fn out(&self, args: &[&str]) -> Result<String> {
        let argv = self.args_for(args);
        let o = Command::new(&self.bin)
            .args(&argv)
            .stdin(Stdio::null())
            .output()
            .with_context(|| format!("running {} {}", self.bin, argv.join(" ")))?;
        let stdout = String::from_utf8_lossy(&o.stdout).to_string();
        if !o.status.success() {
            let err = String::from_utf8_lossy(&o.stderr);
            let detail = format!("{}{}", stdout.trim(), err.trim());
            let tip = advice(&detail);
            let suffix = if tip.is_empty() { String::new() } else { format!("\n  -> {tip}") };
            bail!("adb {} failed: {}{}{suffix}", argv.join(" "), stdout.trim(), err.trim());
        }
        Ok(stdout)
    }

    /// Like out(), but reports failures as text instead of erroring: adb exits
    /// non-zero for plenty of benign cases (already disabled, no such package).
    pub fn try_out(&self, args: &[&str]) -> String {
        self.out(args).unwrap_or_else(|e| e.to_string())
    }

    /// Runs adb with the terminal attached (scrcpy, logcat, interactive shell).
    pub fn run(&self, args: &[&str]) -> Result<()> {
        let argv = self.args_for(args);
        let st = Command::new(&self.bin)
            .args(&argv)
            .status()
            .with_context(|| format!("running {} {}", self.bin, argv.join(" ")))?;
        if !st.success() {
            bail!("adb {} exited with {}", argv.join(" "), st);
        }
        Ok(())
    }

    pub fn devices(&self) -> Result<Vec<Device>> {
        Ok(parse_devices(&self.out(&["devices", "-l"])?))
    }

    pub fn connect(&self, host: &str) -> Result<String> {
        let target = if host.contains(':') { host.to_string() } else { format!("{host}:5555") };
        self.out(&["connect", &target])
    }

    pub fn disconnect(&self) -> Result<String> {
        self.out(&["disconnect"])
    }

    pub fn shell(&self, cmd: &str) -> Result<String> {
        self.out(&["shell", cmd])
    }
}

pub fn parse_devices(raw: &str) -> Vec<Device> {
    raw.lines()
        .skip(1)
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let serial = it.next()?.to_string();
            let state = it.next()?.to_string();
            let model = l.split_whitespace().find_map(|t| t.strip_prefix("model:")).map(str::to_string);
            Some(Device { serial, state, model })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_devices_output() {
        let raw = "List of devices attached\n192.168.1.10:5555\tdevice product:sabrina model:Chromecast device:sabrina\n10.0.0.5:5555\toffline\n";
        let d = parse_devices(raw);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].serial, "192.168.1.10:5555");
        assert_eq!(d[0].state, "device");
        assert_eq!(d[0].model.as_deref(), Some("Chromecast"));
        assert_eq!(d[1].state, "offline");
    }

    #[test]
    fn adb_errors_carry_a_next_step() {
        assert!(advice("adb: device unauthorized.\nThis adb server's $ADB_VENDOR_KEYS is not set").contains("accept the"));
        assert!(advice("error: no devices/emulators found").contains("connect"));
        assert!(advice("error: more than one device/emulator").contains("-s"));
        assert!(advice("error: device offline").contains("offline"));
        assert!(advice("error: closed").is_empty());
    }

    #[test]
    fn serial_is_prepended() {
        let a = Adb { serial: Some("tv:5555".into()), bin: "adb".into() };
        assert_eq!(a.args_for(&["shell", "id"]), vec!["-s", "tv:5555", "shell", "id"]);
        let b = Adb { serial: None, bin: "adb".into() };
        assert_eq!(b.args_for(&["shell", "id"]), vec!["shell", "id"]);
    }
}
