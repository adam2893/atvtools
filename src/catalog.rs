//! Installer catalog: the third-party APKs the original tool fetches.
//! A url is either a real link or a "gh:owner/repo:stable|beta" template that is
//! resolved against the GitHub releases API at install time, so the newest
//! release is picked and the asset matching the TV's ABI is chosen.

use crate::adb::Adb;
use anyhow::{anyhow, Result};

pub struct Entry {
    pub name: &'static str,
    pub url: &'static str,
    pub note: &'static str,
}

pub const GITHUB_API: &str = "https://api.github.com/repos";

pub const CATALOG: &[Entry] = &[
    Entry { name: "smarttube", url: "gh:yuliskov/SmartTube:stable", note: "SmartTube stable - ad-free YouTube for TV (newest release for your TV's CPU)" },
    Entry { name: "smarttube-beta", url: "gh:yuliskov/SmartTube:beta", note: "SmartTube beta (newest pre-release)" },
    Entry { name: "aurora", url: "https://gitlab.com/-/project/6922885/uploads/2d29ca61494fbbddfc3221b2ffec79fd/AuroraStore_4.6.4.apk", note: "Aurora Store 4.6.4" },
    Entry { name: "aptoide", url: "https://pool.apk.aptoide.com/appstvcommunity/cm-aptoidetv-pt-522-69192608-2378269a780b89bbabeef71385194a14.apk", note: "Aptoide TV" },
    Entry { name: "flauncher", url: "https://gitlab.com/flauncher/flauncher/-/releases/0.18.0/downloads/flauncher-0.18.0.apk", note: "FLauncher 0.18.0" },
    Entry { name: "projectivy", url: "https://github.com/spocky/miproja1/releases/download/4.72/ProjectivyLauncher-4.72-c97-xda-release.apk", note: "Projectivy Launcher 4.72, XDA release (spocky/miproja1)" },
    Entry { name: "adguard-tv", url: "https://agrd.io/tvapk", note: "AdGuard for Android TV" },
    Entry { name: "launch-on-boot", url: "https://f-droid.org/repo/news.androidtv.launchonboot_12.apk", note: "Launch on Boot" },
    Entry { name: "shizuku-fork", url: "https://github.com/symbuzzer/fork-Shizuku/releases/download/v1.4.1/Shizuku-fork.apk", note: "fork-Shizuku 1.4.1 - Shizuku variant; activate with: atvtools shizuku start" },
    Entry { name: "nuvio", url: "https://github.com/NuvioMedia/NuvioTV/releases/download/1.0.0/app-full-arm64-v8a-release.apk", note: "Nuvio TV 1.0.0 - arm64-v8a build (NuvioMedia/NuvioTV)" },
    Entry { name: "nuvio-x86", url: "https://github.com/NuvioMedia/NuvioTV/releases/download/1.0.0/app-full-x86_64-release.apk", note: "Nuvio TV 1.0.0 - x86_64 build (Intel boxes, emulators)" },
    Entry { name: "ftvlaunchx", url: "https://github.com/codefaktor/FTVLaunchX/releases/download/v1.0.1/FTVLaunchX-1.0.1.apk", note: "FTVLaunchX" },
];

pub fn find(name: &str) -> Option<&'static Entry> {
    let want = name.trim().to_ascii_lowercase();
    CATALOG.iter().find(|e| e.name == want)
}

pub fn names() -> String {
    CATALOG.iter().map(|e| e.name).collect::<Vec<_>>().join(", ")
}

/// One release, newest first as the API returns them.
pub struct Release {
    pub prerelease: bool,
    pub tag: String,
    /// (file name, download url)
    pub assets: Vec<(String, String)>,
}

