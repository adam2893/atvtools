//! Small interactive menu, mirroring the original tool's top level.

use crate::adb::Adb;
use crate::debloat;
use crate::settings;
use anyhow::Result;
use std::io::{self, Write};

const ITEMS: &[(&str, &str)] = &[
    ("1", "Devices: list / connect over the network"),
    ("2", "Settings: density, font scale, ad-block DNS"),
    ("3", "Presets: debloat / remove ads"),
];

pub fn run() -> Result<()> {
    loop {
        println!("\n  Android TV Tools (Rust)");
        for (k, v) in ITEMS {
            println!("  {k}  {v}");
        }
        println!("  q  quit");
        print!("> ");
        io::stdout().flush()?;

        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            return Ok(());
        }
        match line.trim() {
            "q" | "Q" => return Ok(()),
            "1" => {
                let adb = Adb::new(None);
                for d in adb.devices()? {
                    println!("{}\t{}\t{}", d.serial, d.state, d.model.unwrap_or_default());
                }
                print!("connect to host (empty to skip) > ");
                io::stdout().flush()?;
                let mut host = String::new();
                io::stdin().read_line(&mut host)?;
                if !host.trim().is_empty() {
                    println!("{}", adb.connect(host.trim())?.trim());
                }
            }
            "2" => {
                print!("serial (empty = default) > ");
                io::stdout().flush()?;
                let mut s = String::new();
                io::stdin().read_line(&mut s)?;
                let adb = Adb::new(Some(s.trim().to_string()).filter(|x| !x.is_empty()));
                println!("density    = {}", settings::density(&adb)?);
                println!("font_scale = {}", settings::font_scale(&adb)?);
                println!("dns        = {}", settings::dns_state(&adb)?);
            }
            "3" => println!("presets: {}\nuse: atvtools preset apply <name> --yes", debloat::titles()),
            other => println!("unknown option {other}"),
        }
    }
}
