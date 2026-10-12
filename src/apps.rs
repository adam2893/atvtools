//! Package management: list / install / uninstall / enable / disable / backup.

use crate::adb::Adb;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scope {
    ThirdParty,
    System,
    All,
}

pub fn list(adb: &Adb, scope: Scope) -> Result<Vec<String>> {
    let flag = match scope {
        Scope::ThirdParty => "-3",
        Scope::System => "-s",
        Scope::All => "",
    };
    Ok(parse_package_list(&adb.shell(&format!("pm list packages {flag}"))?))
}

pub fn parse_package_list(raw: &str) -> Vec<String> {
    let mut v: Vec<String> = raw
        .lines()
        .filter_map(|l| l.trim().strip_prefix("package:"))
        .map(str::to_string)
        .collect();
    v.sort();
    v.dedup();
    v
}

pub fn install(adb: &Adb, apks: &[PathBuf]) -> Result<()> {
    install_with(adb, apks, false)
}

/// -r reinstalls, -g grants runtime permissions (both used by the original).
/// With replace, a signature clash (another build of the same package is already
/// installed - the official Shizuku next to the fork, for example) removes the
/// existing package for user 0 and installs again.
pub fn install_with(adb: &Adb, apks: &[PathBuf], replace: bool) -> Result<()> {
    if apks.is_empty() {
        bail!("no apk given");
    }
    for apk in apks {
        let path = apk.to_string_lossy().to_string();
        let install = |p: &str| adb.output(&["install", "-r", "-g", p]);
        let (ok, stdout, stderr) = install(&path)?;
        let out = format!("{}{}", stdout.trim(), stderr.trim());
        if let Some(pkg) = mismatch_package(&out) {
            if !replace {
                bail!(
                    "{pkg} is already installed from a different build.
  -> re-run with --replace to remove that copy first, or run: atvtools apps uninstall {pkg}
  adb said: {out}"
                );
            }
            remove_any_version(adb, &pkg)?;
            let (ok2, stdout2, stderr2) = install(&path)?;
            let again = format!("{}{}", stdout2.trim(), stderr2.trim());
            if !ok2 {
                bail!("reinstalling {} after removing {pkg} also failed: {again}", apk.display());
            }
            println!("{}: {} (replaced the previously installed {pkg})", apk.display(), again);
            continue;
        }
        if !ok {
            bail!("adb install -r -g {} failed: {out}", apk.display());
        }
        println!("{}: {}", apk.display(), out);
    }
    Ok(())
}

