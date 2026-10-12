//! Interactive menu, mirroring the original tool's top level (A Remote Control,
//! B Remove ads, 1..8 the rest). Coloured with ANSI and localised, so the same
//! binary covers the original's _EN and _ES builds.

use crate::adb::Adb;
use crate::apps;
use crate::catalog;
use crate::debloat;
use crate::device;
use crate::net;
use crate::remote;
use crate::settings;
use anyhow::Result;
use std::io::{self, Write};

const TITLE: &str = "\x1b[36m";
const KEY: &str = "\x1b[33m";
const OK: &str = "\x1b[32m";
const DIM: &str = "\x1b[90m";
const RESET: &str = "\x1b[0m";

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Lang {
    En,
    Es,
}

impl Lang {
    pub fn parse(s: &str) -> Lang {
        match s.trim().to_ascii_lowercase().as_str() {
            "es" | "spa" | "espanol" | "español" | "spanish" => Lang::Es,
            _ => Lang::En,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Es => "es",
        }
    }

    fn table(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Lang::En => EN,
            Lang::Es => ES,
        }
    }

    /// Falls back to the key itself, so a missing translation is visible.
    pub fn t(self, key: &'static str) -> &'static str {
        self.table().iter().find(|(k, _)| *k == key).map(|(_, v)| *v).unwrap_or(key)
    }
}

const EN: &[(&str, &str)] = &[
    ("subtitle", "cross-platform rewrite"),
    ("remote", "Remote control"),
    ("adblock", "Remove ads"),
    ("files", "Send / fetch files"),
    ("apps", "Install, disable, back up apps"),
    ("settings", "Display, DNS, clock"),
    ("screen", "Screenshot, record, scrcpy"),
    ("helpers", "Install helper apps"),
    ("launcher", "Launcher"),
    ("optimize", "Optimize"),
    ("doctor", "Check the setup"),
    ("update", "Check for updates"),
    ("lang", "Cambiar a español"),
    ("quit", "quit"),
    ("unknown", "unknown option"),
    ("serial", "serial (empty = adb default)"),
    ("connect", "Connect to the TV"),
    ("find", "find it on the network"),
    ("typeip", "type its IP (Settings > System > Status)"),
    ("last", "last used:"),
    ("back", "back"),
    ("nofind", "no TV answered - turn it on, enable developer options > network debugging, same Wi-Fi"),
    ("ipprompt", "IP address of the TV"),
    ("target", "target"),
    ("default", "adb default"),
    ("nodev", "no device attached - connect the TV, or press T to set its address"),
    ("done", "done"),
];

const ES: &[(&str, &str)] = &[
    ("subtitle", "reescritura multiplataforma"),
    ("remote", "Mando a distancia"),
    ("adblock", "Quitar publicidad"),
    ("files", "Enviar / recibir archivos"),
    ("apps", "Instalar, desactivar, copiar apps"),
    ("settings", "Pantalla, DNS, reloj"),
    ("screen", "Captura, grabar, scrcpy"),
    ("helpers", "Instalar apps de ayuda"),
    ("launcher", "Lanzador"),
    ("optimize", "Optimizar"),
    ("doctor", "Comprobar el entorno"),
    ("update", "Buscar actualizaciones"),
    ("lang", "Switch to English"),
    ("quit", "salir"),
    ("unknown", "opción desconocida"),
    ("serial", "serie (vacío = la de adb por defecto)"),
    ("connect", "Conectar con la TV"),
    ("find", "buscarla en la red"),
    ("typeip", "escribir su IP (Ajustes > Sistema > Estado)"),
    ("last", "ultima usada:"),
    ("back", "volver"),
    ("nofind", "ninguna TV respondio - enciendela, activa opciones de desarrollador > depuracion de red, misma Wi-Fi"),
    ("ipprompt", "direccion IP de la TV"),
    ("target", "destino"),
    ("default", "adb por defecto"),
    ("nodev", "no hay ningun dispositivo - conecta la TV, o pulsa T para dar su direccion"),
    ("done", "hecho"),
];

