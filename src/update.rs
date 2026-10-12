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
    let nums = |s: &str| -> Vec<u32> { s.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
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
    fn comparison_uses_numbers_not_strings() {
        assert!(is_newer("4.0", "4.1"));
        assert!(is_newer("v3.0", "4.0"));
        assert!(!is_newer("4.0", "4.0"));
        assert!(!is_newer("4.0", "3.9"));
    }
}
