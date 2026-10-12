//! Package metadata from 0x192/universal-android-debloater.
//!
//! UAD is not vendored into the binary: its list is about 800 KB and changes
//! often, so we cache the upstream JSON under ~/.atvtools and match it against
//! what the TV reports. That keeps the removal recommendations current without
//! shipping a new release.

use crate::adb::Adb;
use crate::apps::{self, Scope};
use crate::download;
use anyhow::{bail, Context, Result};
use std::path::PathBuf;

pub const URL: &str =
    "https://raw.githubusercontent.com/0x192/universal-android-debloater/main/resources/assets/uad_lists.json";

/// Removal levels, safest first; the index is the level rank.
pub const LEVELS: [&str; 4] = ["recommended", "advanced", "expert", "unsafe"];

#[derive(Debug, Clone, PartialEq)]
pub struct Uad {
    pub id: String,
    pub list: String,
    pub removal: String,
    pub description: String,
}

/// 0 = Recommended ... 3 = Unsafe; unknown values sort last.
pub fn rank(removal: &str) -> u8 {
    match removal {
        "Recommended" => 0,
        "Advanced" => 1,
        "Expert" => 2,
        _ => 3,
    }
}

pub fn level_max(level: &str) -> Option<u8> {
    match level.trim().to_ascii_lowercase().as_str() {
        "recommended" => Some(0),
        "advanced" => Some(1),
        "expert" => Some(2),
        "unsafe" | "any" | "all" => Some(3),
        _ => None,
    }
}

pub fn parse(json: &str) -> Result<Vec<Uad>> {
    let value: serde_json::Value =
        serde_json::from_str(json).context("UAD database is not valid JSON")?;
    let arr = value
        .as_array()
        .context("UAD database is not a JSON array")?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let Some(id) = item.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        out.push(Uad {
            id: id.to_string(),
            list: text(item, "list", "Misc"),
            removal: text(item, "removal", "Unsafe"),
            description: text(item, "description", "").trim().to_string(),
        });
    }
    if out.is_empty() {
        bail!("UAD database has no usable entries");
    }
    Ok(out)
}

fn text(item: &serde_json::Value, key: &str, fallback: &str) -> String {
    item.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or(fallback)
        .to_string()
}

pub fn cache_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".atvtools").join("uad_lists.json"))
}

/// Cached database; refresh re-downloads even when a cache exists.
pub fn load(refresh: bool) -> Result<Vec<Uad>> {
    let path = cache_path();
    if !refresh {
        if let Some(p) = path.as_ref() {
            if let Ok(text) = std::fs::read_to_string(p) {
                if let Ok(db) = parse(&text) {
                    return Ok(db);
                }
            }
        }
    }
    let text = download::fetch_text(URL).context("download the UAD package database")?;
    let db = parse(&text)?;
    if let Some(p) = path.as_ref() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, &text);
    }
    Ok(db)
}

pub fn find<'a>(db: &'a [Uad], package: &str) -> Option<&'a Uad> {
    db.iter().find(|e| e.id == package)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub package: String,
    pub list: String,
    pub removal: String,
    pub description: String,
}

/// Installed packages UAD flags, at or below the requested level, safest first.
pub fn hits(db: &[Uad], installed: &[String], max: u8) -> Vec<Hit> {
    let mut out: Vec<Hit> = installed
        .iter()
        .filter_map(|p| find(db, p))
        .filter(|e| rank(&e.removal) <= max)
        .map(|e| Hit {
            package: e.id.clone(),
            list: e.list.clone(),
            removal: e.removal.clone(),
            description: e.description.clone(),
        })
        .collect();
    out.sort_by(|a, b| {
        rank(&a.removal)
            .cmp(&rank(&b.removal))
            .then_with(|| a.package.cmp(&b.package))
    });
    out.dedup_by(|a, b| a.package == b.package);
    out
}

/// What the TV has installed, matched against the database.
pub fn scan(adb: &Adb, scope: Scope, level: &str, refresh: bool) -> Result<Vec<Hit>> {
    let max = level_max(level).ok_or_else(|| {
        anyhow::anyhow!("unknown level '{level}': use {}", LEVELS.join(", "))
    })?;
    let db = load(refresh)?;
    let installed = apps::list(adb, scope)?;
    Ok(hits(&db, &installed, max))
}