fn ask(prompt: &str) -> Result<String> {
    print!("{prompt} > ");
    io::stdout().flush()?;
    let mut s = String::new();
    // stdin closed (piped input ran out, or ^D): leave instead of looping on "".
    if io::stdin().read_line(&mut s)? == 0 {
        println!();
        std::process::exit(0);
    }
    Ok(s.trim().to_string())
}

/// Empty input means "whatever adb picks"; anything else is a host:port serial.
fn normalize_serial(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() { None } else { Some(s.to_string()) }
}

/// The TV this session talks to. Asked once; remembered even when the answer was "default".
#[derive(Default)]
struct Target {
    serial: Option<String>,
    asked: bool,
}

impl Target {
    fn adb(&mut self, lang: Lang) -> Result<Adb> {
        if !self.asked {
            self.set(ask(lang.t("serial"))?);
        }
        Ok(Adb::new(self.serial.clone()))
    }

    fn set(&mut self, input: String) {
        self.serial = normalize_serial(&input);
        self.asked = true;
    }

    fn resolved(&mut self) {
        self.asked = true;      // the menu tried; do not interrupt an action
    }

    fn label(&self, lang: Lang) -> String {
        self.serial.clone().unwrap_or_else(|| lang.t("default").to_string())
    }
}

pub fn run(lang: &str) -> Result<()> {
    let mut lang = Lang::parse(lang);
    let mut target = Target::default();
    // The original opened on the IP screen; do the same when no TV is attached yet.
    let usable = Adb::new(None)
        .devices()
        .map(|d| d.iter().any(|x| x.state == "device"))
        .unwrap_or(false);
    if !usable && target.serial.is_none() {
        if let Err(e) = connect_screen(lang, &mut target) {
            println!("{DIM}{e}{RESET}");
        }
        target.resolved();
    }
    loop {
        let where_ = target.label(lang);
        println!("\n{TITLE}  Android TV Tools{RESET} {DIM}({}){RESET} {DIM}{} {where_}{RESET}", lang.t("subtitle"), lang.t("target"));
        for (k, key) in [
            ("C", "connect"),
            ("A", "remote"),
            ("B", "adblock"),
            ("1", "files"),
            ("2", "apps"),
            ("3", "settings"),
            ("4", "screen"),
            ("5", "helpers"),
            ("6", "launcher"),
            ("7", "optimize"),
            ("8", "doctor"),
            ("9", "update"),
            ("T", "target"),
            ("L", "lang"),
        ] {
            println!("  {KEY}{k}{RESET}  {}", lang.t(key));
        }
        println!("  {KEY}q{RESET}  {}", lang.t("quit"));
        let choice = ask("")?.trim().to_ascii_lowercase();
        if choice.is_empty() {
            continue;
        }
        if choice == "q" {
            return Ok(());
        }
        if choice == "l" {
            lang = if lang == Lang::En { Lang::Es } else { Lang::En };
            println!("{OK}lang = {}{RESET}", lang.code());
            continue;
        }
        if choice == "c" {
            if let Err(e) = connect_screen(lang, &mut target) {
                println!("{DIM}error: {e}{RESET}");
            }
            continue;
        }
        if choice == "t" {
            target.set(ask(lang.t("serial"))?);
            println!("{OK}{} {}{RESET}", lang.t("target"), target.label(lang));
            continue;
        }
        match action(lang, &mut target, &choice) {
            Ok(()) => println!("{OK}{}{RESET}", lang.t("done")),
            Err(e) => println!("{DIM}error: {e}{RESET}"),
        }
    }
}

