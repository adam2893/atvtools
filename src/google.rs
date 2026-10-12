//! Bare-device bootstrap. The original pulled the Google installer, Play Store,
//! Play Services and a TV launcher from XDA attachments; those links rot, so
//! this installs whatever you already have in one folder, in dependency order.
//! Google Play Services and the Store must be installed before the first sign
//! in, which is why the order matters.

use crate::adb::Adb;
use crate::apps;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// (role, filename fragments, in install order)
pub const ORDER: &[(&str, &[&str])] = &[
    ("google installer", &["googleinstaller", "google_installer", "google-installer"]),
    ("play services", &["playservices", "play-services", "gms"]),
    ("play store", &["playstore", "play-store", "play_store", "vending"]),
    ("launcher", &["launcher", "tvhome", "tv-home", "google-tv-home"]),
];

/// Match files to roles in ORDER; anything unmatched is appended verbatim so
/// nothing the user put in the folder is skipped.
pub fn plan(files: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let mut used = vec![false; files.len()];
    let mut out = Vec::new();
    for (role, needles) in ORDER {
        for (i, f) in files.iter().enumerate() {
            if used[i] {
                continue;
            }
            let name = f.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            if needles.iter().any(|n| name.contains(n)) {
                used[i] = true;
                out.push(((*role).to_string(), f.clone()));
            }
        }
    }
    for (i, f) in files.iter().enumerate() {
        if !used[i] {
            out.push(("extra".to_string(), f.clone()));
        }
    }
    out
}

pub fn install(adb: &Adb, dir: &Path) -> Result<()> {
    let files = crate::apk::apks(dir)?;
    if files.is_empty() {
        anyhow::bail!("no .apk files in {}", dir.display());
    }
    let plan = plan(&files);
    println!("install order:");
    for (role, f) in &plan {
        println!("  {role:<16} {}", f.display());
    }
    for (role, f) in plan {
        println!("-- {role}");
        apps::install(adb, &[f])?;
    }
    println!("now sign in on the TV: Play Store -> your account");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn plan_orders_google_stack_and_keeps_extras() {
        let files = vec![
            p("GoogleTVHome.apk"),
            p("com.android.vending_38.7.29.apk"),
            p("google-installer_3.0.apk"),
            p("my-app.apk"),
        ];
        let plan = plan(&files);
        let roles: Vec<&str> = plan.iter().map(|(r, _)| r.as_str()).collect();
        assert_eq!(roles, vec!["google installer", "play store", "launcher", "extra"]);
        assert!(plan[2].1.to_string_lossy().contains("GoogleTVHome"));
    }
}
