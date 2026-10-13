//! PC <-> TV file transfer over adb. The original opened an FTP app on the TV
//! and ran a client on the PC; adb push/pull needs no extra app, no open port
//! on the TV, and works the same on macOS, Linux and Windows.

use crate::adb::Adb;
use anyhow::Result;
use std::path::Path;

/// Where sideloaded payloads land on the TV.
pub const DOWNLOADS: &str = "/sdcard/Download";

pub fn push(adb: &Adb, local: &Path, remote: &str) -> Result<()> {
    if !local.exists() {
        anyhow::bail!("no such file: {}", local.display());
    }
    let l = local.to_string_lossy().to_string();
    let out = adb.out(&["push", &l, remote])?;
    println!("{}", out.trim());
    Ok(())
}

pub fn pull(adb: &Adb, remote: &str, local: &Path) -> Result<()> {
    let l = local.to_string_lossy().to_string();
    let out = adb.out(&["pull", remote, &l])?;
    println!("{}", out.trim());
    Ok(())
}

pub fn mkdir(adb: &Adb, remote: &str) -> Result<()> {
    adb.shell(&format!("mkdir -p {remote}"))?;
    Ok(())
}

/// True when the device path is a directory (used to make --dir a deliberate choice).
pub fn is_dir(adb: &Adb, remote: &str) -> bool {
    adb.try_out(&["shell", &format!("[ -d {remote} ] && echo yes")])
        .contains("yes")
}

/// Long listing of a device directory, sizes in bytes.
pub fn ls(adb: &Adb, remote: &str) -> Result<String> {
    Ok(adb.shell(&format!("ls -l {remote}"))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloads_dir_is_the_shared_one() {
        assert_eq!(DOWNLOADS, "/sdcard/Download");
    }
}
