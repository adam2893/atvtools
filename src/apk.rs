//! APK metadata for local files, through the host's Android build-tools.
//! The original pushed aapt-arm-pie into /data/local/tmp and ran it on the TV
//! to read labels and versions; reading them on the PC covers the same need
//! (labelling a bulk sideload, checking what a downloaded APK really is)
//! without writing an executable into the device's tmp.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const HINT: &str =
    "install platform-tools/build-tools (brew install --cask android-commandlinetools, \
     apt install aapt, or the Android SDK), or set AAPT=/path/to/aapt2";

#[derive(Debug, PartialEq)]
pub struct Badging {
    pub package: String,
    pub label: String,
    pub version_name: String,
    pub version_code: String,
    pub sdk: String,
    pub target_sdk: String,
    pub launchable: Option<String>,
}

/// AAPT env var first, then aapt2, then aapt.
pub fn aapt_bin() -> Option<String> {
    if let Ok(p) = std::env::var("AAPT") {
        if !p.trim().is_empty() {
            return Some(p);
        }
    }
    for bin in ["aapt2", "aapt"] {
        let ok = Command::new(bin).arg("version").output();
        if ok.map(|o| o.status.success() || !o.stderr.is_empty()).unwrap_or(false) {
            return Some(bin.to_string());
        }
    }
    None
}

pub fn info(file: &Path) -> Result<Badging> {
    let bin = aapt_bin().ok_or_else(|| anyhow::anyhow!("no aapt/aapt2 on PATH - {HINT}"))?;
    if !file.exists() {
        bail!("no such file: {}", file.display());
    }
    let out = Command::new(&bin)
        .args(["dump", "badging", &file.to_string_lossy()])
        .output()?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    parse_badging(&text).ok_or_else(|| anyhow::anyhow!("{bin} could not read {}", file.display()))
}

/// Every .apk in a directory, sorted - the bulk-sideload worklist.
pub fn apks(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.extension().map(|x| x.eq_ignore_ascii_case("apk")).unwrap_or(false) {
            out.push(p);
        }
    }
    out.sort();
    Ok(out)
}

/// aapt dump badging output -> fields. Iterates lines in order so a label
/// containing a quote cannot derail the parser; aapt2 prints the same lines.
pub fn parse_badging(text: &str) -> Option<Badging> {
    let field = |prefix: &str| -> Option<String> {
        text.lines()
            .find(|l| l.starts_with(prefix))
            .and_then(|l| l.split_once(':').map(|(_, v)| v.trim().to_string()))
    };
    let package_line = field("package:")?;
    let attr = |line: &str, key: &str| -> String {
        let needle = format!("{key}='");
        line.find(&needle)
            .map(|i| line[i + needle.len()..].split('\'').next().unwrap_or("").to_string())
            .unwrap_or_default()
    };
    let launchable = text
        .lines()
        .find(|l| l.starts_with("launchable-activity:"))
        .map(|l| attr(l, "name"))
        .filter(|s| !s.is_empty());
    Some(Badging {
        package: attr(&package_line, "name"),
        version_code: attr(&package_line, "versionCode"),
        version_name: attr(&package_line, "versionName"),
        label: field("application-label:").unwrap_or_default().trim_matches('\'').to_string(),
        // aapt quotes numeric fields: sdkVersion:'21'
        sdk: field("sdkVersion:").unwrap_or_default().trim_matches('\'').to_string(),
        target_sdk: field("targetSdkVersion:").unwrap_or_default().trim_matches('\'').to_string(),
        launchable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "package: name='com.example.tv' versionCode='42' versionName='1.2.3' platformBuildVersionName='14'\n\
sdkVersion:'21'\ntargetSdkVersion:'33'\nuses-permission: name='android.permission.INTERNET'\n\
application-label:'Example TV'\napplication-icon-320:'res/mipmap.png'\n\
launchable-activity: name='com.example.tv.MainActivity'  label='' icon=''\n";

    #[test]
    fn badging_fields_are_extracted() {
        let b = parse_badging(SAMPLE).unwrap();
        assert_eq!(b.package, "com.example.tv");
        assert_eq!(b.version_name, "1.2.3");
        assert_eq!(b.version_code, "42");
        assert_eq!(b.label, "Example TV");
        assert_eq!(b.sdk, "21");
        assert_eq!(b.target_sdk, "33");
        assert_eq!(b.launchable.as_deref(), Some("com.example.tv.MainActivity"));
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(parse_badging("not an apk").is_none());
    }
}
