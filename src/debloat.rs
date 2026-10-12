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

/// Numbered list, so the menu can be driven by number or by name.
pub fn numbered() -> String {
    PRESETS
        .iter()
        .enumerate()
        .map(|(i, p)| format!("  {}  {:<16} {}", i + 1, p.title, p.description))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A preset by 1-based number or by title.
pub fn find_any(input: &str) -> Option<&'static Preset> {
    let t = input.trim();
    if let Ok(n) = t.parse::<usize>() {
        return PRESETS.get(n.checked_sub(1)?);
    }
    find(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_findable_by_number_or_name() {
        assert_eq!(find_any("1").unwrap().title, "google-tv-ads");
        assert_eq!(find_any(" 2 ").unwrap().title, "youtube-tv");
        assert_eq!(find_any("YOUTUBE-TV").unwrap().title, "youtube-tv");
        assert!(find_any("0").is_none());
        assert!(find_any("9").is_none());
        assert!(find_any("nope").is_none());
        assert!(numbered().starts_with("  1  google-tv-ads"));
    }

    #[test]
    fn presets_are_findable_case_insensitively() {
        assert_eq!(find("GOOGLE-TV-ADS").unwrap().title, "google-tv-ads");
        assert!(find("nope").is_none());
    }
}