/// The original's IP connection screen: find the TV, type its address, or reuse the last one.
fn connect_screen(lang: Lang, target: &mut Target) -> Result<()> {
    let adb = Adb::new(None);          // the connect itself does not need a serial yet
    loop {
        let last = crate::history().pop();
        println!("\n{TITLE}{}{RESET}", lang.t("connect"));
        println!("  {KEY}1{RESET}  {}", lang.t("find"));
        println!("  {KEY}2{RESET}  {}", lang.t("typeip"));
        println!("  {KEY}3{RESET}  {} {}", lang.t("last"), last.clone().unwrap_or_else(|| "-".to_string()));
        println!("  {KEY}0{RESET}  {}", lang.t("back"));
        let choice = ask("")?.trim().to_ascii_lowercase();
        let host = match choice.as_str() {
            "1" => {
                let hits = net::probe(&adb, &net::candidates(&adb));
                match hits.first() {
                    Some(h) => h.clone(),
                    None => {
                        println!("{DIM}{}{RESET}", lang.t("nofind"));
                        continue;
                    }
                }
            }
            "2" => ask(lang.t("ipprompt"))?,
            "3" => match last {
                Some(h) => h,
                None => {
                    println!("{DIM}-{RESET}");
                    continue;
                }
            },
            _ => return Ok(()),
        };
        match crate::connect_to(&adb, &host) {
            Ok(msg) => {
                target.set(host);
                println!("{OK}{msg}{RESET}");
            }
            Err(e) => println!("{DIM}{e}{RESET}"),
        }
        return Ok(());
    }
}