/// The package named by INSTALL_FAILED_UPDATE_INCOMPATIBLE.
pub fn mismatch_package(out: &str) -> Option<String> {
    if !out.contains("signatures do not match") && !out.contains("INSTALL_FAILED_UPDATE_INCOMPATIBLE") {
        return None;
    }
    const MARK: &str = "Existing package ";
    let at = out.find(MARK)? + MARK.len();
    let name: String = out[at..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// User 0 first: it also clears packages that cannot be removed outright.
fn remove_any_version(adb: &Adb, pkg: &str) -> Result<()> {
    let _ = adb.try_out(&["shell", &format!("pm uninstall --user 0 {pkg}")]);
    let _ = adb.try_out(&["uninstall", pkg]);
    Ok(())
}

pub fn uninstall(adb: &Adb, pkgs: &[String]) -> Result<()> {
    for p in pkgs {
        println!("{p}: {}", adb.try_out(&["shell", &format!("pm uninstall --user 0 {p}")]).trim());
    }
    Ok(())
}

pub fn set_enabled(adb: &Adb, pkgs: &[String], enable: bool) -> Result<()> {
    for p in pkgs {
        let cmd = if enable {
            format!("pm enable {p}")
        } else {
            format!("pm disable-user --user 0 {p}")
        };
        println!("{p}: {}", adb.try_out(&["shell", &cmd]).trim());
    }
    Ok(())
}

/// Pulls every split APK of each package into dir; returns written paths.
pub fn backup(adb: &Adb, pkgs: &[String], dir: &Path) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let mut written = Vec::new();
    for p in pkgs {
        let paths = parse_pm_path(&adb.try_out(&["shell", &format!("pm path {p}")]));
        if paths.is_empty() {
            println!("{p}: no apk path (installed for another user?)");
            continue;
        }
        for (i, remote) in paths.iter().enumerate() {
            let name = if paths.len() == 1 { format!("{p}.apk") } else { format!("{p}.{i}.apk") };
            let dest = dir.join(name);
            adb.out(&["pull", remote, &dest.to_string_lossy()])?;
            written.push(dest);
        }
    }
    Ok(written)
}

pub fn parse_pm_path(raw: &str) -> Vec<String> {
    raw.lines()
        .filter_map(|l| l.trim().strip_prefix("package:"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// A package row from pm list packages -3 -f -i.
#[derive(Debug, Clone, PartialEq)]
pub struct PkgRow {
    pub package: String,
    pub apk_path: Option<String>,
    pub installer: Option<String>,
}

pub fn list_rows(adb: &Adb, scope: Scope, paths: bool, installers: bool) -> Result<Vec<PkgRow>> {
    let mut args: Vec<String> = vec!["shell".into(), "pm list packages".into()];
    args[1] = match scope {
        Scope::ThirdParty => "pm list packages -3".into(),
        Scope::System => "pm list packages -s".into(),
        Scope::All => "pm list packages".into(),
    };
    if paths {
        args[1].push_str(" -f");
    }
    if installers {
        args[1].push_str(" -i");
    }
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    Ok(parse_rows(&adb.out(&refs)?))
}

pub fn parse_rows(raw: &str) -> Vec<PkgRow> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("package:") else { continue };
        let mut path = None;
        let mut installer = None;
        let mut package = rest;
        if let Some((left, right)) = rest.rsplit_once(" installer=") {
            installer = Some(right.trim().to_string());
            package = left;
        }
        if let Some((left, right)) = package.rsplit_once('=') {
            if right.contains('.') || left.contains(".apk") {
                path = Some(left.trim().to_string());
                package = right;
            }
        }
        out.push(PkgRow {
            package: package.split_whitespace().next().unwrap_or(package).to_string(),
            apk_path: path,
            installer,
        });
    }
    out.sort_by(|a, b| a.package.cmp(&b.package));
    out.dedup_by(|a, b| a.package == b.package);
    out
}

/// pm list packages -3 -i, filtered to Play Store installs (the original's
/// "user apps that were installed only from Play Store" list).
pub fn play_store_packages(adb: &Adb) -> Result<Vec<String>> {
    let rows = list_rows(adb, Scope::ThirdParty, false, true)?;
    Ok(rows
        .into_iter()
        .filter(|r| matches!(r.installer.as_deref(), Some("com.android.vending") | Some("com.google.android.feedback")))
        .map(|r| r.package)
        .collect())
}

/// Package name, version and install facts, filtered from dumpsys.
pub fn info(adb: &Adb, package: &str) -> Result<String> {
    let raw = adb.try_out(&["shell", &format!("dumpsys package {package}")]);
    if !raw.contains(package) {
        bail!("{package} not installed");
    }
    let keep = ["versionName=", "versionCode=", "codePath=", "installerPackageName=", "firstInstallTime=", "lastUpdateTime="];
    let mut out = String::new();
    for line in raw.lines() {
        let t = line.trim();
        if keep.iter().any(|k| t.starts_with(k)) {
            out.push_str(t);
            out.push('\n');
        }
    }
    if out.is_empty() {
        out = raw.lines().take(5).collect::<Vec<_>>().join("\n");
    }
    Ok(out)
}

/// Launch an installed user app through its launcher category.
pub fn launch(adb: &Adb, package: &str) -> Result<()> {
    let out = adb.try_out(&["shell", &format!("monkey -p {package} -c android.intent.category.LAUNCHER 1")]);
    if out.contains("No activities found") {
        bail!("{package} has no launcher activity");
    }
    println!("{}", out.lines().last().unwrap_or("").trim());
    Ok(())
}

pub fn force_stop(adb: &Adb, package: &str) -> Result<()> {
    adb.shell(&format!("am force-stop {package}"))?;
    Ok(())
}

pub fn grant(adb: &Adb, package: &str, permission: &str, allow: bool) -> Result<()> {
    let verb = if allow { "grant" } else { "revoke" };
    let out = adb.try_out(&["shell", &format!("pm {verb} {package} {permission}")]);
    println!("{package} {permission}: {}", out.trim());
    Ok(())
}

/// appops-based grants (external storage / write system settings), which is
/// what the original used for "grant write permissions on external USB".
pub fn appop(adb: &Adb, package: &str, op: &str, allow: bool) -> Result<()> {
    let mode = if allow { "allow" } else { "deny" };
    let out = adb.try_out(&["shell", &format!("appops set {package} {op} {mode}")]);
    println!("{package} {op}={mode}: {}", out.trim());
    Ok(())
}

/// Packages that request a permission. One dumpsys call, parsed locally.
pub fn with_permission(adb: &Adb, permission: &str) -> Result<Vec<String>> {
    Ok(parse_permission_scan(&adb.shell("dumpsys package")?, permission))
}

pub fn parse_permission_scan(raw: &str, permission: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    let mut in_requested = false;
    for line in raw.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("Package [") {
            // real dumpsys form: "Package [com.foo] (a1b2c3):"
            current = rest.split(']').next().map(|s| s.trim().to_string());
            in_requested = false;
            continue;
        }
        if t.starts_with("requested permissions:") {
            in_requested = true;
            continue;
        }
        if in_requested {
            if t == permission || t.ends_with(&format!(" {permission}")) || t.ends_with(&format!(":{permission}")) {
                if let Some(c) = &current {
                    out.push(c.clone());
                }
                in_requested = false;
            } else if !t.contains("permission") {
                in_requested = false;
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Pull every third-party APK (the original's "extract all apps".
pub fn backup_all(adb: &Adb, dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>> {
    let pkgs = list(adb, Scope::ThirdParty)?;
    if pkgs.is_empty() {
        println!("no third-party packages found");
    }
    backup(adb, &pkgs, dir)
}

/// Every .apk inside a directory, sorted - used for bulk install and restore.
pub fn apks_in_dir(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>> {
    let mut v: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("apk")).unwrap_or(false))
        .collect();
    v.sort();
    Ok(v)
}

/// Launcher (HOME) activities, for "set any launcher as default".
pub fn home_activities(adb: &Adb) -> Result<Vec<String>> {
    let raw = adb.try_out(&["shell", "cmd package query-activities -a android.intent.action.MAIN -c android.intent.category.HOME"]);
    Ok(parse_home_activities(&raw))
}

pub fn parse_home_activities(raw: &str) -> Vec<String> {
    let mut v: Vec<String> = raw
        .split_whitespace()
        .filter_map(|t| t.strip_prefix("name="))
        .filter(|s| s.contains('/'))
        .map(str::to_string)
        .collect();
    v.sort();
    v.dedup();
    v
}

pub fn set_home(adb: &Adb, component: &str) -> Result<()> {
    let out = adb.try_out(&["shell", &format!("cmd package set-home-activity {component}")]);
    println!("{}", out.trim());
    Ok(())
}

/// pm clear - wipes app data and cache (the script's "delete data and cache").
pub fn clear(adb: &Adb, package: &str) -> Result<()> {
    let out = adb.try_out(&["shell", &format!("pm clear {package}")]);
    println!("{package}: {}", out.trim());
    Ok(())
}

/// Play Store web page for a package; the original generated launcher shortcut
/// icons from a website that took exactly this pair (label + package).
pub fn store_url(package: &str) -> String {
    format!("https://play.google.com/store/apps/details?id={package}")
}

/// Label + store URL rows for every third-party package.
pub fn shortcuts(adb: &Adb) -> Result<Vec<(String, String)>> {
    Ok(list(adb, Scope::ThirdParty)?
        .into_iter()
        .map(|p| (p.clone(), store_url(&p)))
        .collect())
}

/// Open the Play Store listing on the device (Play-installable apps only).
pub fn open_store(adb: &Adb, package: &str) -> Result<()> {
    adb.shell(&format!("am start -a android.intent.action.VIEW -d market://details?id={package}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clashing_package_is_read_from_the_error() {
        let real = "Performing Streamed Installadb: failed to install /tmp/a.apk: Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: Existing package moe.shizuku.privileged.api signatures do not match newer version; ignoring!]";
        assert_eq!(mismatch_package(real).as_deref(), Some("moe.shizuku.privileged.api"));
        assert_eq!(mismatch_package("Success"), None);
    }

    #[test]
    fn store_url_is_built() {
        assert_eq!(store_url("com.a"), "https://play.google.com/store/apps/details?id=com.a");
    }

    #[test]
    fn rows_parse_paths_and_installers() {
        let raw = "package:/data/app/~~x/base.apk=com.a installer=com.android.vending\npackage:com.b installer=com.google.android.feedback\n";
        let r = parse_rows(raw);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].package, "com.a");
        assert_eq!(r[0].installer.as_deref(), Some("com.android.vending"));
        assert!(r[0].apk_path.as_deref().unwrap_or("").ends_with("base.apk"));
        assert_eq!(r[1].package, "com.b");
        assert!(r[1].apk_path.is_none());
    }

    #[test]
    fn permission_scan_finds_requesting_packages() {
        let raw = "  Package [com.a] (abc):\n    requested permissions:\n      android.permission.INTERNET\n      android.permission.WRITE_SECURE_SETTINGS\n  Package [com.b] (def):\n    requested permissions:\n      android.permission.INTERNET\n";
        assert_eq!(parse_permission_scan(raw, "android.permission.WRITE_SECURE_SETTINGS"), vec!["com.a"]);
        assert_eq!(parse_permission_scan(raw, "android.permission.INTERNET"), vec!["com.a", "com.b"]);
    }

    #[test]
    fn home_activities_are_parsed() {
        let raw = "    Activity #0:\n      name=com.a/.Home\n    Activity #1:\n      name=com.b/com.b.MainActivity\n";
        assert_eq!(parse_home_activities(raw), vec!["com.a/.Home", "com.b/com.b.MainActivity"]);
    }

    #[test]
    fn package_list_is_sorted_and_deduped() {
        let raw = "package:com.b\npackage:com.a\npackage:com.a\n";
        assert_eq!(parse_package_list(raw), vec!["com.a", "com.b"]);
    }

    #[test]
    fn pm_path_handles_splits() {
        let raw = "package:/data/app/~~x/com.a-y=/base.apk\npackage:/data/app/~~x/com.a-y=/split_config.arm64.apk\n";
        let p = parse_pm_path(raw);
        assert_eq!(p.len(), 2);
        assert!(p[0].ends_with("base.apk"));
        assert!(p[1].ends_with("split_config.arm64.apk"));
    }
}

#[cfg(all(test, unix))]
mod install_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A fake adb: rejects the install once with the real signature-mismatch text, and succeeds
    /// after anything ran an uninstall.
    fn fake_adb(dir: &std::path::Path) -> Adb {
        let marker = dir.join("uninstalled");
        let script = dir.join("adb");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\ncase \"$*\" in\n  *uninstall*) touch {}\n    echo Success\n    exit 0 ;;\n  *install*)\n    if [ -f {} ]; then echo 'Success'; exit 0; fi\n    echo 'adb: failed to install x: Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE: Existing package moe.shizuku.privileged.api signatures do not match newer version; ignoring!]' 1>&2\n    exit 1 ;;\nesac\necho ''\n",
                marker.display(),
                marker.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut adb = Adb::new(None);
        adb.serial = None;
        adb.bin = script.to_string_lossy().to_string();
        adb
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("atvtools-install-test-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_signature_mismatch_asks_for_replace_instead_of_dumping_adb_text() {
        let d = tmp("mismatch");
        let apk = d.join("shizuku-fork.apk");
        std::fs::write(&apk, b"apk").unwrap();
        let err = install_with(&fake_adb(&d), &[apk], false).unwrap_err().to_string();
        assert!(err.contains("--replace"), "{err}");
        assert!(err.contains("moe.shizuku.privileged.api"), "{err}");
        assert!(!d.join("uninstalled").exists(), "nothing may be removed without --replace");
    }

    #[test]
    fn replace_removes_the_other_build_and_installs() {
        let d = tmp("replace");
        let apk = d.join("shizuku-fork.apk");
        std::fs::write(&apk, b"apk").unwrap();
        install_with(&fake_adb(&d), &[apk], true).unwrap();
        assert!(d.join("uninstalled").exists(), "the old build should have been removed");
    }
}
