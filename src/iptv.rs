//! IPTV playlist helper. The original walked you through pasting an m3u list
//! into the TV; anything that reads playlists (TiviMate, Kodi, TDTChannels)
//! also accepts a direct VIEW intent, which is what this drives.

use crate::adb::Adb;
use crate::transfer;
use anyhow::Result;
use std::path::PathBuf;

/// Players we can hand a playlist to.
pub const PLAYERS: &[(&str, &str)] = &[
    ("tivimate", "ar.tvplayer.tv"),
    ("kodi", "org.xbmc.kodi"),
    ("tdtchannels", "com.tdtchannels.app"),
    ("vlc", "org.videolan.vlc"),
];

pub fn player(name: &str) -> Option<&'static str> {
    let n = name.trim().to_ascii_lowercase();
    PLAYERS.iter().find(|(k, _)| *k == n).map(|(_, p)| *p)
}

pub fn players() -> String {
    PLAYERS.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(", ")
}

/// Last path segment of the playlist URL, sanitised, always .m3u.
pub fn playlist_name(url: &str) -> String {
    let no_query = url.split(['?', '#']).next().unwrap_or(url);
    // drop scheme and host, so https://host/ has no filename at all
    let path = match no_query.split_once("://") {
        Some((_, rest)) => rest.splitn(2, '/').nth(1).unwrap_or(""),
        None => no_query,
    };
    let last = path.rsplit('/').find(|s| !s.is_empty()).unwrap_or("playlist");
    let cleaned: String = last
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if cleaned.is_empty() {
        return "playlist.m3u".to_string();
    }
    if cleaned.ends_with(".m3u") || cleaned.ends_with(".m3u8") {
        cleaned
    } else {
        format!("{cleaned}.m3u")
    }
}

/// Download the list on the PC, then push it into the TV's shared storage.
pub fn add(adb: &Adb, url: &str, name: Option<&str>) -> Result<String> {
    let file = name.map(str::to_string).unwrap_or_else(|| playlist_name(url));
    let file = if file.ends_with(".m3u") || file.ends_with(".m3u8") { file } else { format!("{file}.m3u") };
    let tmp: PathBuf = std::env::temp_dir().join("atvtools").join(&file);
    crate::download::fetch(url, &tmp)?;
    transfer::mkdir(adb, transfer::DOWNLOADS)?;
    let remote = format!("{}/{}", transfer::DOWNLOADS, file);
    transfer::push(adb, &tmp, &remote)?;
    Ok(remote)
}

pub fn list(adb: &Adb) -> Result<String> {
    Ok(adb.shell(&format!("ls -l {}/*.m3u*", transfer::DOWNLOADS))?)
}

/// Hand the playlist URL straight to the player, so it streams and refreshes
/// the list itself instead of playing a stale copy.
pub fn open(adb: &Adb, url: &str) -> Result<()> {
    adb.shell(&format!(
        "am start -a android.intent.action.VIEW -d '{url}' -t audio/x-mpegurl"
    ))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playlist_names_are_sanitised() {
        assert_eq!(playlist_name("https://x.tld/a/list.m3u"), "list.m3u");
        assert_eq!(playlist_name("https://x.tld/a/spain?token=1"), "spain.m3u");
        assert_eq!(playlist_name("https://x.tld/"), "playlist.m3u");
        assert_eq!(playlist_name("https://x.tld/canales:2024"), "canales_2024.m3u");
    }

    #[test]
    fn players_resolve_by_name() {
        assert_eq!(player("Kodi"), Some("org.xbmc.kodi"));
        assert!(players().contains("tivimate"));
    }
}
