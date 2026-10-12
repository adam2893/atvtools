//! atvtools - cross-platform Android TV / Google TV toolbox.
//!
//! Rust port of "Android TV Tools v4.0" (originally a 341 KB Windows batch
//! script wrapped in a self-extracting EXE). README.md maps the original menu
//! onto these subcommands.

mod adb;
mod apps;
mod catalog;
mod debloat;
mod device;
mod download;
mod menu;
mod screen;
mod settings;
mod shizuku;

use adb::Adb;
use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "atvtools", version, about = "Android TV / Google TV toolbox over adb")]
struct Cli {
    /// Device serial (host:port); falls back to adb's default device.
    #[arg(short, long, global = true)]
    serial: Option<String>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// List attached devices.
    Devices,
    /// Connect to a TV over the network (port 5555 assumed).
    Connect { host: String },
    /// Disconnect network devices.
    Disconnect,
    /// Run a raw adb shell command.
    Shell { command: Vec<String> },
    /// Package management.
    Apps {
        #[command(subcommand)]
        op: AppsOp,
    },
    /// Display, DNS and clock settings.
    Settings {
        #[command(subcommand)]
        op: SettingsOp,
    },
    /// Screenshot, screen recording, scrcpy.
    Screen {
        #[command(subcommand)]
        op: ScreenOp,
    },
    /// Show the installer catalog.
    Catalog,
    /// Download and install a catalog app by name.
    Install { name: String },
    /// Debloat presets.
    Preset {
        #[command(subcommand)]
        op: PresetOp,
    },
    /// Device facts: model, Android version, resolution, battery.
    Info,
    /// Remote keys, text, power, intents, settings screens.
    Device {
        #[command(subcommand)]
        op: DeviceOp,
    },
    /// Read or change a custom TV setting.
    Toggle {
        name: Option<String>,
        /// Turn it on.
        #[arg(long, conflicts_with = "off")]
        on: bool,
        /// Turn it off.
        #[arg(long)]
        off: bool,
        /// List the known toggles.
        #[arg(long)]
        list: bool,
    },
    /// Screen-off timeout in milliseconds (no argument prints it).
    ScreenTimeout { ms: Option<u32> },
    /// Lock the screen rotation: 0=0deg 1=90 2=180 3=270.
    Rotate { rotation: u8 },
    /// Set the screen saver component ("off" disables it).
    Screensaver { component: String },
    /// Installed launcher (HOME) activities.
    Launcher {
        #[command(subcommand)]
        op: LauncherOp,
    },
    /// Suggest TV IPs through adb mDNS discovery.
    Mdns,
    /// Install and activate Shizuku (fork-Shizuku).
    Shizuku {
        #[command(subcommand)]
        op: ShizukuOp,
    },
    /// Open a package's Play Store listing on the device.
    Store { package: String },
    /// Previously connected devices.
    History,
    /// Animation scales, cache trim, background app kill.
    Optimize,
    /// Interactive menu.
    Menu,
}

#[derive(Subcommand, Debug)]
enum DeviceOp {
    /// Send remote key events (up, down, ok, home, playpause, ... or a numeric keycode).
    Key { key: Vec<String> },
    /// Type text on the device.
    Text { words: Vec<String> },
    /// Open a URL on the device (IPTV list, store page, ...).
    Url { url: String },
    /// Wake the device.
    Wake,
    /// Send it to sleep.
    Sleep,
    /// Reboot, optionally into recovery or bootloader.
    Reboot {
        #[arg(long)]
        mode: Option<String>,
    },
    /// Open an Android settings screen.
    Screen { page: String },
    /// List the settings screens this build knows.
    Screens,
    /// Pull down the notification shade.
    Notifications,
}

#[derive(Subcommand, Debug)]
enum LauncherOp {
    /// List installed launcher activities.
    List,
    /// Set the default launcher, e.g. com.example/.Home.
    Set { component: String },
}

