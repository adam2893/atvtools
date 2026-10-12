//! Interactive remote control. One character per keypress on this terminal is
//! forwarded as an adb keyevent, so the whole thing works over any ssh or
//! terminal. Replaces the original's cmdmax.exe + generated RC_Emulator.bat
//! window pair, which only existed on Windows.

use crate::adb::Adb;
use crate::device;
use anyhow::Result;
use std::io::{self, Write};

/// Shorthand -> Android key name. Digits map to the number keycodes.
pub const SHORTHAND: &[(&str, &str)] = &[
    ("u", "up"), ("d", "down"), ("l", "left"), ("r", "right"),
    ("o", "ok"), ("e", "enter"), ("b", "back"), ("h", "home"),
    ("m", "menu"), ("s", "search"), ("p", "playpause"), ("x", "mute"),
    ("w", "wake"), ("z", "sleep"), ("v", "volup"), ("V", "voldown"),
];

/// Expand a shorthand string such as uudr into device key names.
/// Unknown characters are skipped; a digit becomes the matching keyevent.
pub fn expand(seq: &str) -> Vec<String> {
    let mut out = Vec::new();
    for c in seq.chars() {
        let s = c.to_string();
        if let Some((_, name)) = SHORTHAND.iter().find(|(k, _)| *k == s) {
            out.push((*name).to_string());
        } else if c.is_ascii_digit() {
            out.push((7 + c as u32 - '0' as u32).to_string());
        }
    }
    out
}

pub fn help() {
    let keys: Vec<String> = SHORTHAND.iter().map(|(k, v)| format!("{k}={v}")).collect();
    println!("  {}", keys.join("  "));
    println!("  t <text>  type text        url <addr>  open a link");
    println!("  q         quit");
}

/// Read commands until quit. A line of shorthand keys is forwarded as-is, so
/// holding uuuu walks the focus up four times.
pub fn run(adb: &Adb) -> Result<()> {
    help();
    let stdin = io::stdin();
    loop {
        print!("remote> ");
        io::stdout().flush()?;
        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        if line == "q" || line == "quit" {
            return Ok(());
        }
        if let Some(text) = line.strip_prefix("t ") {
            device::text(adb, &text.split(' ').map(str::to_string).collect::<Vec<_>>())?;
            continue;
        }
        if let Some(url) = line.strip_prefix("url ") {
            device::open_url(adb, url.trim())?;
            continue;
        }
        let keys = expand(&line);
        if keys.is_empty() {
            println!("  unknown key {line}");
            continue;
        }
        device::key(adb, &keys)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorthand_expands_to_key_names() {
        assert_eq!(expand("uod"), vec!["up", "ok", "down"]);
        assert_eq!(expand("2"), vec!["9"]);
        assert!(expand("??").is_empty());
    }
}
