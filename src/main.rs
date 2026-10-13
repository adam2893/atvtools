//! atvtools - cross-platform Android TV / Google TV toolbox.
//!
//! Rust port of "Android TV Tools v4.0" (originally a 341 KB Windows batch
//! script wrapped in a self-extracting EXE). README.md maps the original menu
//! onto these subcommands.

mod adb;
mod apk;
mod apps;
mod catalog;
mod debloat;
mod device;
mod doctor;
mod download;
mod google;
mod gui;
mod iptv;
mod logs;
mod menu;
mod net;
mod profile;
mod remote;
mod screen;
mod settings;
mod shizuku;
mod transfer;
mod uad;
mod undo;
mod update;

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

    /// Wait for the device to answer before running (up to 120s).
    #[arg(long, global = true)]
    wait: bool,

    /// Subcommand; bare `atvtools` opens the interactive menu.
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// List attached devices.
    Devices {
        /// One JSON object per device.
        #[arg(long)]
        json: bool,
    },
    /// Connect to a TV over the network; with no address, scan for one (port 5555 assumed).
    Connect { host: Option<String> },
    /// Disconnect network devices.
    Disconnect,
    /// Pair for Android 11+ wireless debugging; with no address, list the pairing ports.
    Pair {
        /// The host:port the pairing dialog shows.
        addr: Option<String>,
        /// The 6-digit code shown next to it.
        #[arg(long)]
        code: Option<String>,
    },
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
    Install {
        name: String,
        /// Remove an existing copy first when its signature differs
        #[arg(long)]
        replace: bool,
        /// Run on every authorized TV.
        #[arg(long)]
        all_devices: bool,
    },
    /// Debloat presets.
    Preset {
        #[command(subcommand)]
        op: PresetOp,
    },
    /// Device facts: model, Android version, resolution, battery.
    Info {
        /// One JSON object.
        #[arg(long)]
        json: bool,
    },
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
        /// Run on every authorized TV.
        #[arg(long)]
        all_devices: bool,
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
    Optimize {
        /// Run on every authorized TV.
        #[arg(long)]
        all_devices: bool,
    },
    /// Push and pull files between this machine and the TV.
    Transfer {
        #[command(subcommand)]
        op: TransferOp,
    },
    /// Interactive remote control.
    Remote {
        /// Scripted key sequence instead of the prompt, e.g. uudr.
        keys: Option<String>,
        /// Run a macro file instead (key names or shorthand, delay, text, url).
        #[arg(long, conflicts_with = "keys")]
        script: Option<PathBuf>,
        /// Repeat the sequence this many times.
        #[arg(long, default_value_t = 1, requires = "keys")]
        repeat: u32,
        /// Serve a browser D-pad instead (default 0.0.0.0:8080).
        #[arg(long, num_args = 0..=1, default_missing_value = "0.0.0.0:8080", value_name = "ADDR")]
        serve: Option<String>,
    },
    /// IPTV playlists (m3u) on the device.
    Iptv {
        #[command(subcommand)]
        op: IptvOp,
    },
    /// Preflight: adb, device, internet, region, OTA.
    Doctor {
        /// Repair what can be repaired here: restart the adb server, and put a
        /// USB-attached TV into TCP mode on port 5555.
        #[arg(long)]
        fix: bool,
    },
    /// Device logs: a bounded logcat dump, or a live stream.
    Logs {
        /// Only one logcat tag, e.g. System.out.
        #[arg(long)]
        tag: Option<String>,
        /// Only lines containing this text.
        #[arg(long)]
        grep: Option<String>,
        /// Keep streaming until interrupted (Ctrl-C).
        #[arg(long)]
        follow: bool,
        /// How many lines a dump keeps.
        #[arg(long, default_value_t = logs::DEFAULT_LINES)]
        lines: usize,
    },
    /// Recent app crashes and ANRs, out of the device dropbox.
    Crash {
        /// Only entries containing this text.
        #[arg(long)]
        grep: Option<String>,
        #[arg(long, default_value_t = logs::DEFAULT_LINES)]
        lines: usize,
    },
    /// Full bugreport archive, written by the device into a directory here.
    Bugreport {
        #[arg(short, long, default_value = "bugreport")]
        dir: PathBuf,
    },
    /// Enable again the packages the last debloat/preset batch disabled.
    Undo {
        /// List the journal instead of restoring anything.
        #[arg(long)]
        list: bool,
        /// A stamp from the list, instead of the newest entry.
        #[arg(long)]
        stamp: Option<String>,
        /// Skip the confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// Apply a device profile (.atvtools.toml); --dry-run shows the plan.
    Apply {
        #[arg(short, long)]
        file: Option<PathBuf>,
        /// Print what would change and stop.
        #[arg(long)]
        dry_run: bool,
        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
        /// Run on every authorized TV.
        #[arg(long)]
        all_devices: bool,
    },
    /// Several TVs at once.
    Fleet {
        #[command(subcommand)]
        op: FleetOp,
    },
    /// Check for a newer build.
    Update {
        /// Read the old tool's "version;url;changelog" beacon instead.
        #[arg(long)]
        beacon: Option<String>,
    },
    /// APK metadata for local files: AndroidManifest.xml and resources.arsc are
    /// parsed here, so no aapt2 is needed (AAPT=<bin> uses that one instead).
    Apk {
        #[command(subcommand)]
        op: ApkOp,
    },
    /// Bare-device bootstrap: install the Google stack from a folder of APKs.
    Google {
        #[arg(short, long, default_value = ".")]
        dir: PathBuf,
    },
    /// Assistant key, optionally followed by the text to dictate.
    Voice { words: Vec<String> },
    /// Interactive menu.
    Menu {
        /// Menu language: en or es.
        #[arg(long, default_value = "en")]
        lang: String,
    },
    /// Debloat with the universal-android-debloater package database.
    Uad {
        #[command(subcommand)]
        op: UadOp,
    },
    /// Native desktop window (needs a display).
    Gui,
}

