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

/// Ok(true) when the program exists and exited 0; Ok(false) when it is absent.
fn try_cmd(bin: &str, args: &[&str]) -> Result<bool> {
    match Command::new(bin).args(args).status() {
        Ok(st) => Ok(st.success()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
