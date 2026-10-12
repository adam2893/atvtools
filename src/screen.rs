//! Screenshots and screen recording (screencap / screenrecord + pull).

use crate::adb::Adb;
use anyhow::{bail, Result};
use std::path::Path;

pub fn screenshot(adb: &Adb, dest: &Path) -> Result<()> {
    let remote = "/sdcard/atvtools_shot.png";
    adb.shell(&format!("screencap -p {remote}"))?;
    adb.out(&["pull", remote, &dest.to_string_lossy()])?;
    adb.try_out(&["shell", &format!("rm -f {remote}")]);
    Ok(())
}

pub fn record(adb: &Adb, seconds: u32, dest: &Path) -> Result<()> {
    let remote = "/sdcard/atvtools_rec.mp4";
    adb.shell(&format!("screenrecord --time-limit {seconds} {remote}"))?;
    adb.out(&["pull", remote, &dest.to_string_lossy()])?;
    adb.try_out(&["shell", &format!("rm -f {remote}")]);
    Ok(())
}

/// scrcpy is an external binary; the original bundled a Windows copy, we just
/// launch whatever is on PATH (or SCRCPY).
pub fn scrcpy(serial: Option<&str>, extra: &[String]) -> Result<()> {
    let bin = std::env::var("SCRCPY").unwrap_or_else(|_| "scrcpy".to_string());
    let mut c = std::process::Command::new(&bin);
    if let Some(s) = serial {
        c.args(["-s", s]);
    }
    c.args(extra);
    let st = c.status()?;
    if !st.success() {
        bail!("{bin} exited with {st}");
    }
    Ok(())
}