#[derive(Subcommand, Debug)]
enum TransferOp {
    /// Send a local file or directory to the TV.
    Push {
        local: PathBuf,
        #[arg(default_value = transfer::DOWNLOADS)]
        remote: String,
        /// Copy a whole directory tree.
        #[arg(long)]
        dir: bool,
    },
    /// Fetch a file or directory from the TV.
    Pull {
        remote: String,
        local: PathBuf,
        /// Copy a whole directory tree.
        #[arg(long)]
        dir: bool,
    },
    /// List a device directory.
    Ls {
        #[arg(default_value = transfer::DOWNLOADS)]
        remote: String,
    },
    /// Create a device directory.
    Mkdir { remote: String },
}

#[derive(Subcommand, Debug)]
enum IptvOp {
    /// Download an m3u playlist and copy it into the TV's Downloads folder.
    Add {
        url: String,
        #[arg(long)]
        name: Option<String>,
    },
    /// List playlists already on the device.
    List,
    /// Hand a playlist URL to the player app on the TV.
    Open { url: String },
    /// Open a player's install listing (no argument lists them).
    Install { player: Option<String> },
}

#[derive(Subcommand, Debug)]
enum ApkOp {
    /// Package, version, label and launch activity of one APK.
    Info { file: PathBuf },
    /// Every .apk in a folder, in install order.
    List { dir: PathBuf },
}

