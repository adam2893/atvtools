//! Package presets for the clean-up / ad-removal workflows. Every package
//! listed here is toggled by the original v4.0 script.

pub struct Preset {
    pub title: &'static str,
    pub packages: &'static [&'static str],
    pub description: &'static str,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        title: "google-tv-ads",
        description: "Google TV launcher churn; enable again to restore the stock UI",
        packages: &[
            "com.google.android.apps.tv.launcherx",
            "com.google.android.tungsten.setupwraith",
            "com.google.android.tvlauncher",
            "com.google.android.leanbacklauncher",
        ],
    },
    Preset {
        title: "youtube-tv",
        description: "Stock YouTube on Android TV (use SmartTube instead)",
        packages: &["com.google.android.youtube.tv"],
    },
];

pub fn find(title: &str) -> Option<&'static Preset> {
    let t = title.trim().to_ascii_lowercase();
    PRESETS.iter().find(|p| p.title == t)
}

pub fn titles() -> String {
    PRESETS.iter().map(|p| p.title).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_findable_case_insensitively() {
        assert_eq!(find("GOOGLE-TV-ADS").unwrap().title, "google-tv-ads");
        assert!(find("nope").is_none());
    }
}