/// Upstream release asset name for the host OS (all three are published).
pub fn gui_asset() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("uad_gui-macos.tar.gz")
    } else if cfg!(target_os = "windows") {
        Some("uad_gui-windows.exe")
    } else if cfg!(target_os = "linux") {
        Some("uad_gui-linux.tar.gz")
    } else {
        None
    }
}

/// The executable inside that archive.
fn gui_exe_name() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("uad_gui-macos")
    } else if cfg!(target_os = "windows") {
        Some("uad_gui-windows.exe")
    } else if cfg!(target_os = "linux") {
        Some("uad_gui-linux")
    } else {
        None
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Where the upstream GUI is unpacked; never mixed with the database cache.
pub fn gui_dir() -> Option<PathBuf> {
    Some(home_dir()?.join(".atvtools").join("uad"))
}

/// Tag and download URL of the newest release that publishes `asset`.
pub fn find_gui_release(json: &str, asset: &str) -> Result<(String, String)> {
    let rels = crate::catalog::parse_releases(json)?;
    for r in &rels {
        if let Some((_, url)) = r.assets.iter().find(|(n, _)| n == asset) {
            return Ok((r.tag.clone(), url.clone()));
        }
    }
    bail!("no UAD release publishes {asset}")
}

/// Download the upstream GUI once and unpack it. Returns the executable path.
pub fn ensure_gui(refresh: bool) -> Result<PathBuf> {
    let dir = gui_dir().ok_or_else(|| anyhow::anyhow!("no HOME/USERPROFILE in the environment"))?;
    let exe = dir.join(gui_exe_name().ok_or_else(|| anyhow::anyhow!("no UAD build for this OS"))?);
    if exe.is_file() && !refresh {
        return Ok(exe);
    }
    let asset = gui_asset().ok_or_else(|| anyhow::anyhow!("no UAD build for this OS"))?;
    std::fs::create_dir_all(&dir)?;
    let json = download::fetch_text(&format!(
        "{}/0x192/universal-android-debloater/releases?per_page=10",
        crate::catalog::GITHUB_API
    ))?;
    let (tag, url) = find_gui_release(&json, asset)?;
    println!("UAD {tag}: {url}");
    let archive = dir.join(asset);
    download::fetch(&url, &archive)?;
    if asset.ends_with(".tar.gz") {
        let ok = std::process::Command::new("tar")
            .arg("-xzf")
            .arg(&archive)
            .arg("-C")
            .arg(&dir)
            .status()
            .context("run tar (is it on PATH?)")?;
        if !ok.success() {
            bail!("tar could not unpack {asset}");
        }
    }
    if !exe.is_file() {
        bail!("{asset} did not contain {}", exe.display());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755));
    }
    Ok(exe)
}

/// Directory holding the adb atvtools uses, so the GUI can be pointed at it.
/// Places adb usually lives, for launches from a Finder/dock context where
/// PATH is bare. Only used when the normal lookup finds nothing.
pub fn adb_candidates() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
        .iter()
        .filter_map(|v| std::env::var_os(v))
        .map(|v| PathBuf::from(v).join("platform-tools"))
        .collect();
    if let Some(home) = home_dir() {
        dirs.push(home.join("Library/Android/sdk/platform-tools"));
        dirs.push(home.join("Android/Sdk/platform-tools"));
    }
    dirs.extend(
        ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/opt/android-sdk/platform-tools"]
            .iter()
            .map(PathBuf::from),
    );
    dirs
}

pub fn adb_dir(bin: &str) -> Option<PathBuf> {
    let p = std::path::Path::new(bin);
    if p.is_absolute() || p.components().count() > 1 {
        return p.parent().map(|d| d.to_path_buf());
    }
    if let Some(path) = std::env::var_os("PATH") {
        if let Some(dir) = std::env::split_paths(&path)
            .map(|d| d.join(bin))
            .find(|c| c.is_file())
            .and_then(|c| c.parent().map(|d| d.to_path_buf()))
        {
            return Some(dir);
        }
    }
    let name = if cfg!(windows) { "adb.exe" } else { "adb" };
    adb_candidates().into_iter().find(|d| d.join(name).is_file())
}

