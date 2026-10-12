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
