//! Device logs: logcat (dump or live), the crash dropbox and full bugreports.
//! The original tool asked for a log file path and ran one unbounded logcat;
//! everything here is bounded by default so a dump cannot fill the terminal.

use crate::adb::Adb;
use anyhow::Result;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

pub const DEFAULT_LINES: usize = 400;

/// Reads the last `lines` logcat lines, optionally only from one tag and
/// only those containing `grep`. With follow, the stream is printed until
/// interrupted (the returned text is empty in that case).
pub fn logcat(adb: &Adb, tag: Option<&str>, grep: Option<&str>, follow: bool, lines: usize) -> Result<String> {
    let mut args: Vec<String> = vec!["logcat".into(), "-v".into(), "time".into()];
    if !follow {
        args.push("-d".into());
        args.push("-t".into());
        args.push(lines.to_string());
    }
    if let Some(t) = tag {
        args.push("-s".into());
        args.push(t.to_string());
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    if follow {
        return follow_logcat(adb, &args, grep);
    }
    Ok(keep_matching(&adb.out(&args)?, grep))
}

/// Streams logcat until Ctrl-C, printing only the lines that match.
fn follow_logcat(adb: &Adb, args: &[&str], grep: Option<&str>) -> Result<String> {
    let argv = adb.args_for(args);
    let mut child = std::process::Command::new(&adb.bin)
        .args(&argv)
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    let pipe = child.stdout.take().ok_or_else(|| anyhow::anyhow!("logcat produced no output stream"))?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in BufReader::new(pipe).lines() {
        let line = line?;
        if matches(line.as_str(), grep) {
            writeln!(out, "{line}")?;
        }
    }
    let _ = child.wait();
    Ok(String::new())
}

fn matches(line: &str, grep: Option<&str>) -> bool {
    match grep {
        Some(g) => line.contains(g),
        None => true,
    }
}

fn keep_matching(raw: &str, grep: Option<&str>) -> String {
    if grep.is_none() {
        return raw.to_string();
    }
    raw.lines().filter(|l| matches(l, grep)).map(|l| format!("{l}\n")).collect()
}

/// Crash reports out of the dropbox. The file names differ between Android
/// versions, so each known one is tried and the plain (unfiltered) dump is the
/// last resort - on an OEM build that renames them, that still shows something.
pub fn crash(adb: &Adb, grep: Option<&str>, lines: usize) -> Result<String> {
    for file in ["system_app_crash", "data_app_crash", "system_app_anr"] {
        let raw = adb.try_out(&["shell", &format!("dumpsys dropbox --print --file {file}")]);
        let hits = keep_matching(&raw, grep);
        if !raw.trim().is_empty() && raw.contains("Package:") {
            return Ok(tail(&hits, lines));
        }
    }
    let raw = adb.try_out(&["shell", "dumpsys dropbox --print"]);
    Ok(tail(&keep_matching(&raw, grep), lines))
}

fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    let mut out: String = all[start..].iter().map(|l| format!("{l}\n")).collect();
    if start > 0 {
        out = format!("... {} earlier lines omitted (...)\n{out}", start);
    }
    out
}

/// Full bugreport archive, written by the device into `dir`.
pub fn bugreport(adb: &Adb, dir: &Path) -> Result<String> {
    std::fs::create_dir_all(dir)?;
    Ok(adb.out(&["bugreport", &dir.to_string_lossy()])?.trim().to_string())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn only_matching_lines_survive_a_grep() {
        let raw = "01-01 10:00:00.000 I/A: keep me\n01-01 10:00:01.000 E/B: drop me\n";
        assert_eq!(keep_matching(raw, Some("keep")), "01-01 10:00:00.000 I/A: keep me\n");
        assert_eq!(keep_matching(raw, None), raw);
        assert!(keep_matching(raw, Some("nothing")).is_empty());
    }

    #[test]
    fn a_dump_is_cut_to_the_last_lines() {
        let raw = (1..=5).map(|i| format!("line {i}\n")).collect::<String>();
        let t = tail(&raw, 2);
        assert!(t.contains("line 4") && t.contains("line 5"));
        assert!(!t.contains("line 2"));
        assert!(t.contains("3 earlier lines omitted"));
    }
}