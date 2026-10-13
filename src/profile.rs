//! Expected state, in one small `.atvtools.toml`: what a TV should look like.
//! The reader takes a deliberate subset of TOML - sections, `key = value` with
//! strings, numbers and flat arrays - because the schema here is fixed and a
//! full parser would be a dependency for nothing.

use crate::adb::Adb;
use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Default, Debug)]
pub struct Profile {
    pub serial: Option<String>,
    pub density: Option<u32>,
    pub font_scale: Option<f32>,
    pub private_dns: Option<String>,
    pub screen_off_timeout: Option<u32>,
    pub install: Vec<String>,
    pub disable: Vec<String>,
    pub enable: Vec<String>,
}

pub fn default_path() -> PathBuf {
    PathBuf::from(".atvtools.toml")
}

pub fn load(path: &Path) -> Result<Profile> {
    let text = std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
    parse(&text)
}

pub fn parse(text: &str) -> Result<Profile> {
    let mut p = Profile::default();
    let mut section = String::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let bad = |why: String| anyhow::anyhow!("line {}: {why}", n + 1);
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.trim().to_ascii_lowercase();
            continue;
        }
        let (key, value) = line.split_once('=').ok_or_else(|| bad(format!("expected key = value, got '{line}'")))?;
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        let number = |what: &str| -> Result<u32> {
            value.parse().map_err(|_| anyhow::anyhow!("line {}: {what} needs a whole number, got '{value}'", n + 1))
        };
        match (section.as_str(), key.as_str()) {
            ("device", "serial") => p.serial = Some(unquote(value)?),
            ("settings", "density") => p.density = Some(number("density")?),
            ("settings", "font_scale") => {
                p.font_scale = Some(value.parse().map_err(|_| bad(format!("font_scale needs a number, got '{value}'")))?)
            }
            ("settings", "private_dns") => p.private_dns = Some(unquote(value)?),
            ("settings", "screen_off_timeout") => p.screen_off_timeout = Some(number("screen_off_timeout")?),
            ("apps", "install") => p.install = array(value)?,
            ("apps", "disable") => p.disable = array(value)?,
            ("apps", "enable") => p.enable = array(value)?,
            (s, k) => anyhow::bail!("line {}: unknown key {k} in [{s}]", n + 1),
        }
    }
    Ok(p)
}

fn unquote(v: &str) -> Result<String> {
    let t = v.trim();
    match t.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        Some(inner) => Ok(inner.to_string()),
        None => Ok(t.to_string()), // bare words are accepted too
    }
}

fn array(v: &str) -> Result<Vec<String>> {
    let t = v.trim();
    let inner = t
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .ok_or_else(|| anyhow::anyhow!("expected a [\"a\", \"b\"] list, got '{v}'"))?;
    inner
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(unquote)
        .collect()
}

fn list(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|i| format!("\"{i}\"")).collect();
    format!("[{}]", quoted.join(", "))
}

/// One line per change. `apply --dry-run` prints exactly this.
pub fn plan(p: &Profile) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(d) = p.density {
        out.push(format!("density {d}"));
    }
    if let Some(f) = p.font_scale {
        out.push(format!("font scale {f}"));
    }
    if let Some(h) = &p.private_dns {
        out.push(format!("private DNS {h}"));
    }
    if let Some(t) = p.screen_off_timeout {
        out.push(format!("screen off timeout {t}ms"));
    }
    for name in &p.install {
        out.push(format!("install {name}"));
    }
    if !p.enable.is_empty() {
        out.push(format!("enable {}", p.enable.join(", ")));
    }
    if !p.disable.is_empty() {
        out.push(format!("disable {}", p.disable.join(", ")));
    }
    out
}