pub fn parse_releases(json: &str) -> Result<Vec<Release>> {
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| anyhow!("GitHub returned something that is not JSON ({e}); rate limit?"))?;
    let arr = v.as_array().ok_or_else(|| anyhow!("GitHub release list is not an array"))?;
    Ok(arr
        .iter()
        .map(|r| Release {
            prerelease: r["prerelease"].as_bool().unwrap_or(false),
            tag: r["tag_name"].as_str().unwrap_or_default().to_string(),
            assets: r["assets"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| {
                            Some((
                                x["name"].as_str()?.to_string(),
                                x["browser_download_url"].as_str()?.to_string(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect())
}

/// The ABI names a release asset may carry, mapped onto the CPU names Android reports.
pub fn abi_tag(abi: &str) -> Option<&'static str> {
    match abi {
        "arm64-v8a" => Some("arm64-v8a"),
        "armeabi-v7a" | "armeabi" => Some("armeabi-v7a"),
        "x86" | "x86_64" => Some("x86"),
        _ => None,
    }
}

/// Lower is better: the TV's own ABI first, then the rest of its ABI list, then a universal build.
pub fn rank_asset(asset: &str, abis: &[&str]) -> Option<usize> {
    for (i, a) in abis.iter().enumerate() {
        if let Some(tag) = abi_tag(a) {
            if asset.contains(tag) {
                return Some(i);
            }
        }
    }
    if asset.contains("universal") {
        Some(abis.len() + 1)
    } else {
        None
    }
}

/// stable = the newest published release, beta = the newest pre-release.
pub fn pick_release<'a>(rels: &'a [Release], beta: bool) -> Option<&'a Release> {
    rels.iter().find(|r| !r.assets.is_empty() && r.prerelease == beta)
}

pub fn pick_asset<'a>(rel: &'a Release, abis: &[&str]) -> Option<&'a str> {
    rel.assets
        .iter()
        .filter_map(|(name, url)| rank_asset(name, abis).map(|k| (k, url.as_str())))
        .min_by_key(|(k, _)| *k)
        .map(|(_, url)| url)
}

/// Expand a catalog url: "gh:owner/repo:stable|beta" is resolved now, a real url is returned as is.
pub fn resolve(url: &str, abis: &[&str]) -> Result<String> {
    let spec = match url.strip_prefix("gh:") {
        Some(s) => s,
        None => return Ok(url.to_string()),
    };
    let (repo, kind) = spec.split_once(':').unwrap_or((spec, "stable"));
    let json = crate::download::fetch_text(&format!("{GITHUB_API}/{repo}/releases?per_page=15"))?;
    let rels = parse_releases(&json)?;
    let rel = pick_release(&rels, kind == "beta")
        .ok_or_else(|| anyhow!("{repo} has no {kind} release with downloadable files"))?;
    let url = pick_asset(rel, abis).ok_or_else(|| {
        anyhow!(
            "{repo} {} has no build for this TV (abis: {})",
            rel.tag,
            if abis.is_empty() { "unknown".to_string() } else { abis.join(", ") }
        )
    })?;
    Ok(url.to_string())
}

pub fn url_for(entry: &Entry, abis: &[&str]) -> Result<String> {
    resolve(entry.url, abis)
}

/// The TV's supported ABIs, best first; empty when the device cannot be asked.
pub fn abis(adb: &Adb) -> Vec<String> {
    let mut out: Vec<String> = adb
        .try_out(&["shell", "getprop ro.product.cpu.abilist"])
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if out.is_empty() {
        let one = adb.try_out(&["shell", "getprop ro.product.cpu.abi"]).trim().to_string();
        if !one.is_empty() {
            out.push(one);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_is_case_insensitive() {
        assert_eq!(find("SmartTube").map(|e| e.name), Some("smarttube"));
        assert_eq!(find(" nuvio ").map(|e| e.name), Some("nuvio"));
        assert!(find("nope").is_none());
    }

    #[test]
    fn github_entries_resolve() {
        for e in CATALOG {
            assert!(
                e.url.starts_with("https://") || e.url.starts_with("gh:"),
                "{} has a strange url: {}",
                e.name,
                e.url
            );
        }
        assert_eq!(find("shizuku-fork").map(|e| e.url.ends_with("Shizuku-fork.apk")), Some(true));
        assert!(find("nuvio").unwrap().url.contains("arm64-v8a"));
    }

    const RELEASES: &str = r#"[
      {"tag_name":"32.56s","prerelease":false,"assets":[
        {"name":"SmartTube_stable_32.56_arm64-v8a.apk","browser_download_url":"https://x/stable-arm64.apk"},
        {"name":"SmartTube_stable_32.56_armeabi-v7a.apk","browser_download_url":"https://x/stable-arm32.apk"},
        {"name":"SmartTube_stable_32.56_universal.apk","browser_download_url":"https://x/stable-univ.apk"}]},
      {"tag_name":"32.56","prerelease":true,"assets":[
        {"name":"SmartTube_beta_32.56_arm64-v8a.apk","browser_download_url":"https://x/beta-arm64.apk"},
        {"name":"SmartTube_beta_32.56_x86.apk","browser_download_url":"https://x/beta-x86.apk"}]}
    ]"#;

    #[test]
    fn the_newest_matching_asset_is_picked() {
        let rels = parse_releases(RELEASES).unwrap();
        let stable = pick_release(&rels, false).unwrap();
        assert_eq!(stable.tag, "32.56s");
        assert_eq!(pick_asset(stable, &["arm64-v8a", "armeabi-v7a"]), Some("https://x/stable-arm64.apk"));
        assert_eq!(pick_asset(stable, &["armeabi-v7a"]), Some("https://x/stable-arm32.apk"));
        // an ABI list nothing in the release matches falls back to universal
        assert_eq!(pick_asset(stable, &["mips"]), Some("https://x/stable-univ.apk"));
        let beta = pick_release(&rels, true).unwrap();
        assert_eq!(beta.tag, "32.56");
        assert_eq!(pick_asset(beta, &["x86_64", "x86"]), Some("https://x/beta-x86.apk"));
    }

    #[test]
    fn a_plain_url_is_returned_untouched() {
        assert_eq!(resolve("https://example.tld/a.apk", &[]).unwrap(), "https://example.tld/a.apk");
    }

    #[test]
    fn abi_names_map_to_asset_tags() {
        assert_eq!(abi_tag("armeabi"), Some("armeabi-v7a"));
        assert_eq!(abi_tag("x86_64"), Some("x86"));
        assert_eq!(abi_tag("mips"), None);
    }
}
