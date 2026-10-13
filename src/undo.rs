//! Undo journal. Every batch that disables packages files one record, so the
//! previous state can be restored without remembering which preset did it.
//! Restoring moves the record aside, so repeated `undo` walks back in order.

use anyhow::Result;
use std::path::PathBuf;

pub struct Record {
    pub stamp: String,
    pub what: String,
    pub serial: String,
    pub packages: Vec<String>,
}

pub fn dir() -> Option<PathBuf> {
    crate::uad::home_dir().map(|h| h.join(".atvtools").join("undo"))
}

/// Files one batch. The stamp sorts chronologically and is the name on disk.
pub fn note(what: &str, serial: Option<&str>, packages: &[String]) -> Result<PathBuf> {
    let dir = dir().ok_or_else(|| anyhow::anyhow!("no home directory to keep the undo journal in"))?;
    std::fs::create_dir_all(&dir)?;
    let stamp = stamp();
    let path = dir.join(format!("{stamp}.json"));
    let body = serde_json::json!({
        "stamp": stamp,
        "what": what,
        "serial": serial.unwrap_or(""),
        "packages": packages,
    });
    std::fs::write(&path, serde_json::to_string_pretty(&body)?)?;
    Ok(path)
}

fn stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs:010}")
}

/// Every recorded batch, oldest first. Files ending in .undone.json are skipped.
pub fn entries() -> Vec<Record> {
    let Some(dir) = dir() else { return Vec::new() };
    let Ok(read) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut out: Vec<Record> = read
        .flatten()
        .filter(|e| {
            // Retired records stay on disk for the audit trail; .undone.json
            // also ends in .json, so it needs an explicit skip.
            let name = e.file_name().to_string_lossy().to_string();
            name.ends_with(".json") && !name.ends_with(".undone.json")
        })
        .filter_map(|e| {
            let stem = e.path().file_stem()?.to_string_lossy().to_string();
            let mut r = parse(&std::fs::read_to_string(e.path()).ok()?)?;
            r.stamp = stem;
            Some(r)
        })
        .collect();
    out.sort_by(|a, b| a.stamp.cmp(&b.stamp));
    out
}

fn parse(text: &str) -> Option<Record> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let packages: Vec<String> = v
        .get("packages")?
        .as_array()?
        .iter()
        .filter_map(|p| p.as_str().map(str::to_string))
        .collect();
    Some(Record {
        // Filled in by entries() from the file name, which is the stamp.
        stamp: String::new(),
        what: v.get("what").and_then(|s| s.as_str()).unwrap_or("disable").to_string(),
        serial: v.get("serial").and_then(|s| s.as_str()).unwrap_or("").to_string(),
        packages,
    })
}

/// The newest entry, or one named by its stamp prefix.
pub fn last(which: Option<&str>) -> Option<Record> {
    let all = entries();
    match which {
        Some(w) => all.into_iter().find(|r| r.stamp == w || r.stamp.starts_with(w)),
        None => all.into_iter().next_back(),
    }
}

/// Retires a record so the next `undo` moves on to the one before it.
pub fn retire(stamp: &str) -> Option<PathBuf> {
    let dir = dir()?;
    let from = dir.join(format!("{stamp}.json"));
    let to = dir.join(format!("{stamp}.undone.json"));
    std::fs::rename(&from, &to).ok()?;
    Some(to)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_record_round_trips() {
        let text = r#"{"what":"preset google-tv-ads","serial":"1.2.3.4:5555","packages":["a","b"]}"#;
        let r = parse(text).unwrap();
        assert_eq!(r.what, "preset google-tv-ads");
        assert_eq!(r.packages, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(r.serial, "1.2.3.4:5555");
    }

    #[test]
    fn a_record_without_packages_is_not_an_entry() {
        assert!(parse(r#"{"what":"x"}"#).is_none());
        assert!(parse("not json").is_none());
    }

    #[test]
    fn stamps_sort_chronologically_as_text() {
        let a = format!("{:010}", 1000);
        let b = format!("{:010}", 9999);
        assert!(a < b);
    }
}