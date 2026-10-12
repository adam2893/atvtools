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
    // A browser UA: some hosts (link shorteners, CDNs) refuse anything else.
    let ua = "Mozilla/5.0 (compatible; atvtools)";
    if let Some(err) = curl_to(url, &d, ua)? {
        bail!("download failed for {url}: {}", last_line(&err));
    }
    if cfg!(windows) {
        let ps = format!("Invoke-WebRequest -Uri '{url}' -OutFile '{d}'");
        if try_cmd("powershell", &["-NoProfile", "-Command", &ps])? {
            return Ok(());
        }
    }
    Ok(())
}

/// Ok(Some(stderr)) when curl failed, Ok(None) when it worked, Err when curl is absent.
fn curl_to(url: &str, dest: &str, ua: &str) -> Result<Option<String>> {
    match Command::new("curl")
        .args(["-fL", "--retry", "2", "--max-time", "900", "-A", ua, "-o", dest, url])
        .output()
    {
        Ok(out) if out.status.success() => Ok(None),
        Ok(out) => Ok(Some(String::from_utf8_lossy(&out.stderr).to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(e.into()),
        Err(e) => Err(e.into()),
    }
}

fn last_line(s: &str) -> String {
    s.lines().filter(|l| !l.trim().is_empty()).last().unwrap_or("no output").trim().to_string()
}

/// Download a catalog entry and sideload it.
pub fn install_catalog(adb: &crate::adb::Adb, name: &str) -> Result<()> {
    install_catalog_with(adb, name, false)
}

/// As install_catalog, but a signature clash may remove the existing copy first.
pub fn install_catalog_with(adb: &crate::adb::Adb, name: &str, replace: bool) -> Result<()> {
    let e = crate::catalog::find(name)
        .ok_or_else(|| anyhow::anyhow!("unknown app '{name}'. known: {}", crate::catalog::names()))?;
    let abis = crate::catalog::abis(adb);
    let abis: Vec<&str> = abis.iter().map(String::as_str).collect();
    let url = crate::catalog::url_for(e, &abis)?;
    let dest = std::env::temp_dir().join("atvtools").join(format!("{}.apk", e.name));
    fetch(&url, &dest)?;
    crate::apps::install_with(adb, &[dest], replace)
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
