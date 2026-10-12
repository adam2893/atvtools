//! Installer catalog: the third-party APKs the original tool fetches.
//! URLs point at upstream release endpoints, not mirrors.

pub struct Entry {
    pub name: &'static str,
    pub url: &'static str,
    pub note: &'static str,
}

pub const CATALOG: &[Entry] = &[
    Entry { name: "smarttube", url: "https://kutt.it/stn_stable", note: "SmartTube stable (ad-free YouTube for TV)" },
    Entry { name: "smarttube-beta", url: "https://kutt.it/stn_beta", note: "SmartTube beta" },
    Entry { name: "smarttube-bridge-atv", url: "https://kutt.it/stn_bridge_atv", note: "SmartTube bridge for Android TV" },
    Entry { name: "smarttube-bridge-fire", url: "https://kutt.it/stn_bridge_amazon", note: "SmartTube bridge for Fire TV" },
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
    let n = name.trim().to_ascii_lowercase();
    CATALOG.iter().find(|e| e.name == n)
}

pub fn names() -> String {
    CATALOG.iter().map(|e| e.name).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_is_case_insensitive() {
        assert_eq!(find("SmartTube").unwrap().name, "smarttube");
        assert!(find("nope").is_none());
        assert!(names().contains("flauncher"));
    }

    #[test]
    fn github_entries_resolve() {
        for n in ["shizuku-fork", "nuvio", "nuvio-x86"] {
            let e = find(n).unwrap_or_else(|| panic!("{n} missing"));
            assert!(e.url.starts_with("https://github.com/"), "{n} url is {}", e.url);
        }
        assert!(find("nuvio").unwrap().url.contains("arm64-v8a"));
        assert!(find("shizuku-fork").unwrap().url.ends_with("Shizuku-fork.apk"));
    }
}