#[derive(Subcommand, Debug)]
enum AppsOp {
    /// List installed packages.
    List {
        #[arg(long, value_enum, default_value_t = ScopeArg::ThirdParty)]
        scope: ScopeArg,
        /// Show the apk path.
        #[arg(long)]
        paths: bool,
        /// Show the installer package.
        #[arg(long)]
        installer: bool,
        /// Only packages installed from the Play Store.
        #[arg(long)]
        play_only: bool,
    },
    /// Install one or more APK files.
    Install { apk: Vec<PathBuf> },
    /// Uninstall packages for user 0.
    Uninstall { package: Vec<String> },
    /// Disable packages (pm disable-user --user 0).
    Disable { package: Vec<String> },
    /// Re-enable packages.
    Enable { package: Vec<String> },
    /// Pull the APK(s) of each package into a directory.
    Backup {
        #[arg(short, long, default_value = "backup")]
        dir: PathBuf,
        package: Vec<String>,
    },
    /// Package name, version and install times.
    Info { package: String },
    /// Launch an app by package name.
    Launch { package: String },
    /// Force-stop an app.
    Stop { package: String },
    /// Grant a runtime permission.
    Grant { package: String, permission: String },
    /// Revoke a runtime permission.
    Revoke { package: String, permission: String },
    /// appops grant (storage = MANAGE_EXTERNAL_STORAGE).
    Appop {
        /// Package to change; omit it with --all.
        package: Option<String>,
        #[arg(value_enum)]
        op: AppopArg,
        /// Deny instead of allow.
        #[arg(long)]
        deny: bool,
        /// Apply to every third-party package, plus the storage provider for
        /// storage grants.
        #[arg(long, conflicts_with = "package")]
        all: bool,
    },
    /// Packages that request a permission (one dumpsys scan).
    WithPermission { permission: String },
    /// Pull every third-party APK.
    BackupAll {
        #[arg(short, long, default_value = "backup")]
        dir: PathBuf,
    },
    /// Install every apk in a directory (bulk sideload / restore).
    Restore { dir: PathBuf },
    /// Wipe an app's data and cache (pm clear).
    Clear { package: String },
    /// Label + Play Store URL for every third-party package (shortcut source).
    Shortcuts,
}

