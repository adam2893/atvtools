//! Shizuku & Stellar (optional alternative) install + activation over adb.
//!
//! adb alone starts the server, but the app must be opened once on the TV
//! first so it writes its starter scripts into its external files dir.

use crate::adb::Adb;
use anyhow::{bail, Result};

/// Same application id as upstream Shizuku; fork-Shizuku keeps it.
pub const PACKAGE: &str = "moe.shizuku.privileged.api";
const DIR: &str = "/storage/emulated/0/Android/data/moe.shizuku.privileged.api";

/// Stellar (`roro2239/Stellar`) — optional Shizuku-compatible alternative.
pub const STELLAR_PACKAGE: &str = "roro.stellar.manager";
const STELLAR_DIR: &str = "/storage/emulated/0/Android/data/roro.stellar.manager";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Shizuku,
    Stellar,
}

impl Provider {
    pub fn label(self) -> &'static str {
        match self {
            Self::Shizuku => "shizuku",
            Self::Stellar => "stellar",
        }
    }

    pub fn package(self) -> &'static str {
        match self {
            Self::Shizuku => PACKAGE,
            Self::Stellar => STELLAR_PACKAGE,
        }
    }

    pub fn catalog_name(self) -> &'static str {
        match self {
            Self::Shizuku => "shizuku-fork",
            Self::Stellar => "stellar",
        }
    }

    pub fn dir(self) -> &'static str {
        match self {
            Self::Shizuku => DIR,
            Self::Stellar => STELLAR_DIR,
        }
    }

    pub fn process_needle(self) -> &'static str {
        match self {
            Self::Shizuku => "shizuku",
            Self::Stellar => "stellar",
        }
    }
}

pub fn installed_for(adb: &Adb, provider: Provider) -> Result<bool> {
    let pkg = provider.package();
    Ok(adb
        .out(&["shell", &format!("pm list packages {pkg}")])?
        .contains(pkg))
}

/// The server runs as a shell app_process, so any matching process counts.
pub fn running_for(adb: &Adb, provider: Provider) -> Result<bool> {
    let needle = provider.process_needle();
    let ps = adb.try_out(&["shell", "ps -A"]);
    let ps = if ps.contains(needle) {
        ps
    } else {
        adb.try_out(&["shell", "ps"])
    };
    Ok(ps.lines().any(|l| l.contains(needle)))
}

pub fn start(adb: &Adb) -> Result<()> {
    start_for(adb, Provider::Shizuku)
}

pub fn start_for(adb: &Adb, provider: Provider) -> Result<()> {
    let pkg = provider.package();
    let label = provider.label();
    if !installed_for(adb, provider)? {
        bail!(
            "{pkg} is not installed - run: atvtools install {}",
            provider.catalog_name()
        );
    }
    let script = format!("{}/start.sh", provider.dir());
    if !exists(adb, &script)? {
        bail!("{script} is missing - open {label} on the TV once, then retry");
    }
    println!("{}", adb.out(&["shell", &format!("sh {script}")])?.trim());
    if running_for(adb, provider)? {
        println!("{label} server: running");
    } else {
        println!("starter ran but no {label} process yet - open the app and press Start");
    }
    Ok(())
}

pub fn stop(adb: &Adb) -> Result<()> {
    stop_for(adb, Provider::Shizuku)
}

pub fn stop_for(adb: &Adb, provider: Provider) -> Result<()> {
    let script = format!("{}/stop.sh", provider.dir());
    if !exists(adb, &script)? {
        bail!(
            "{script} is missing - open {} on the TV once, then retry",
            provider.label()
        );
    }
    println!("{}", adb.out(&["shell", &format!("sh {script}")])?.trim());
    Ok(())
}

pub fn status(adb: &Adb) -> Result<()> {
    status_for(adb, Provider::Shizuku)
}

pub fn status_for(adb: &Adb, provider: Provider) -> Result<()> {
    let pkg = provider.package();
    let label = provider.label();
    let inst = installed_for(adb, provider)?;
    println!(
        "package:  {pkg} ({})",
        if inst { "installed" } else { "missing" }
    );
    println!(
        "server:   {}",
        if running_for(adb, provider)? {
            "running"
        } else {
            "stopped"
        }
    );
    if inst {
        let s = exists(adb, &format!("{}/start.sh", provider.dir()))?;
        println!(
            "start.sh: {}",
            if s {
                format!("present - run: atvtools {label} start")
            } else {
                "absent - open the app once".to_string()
            }
        );
    }
    Ok(())
}

fn exists(adb: &Adb, path: &str) -> Result<bool> {
    Ok(adb
        .try_out(&["shell", &format!("[ -f {path} ] && echo yes")])
        .trim()
        == "yes")
}