#[derive(Subcommand, Debug)]
enum UadOp {
    /// Installed packages UAD flags, at or below a removal level.
    List {
        #[arg(long, default_value = "recommended")]
        level: String,
        #[arg(long, value_enum, default_value_t = ScopeArg::All)]
        scope: ScopeArg,
        /// Re-download the database first.
        #[arg(long)]
        refresh: bool,
        /// Print every line of the description, not just the first.
        #[arg(long)]
        full: bool,
    },
    /// The database entry for one package.
    Info { package: String },
    /// Run the upstream cross-platform UAD GUI (downloaded once, ~/.atvtools/uad).
    Gui {
        /// Re-download even when a copy is already unpacked.
        #[arg(long)]
        refresh: bool,
    },
    /// Disable every flagged package (pm disable-user --user 0).
    Apply {
        #[arg(long, default_value = "recommended")]
        level: String,
        #[arg(long, value_enum, default_value_t = ScopeArg::All)]
        scope: ScopeArg,
        #[arg(long)]
        refresh: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Re-download the database into ~/.atvtools.
    Refresh,
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
    Wake {
        /// Over the network (wake-on-LAN) instead of a key event.
        #[arg(long)]
        lan: bool,
    },
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
        /// Only packages that are disabled (the preset/debloat leftovers).
        #[arg(long)]
        disabled: bool,
        /// One JSON object per package.
        #[arg(long)]
        json: bool,
    },
    /// Install one or more APK files.
    Install {
        apk: Vec<PathBuf>,
        /// Remove an existing copy first when its signature differs (fork vs official Shizuku)
        #[arg(long)]
        replace: bool,
        /// Run on every authorized TV.
        #[arg(long)]
        all_devices: bool,
    },
    /// Uninstall packages for user 0.
    Uninstall { package: Vec<String> },
    /// Disable packages (pm disable-user --user 0).
    Disable {
        package: Vec<String>,
        /// Run on every authorized TV.
        #[arg(long)]
        all_devices: bool,
    },
    /// Re-enable packages.
    Enable { package: Vec<String> },
    /// Pull the APK(s) of each package into a directory.
    Backup {
        #[arg(short, long, default_value = "backup")]
        dir: PathBuf,
        package: Vec<String>,
    },
    /// Installed version vs the newest catalog release.
    Outdated,
    /// Install the newest build of a catalog app.
    Upgrade { name: String },
    /// Pull the apk(s) of a package (base + splits) into a directory.
    Apk {
        package: String,
        #[arg(short, long, default_value = "apk")]
        dir: PathBuf,
    },
    /// Biggest packages on the TV (apk size then data size).
    Size {
        #[arg(long, default_value_t = 20)]
        top: usize,
        #[arg(long, value_enum, default_value_t = ScopeArg::ThirdParty)]
        scope: ScopeArg,
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
    Install {
        /// Remove an official Shizuku of the same id first (their signatures differ).
        #[arg(long)]
        replace: bool,
    },
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

impl ScopeArg {
    fn as_scope(self) -> apps::Scope {
        match self {
            ScopeArg::ThirdParty => apps::Scope::ThirdParty,
            ScopeArg::System => apps::Scope::System,
            ScopeArg::All => apps::Scope::All,
        }
    }
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
    /// Print private DNS state, or point it at a provider.
    Dns {
        /// adguard | cloudflare | nextdns | quad9 | google
        #[arg(long)]
        provider: Option<String>,
        /// NextDNS profile id, used as the hostname label.
        #[arg(long)]
        id: Option<String>,
    },
    /// Set the NTP server.
    Ntp { host: String },
    /// Android TV OTA updates (no flag prints the state).
    Ota {
        /// Block them.
        #[arg(long, conflicts_with = "on")]
        off: bool,
        /// Allow them again.
        #[arg(long)]
        on: bool,
    },
    /// Idle/ambient timeout in seconds (no argument prints it).
    IdleTimeout { seconds: Option<u32> },
}

#[derive(Subcommand, Debug)]
enum ScreenOp {
    /// Screenshot the device and pull it locally.
    Shot {
        #[arg(default_value = "shot.png")]
        file: PathBuf,
        /// Shoot every authorized TV into shot-<serial>.png.
        #[arg(long)]
        all_devices: bool,
    },
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
        /// Run on every authorized TV.
        #[arg(long)]
        all_devices: bool,
    },
    /// Re-enable every package of a preset.
    Undo { name: String },
    /// Write the TV's current state out as a profile file.
    Export {
        #[arg(short, long, default_value = ".atvtools.toml")]
        file: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum FleetOp {
    /// What every attached TV is: model, Android version, disabled count.
    Status,
    /// Apply the profile on every authorized TV.
    Sync {
        #[arg(short, long)]
        file: Option<PathBuf>,
        /// Print what would change and stop.
        #[arg(long)]
        dry_run: bool,
        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adb_error_text_is_not_mistaken_for_a_device_name() {
        assert!(plain_answer("Chromecast HD"));
        assert!(plain_answer("NVIDIA SHIELD Android TV"));
        assert!(!plain_answer(""));
        assert!(!plain_answer("adb -s 192.168.1.5:5555 shell getprop ro.product.model failed: adb: device unauthorized"));
        assert!(!plain_answer("error: device offline"));
        assert!(!plain_answer("unauthorized"));
        assert!(!plain_answer(&"x".repeat(80)));
    }
}

/// The devices a fan-out command touches: the one adb is pointed at, or every authorized TV.
fn fanout(adb: &Adb, all: bool) -> Result<Vec<Adb>> {
    if !all {
        return Ok(vec![adb.clone()]);
    }
    let found: Vec<Adb> = adb
        .devices()?
        .into_iter()
        .filter(|d| d.state == "device")
        .map(|d| Adb { serial: Some(d.serial), bin: adb.bin.clone() })
        .collect();
    if found.is_empty() {
        anyhow::bail!("--all-devices found no authorized TV; run: atvtools devices");
    }
    Ok(found)
}

/// Runs one action per target, with a header per TV when there is more than one.
fn run_each(adb: &Adb, all: bool, mut f: impl FnMut(&Adb) -> Result<()>) -> Result<()> {
    let targets = fanout(adb, all)?;
    let many = targets.len() > 1;
    for a in &targets {
        if many {
            println!("== {}", a.serial.as_deref().unwrap_or("default device"));
        }
        f(a)?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let adb = Adb::new(cli.serial.clone());

    if cli.wait {
        wait_for(&adb)?;
    }

    let cmd = cli.cmd.unwrap_or(Cmd::Menu { lang: "en".to_string() });
    match cmd {
        Cmd::Devices { json } => {
            let found = adb.devices()?;
            if json {
                let rows: Vec<serde_json::Value> = found
                    .iter()
                    .map(|d| {
                        serde_json::json!({
                            "serial": &d.serial,
                            "state": &d.state,
                            "model": &d.model,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else {
                for d in found {
                    println!("{}\t{}\t{}", d.serial, d.state, d.model.unwrap_or_default());
                }
            }
        }
        Cmd::Connect { host } => match host {
            Some(host) => {
                println!("{}", connect_to(&adb, &host)?);
            }
            None => {
                let found = net::probe(&adb, &net::candidates(&adb));
                if found.is_empty() {
                    println!("no TV answered on this network");
                    println!("  - enable Developer options > USB/network debugging on the TV");
                    println!("  - keep the TV on the same Wi-Fi, then: atvtools connect <tv-ip>");
                } else {
                    for host in &found {
                        println!("{host}");
                    }
                    println!("connect with: atvtools connect <host>");
                }
            }
        },
        Cmd::Disconnect => println!("{}", adb.disconnect()?.trim()),
        Cmd::Pair { addr, code } => match (addr, code) {
            (Some(addr), Some(code)) => println!("{}", adb.pair(&addr, &code)?.trim()),
            (Some(_), None) => anyhow::bail!(
                "pairing needs the 6-digit code from the TV: atvtools pair <host:port> --code 123456"
            ),
            (None, _) => {
                let found = net::pairing_candidates(&adb);
                if found.is_empty() {
                    println!("no TV is offering a pairing port right now");
                    println!("  - on the TV: Developer options > Wireless debugging > Pair device with pairing code");
                    println!("  - then: atvtools pair <host:port> --code <6 digits>");
                } else {
                    for h in &found {
                        println!("{h}");
                    }
                    println!("pair with: atvtools pair <host:port> --code <6 digits>");
                }
            }
        },
        Cmd::Shell { command } => {
            if command.is_empty() {
                adb.run(&["shell"])?;   // attached, interactive
            } else {
                print!("{}", adb.shell(&command.join(" "))?);
            }
        }
        Cmd::Apps { op } => match op {
            AppsOp::List { scope, paths, installer, play_only, disabled, json } => {
                if disabled {
                    let scope = scope.as_scope();
                    if json {
                        println!("{}", serde_json::to_string_pretty(&apps::disabled(&adb, scope)?)?);
                    } else {
                        for p in apps::disabled(&adb, scope)? {
                            println!("{p}");
                        }
                    }
                } else if play_only {
                    for p in apps::play_store_packages(&adb)? {
                        println!("{p}");
                    }
                } else {
                    let scope = match scope {
                        ScopeArg::ThirdParty => apps::Scope::ThirdParty,
                        ScopeArg::System => apps::Scope::System,
                        ScopeArg::All => apps::Scope::All,
                    };
                    let rows = apps::list_rows(&adb, scope, paths, installer)?;
                    if json {
                        let out: Vec<serde_json::Value> = rows
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "package": &r.package,
                                    "apk_path": &r.apk_path,
                                    "installer": &r.installer,
                                })
                            })
                            .collect();
                        println!("{}", serde_json::to_string_pretty(&out)?);
                    } else {
                        for r in rows {
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
            }
            AppsOp::Install { apk, replace, all_devices } => {
                run_each(&adb, all_devices, |a| apps::install_with(a, &apk, replace))?
            }
            AppsOp::Uninstall { package } => apps::uninstall(&adb, &package)?,
            AppsOp::Disable { package, all_devices } => run_each(&adb, all_devices, |a| {
                apps::set_enabled(a, &package, false)?;
                journals(a, "apps disable", &package);
                Ok(())
            })?,
            AppsOp::Enable { package } => apps::set_enabled(&adb, &package, true)?,
            AppsOp::Backup { dir, package } => {
                for p in apps::backup(&adb, &package, &dir)? {
                    println!("{}", p.display());
                }
            }
            AppsOp::Outdated => {
                for r in update::catalog_status(&adb)? {
                    let flag = if r.newer { "-> newer build available" } else { "" };
                    println!(
                        "{:<16} {:<28} {:<12} {:<12} {flag}",
                        r.name,
                        r.package.unwrap_or_else(|| "-".to_string()),
                        r.installed.unwrap_or_else(|| "not installed".to_string()),
                        r.latest.unwrap_or_else(|| "-".to_string()),
                    );
                }
            }
            AppsOp::Upgrade { name } => {
                if catalog::find(&name).is_none() {
                    anyhow::bail!("unknown app '{name}'. known: {}", catalog::names());
                }
                let before = catalog::packages(&name)
                    .iter()
                    .find_map(|p| apps::installed_version(&adb, p).map(|v| ((*p).to_string(), v)));
                download::install_catalog_with(&adb, &name, false)?;
                if let Some((p, was)) = before {
                    let now = apps::installed_version(&adb, &p).unwrap_or_else(|| "?".to_string());
                    println!("{p}: {was} -> {now}");
                }
            }
            AppsOp::Apk { package, dir } => {
                for p in apps::backup(&adb, &[package], &dir)? {
                    println!("{}", p.display());
                }
            }
            AppsOp::Size { top, scope } => {
                for (package, apk, data) in apps::sizes(&adb, scope.as_scope(), top)? {
                    println!("{:>10} {:>10}  {package}", apps::mb(apk), apps::mb(data));
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
            SettingsOp::Dns { provider, id } => match provider {
                Some(p) => {
                    let host = settings::dns_host(&p, id.as_deref())?;
                    settings::set_dns(&adb, &host)?;
                    println!("{}", settings::dns_state(&adb)?);
                }
                None => {
                    println!("{}", settings::dns_state(&adb)?);
                    if id.is_some() {
                        println!("(--id only means something with --provider nextdns)");
                    }
                }
            },
            SettingsOp::Ntp { host } => settings::set_ntp(&adb, &host)?,
            SettingsOp::Ota { off, on } => {
                if !off && !on {
                    println!("ota updates: {}", settings::ota_state(&adb)?);
                } else {
                    settings::ota(&adb, off)?;
                }
            }
            SettingsOp::IdleTimeout { seconds } => match seconds {
                Some(s) => println!("{}", settings::set_idle_timeout(&adb, s)?),
                None => println!("{}", settings::idle_timeout(&adb)?),
            },
        },
        Cmd::Screen { op } => match op {
            ScreenOp::Shot { file, all_devices } => run_each(&adb, all_devices, |a| {
                let dest = if all_devices {
                    let serial = a.serial.clone().unwrap_or_default().replace(':', "-");
                    PathBuf::from(format!("shot-{serial}.png"))
                } else {
                    file.clone()
                };
                screen::screenshot(a, &dest)?;
                Ok(())
            })?,
            ScreenOp::Record { seconds, file } => screen::record(&adb, seconds, &file)?,
            ScreenOp::Scrcpy { extra } => screen::scrcpy(cli.serial.as_deref(), &extra)?,
        },
        Cmd::Catalog => {
            for e in catalog::CATALOG {
                println!("{:<24} {}", e.name, e.note);
            }
        }
        Cmd::Install { name, replace, all_devices } => {
            run_each(&adb, all_devices, |a| download::install_catalog_with(a, &name, replace))?
        }
        Cmd::Shizuku { op } => match op {
            ShizukuOp::Install { replace } => {
                let e = catalog::find("shizuku-fork")
                    .ok_or_else(|| anyhow::anyhow!("shizuku-fork is missing from the catalog"))?;
                let dest = std::env::temp_dir().join("atvtools").join(format!("{}.apk", e.name));
                download::fetch(e.url, &dest)?;
                apps::install_with(&adb, &[dest], replace)?;
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
            PresetOp::Apply { name, yes, all_devices } => {
                let p = debloat::find_any(&name)
                    .ok_or_else(|| anyhow::anyhow!("unknown preset '{name}'. known: {}", debloat::titles()))?;
                if !yes && !confirm(&format!("disable {} packages from '{}'?", p.packages.len(), p.title))? {
                    println!("aborted");
                    return Ok(());
                }
                let pkgs: Vec<String> = p.packages.iter().map(|s| (*s).to_string()).collect();
                let what = format!("preset {}", p.title);
                run_each(&adb, all_devices, |a| {
                    apps::set_enabled(a, &pkgs, false)?;
                    journals(a, &what, &pkgs);
                    Ok(())
                })?;
            }
            PresetOp::Export { file } => {
                let body = profile::export(&adb)?;
                std::fs::write(&file, &body)?;
                println!("wrote {} ({} line(s))", file.display(), body.lines().count());
            }
            PresetOp::Undo { name } => {
                let p = debloat::find_any(&name)
                    .ok_or_else(|| anyhow::anyhow!("unknown preset '{name}'. known: {}", debloat::titles()))?;
                let pkgs: Vec<String> = p.packages.iter().map(|s| (*s).to_string()).collect();
                apps::set_enabled(&adb, &pkgs, true)?;
            }
        },
        Cmd::Info { json } => {
            let rows = device::info(&adb)?;
            if json {
                let obj: serde_json::Map<String, serde_json::Value> = rows
                    .into_iter()
                    .map(|(k, v)| (k, serde_json::Value::String(v)))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&obj)?);
            } else {
                for (k, v) in rows {
                    println!("{k:<14} {v}");
                }
            }
        }
        Cmd::Device { op } => match op {
            DeviceOp::Key { key } => device::key(&adb, &key)?,
            DeviceOp::Text { words } => device::text(&adb, &words)?,
            DeviceOp::Url { url } => device::open_url(&adb, &url)?,
            DeviceOp::Wake { lan } => {
                if lan {
                    println!("{}", device::wake_lan(&adb)?);
                } else {
                    device::wake(&adb)?;
                }
            }
            DeviceOp::Sleep => device::sleep(&adb)?,
            DeviceOp::Reboot { mode } => device::reboot(&adb, mode.as_deref())?,
            DeviceOp::Screen { page } => device::screen(&adb, &page)?,
            DeviceOp::Screens => println!("{}", device::screen_names()),
            DeviceOp::Notifications => device::expand_notifications(&adb)?,
        },
        Cmd::Toggle { name, on, off, list, all_devices } => {
            if list || name.is_none() {
                println!("{}", settings::toggle_names());
            } else {
                let n = name.unwrap_or_default();
                let state = if on { Some(true) } else if off { Some(false) } else { None };
                run_each(&adb, all_devices, |a| {
                    settings::toggle(a, &n, state).map(|v| println!("{n} = {v}"))
                })?;
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
        Cmd::Optimize { all_devices } => run_each(&adb, all_devices, |a| {
            print!("{}", device::optimize(a)?);
            Ok(())
        })?,
        Cmd::Transfer { op } => match op {
            TransferOp::Push { local, remote, dir } => {
                // adb copies directories recursively on its own; the flag just makes it deliberate.
                if local.is_dir() && !dir {
                    anyhow::bail!("{} is a directory; add --dir to send the whole tree", local.display());
                }
                transfer::push(&adb, &local, &remote)?;
            }
            TransferOp::Pull { remote, local, dir } => {
                if transfer::is_dir(&adb, &remote) && !dir {
                    anyhow::bail!("{remote} is a directory; add --dir to fetch the whole tree");
                }
                transfer::pull(&adb, &remote, &local)?;
            }
            TransferOp::Ls { remote } => println!("{}", transfer::ls(&adb, &remote)?),
            TransferOp::Mkdir { remote } => transfer::mkdir(&adb, &remote)?,
        },
        Cmd::Remote { keys, script, repeat, serve } => match (serve, keys, script) {
            (Some(addr), None, None) => remote::serve(&adb, &addr)?,
            (Some(_), _, _) => anyhow::bail!("--serve cannot be combined with a key sequence or a script"),
            (None, keys, script) => remote_arm(&adb, keys, script, repeat)?,
        },
        Cmd::Iptv { op } => match op {
            IptvOp::Add { url, name } => println!("{}", iptv::add(&adb, &url, name.as_deref())?),
            IptvOp::List => println!("{}", iptv::list(&adb)?),
            IptvOp::Open { url } => iptv::open(&adb, &url)?,
            IptvOp::Install { player } => match player {
                Some(p) => {
                    let pkg = iptv::player(&p)
                        .ok_or_else(|| anyhow::anyhow!("unknown player '{p}'. known: {}", iptv::players()))?;
                    apps::open_store(&adb, pkg)?;
                }
                None => println!("{}", iptv::players()),
            },
        },
        Cmd::Doctor { fix } => {
            if fix {
                print!("{}", doctor::fix(&adb)?);
            }
            print!("{}", doctor::run(&adb)?);
        }
        Cmd::Logs { tag, grep, follow, lines } => {
            print!("{}", logs::logcat(&adb, tag.as_deref(), grep.as_deref(), follow, lines)?);
        }
        Cmd::Crash { grep, lines } => print!("{}", logs::crash(&adb, grep.as_deref(), lines)?),
        Cmd::Bugreport { dir } => println!("{}", logs::bugreport(&adb, &dir)?),
        Cmd::Undo { list, stamp, yes } => {
            if list {
                let entries = undo::entries();
                if entries.is_empty() {
                    println!("nothing to undo: no journal entries in {}", undo::dir().map(|d| d.display().to_string()).unwrap_or_else(|| "?".to_string()));
                }
                for r in entries {
                    println!("{:<10} {:<24} {:>3} package(s)  {}", r.stamp, r.what, r.packages.len(), r.serial);
                }
            } else {
                let r = undo::last(stamp.as_deref()).ok_or_else(|| {
                    anyhow::anyhow!("no journal entry to undo; run: atvtools undo --list")
                })?;
                println!("{:<10} {:<24} {} package(s)", r.stamp, r.what, r.packages.len());
                for p in &r.packages {
                    println!("  {p}");
                }
                if !yes && !confirm(&format!("enable those {} packages again?", r.packages.len()))? {
                    println!("aborted");
                    return Ok(());
                }
                let target = if r.serial.is_empty() { adb.clone() } else { Adb::new(Some(r.serial.clone())) };
                apps::set_enabled(&target, &r.packages, true)?;
                match undo::retire(&r.stamp) {
                    Some(p) => println!("restored; journal entry retired as {}", p.display()),
                    None => println!("restored"),
                }
            }
        }
        Cmd::Apply { file, dry_run, yes, all_devices } => {
            let path = file.unwrap_or_else(profile::default_path);
            let profile = profile::load(&path)?;
            let steps = profile::plan(&profile);
            if steps.is_empty() {
                println!("{} names nothing to change", path.display());
                return Ok(());
            }
            for s in &steps {
                println!("{s}");
            }
            if dry_run {
                return Ok(());
            }
            if !yes && !confirm(&format!("apply {} change(s) from {}?", steps.len(), path.display()))? {
                println!("aborted");
                return Ok(());
            }
            // A profile may name its own TV; --serial still wins.
            let adb = match (cli.serial.is_none(), profile.serial.clone()) {
                (true, Some(s)) => Adb::new(Some(s)),
                _ => adb,
            };
            run_each(&adb, all_devices, |a| profile::apply(a, &profile))?;
        }
        Cmd::Fleet { op } => match op {
            FleetOp::Status => {
                let tvs: Vec<Adb> = adb
                    .devices()?
                    .into_iter()
                    .filter(|d| d.state == "device")
                    .map(|d| Adb::new(Some(d.serial)))
                    .collect();
                if tvs.is_empty() {
                    println!("no authorized TV; run: atvtools devices");
                }
                for a in tvs {
                    let serial = a.serial.clone().unwrap_or_default();
                    let model = prop_value(&a, "ro.product.model").unwrap_or_else(|| "-".to_string());
                    let android = prop_value(&a, "ro.build.version.release").unwrap_or_else(|| "-".to_string());
                    let disabled = apps::disabled(&a, apps::Scope::ThirdParty).map(|d| d.len()).unwrap_or(0);
                    println!("{serial:<24} {model:<16} android {android:<5} {disabled} disabled");
                }
            }
            FleetOp::Sync { file, dry_run, yes } => {
                let path = file.unwrap_or_else(profile::default_path);
                let profile = profile::load(&path)?;
                let steps = profile::plan(&profile);
                if steps.is_empty() {
                    println!("{} names nothing to change", path.display());
                    return Ok(());
                }
                for s in &steps {
                    println!("{s}");
                }
                if dry_run {
                    return Ok(());
                }
                if !yes && !confirm(&format!("apply {} change(s) from {} to every TV?", steps.len(), path.display()))? {
                    println!("aborted");
                    return Ok(());
                }
                run_each(&adb, true, |a| profile::apply(a, &profile))?;
            }
        },
        Cmd::Update { beacon } => print!("{}", update::check(env!("CARGO_PKG_VERSION"), beacon.as_deref())?),
        Cmd::Apk { op } => match op {
            ApkOp::Info { file } => {
                let b = apk::info(&file)?;
                println!("package      : {}", b.package);
                println!("label        : {}", b.label);
                println!("version      : {} ({})", b.version_name, b.version_code);
                println!("sdk / target : {} / {}", b.sdk, b.target_sdk);
                if let Some(a) = b.launchable {
                    println!("launchable   : {a}");
                }
            }
            ApkOp::List { dir } => {
                for f in apk::apks(&dir)? {
                    match apk::info(&f) {
                        Ok(b) => println!("{:<40} {:<28} {}", b.package, b.label, b.version_name),
                        Err(e) => println!("{:<40} ({e})", f.display()),
                    }
                }
            }
        },
        Cmd::Google { dir } => google::install(&adb, &dir)?,
        Cmd::Voice { words } => device::assist(&adb, &words)?,
        Cmd::Menu { lang } => menu::run(&lang)?,
        Cmd::Uad { op } => match op {
            UadOp::List { level, scope, refresh, full } => {
                let hits = uad::scan(&adb, scope.as_scope(), &level, refresh)?;
                if hits.is_empty() {
                    println!("nothing installed is flagged at level '{level}'");
                }
                for h in hits {
                    println!("{:<11} {:<7} {}", h.removal, h.list, h.package);
                    let lines = h.description.lines().filter(|l| !l.trim().is_empty());
                    if full {
                        for line in lines {
                            println!("             {line}");
                        }
                    } else if let Some(first) = lines.into_iter().next() {
                        println!("             {first}");
                    }
                }
            }
            UadOp::Info { package } => {
                let db = uad::load(false)?;
                match uad::find(&db, &package) {
                    Some(e) => {
                        println!("{} ({})", e.id, e.list);
                        println!("removal: {}", e.removal);
                        for line in e.description.lines() {
                            println!("{line}");
                        }
                    }
                    None => println!("{package} is not in the UAD database"),
                }
            }
            UadOp::Apply { level, scope, refresh, yes } => {
                let hits = uad::scan(&adb, scope.as_scope(), &level, refresh)?;
                if hits.is_empty() {
                    println!("nothing installed is flagged at level '{level}'");
                    return Ok(());
                }
                for h in &hits {
                    println!("{:<11} {}", h.removal, h.package);
                }
                let pkgs: Vec<String> = hits.iter().map(|h| h.package.clone()).collect();
                if !yes && !confirm(&format!("disable {} packages at level '{level}'? (atvtools uad apply level unsafe --yes to be blunt)", pkgs.len()))? {
                    println!("aborted");
                    return Ok(());
                }
                apps::set_enabled(&adb, &pkgs, false)?;
                journals(&adb, &format!("uad {level}"), &pkgs);
            }
            UadOp::Gui { refresh } => {
                println!("{}", uad::launch_gui(&adb, refresh)?);
            }
            UadOp::Refresh => {
                let db = uad::load(true)?;
                println!("{} packages cached at {}", db.len(), uad::cache_path().map(|p| p.display().to_string()).unwrap_or_else(|| "?".to_string()));
            }
        },
        Cmd::Gui => gui::run(cli.serial.clone())?,
    }
    Ok(())
}

/// Blocks until the target answers, so a script can start before the TV has
/// finished booting or before the network handle is up.
fn wait_for(adb: &Adb) -> Result<()> {
    const LIMIT: std::time::Duration = std::time::Duration::from_secs(120);
    let deadline = std::time::Instant::now() + LIMIT;
    loop {
        let found = adb.devices()?;
        let ready = match &adb.serial {
            Some(s) => found.iter().any(|d| d.serial == *s && d.state == "device"),
            None => found.iter().any(|d| d.state == "device"),
        };
        if ready {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            let seen: Vec<String> = found.iter().map(|d| format!("{} ({})", d.serial, d.state)).collect();
            anyhow::bail!("no authorized device after 120s; adb sees: {}", if seen.is_empty() { "nothing".to_string() } else { seen.join(", ") });
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

/// Files a batch in the undo journal. A journal that cannot be written is
/// reported but never undoes a disable that already happened.
fn journals(adb: &Adb, what: &str, packages: &[String]) {
    match undo::note(what, adb.serial.as_deref(), packages) {
        Ok(p) => println!("journal: {} (atvtools undo puts these back)", p.display()),
        Err(e) => println!("journal: not written ({e})"),
    }
}

/// Connects, remembers the address and reports what answered.
/// The three ways to drive the remote: a key sequence, a macro file, or the prompt.
fn remote_arm(adb: &Adb, keys: Option<String>, script: Option<PathBuf>, repeat: u32) -> Result<()> {
    match (keys, script) {
        (Some(seq), _) => {
            let names = remote::expand(&seq);
            for _ in 0..repeat.max(1) {
                device::key(adb, &names)?;
            }
            println!(
                "sent {} key(s){}{}",
                names.len(),
                if repeat > 1 { format!(" x{repeat}") } else { String::new() },
                if names.is_empty() { " (nothing matched)" } else { "" }
            );
        }
        (None, Some(path)) => println!("{}", remote::script(adb, &path)?),
        (None, None) => remote::run(adb)?,
    }
    Ok(())
}

pub fn connect_to(adb: &Adb, host: &str) -> Result<String> {
    let target = net::normalize_target(host, net::ADB_PORT);
    let out = adb.connect(&target)?.trim().to_string();
    if !out.to_ascii_lowercase().contains("connected to") {
        anyhow::bail!("{out}");
    }
    remember(&target);
    let named = Adb::new(Some(target.clone()));
    let model = prop_value(&named, "ro.product.model");
    let maker = prop_value(&named, "ro.product.manufacturer");
    if model.is_none() && maker.is_none() {
        // adb answered but the TV has not trusted this computer yet.
        let state = named.try_out(&["get-state"]).trim().to_string();
        return Ok(if state.contains("unauthorized") {
            format!("{out}\n  -> the TV is showing an authorization dialog: allow it, then press C again")
        } else {
            out
        });
    }
    let who = [maker, model].into_iter().flatten().collect::<Vec<_>>().join(" ");
    Ok(format!("{out} ({who})"))
}

/// A getprop answer, or None when adb failed and printed its own error at us.
fn prop_value(adb: &Adb, name: &str) -> Option<String> {
    let v = adb.try_out(&["shell", &format!("getprop {name}")]).trim().to_string();
    if plain_answer(&v) { Some(v) } else { None }
}

fn plain_answer(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 40
        && !v.contains('\n')
        && !v.to_ascii_lowercase().contains("failed")
        && !v.to_ascii_lowercase().contains("error")
        && !v.to_ascii_lowercase().contains("unauthorized")
}

/// ~/.atvtools/history.txt - the original kept a history of TV devices.
pub fn history_file() -> Option<PathBuf> {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).ok()?;
    let dir = PathBuf::from(home).join(".atvtools");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("history.txt"))
}

pub fn history() -> Vec<String> {
    let mut v = history_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|s| s.lines().map(str::to_string).filter(|l| !l.trim().is_empty()).collect())
        .unwrap_or_else(Vec::new);
    v.dedup();
    v
}

pub fn remember(host: &str) {
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


/// Tests for the fan-out helper and for pairing never carrying a "-s".
#[cfg(all(test, unix))]
mod cli_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    fn fake_adb(dir: &Path, body: &str) -> String {
        let path = dir.join("adb");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write fake adb");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        path.to_string_lossy().to_string()
    }

    /// One directory per test: they run in parallel and each writes its own fake adb.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("atvtools-cli-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn without_all_devices_the_given_handle_is_the_only_target() {
        let adb = Adb { serial: Some("tv:5555".into()), bin: "/nonexistent/adb".into() };
        let targets = fanout(&adb, false).expect("one target");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].serial.as_deref(), Some("tv:5555"));
    }

    #[test]
    fn all_devices_keeps_only_authorized_tvs() {
        let dir = scratch("devices");
        let bin = fake_adb(
            &dir,
            r#"echo 'List of devices attached'
printf 'tv1:5555\tdevice product:x model:TV_One transport_id:1\n'
printf 'tv2:5555\tunauthorized usb:1-1\n'"#,
        );
        let adb = Adb { serial: None, bin };
        let targets = fanout(&adb, true).expect("targets");
        let serials: Vec<String> = targets.iter().filter_map(|t| t.serial.clone()).collect();
        assert_eq!(serials, vec!["tv1:5555".to_string()]);
    }

    #[test]
    fn pairing_runs_without_a_device_argument() {
        let dir = scratch("pair");
        let argv = dir.join("argv.txt");
        let bin = fake_adb(&dir, &format!(r#"printf '%s\n' "$@" > {}"#, argv.display()));
        let adb = Adb { serial: Some("tv:5555".into()), bin };
        adb.pair("192.168.1.105:37061", "123456").expect("pair");
        let seen = std::fs::read_to_string(&argv).expect("argv file");
        assert_eq!(
            seen.lines().collect::<Vec<_>>(),
            vec!["pair", "192.168.1.105:37061", "123456"]
        );
    }
}