/// PATH with that directory in front. UAD calls `Command::new("adb")` and
/// reports "ADB was not found" on a bare launch, so hand it the same binary.
pub fn path_with_adb(bin: &str, path: Option<&std::ffi::OsStr>) -> Option<std::ffi::OsString> {
    let dir = adb_dir(bin)?;
    let mut parts = vec![dir];
    if let Some(p) = path {
        parts.extend(std::env::split_paths(p));
    }
    std::env::join_paths(parts).ok()
}

/// UAD keeps its settings in the OS config dir; 0.5.1 panics on a stale file.
pub fn config_file() -> Option<PathBuf> {
    let base = if cfg!(target_os = "macos") {
        home_dir()?.join("Library/Application Support")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)?
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().unwrap_or_default().join(".config"))
    };
    Some(base.join("uad").join("config.toml"))
}

/// Move a config file the shipped binary refuses to parse out of the way;
/// UAD then writes a fresh one. Returns where the old file went.
fn quarantine_config(path: &std::path::Path) -> Option<PathBuf> {
    let backup = path.with_extension("toml.old");
    std::fs::rename(path, &backup).ok()?;
    Some(backup)
}

/// Run the upstream GUI and wait for it to close. First launch from a stale
/// config panics with "Invalid config file", and a bare PATH makes it report
/// "ADB: No such file or directory"; both are handled or explained here.
pub fn launch_gui(adb: &Adb, refresh: bool) -> Result<String> {
    let exe = ensure_gui(refresh)?;
    let config = config_file();
    let mut notes: Vec<String> = Vec::new();
    let mut attempt = 0;
    let status = loop {
        let mut cmd = std::process::Command::new(&exe);
        if let Some(p) = path_with_adb(&adb.bin, std::env::var_os("PATH").as_deref()) {
            cmd.env("PATH", p);
        }
        if let Some(serial) = adb.serial.as_deref() {
            cmd.env("ANDROID_SERIAL", serial);
        }
        let out = cmd
            .stderr(std::process::Stdio::piped())
            .output()
            .context("start the UAD GUI")?;
        let err = String::from_utf8_lossy(&out.stderr).to_string();
        if out.status.success() {
            break out.status;
        }
        attempt += 1;
        if attempt == 1 && err.contains("Invalid config file") {
            if let Some(path) = config.as_deref().filter(|p| p.is_file()) {
                if let Some(backup) = quarantine_config(path) {
                    notes.push(format!(
                        "moved the config this build cannot read to {} and started again",
                        backup.display()
                    ));
                    continue;
                }
            }
        }
        if err.contains("ADB: No such file") {
            notes.push(format!(
                "it could not find adb (looked on this PATH and in {}); relaunch with ADB=/absolute/path/to/adb",
                adb_candidates()
                    .iter()
                    .map(|d| d.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        } else if let Some(line) = err.lines().last() {
            notes.push(line.to_string());
        }
        break out.status;
    };
    notes.insert(
        0,
        format!(
            "UAD GUI exited ({status}); settings live in {} and the binary stays at {}",
            config.as_deref().map(|p| p.display().to_string()).unwrap_or_else(|| "?".into()),
            exe.display()
        ),
    );
    Ok(notes.join("\n"))
}
#[cfg(test)]
mod gui_tests {
    use super::*;

    const FIXTURE: &str = r#"[
      {"tag_name":"0.6.0","prerelease":false,"assets":[{"name":"uad_gui-linux.tar.gz","browser_download_url":"https://example/linux"}]},
      {"tag_name":"0.5.1","prerelease":false,"assets":[{"name":"uad_gui-macos.tar.gz","browser_download_url":"https://example/macos"}]}
    ]"#;

    #[test]
    fn the_asset_matches_the_host() {
        let asset = gui_asset().expect("a build for this OS");
        let exe = gui_exe_name().expect("an executable for this OS");
        if cfg!(target_os = "macos") {
            assert_eq!(asset, "uad_gui-macos.tar.gz");
            assert_eq!(exe, "uad_gui-macos");
        }
        assert_eq!(asset.ends_with(".tar.gz"), !cfg!(target_os = "windows"));
    }

    #[test]
    fn the_newest_release_publishing_the_asset_wins() {
        let (tag, url) = find_gui_release(FIXTURE, "uad_gui-macos.tar.gz").unwrap();
        assert_eq!(tag, "0.5.1");
        assert_eq!(url, "https://example/macos");
        assert!(find_gui_release(FIXTURE, "uad_gui-plan9.tar.gz").is_err());
    }

    #[test]
    fn adb_binary_directory_comes_first_on_path() {
        let joined = path_with_adb("/opt/homebrew/bin/adb", Some(std::ffi::OsStr::new("/usr/bin:/bin"))).unwrap();
        let parts: Vec<PathBuf> = std::env::split_paths(&joined).collect();
        assert_eq!(parts[0], PathBuf::from("/opt/homebrew/bin"));
        assert_eq!(parts[1], PathBuf::from("/usr/bin"));
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn gui_lives_beside_the_database_cache() {
        if let (Some(gui), Some(db)) = (gui_dir(), cache_path()) {
            assert_eq!(gui.parent(), db.parent());
        }
    }
    #[test]
    fn config_file_sits_under_the_platform_config_dir() {
        let f = config_file().expect("a config path");
        assert!(f.ends_with("uad/config.toml") || f.ends_with("uad\\config.toml"));
    }

    #[test]
    fn android_sdk_platform_tools_are_searched() {
        let dirs = adb_candidates();
        assert!(dirs.iter().any(|d| d.ends_with("platform-tools")));
        assert!(dirs.iter().any(|d| d == &PathBuf::from("/opt/homebrew/bin")));
    }

    #[test]
    fn a_stale_config_is_quarantined_not_deleted() {
        let dir = std::env::temp_dir().join(format!("atvtools-uad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = dir.join("config.toml");
        std::fs::write(&cfg, "[general]\ntheme = \"Auto\"\n").unwrap();
        let backup = quarantine_config(&cfg).expect("moved aside");
        assert!(!cfg.exists());
        assert_eq!(backup.file_name().unwrap(), "config.toml.old");
        assert!(std::fs::read_to_string(&backup).unwrap().contains("Auto"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"[
      {"id":"com.example.ads","list":"Oem","description":"Sells ads\nsecond line","dependencies":[],"neededBy":[],"labels":[],"removal":"Recommended"},
      {"id":"com.example.core","list":"Aosp","description":"Core","dependencies":[],"neededBy":[],"labels":[],"removal":"Expert"},
      {"id":"broken"}
    ]"#;

    #[test]
    fn entries_are_parsed_with_defaults() {
        let db = parse(FIXTURE).unwrap();
        assert_eq!(db.len(), 3);
        assert_eq!(db[0].removal, "Recommended");
        assert_eq!(db[2].id, "broken");
        assert_eq!(db[2].list, "Misc");
        assert_eq!(db[2].removal, "Unsafe");
    }

    #[test]
    fn junk_is_rejected() {
        assert!(parse("{\"nope\":1}").is_err());
    }

    #[test]
    fn levels_map_to_ranks() {
        assert_eq!(level_max("Recommended"), Some(0));
        assert_eq!(level_max("unsafe"), Some(3));
        assert_eq!(level_max("whatever"), None);
        assert!(rank("Expert") < rank("Unsafe"));
    }

    #[test]
    fn hits_respect_the_level_and_sort_safest_first() {
        let db = parse(FIXTURE).unwrap();
        let installed = vec![
            "com.example.core".to_string(),
            "com.example.ads".to_string(),
            "com.other".to_string(),
        ];
        let safe = hits(&db, &installed, 0);
        assert_eq!(safe.len(), 1);
        assert_eq!(safe[0].package, "com.example.ads");
        let all = hits(&db, &installed, 3);
        assert_eq!(all[0].package, "com.example.ads");
        assert_eq!(all[1].package, "com.example.core");
    }
}