/// One menu entry. Never exits the program; errors surface in the menu.
fn action(lang: Lang, target: &mut Target, choice: &str) -> Result<()> {
    match choice {
            "a" => {
                let adb = target.adb(lang)?;
                if adb.devices().map(|d| d.is_empty()).unwrap_or(false) {
                    println!("{DIM}{}{RESET}", lang.t("nodev"));
                    return Ok(());
                }
                remote::run(&adb)?;
            }
            "b" => {
                let adb = target.adb(lang)?;
                println!("1 private DNS on   2 off   3 state");
                match ask("")?.as_str() {
                    "1" => settings::adblock_on(&adb)?,
                    "2" => settings::adblock_off(&adb)?,
                    _ => println!("dns = {}", settings::dns_state(&adb)?),
                }
            }
            "1" => {
                let adb = target.adb(lang)?;
                println!("1 push a file   2 pull a file   3 list {}/", crate::transfer::DOWNLOADS);
                // Ask what to do first: the original asked for the path before the choice.
                match ask("")?.trim() {
                    "1" => {
                        let local = ask("local file")?;
                        crate::transfer::push(&adb, std::path::Path::new(&local), crate::transfer::DOWNLOADS)?;
                    }
                    "2" => {
                        let remote = ask("file on the TV")?;
                        let save = ask("save as (empty = its own name)")?;
                        let save = if save.is_empty() {
                            std::path::Path::new(&remote)
                                .file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_else(|| "pulled.bin".to_string())
                        } else {
                            save
                        };
                        crate::transfer::pull(&adb, &remote, std::path::Path::new(&save))?;
                    }
                    "3" | "" => println!("{}", crate::transfer::ls(&adb, crate::transfer::DOWNLOADS)?),
                    other => println!("{} {other}", lang.t("unknown")),
                }
            }
            "2" => {
                println!("1 list   2 install apk   3 backup all   4 debloat preset");
                let adb = target.adb(lang)?;
                match ask("")?.as_str() {
                    "1" => {
                        for p in apps::list(&adb, apps::Scope::ThirdParty)? {
                            println!("{p}");
                        }
                    }
                    "2" => apps::install(&adb, &[std::path::PathBuf::from(ask("apk path")?)])?,
                    "3" => {
                        apps::backup_all(&adb, std::path::Path::new(&ask("directory")?))?;
                    }
                    _ => {
                        println!("{}", debloat::numbered());
                        println!("{DIM}{}{RESET}", debloat::titles());
                        let name = ask("preset")?;
                        let preset = debloat::find_any(&name)
                            .ok_or_else(|| anyhow::anyhow!("pick a number (1-{}) or a name, not '{name}'", debloat::PRESETS.len()))?;
                        apps::set_enabled(&adb, &preset.packages.iter().map(|p| p.to_string()).collect::<Vec<_>>(), false)?;
                    }
                }
            }
            "3" => {
                let adb = target.adb(lang)?;
                println!("density    = {}", settings::density(&adb)?);
                println!("font_scale = {}", settings::font_scale(&adb)?);
                println!("dns        = {}", settings::dns_state(&adb)?);
                println!("timeouts   = {}", settings::idle_timeout(&adb)?);
                let dpi = ask("new density (empty = keep)")?;
                if !dpi.is_empty() {
                    settings::set_density(&adb, dpi.parse().unwrap_or(320))?;
                }
            }
            "4" => {
                let adb = target.adb(lang)?;
                println!("1 screenshot   2 record   3 scrcpy");
                match ask("")?.as_str() {
                    "1" => crate::screen::screenshot(&adb, std::path::Path::new("atvtools-shot.png"))?,
                    "2" => crate::screen::record(&adb, ask("seconds")?.parse().unwrap_or(10), std::path::Path::new("atvtools-record.mp4"))?,
                    _ => crate::screen::scrcpy(adb.serial.as_deref(), &[])?,
                }
            }
            "5" => {
                println!("{DIM}{}{RESET}", catalog::names());
                let name = ask("install which app (empty = keep)")?;
                if !name.is_empty() {
                    let adb = target.adb(lang)?;
                    if let Err(e) = crate::download::install_catalog(&adb, &name) {
                        println!("{e}");
                        let yes = ask("replace the installed copy with this build (y/N)")?;
                        if yes.trim().eq_ignore_ascii_case("y") {
                            crate::download::install_catalog_with(&adb, &name, true)?;
                        }
                    }
                }
            }
            "6" => {
                let adb = target.adb(lang)?;
                for c in apps::home_activities(&adb)? {
                    println!("{c}");
                }
                let target = ask("set default launcher (empty = keep)")?;
                if !target.is_empty() {
                    apps::set_home(&adb, &target)?;
                }
            }
            "7" => {
                let adb = target.adb(lang)?;
                print!("{}", device::optimize(&adb)?);
            }
            "8" => {
                let adb = target.adb(lang)?;
                print!("{}", crate::doctor::run(&adb)?);
            }
            "9" => print!("{}", crate::update::check(env!("CARGO_PKG_VERSION"), None)?),
            other => println!("{} {other}", lang.t("unknown")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_input_is_trimmed_and_empty_means_default() {
        assert_eq!(normalize_serial(""), None);
        assert_eq!(normalize_serial("   "), None);
        assert_eq!(normalize_serial(" 192.168.1.50:5555\n"), Some("192.168.1.50:5555".to_string()));
    }

    #[test]
    fn the_target_is_asked_once_even_when_the_answer_is_default() {
        let mut t = Target::default();
        assert_eq!(t.label(Lang::En), "adb default");
        t.set("  ".to_string());              // pressed enter: adb picks the device
        assert!(t.asked);
        assert_eq!(t.serial, None);
        t.set("192.168.1.50:5555".to_string());
        assert_eq!(t.label(Lang::En), "192.168.1.50:5555");
        assert_eq!(t.label(Lang::Es), "192.168.1.50:5555");
    }

    #[test]
    fn languages_fall_back_to_the_key() {
        assert_eq!(Lang::parse("es").t("remote"), "Mando a distancia");
        assert_eq!(Lang::parse("en").t("remote"), "Remote control");
        assert_eq!(Lang::parse("xx").code(), "en");
        assert_eq!(Lang::En.t("missing.key"), "missing.key");
    }
}