/// Applies what the profile names, through the same functions the single
/// commands use - so a disable here lands in the undo journal like any other.
pub fn apply(adb: &Adb, p: &Profile) -> Result<()> {
    if let Some(d) = p.density {
        crate::settings::set_density(adb, d)?;
        println!("density: {d}");
    }
    if let Some(f) = p.font_scale {
        crate::settings::set_font_scale(adb, f)?;
        println!("font scale: {f}");
    }
    if let Some(h) = &p.private_dns {
        crate::settings::set_dns(adb, h)?;
        println!("private DNS: {h}");
    }
    if let Some(t) = p.screen_off_timeout {
        println!("screen off timeout: {}", crate::settings::set_idle_timeout(adb, t / 1000)?);
    }
    for name in &p.install {
        // Not replace: a profile never silently removes an existing install.
        crate::download::install_catalog_with(adb, name, false)?;
        println!("installed {name}");
    }
    if !p.enable.is_empty() {
        crate::apps::set_enabled(adb, &p.enable, true)?;
    }
    if !p.disable.is_empty() {
        crate::apps::set_enabled(adb, &p.disable, false)?;
        crate::journals(adb, "profile", &p.disable);
    }
    Ok(())
}

/// The TV as it is now, ready to write to a profile file.
pub fn export(adb: &Adb) -> Result<String> {
    let mut out = String::from("# written by atvtools preset export\n\n[device]\n");
    if let Some(s) = &adb.serial {
        out.push_str(&format!("serial = \"{s}\"\n"));
    }
    out.push_str("\n[settings]\n");
    if let Ok(d) = crate::settings::density(adb)?.trim().parse::<u32>() {
        out.push_str(&format!("density = {d}\n"));
    }
    if let Ok(f) = crate::settings::font_scale(adb)?.trim().parse::<f32>() {
        out.push_str(&format!("font_scale = {f}\n"));
    }
    if let Some(host) = specifier(&crate::settings::dns_state(adb)?) {
        out.push_str(&format!("private_dns = \"{host}\"\n"));
    }
    if let Ok(t) = crate::settings::screen_off_timeout(adb, None)?.trim().parse::<u32>() {
        out.push_str(&format!("screen_off_timeout = {t}\n"));
    }
    let installed: Vec<String> = crate::update::catalog_status(adb)?
        .into_iter()
        .filter(|r| r.installed.is_some())
        .map(|r| r.name)
        .collect();
    out.push_str("\n[apps]\n");
    out.push_str(&format!("install = {}\n", list(&installed)));
    let disabled = crate::apps::disabled(adb, crate::apps::Scope::ThirdParty)?;
    if !disabled.is_empty() {
        out.push_str(&format!("disable = {}\n", list(&disabled)));
    }
    Ok(out)
}

/// The hostname out of `mode=hostname specifier=dns.adguard.com`.
pub fn specifier(state: &str) -> Option<String> {
    let host = state
        .split_whitespace()
        .find_map(|t| t.strip_prefix("specifier="))?
        .trim();
    if host.is_empty() || host == "null" {
        None
    } else {
        Some(host.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_round_trips_through_the_parser() {
        let text = "# comment\n[device]\nserial = \"1.2.3.4:5555\"\n\n[settings]\ndensity = 320\nfont_scale = 1.5\nprivate_dns = \"dns.adguard.com\"\n\n[apps]\ninstall = [\"smarttube\", \"aurora\"]\ndisable = [\"flix.com.vision\"]\n";
        let p = parse(text).unwrap();
        assert_eq!(p.serial.as_deref(), Some("1.2.3.4:5555"));
        assert_eq!(p.density, Some(320));
        assert_eq!(p.font_scale, Some(1.5));
        assert_eq!(p.private_dns.as_deref(), Some("dns.adguard.com"));
        assert_eq!(p.install, vec!["smarttube", "aurora"]);
        assert_eq!(p.disable, vec!["flix.com.vision"]);
        assert_eq!(plan(&p).len(), 6);
    }

    #[test]
    fn nonsense_is_rejected_with_the_line_number() {
        let e = parse("[settings]\ndensity = big\n").unwrap_err().to_string();
        assert!(e.contains("line 2"), "{e}");
        let e = parse("[widgets]\nsize = 3\n").unwrap_err().to_string();
        assert!(e.contains("unknown key size"), "{e}");
        assert!(parse("[apps]\ninstall = smarttube\n").is_err());
    }

    #[test]
    fn an_empty_specifier_is_not_a_hostname() {
        assert_eq!(specifier("mode=hostname specifier=dns.adguard.com"), Some("dns.adguard.com".to_string()));
        assert_eq!(specifier("mode=null specifier=null"), None);
        assert_eq!(specifier("mode=opportunistic"), None);
    }
}