//! Interactive menu, mirroring the original tool's top level (A Remote Control,
//! B Remove ads, 1..8 the rest). Coloured with ANSI and localised, so the same
//! binary covers the original's _EN and _ES builds.

use crate::adb::Adb;
use crate::apps;
use crate::catalog;
use crate::debloat;
use crate::device;
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
    ("serial", "serial (empty = default)"),
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
    ("serial", "serie (vacío = por defecto)"),
    ("done", "hecho"),
];

fn ask(prompt: &str) -> Result<String> {
    print!("{prompt} > ");
    io::stdout().flush()?;
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}

fn adb_for(lang: Lang) -> Result<Adb> {
    let s = ask(lang.t("serial"))?;
    Ok(Adb::new(Some(s).filter(|x| !x.is_empty())))
}

pub fn run(lang: &str) -> Result<()> {
    let mut lang = Lang::parse(lang);
    loop {
        println!("\n{TITLE}  Android TV Tools{RESET} {DIM}({}){RESET}", lang.t("subtitle"));
        for (k, key) in [
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
            ("L", "lang"),
        ] {
            println!("  {KEY}{k}{RESET}  {}", lang.t(key));
        }
        println!("  {KEY}q{RESET}  {}", lang.t("quit"));
        let choice = ask("")?;
        match choice.as_str() {
            "q" | "Q" => return Ok(()),
            "A" | "a" => {
                let adb = adb_for(lang)?;
                if let Ok(found) = adb.devices() {
                    if found.is_empty() {
                        continue;
                    }
                }
                remote::run(&adb)?;
            }
            "B" | "b" => {
                let adb = adb_for(lang)?;
                println!("1 private DNS on   2 off   3 state");
                match ask("")?.as_str() {
                    "1" => settings::adblock_on(&adb)?,
                    "2" => settings::adblock_off(&adb)?,
                    _ => println!("dns = {}", settings::dns_state(&adb)?),
                }
            }
            "1" => {
                let adb = adb_for(lang)?;
                println!("1 push a file   2 pull a file   3 list {}/", crate::transfer::DOWNLOADS);
                let local = ask("local path")?;
                match ask("")?.as_str() {
                    "1" => crate::transfer::push(&adb, std::path::Path::new(&local), crate::transfer::DOWNLOADS)?,
                    "2" => crate::transfer::pull(
                        &adb,
                        &local,
                        std::path::Path::new(&ask("save as")?),
                    )?,
                    _ => println!("{}", crate::transfer::ls(&adb, crate::transfer::DOWNLOADS)?),
                }
            }
            "2" => {
                let adb = adb_for(lang)?;
                println!("1 list   2 install apk   3 backup all   4 debloat preset");
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
                        println!("{}", debloat::titles());
                        let name = ask("preset")?;
                        let preset = debloat::find(&name)
                            .ok_or_else(|| anyhow::anyhow!("unknown preset '{name}'"))?;
                        apps::set_enabled(&adb, &preset.packages.iter().map(|p| p.to_string()).collect::<Vec<_>>(), false)?;
                    }
                }
            }
            "3" => {
                let adb = adb_for(lang)?;
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
                let adb = adb_for(lang)?;
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
                    let adb = adb_for(lang)?;
                    crate::download::install_catalog(&adb, &name)?;
                }
            }
            "6" => {
                let adb = adb_for(lang)?;
                for c in apps::home_activities(&adb)? {
                    println!("{c}");
                }
                let target = ask("set default launcher (empty = keep)")?;
                if !target.is_empty() {
                    apps::set_home(&adb, &target)?;
                }
            }
            "7" => {
                let adb = adb_for(lang)?;
                device::optimize(&adb)?;
            }
            "8" => {
                let adb = adb_for(lang)?;
                crate::doctor::run(&adb)?;
            }
            "9" => crate::update::check(env!("CARGO_PKG_VERSION"), None)?,
            "L" | "l" => {
                lang = if lang == Lang::En { Lang::Es } else { Lang::En };
                println!("{OK}lang = {}{RESET}", lang.code());
            }
            other => println!("{} {other}", lang.t("unknown")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_fall_back_to_the_key() {
        assert_eq!(Lang::parse("es").t("remote"), "Mando a distancia");
        assert_eq!(Lang::parse("en").t("remote"), "Remote control");
        assert_eq!(Lang::parse("xx").code(), "en");
        assert_eq!(Lang::En.t("missing.key"), "missing.key");
    }
}
