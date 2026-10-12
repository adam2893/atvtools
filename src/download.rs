//! Cross-platform downloader built on stock system tools: curl (Windows 10+,
//! macOS, most Linux) with a PowerShell fallback. Keeps the binary free of a
//! TLS stack.

use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

pub fn fetch(url: &str, dest: &Path) -> Result<()> {
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    let d = dest.to_string_lossy().to_string();
    println!("downloading {url}");
    if try_cmd("curl", &["-fL", "--retry", "2", "-o", &d, url])? {
        return Ok(());
    }
    if cfg!(windows) {
        let ps = format!("Invoke-WebRequest -Uri '{url}' -OutFile '{d}'");
        if try_cmd("powershell", &["-NoProfile", "-Command", &ps])? {
            return Ok(());
        }
    }
    bail!("download failed: no working curl/PowerShell for {url}")
}

/// Download a catalog entry and sideload it.
pub fn install_catalog(adb: &crate::adb::Adb, name: &str) -> Result<()> {
    let e = crate::catalog::find(name)
        .ok_or_else(|| anyhow::anyhow!("unknown app '{name}'. known: {}", crate::catalog::names()))?;
    let dest = std::env::temp_dir().join("atvtools").join(format!("{}.apk", e.name));
    fetch(e.url, &dest)?;
    crate::apps::install(adb, &[dest])
}

/// Fetch straight to stdout - for small API/version lookups that must not
/// leave a file behind. Same curl-first policy as fetch().
pub fn fetch_text(url: &str) -> Result<String> {
    let out = Command::new("curl")
        .args(["-fsSL", "--max-time", "15", url])
        .output()
        .map_err(|e| anyhow::anyhow!("curl unavailable: {e}"))?;
    if !out.status.success() {
        bail!("request failed: {url}");
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Ok(true) when the program exists and exited 0; Ok(false) when it is absent.
fn try_cmd(bin: &str, args: &[&str]) -> Result<bool> {
    match Command::new(bin).args(args).status() {
        Ok(st) => Ok(st.success()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
