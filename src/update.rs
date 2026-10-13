//! Version check. The original polled a Dropbox beacon ("version;zip_url;
//! changelog") and overwrote its own folder with whatever zip the beacon named
//! - which is how the "v5.0" download turned out to be an older 3.0 build.
//! This checks the project's own releases and never self-overwrites.

use anyhow::Result;
use std::fmt::Write as _;

pub const REPO_API: &str = "https://api.github.com/repos/adam2893/atvtools/releases/latest";

/// The old tool's beacon format, kept so an old beacon can still be read.
pub struct Beacon {
    pub version: String,
    pub url: String,
    pub changelog: String,
}

/// Pull "tag_name" out of a GitHub releases JSON body without a JSON dependency.
pub fn parse_tag(json: &str) -> Option<String> {
    let rest = &json[json.find("\"tag_name\"")?..];
    let rest = rest[rest.find(':')? + 1..].trim_start();
    let rest = rest.strip_prefix('"')?;
    Some(rest[..rest.find('"')?].trim_start_matches('v').to_string())
}

/// "4.0;https://.../v5.zip;https://.../thread" -> Beacon.
pub fn parse_beacon(text: &str) -> Option<Beacon> {
    let mut f = text.trim().split(';');
    let version = f.next()?.trim().to_string();
    if version.is_empty() {
        return None;
    }
    Some(Beacon {
        version,
        url: f.next().unwrap_or("").trim().to_string(),
        changelog: f.next().unwrap_or("").trim().to_string(),
    })
}

/// True when the remote build differs from the running one.
pub fn is_newer(current: &str, remote: &str) -> bool {
    let norm = |s: &str| s.trim().trim_start_matches('v').to_string();
    let (c, r) = (norm(current), norm(remote));
    if c == r {
        return false;
    }
    // "32.56s" -> [32, 56]: some projects suffix their tags (SmartTube), the
    // digits are what identifies the build.
    let nums = |s: &str| -> Vec<u32> {
        s.split('.')
            .map(|p| {
                let digits: String = p
                    .chars()
                    .skip_while(|c| !c.is_ascii_digit())
                    .take_while(char::is_ascii_digit)
                    .collect();
                digits.parse().unwrap_or(0)
            })
            .collect()
    };
    let (cv, rv) = (nums(&c), nums(&r));
    for i in 0..cv.len().max(rv.len()) {
        let (a, b) = (cv.get(i).copied().unwrap_or(0), rv.get(i).copied().unwrap_or(0));
        if a != b {
            return b > a;
        }
    }
    false
}

/// Check the release feed, or a legacy beacon URL when one is given.
pub fn check(current: &str, beacon: Option<&str>) -> Result<String> {
    let mut out = String::new();
    match beacon {
        Some(url) => {
            let body = crate::download::fetch_text(url)?;
            let b = parse_beacon(&body).ok_or_else(|| anyhow::anyhow!("unreadable beacon at {url}"))?;
            let _ = writeln!(out, "beacon version : {}", b.version);
            let _ = writeln!(out, "beacon payload : {}", b.url);
            if !b.changelog.is_empty() {
                let _ = writeln!(out, "changelog      : {}", b.changelog);
            }
            if is_newer(current, &b.version) {
                let _ = writeln!(out, "-> a newer build is advertised: {}", b.version);
            } else {
                let _ = writeln!(out, "-> up to date (running {current})");
            }
        }
        None => {
            // offline, or nothing published yet: neither is an error here
            let Ok(body) = crate::download::fetch_text(REPO_API) else {
                let _ = writeln!(out, "no release feed reachable (running {current})");
                return Ok(out);
            };
            match parse_tag(&body) {
                Some(tag) if is_newer(current, &tag) => {
                    let _ = writeln!(out, "{tag} is available (running {current}): https://github.com/adam2893/atvtools/releases");
                }
                Some(_) => {
                    let _ = writeln!(out, "up to date ({current})");
                }
                None => {
                    let _ = writeln!(out, "no release published yet (running {current})");
                }
            }
        }
    }
    Ok(out)
}

/// One catalog entry as it stands on the TV.
pub struct Row {
    pub name: String,
    pub package: Option<String>,
    pub installed: Option<String>,
    pub latest: Option<String>,
    pub newer: bool,
}

/// Catalog apps vs what the TV runs: the installed version comes from one
/// dumpsys per package that is actually there, the newest from the release
/// feed. An entry with no package id, or a feed that cannot be read, is left
/// unknown rather than reported as up to date.
pub fn catalog_status(adb: &crate::adb::Adb) -> Result<Vec<Row>> {
    let installed = crate::apps::list(adb, crate::apps::Scope::ThirdParty)?;
    let mut rows = Vec::new();
    for entry in crate::catalog::CATALOG {
        let package = crate::catalog::packages(entry.name)
            .iter()
            .find(|p| installed.iter().any(|i| i == *p))
            .map(|p| (*p).to_string());
        let version = package.as_ref().and_then(|p| crate::apps::installed_version(adb, p));
        let latest = crate::catalog::latest_tag(entry.url).ok().flatten();
        let newer = match (&version, &latest) {
            (Some(v), Some(l)) => is_newer(v, l),
            _ => false,
        };
        rows.push(Row { name: entry.name.to_string(), package, installed: version, latest, newer });
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_is_read_from_a_release_body() {
        let body = r#"{"url":"x","tag_name":"v4.1.0","prerelease":false}"#;
        assert_eq!(parse_tag(body).unwrap(), "4.1.0");
        assert!(parse_tag("{}").is_none());
    }

    #[test]
    fn beacon_fields_are_split() {
        let b = parse_beacon("4.0;https://x/v5.zip;https://x/thread\n").unwrap();
        assert_eq!(b.version, "4.0");
        assert_eq!(b.url, "https://x/v5.zip");
        assert_eq!(b.changelog, "https://x/thread");
        assert!(parse_beacon("").is_none());
    }

    #[test]
    fn a_suffix_letter_does_not_hide_the_build_number() {
        assert!(is_newer("32.55", "32.56s"));
        assert!(!is_newer("32.56s", "32.56"));
        assert!(is_newer("1.23.2", "1.24"));
    }

    #[test]
    fn comparison_uses_numbers_not_strings() {
        assert!(is_newer("4.0", "4.1"));
        assert!(is_newer("v3.0", "4.0"));
        assert!(!is_newer("4.0", "4.0"));
        assert!(!is_newer("4.0", "3.9"));
    }
}
