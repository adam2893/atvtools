//! fork-Shizuku install + activation over adb.
//!
//! adb alone starts the Shizuku server, but the app must be opened once on the
//! TV first: it writes start.sh / stop.sh into its external files dir, and
//! those are the scripts the shell user is allowed to run.

use crate::adb::Adb;
use anyhow::{bail, Result};

/// Same application id as upstream Shizuku; fork-Shizuku keeps it.
pub const PACKAGE: &str = "moe.shizuku.privileged.api";
const DIR: &str = "/storage/emulated/0/Android/data/moe.shizuku.privileged.api";

pub fn installed(adb: &Adb) -> Result<bool> {
    Ok(adb.out(&["shell", &format!("pm list packages {PACKAGE}")])?.contains(PACKAGE))
}

/// The server runs as a shell app_process, so any shizuku process counts.
pub fn running(adb: &Adb) -> Result<bool> {
    let ps = adb.try_out(&["shell", "ps -A"]);
    let ps = if ps.contains("shizuku") { ps } else { adb.try_out(&["shell", "ps"]) };
    Ok(ps.lines().any(|l| l.contains("shizuku")))
}

pub fn start(adb: &Adb) -> Result<()> {
    if !installed(adb)? {
        bail!("{PACKAGE} is not installed - run: atvtools install shizuku-fork");
    }
    let script = format!("{DIR}/start.sh");
    if !exists(adb, &script)? {
        bail!("{script} is missing - open Shizuku on the TV once, then retry");
    }
    println!("{}", adb.out(&["shell", &format!("sh {script}")])?.trim());
    if running(adb)? {
        println!("shizuku server: running");
    } else {
        println!("starter ran but no shizuku process yet - open the app and press Start");
    }
    Ok(())
}

pub fn stop(adb: &Adb) -> Result<()> {
    let script = format!("{DIR}/stop.sh");
    if !exists(adb, &script)? {
        bail!("{script} is missing - open Shizuku on the TV once, then retry");
    }
    println!("{}", adb.out(&["shell", &format!("sh {script}")])?.trim());
    Ok(())
}

pub fn status(adb: &Adb) -> Result<()> {
    let inst = installed(adb)?;
    println!("package:  {PACKAGE} ({})", if inst { "installed" } else { "missing" });
    println!("server:   {}", if running(adb)? { "running" } else { "stopped" });
    if inst {
        let s = exists(adb, &format!("{DIR}/start.sh"))?;
        println!("start.sh: {}", if s { "present - run: atvtools shizuku start" } else { "absent - open the app once" });
    }
    Ok(())
}

fn exists(adb: &Adb, path: &str) -> Result<bool> {
    Ok(adb.try_out(&["shell", &format!("[ -f {path} ] && echo yes")]).trim() == "yes")
}