#[derive(Subcommand, Debug)]
enum ShizukuOp {
    /// Download and install fork-Shizuku.
    Install,
    /// Start the Shizuku server over adb (open the app on the TV once first).
    Start,
    /// Stop the Shizuku server.
    Stop,
    /// Installed / running / starter-script state.
    Status,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum AppopArg {
    Storage,
    WriteSettings,
}

impl AppopArg {
    fn as_op(self) -> &'static str {
        match self {
            AppopArg::Storage => "MANAGE_EXTERNAL_STORAGE",
            AppopArg::WriteSettings => "WRITE_SETTINGS",
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum ScopeArg {
    ThirdParty,
    System,
    All,
}

#[derive(Subcommand, Debug)]
enum SettingsOp {
    /// Print the current density.
    Density,
    /// Set the density in DPI (>= 72).
    SetDensity { dpi: u32 },
    /// Restore the device default density.
    ResetDensity,
    /// Print the current font scale.
    FontScale,
    /// Set the font scale (0.5 - 2.0).
    SetFontScale { scale: f32 },
    /// Block ads with AdGuard DNS.
    AdblockOn,
    /// Disable private DNS.
    AdblockOff,
    /// Print private DNS state.
    Dns,
    /// Set the NTP server.
    Ntp { host: String },
}

#[derive(Subcommand, Debug)]
enum ScreenOp {
    /// Screenshot the device and pull it locally.
    Shot { file: PathBuf },
    /// Record N seconds and pull the video locally.
    Record { seconds: u32, file: PathBuf },
    /// Launch scrcpy (needs scrcpy on PATH, or SCRCPY).
    Scrcpy {
        #[arg(trailing_var_arg = true)]
        extra: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
enum PresetOp {
    /// List presets.
    List,
    /// Disable every package of a preset.
    Apply {
        name: String,
        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Re-enable every package of a preset.
    Undo { name: String },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let adb = Adb::new(cli.serial.clone());

    match cli.cmd {
        Cmd::Devices => {
            for d in adb.devices()? {
                println!("{}\t{}\t{}", d.serial, d.state, d.model.unwrap_or_default());
            }
        }
        Cmd::Connect { host } => {
            let out = adb.connect(&host)?.trim().to_string();
            println!("{out}");
            if out.to_ascii_lowercase().contains("connected") {
                remember(&host);
            }
        }
        Cmd::Disconnect => println!("{}", adb.disconnect()?.trim()),
        Cmd::Shell { command } => {
            if command.is_empty() {
                adb.run(&["shell"])?;   // attached, interactive
            } else {
                print!("{}", adb.shell(&command.join(" "))?);
            }
        }
        Cmd::Apps { op } => match op {
            AppsOp::List { scope, paths, installer, play_only } => {
                if play_only {
                    for p in apps::play_store_packages(&adb)? {
                        println!("{p}");
                    }
                } else {
                    let scope = match scope {
                        ScopeArg::ThirdParty => apps::Scope::ThirdParty,
                        ScopeArg::System => apps::Scope::System,
                        ScopeArg::All => apps::Scope::All,
                    };
                    for r in apps::list_rows(&adb, scope, paths, installer)? {
                        if paths || installer {
                            println!(
                                "{:<40} {}{}",
                                r.package,
                                r.apk_path.unwrap_or_default(),
                                r.installer.map(|i| format!(" [{i}]")).unwrap_or_default()
                            );
                        } else {
                            println!("{}", r.package);
                        }
                    }
                }
            }
            AppsOp::Install { apk } => apps::install(&adb, &apk)?,
            AppsOp::Uninstall { package } => apps::uninstall(&adb, &package)?,
            AppsOp::Disable { package } => apps::set_enabled(&adb, &package, false)?,
            AppsOp::Enable { package } => apps::set_enabled(&adb, &package, true)?,
            AppsOp::Backup { dir, package } => {
                for p in apps::backup(&adb, &package, &dir)? {
                    println!("{}", p.display());
                }
            }
            AppsOp::Info { package } => print!("{}", apps::info(&adb, &package)?),
            AppsOp::Launch { package } => apps::launch(&adb, &package)?,
            AppsOp::Stop { package } => apps::force_stop(&adb, &package)?,
            AppsOp::Grant { package, permission } => apps::grant(&adb, &package, &permission, true)?,
            AppsOp::Revoke { package, permission } => apps::grant(&adb, &package, &permission, false)?,
            AppsOp::Appop { package, op, deny, all } => match (package, all) {
                (Some(p), _) => apps::appop(&adb, &p, op.as_op(), !deny)?,
                (None, true) => settings::set_default_permissions(&adb, op.as_op(), !deny)?,
                (None, false) => {
                    anyhow::bail!("name a package, or pass --all to change every third-party package")
                }
            },
            AppsOp::WithPermission { permission } => {
                for p in apps::with_permission(&adb, &permission)? {
                    println!("{p}");
                }
            }
            AppsOp::BackupAll { dir } => {
                for p in apps::backup_all(&adb, &dir)? {
                    println!("{}", p.display());
                }
            }
            AppsOp::Clear { package } => apps::clear(&adb, &package)?,
            AppsOp::Shortcuts => {
                for (label, url) in apps::shortcuts(&adb)? {
                    println!("{label}\t{url}");
                }
            }
            AppsOp::Restore { dir } => {
                let apks = apps::apks_in_dir(&dir)?;
                println!("installing {} apk(s) from {}", apks.len(), dir.display());
                apps::install(&adb, &apks)?;
            }
        },
        Cmd::Settings { op } => match op {
            SettingsOp::Density => println!("{}", settings::density(&adb)?),
            SettingsOp::SetDensity { dpi } => settings::set_density(&adb, dpi)?,
            SettingsOp::ResetDensity => settings::reset_density(&adb)?,
            SettingsOp::FontScale => println!("{}", settings::font_scale(&adb)?),
            SettingsOp::SetFontScale { scale } => settings::set_font_scale(&adb, scale)?,
            SettingsOp::AdblockOn => settings::adblock_on(&adb)?,
            SettingsOp::AdblockOff => settings::adblock_off(&adb)?,
            SettingsOp::Dns => println!("{}", settings::dns_state(&adb)?),
            SettingsOp::Ntp { host } => settings::set_ntp(&adb, &host)?,
        },
        Cmd::Screen { op } => match op {
            ScreenOp::Shot { file } => screen::screenshot(&adb, &file)?,
            ScreenOp::Record { seconds, file } => screen::record(&adb, seconds, &file)?,
            ScreenOp::Scrcpy { extra } => screen::scrcpy(cli.serial.as_deref(), &extra)?,
        },
        Cmd::Catalog => {
            for e in catalog::CATALOG {
                println!("{:<24} {}", e.name, e.note);
            }
        }
        Cmd::Install { name } => {
            let e = catalog::find(&name)
                .ok_or_else(|| anyhow::anyhow!("unknown app '{name}'. known: {}", catalog::names()))?;
            let dest = std::env::temp_dir().join("atvtools").join(format!("{}.apk", e.name));
            download::fetch(e.url, &dest)?;
            apps::install(&adb, &[dest])?;
        }
        Cmd::Shizuku { op } => match op {
            ShizukuOp::Install => {
                let e = catalog::find("shizuku-fork")
                    .ok_or_else(|| anyhow::anyhow!("shizuku-fork is missing from the catalog"))?;
                let dest = std::env::temp_dir().join("atvtools").join(format!("{}.apk", e.name));
                download::fetch(e.url, &dest)?;
                apps::install(&adb, &[dest])?;
            }
            ShizukuOp::Start => shizuku::start(&adb)?,
            ShizukuOp::Stop => shizuku::stop(&adb)?,
            ShizukuOp::Status => shizuku::status(&adb)?,
        },
        Cmd::Preset { op } => match op {
            PresetOp::List => {
                for p in debloat::PRESETS {
                    println!("{:<16} {}", p.title, p.description);
                }
            }
            PresetOp::Apply { name, yes } => {
                let p = debloat::find(&name)
                    .ok_or_else(|| anyhow::anyhow!("unknown preset '{name}'. known: {}", debloat::titles()))?;
                if !yes && !confirm(&format!("disable {} packages from '{}'?", p.packages.len(), p.title))? {
                    println!("aborted");
                    return Ok(());
                }
                let pkgs: Vec<String> = p.packages.iter().map(|s| (*s).to_string()).collect();
                apps::set_enabled(&adb, &pkgs, false)?;
            }
            PresetOp::Undo { name } => {
                let p = debloat::find(&name)
                    .ok_or_else(|| anyhow::anyhow!("unknown preset '{name}'. known: {}", debloat::titles()))?;
                let pkgs: Vec<String> = p.packages.iter().map(|s| (*s).to_string()).collect();
                apps::set_enabled(&adb, &pkgs, true)?;
            }
        },
        Cmd::Info => {
            for (k, v) in device::info(&adb)? {
                println!("{k:<14} {v}");
            }
        }
        Cmd::Device { op } => match op {
            DeviceOp::Key { key } => device::key(&adb, &key)?,
            DeviceOp::Text { words } => device::text(&adb, &words)?,
            DeviceOp::Url { url } => device::open_url(&adb, &url)?,
            DeviceOp::Wake => device::wake(&adb)?,
            DeviceOp::Sleep => device::sleep(&adb)?,
            DeviceOp::Reboot { mode } => device::reboot(&adb, mode.as_deref())?,
            DeviceOp::Screen { page } => device::screen(&adb, &page)?,
            DeviceOp::Screens => println!("{}", device::screen_names()),
            DeviceOp::Notifications => device::expand_notifications(&adb)?,
        },
        Cmd::Toggle { name, on, off, list } => {
            if list || name.is_none() {
                println!("{}", settings::toggle_names());
            } else {
                let n = name.unwrap_or_default();
                let state = if on { Some(true) } else if off { Some(false) } else { None };
                println!("{n} = {}", settings::toggle(&adb, &n, state)?);
            }
        }
        Cmd::ScreenTimeout { ms } => println!("{}", settings::screen_off_timeout(&adb, ms)?),
        Cmd::Rotate { rotation } => settings::rotate(&adb, rotation)?,
        Cmd::Screensaver { component } => settings::screensaver(&adb, &component)?,
        Cmd::Launcher { op } => match op {
            LauncherOp::List => {
                for c in apps::home_activities(&adb)? {
                    println!("{c}");
                }
            }
            LauncherOp::Set { component } => apps::set_home(&adb, &component)?,
        },
        Cmd::Mdns => print!("{}", device::mdns(&adb)?),
        Cmd::Store { package } => apps::open_store(&adb, &package)?,
        Cmd::History => {
            for h in history() {
                println!("{h}");
            }
        }
        Cmd::Optimize => device::optimize(&adb)?,
        Cmd::Menu => menu::run()?,
    }
    Ok(())
}

/// ~/.atvtools/history.txt - the original kept a history of TV devices.
fn history_file() -> Option<PathBuf> {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).ok()?;
    let dir = PathBuf::from(home).join(".atvtools");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("history.txt"))
}

fn history() -> Vec<String> {
    let mut v = history_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|s| s.lines().map(str::to_string).filter(|l| !l.trim().is_empty()).collect())
        .unwrap_or_else(Vec::new);
    v.dedup();
    v
}

fn remember(host: &str) {
    let Some(f) = history_file() else { return };
    let mut seen = history();
    if !seen.iter().any(|h| h == host) {
        seen.push(host.to_string());
        if let Err(e) = std::fs::write(&f, seen.join("\n") + "\n") {
            eprintln!("could not write history: {e}");
        }
    }
}

fn confirm(question: &str) -> Result<bool> {
    use std::io::Write;
    print!("{question} [y/N] ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim(), "y" | "Y" | "yes"))
}
