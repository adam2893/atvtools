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

/// Runs a macro file, one command per line:
///
/// ```text
/// home          # a key name, or shorthand like uudr
/// delay 500     # milliseconds, capped at 60s
/// text hello    # typed on the device
/// url http://x  # opened on the device
/// ```
pub fn script(adb: &Adb, path: &std::path::Path) -> Result<String> {
    let text = std::fs::read_to_string(path)?;
    let (mut steps, mut keys) = (0usize, 0usize);
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        steps += 1;
        let (word, rest) = match line.split_once(char::is_whitespace) {
            Some((w, r)) => (w, r.trim()),
            None => (line, ""),
        };
        match word.to_ascii_lowercase().as_str() {
            "delay" | "sleep" => {
                let ms: u64 = rest
                    .parse()
                    .map_err(|_| anyhow::anyhow!("line {}: delay needs milliseconds, got '{rest}'", n + 1))?;
                std::thread::sleep(std::time::Duration::from_millis(ms.min(60_000)));
            }
            "text" => device::text(adb, &rest.split(' ').map(str::to_string).collect::<Vec<_>>())?,
            "url" => device::open_url(adb, rest)?,
            _ if rest.is_empty() => {
                let names = if device::keycode(word).is_some() { vec![word.to_string()] } else { expand(word) };
                if names.is_empty() {
                    anyhow::bail!("line {}: unknown key '{word}'", n + 1);
                }
                keys += names.len();
                device::key(adb, &names)?;
            }
            _ => anyhow::bail!("line {}: unknown macro command '{word} {rest}'", n + 1),
        }
    }
    Ok(format!("ran {steps} line(s), {keys} key event(s)"))
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

const PAGE: &str = r#"<!doctype html><html><head><meta name=viewport content="width=device-width,initial-scale=1"><title>atvtools remote</title><style>
body{background:#111;color:#eee;font:16px/1.4 system-ui;margin:0;padding:16px;text-align:center}
a{display:inline-block;min-width:70px;padding:18px 14px;margin:4px;background:#2a2a2a;color:#eee;text-decoration:none;border-radius:10px}
a:active{background:#3d6}h1{font-size:14px;font-weight:400;color:#888}
</style></head><body><h1>atvtools remote</h1>
<div><a href="/k/power">power</a><a href="/k/wake">wake</a><a href="/k/mute">mute</a></div>
<div><a href="/k/volup">vol +</a><a href="/k/home">home</a><a href="/k/voldown">vol -</a></div>
<div><a href="/k/up">up</a></div>
<div><a href="/k/left">left</a><a href="/k/ok">ok</a><a href="/k/right">right</a></div>
<div><a href="/k/down">down</a></div>
<div><a href="/k/back">back</a><a href="/k/menu">menu</a><a href="/k/playpause">play</a></div>
<div><a href="/k/channelup">ch +</a><a href="/k/tv_input">input</a><a href="/k/channeldown">ch -</a></div>
</body></html>"#;

/// A phone-friendly D-pad served from this PC: open the printed address in any
/// browser on the same network and each tap becomes a keyevent on the TV.
/// Hand-rolled on std::net so it costs no dependency and no separate app.
pub fn serve(adb: &Adb, addr: &str) -> Result<()> {
    let listener = std::net::TcpListener::bind(addr)
        .map_err(|e| anyhow::anyhow!("cannot listen on {addr}: {e}"))?;
    let port = listener.local_addr()?.port();
    println!("d-pad on http://{}:{port}/ -- open it on the phone (ctrl-c to stop)", lan_ip());
    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mut buf = [0u8; 2048];
        let n = io::Read::read(&mut stream, &mut buf).unwrap_or(0);
        let head = String::from_utf8_lossy(&buf[..n]).into_owned();
        let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
        let body = reply(adb, &path);
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    }
    Ok(())
}

/// "/" serves the pad; "/k/<key>" presses one key or shorthand string.
fn reply(adb: &Adb, path: &str) -> String {
    let Some(key) = path.strip_prefix("/k/") else {
        return PAGE.to_string();
    };
    let key = key.trim_end_matches('/');
    let keys = expand(key);
    let what = if keys.is_empty() {
        "unknown key".to_string()
    } else {
        match device::key(adb, &keys) {
            Ok(()) => "ok".to_string(),
            Err(e) => e.to_string(),
        }
    };
    println!("{key}: {}", what.trim());
    format!("<meta http-equiv=refresh content=0;url=/><p>{key}</p>")
}

/// The PC address the phone should use; a UDP connect reveals the LAN route.
fn lan_ip() -> String {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0");
    let addr = sock.and_then(|s| {
        s.connect("8.8.8.8:80")?;
        s.local_addr()
    });
    addr.map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
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

    #[test]
    fn the_page_route_is_not_a_key_press() {
        let adb = Adb::new(None);
        assert!(reply(&adb, "/").contains("d-pad") || reply(&adb, "/").contains("href"));
        assert_eq!(lan_ip().split('.').count(), 4);
    }

    #[test]
    fn every_shorthand_character_is_a_real_key() {
        for (_, name) in SHORTHAND {
            assert!(device::keycode(name).is_some(), "{name} is not in device::KEYS");
        }
    }
}
