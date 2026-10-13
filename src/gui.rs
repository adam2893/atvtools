//! Native desktop window (egui) over the same functions the CLI calls.
//!
//! shortcut: actions run on the UI thread, so a slow adb call (pull, download,
//! backup) freezes the window while it runs; move them to worker threads if the
//! stutter starts to hurt.

use crate::adb::Adb;
use crate::apps::{self, Scope};
use crate::{catalog, device, download, net, screen, settings, transfer, uad};
use anyhow::Result;
use eframe::egui;
use std::path::PathBuf;

const SHOT: &str = "atvtools-shot.png";
const SCOPES: [&str; 3] = ["third-party", "system", "all"];

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Connect,
    Apps,
    Files,
    Debloat,
    Settings,
    Screen,
    Remote,
    Helpers,
}

/// Every activity line also lands in ~/.atvtools/log, one rotation deep: once the
/// file passes MAX_LOG_BYTES it is renamed to log.1 and started fresh.
const MAX_LOG_BYTES: u64 = 1_000_000;

fn append_log(line: &str) {
    use std::io::Write as _;
    let Some(dir) = uad::home_dir().map(|h| h.join(".atvtools")) else { return };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("log");
    if std::fs::metadata(&path).map(|m| m.len() > MAX_LOG_BYTES).unwrap_or(false) {
        let _ = std::fs::rename(&path, dir.join("log.1"));
    }
    let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let _ = writeln!(file, "{line}");
}

pub fn run(serial: Option<String>) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1020.0, 720.0])
            .with_min_inner_size([780.0, 540.0])
            .with_title("atvtools"),
        ..Default::default()
    };
    eframe::run_native(
        "atvtools",
        options,
        Box::new(move |_cc| Ok(Box::new(App::new(serial)) as Box<dyn eframe::App>)),
    )
    .map_err(|e| anyhow::anyhow!("could not start the GUI: {e}"))
}

struct App {
    adb: Adb,
    serial_input: String,
    tab: Tab,
    log: Vec<String>,
    found: Vec<String>,
    packages: Vec<apps::PkgRow>,
    scope: usize,
    filter: String,
    ticked: Vec<String>,
    hits: Vec<uad::Hit>,
    level: usize,
    confirm: bool,
    text_in: String,
    url_in: String,
    dpi: String,
    scale: String,
    seconds: String,
    remote_path: String,
    local_path: String,
    /// Attached devices, refreshed by the Connect tab; picking one retargets the app.
    attached: Vec<crate::adb::Device>,
    /// Decoded screenshot, shown inline under the Screen tab.
    shot: Option<egui::TextureHandle>,
}

impl App {
    fn new(serial: Option<String>) -> Self {
        let serial_input = serial.clone().unwrap_or_default();
        let mut app = App {
            adb: Adb::new(serial),
            serial_input,
            tab: Tab::Connect,
            log: Vec::new(),
            found: Vec::new(),
            packages: Vec::new(),
            scope: 0,
            filter: String::new(),
            ticked: Vec::new(),
            hits: Vec::new(),
            level: 0,
            confirm: false,
            text_in: String::new(),
            url_in: String::new(),
            dpi: String::new(),
            scale: String::new(),
            seconds: "20".to_string(),
            remote_path: transfer::DOWNLOADS.to_string(),
            local_path: String::new(),
            attached: Vec::new(),
            shot: None,
        };
        app.say("atvtools GUI. Connect to a TV on the first tab, then pick a tab above.");
        app
    }

    fn scope(&self) -> Scope {
        match self.scope {
            0 => Scope::ThirdParty,
            1 => Scope::System,
            _ => Scope::All,
        }
    }

    fn say(&mut self, line: impl Into<String>) {
        let line = line.into();
        if !line.trim().is_empty() {
            self.log.push(line.clone());
            append_log(&line);
        }
        if self.log.len() > 400 {
            self.log.drain(0..200);
        }
    }

    fn act(&mut self, what: &str, f: impl FnOnce(&Adb) -> Result<String>) {
        let res = f(&self.adb);
        match res {
            Ok(v) => self.say(format!("{what}: {v}")),
            Err(e) => self.say(format!("{what}: {e}")),
        }
    }

    fn tick(&self, package: &str) -> bool {
        self.ticked.iter().any(|p| p == package)
    }

    fn toggle_tick(&mut self, package: &str) {
        if let Some(i) = self.ticked.iter().position(|p| p == package) {
            self.ticked.remove(i);
        } else {
            self.ticked.push(package.to_string());
        }
    }

    fn status_line(&self, ui: &mut egui::Ui) {
        let target = if self.serial_input.trim().is_empty() {
            "adb default".to_string()
        } else {
            self.serial_input.trim().to_string()
        };
        ui.label(egui::RichText::new(format!("target: {target}    adb: {}", self.adb.bin)).small());
    }

    fn connect_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Connect");
        self.status_line(ui);
        ui.horizontal(|ui| {
            ui.label("TV address (host:port; empty = adb default)");
            ui.add(egui::TextEdit::singleline(&mut self.serial_input).desired_width(220.0));
            if ui.button("use it").clicked() {
                let s = self.serial_input.trim().to_string();
                self.adb = Adb::new(if s.is_empty() { None } else { Some(s.clone()) });
                self.say(format!("target set to {}", if s.is_empty() { "adb default".to_string() } else { s }));
            }
        });
        ui.horizontal(|ui| {
            if ui.button("scan the network").clicked() {
                let hosts = net::candidates(&self.adb);
                let answers = net::probe(&self.adb, &hosts);
                self.say(format!(
                    "scanned {} neighbours, {} answered on {}",
                    hosts.len(),
                    answers.len(),
                    net::ADB_PORT
                ));
                self.found = answers;
            }
            if ui.button("list devices").clicked() {
                match self.adb.devices() {
                    Ok(d) => {
                        self.say(format!("{} device(s) attached", d.len()));
                        self.attached = d;
                    }
                    Err(e) => self.say(format!("devices: {e}")),
                }
            }
        });
        let mut pick: Option<String> = None;
        for host in &self.found {
            ui.horizontal(|ui| {
                ui.monospace(host);
                if ui.button("connect").clicked() {
                    pick = Some(host.clone());
                }
            });
        }
        let mut use_this: Option<String> = None;
        for d in &self.attached {
            ui.horizontal(|ui| {
                ui.monospace(format!("{:<24} {:<16} {}", d.serial, d.model.as_deref().unwrap_or("-"), d.state));
                if ui.small_button("use").clicked() {
                    use_this = Some(d.serial.clone());
                }
            });
        }
        if let Some(serial) = use_this {
            self.serial_input = serial.clone();
            self.adb = Adb::new(Some(serial.clone()));
            self.say(format!("target set to {serial}"));
        }
        if let Some(host) = pick {
            let res = crate::connect_to(&self.adb, &host);
            match res {
                Ok(name) => {
                    self.serial_input = host.clone();
                    self.adb = Adb::new(Some(host));
                    self.say(name);
                }
                Err(e) => self.say(format!("connect: {e}")),
            }
        }
        ui.separator();
        ui.label("Tips: enable network debugging on the TV, accept the Allow debugging? dialog, and keep both on the same network.");
        ui.label("No adb? brew install --cask android-platform-tools  (macOS) or your distro package.");
    }

    fn apps_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Apps");
        ui.horizontal(|ui| {
            ui.label("scope");
            egui::ComboBox::from_id_salt("scope")
                .selected_text(SCOPES[self.scope])
                .show_ui(ui, |ui| {
                    for (i, name) in SCOPES.iter().enumerate() {
                        ui.selectable_value(&mut self.scope, i, *name);
                    }
                });
            if ui.button("reload").clicked() {
                let rows = apps::list_rows(&self.adb, self.scope(), true, true);
                match rows {
                    Ok(r) => {
                        self.say(format!("{} packages listed", r.len()));
                        self.packages = r;
                        self.ticked.clear();
                    }
                    Err(e) => self.say(format!("apps: {e}")),
                }
            }
            ui.label("filter");
            ui.add(egui::TextEdit::singleline(&mut self.filter).desired_width(160.0));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{} ticked", self.ticked.len()));
            if ui.button("disable").clicked() {
                let pkgs = std::mem::take(&mut self.ticked);
                self.act("disable", move |adb| {
                    apps::set_enabled(adb, &pkgs, false).map(|_| format!("disabled {}", pkgs.len()))
                });
            }
            if ui.button("enable").clicked() {
                let pkgs = std::mem::take(&mut self.ticked);
                self.act("enable", move |adb| {
                    apps::set_enabled(adb, &pkgs, true).map(|_| format!("enabled {}", pkgs.len()))
                });
            }
            if ui.button("uninstall").clicked() {
                let pkgs = std::mem::take(&mut self.ticked);
                self.act("uninstall", move |adb| {
                    apps::uninstall(adb, &pkgs).map(|_| format!("uninstalled {}", pkgs.len()))
                });
            }
            if ui.button("launch").clicked() {
                let pkgs = std::mem::take(&mut self.ticked);
                self.act("launch", move |adb| {
                    for p in &pkgs {
                        apps::launch(adb, p)?;
                    }
                    Ok(format!("launched {}", pkgs.len()))
                });
            }
            if ui.button("force stop").clicked() {
                let pkgs = std::mem::take(&mut self.ticked);
                self.act("stop", move |adb| {
                    for p in &pkgs {
                        apps::force_stop(adb, p)?;
                    }
                    Ok(format!("stopped {}", pkgs.len()))
                });
            }
            if ui.button("info").clicked() {
                let pkgs = std::mem::take(&mut self.ticked);
                self.act("info", move |adb| {
                    let mut out = String::new();
                    for p in &pkgs {
                        out.push_str(&apps::info(adb, p)?);
                    }
                    Ok(out)
                });
            }
        });
        let rows: Vec<(String, String)> = self
            .packages
            .iter()
            .filter(|r| {
                self.filter.is_empty()
                    || r.package
                        .to_ascii_lowercase()
                        .contains(&self.filter.to_ascii_lowercase())
            })
            .map(|r| (r.package.clone(), r.installer.clone().unwrap_or_default()))
            .collect();
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (package, installer) in rows {
                    ui.horizontal(|ui| {
                        let mut on = self.tick(&package);
                        if ui.checkbox(&mut on, "").changed() {
                            self.toggle_tick(&package);
                        }
                        ui.monospace(&package);
                        if !installer.is_empty() {
                            ui.label(egui::RichText::new(installer).small());
                        }
                    });
                }
            });
    }

    fn files_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Files");
        ui.label("adb moves the bytes; no app on the TV and no open port are needed.");
        ui.horizontal(|ui| {
            ui.label("on the TV");
            ui.add(egui::TextEdit::singleline(&mut self.remote_path).desired_width(280.0));
        });
        ui.horizontal(|ui| {
            ui.label("on this machine");
            ui.add(egui::TextEdit::singleline(&mut self.local_path).desired_width(280.0));
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("list TV folder").clicked() {
                let remote = self.remote_path.clone();
                self.act("ls", move |adb| Ok(transfer::ls(adb, &remote)?.trim().to_string()));
            }
            if ui.button("send to TV").clicked() {
                let remote = self.remote_path.clone();
                let local = PathBuf::from(self.local_path.clone());
                self.act("push", move |adb| {
                    transfer::push(adb, &local, &remote)?;
                    Ok(format!("sent {}", local.display()))
                });
            }
            if ui.button("copy from TV").clicked() {
                let remote = self.remote_path.clone();
                let local = PathBuf::from(self.local_path.clone());
                self.act("pull", move |adb| {
                    transfer::pull(adb, &remote, &local)?;
                    Ok(format!("copied to {}", local.display()))
                });
            }
            if ui.button("make folder").clicked() {
                let remote = self.remote_path.clone();
                self.act("mkdir", move |adb| {
                    transfer::mkdir(adb, &remote)?;
                    Ok(format!("created {remote}"))
                });
            }
        });
        ui.label(format!("sideloads land in {}", transfer::DOWNLOADS));
    }

    fn debloat_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Debloat");
        ui.label("Levels and descriptions come from 0x192/universal-android-debloater. Nothing changes until you press disable.");
        ui.horizontal(|ui| {
            ui.label("scope");
            egui::ComboBox::from_id_salt("debloat-scope")
                .selected_text(SCOPES[self.scope])
                .show_ui(ui, |ui| {
                    for (i, name) in SCOPES.iter().enumerate() {
                        ui.selectable_value(&mut self.scope, i, *name);
                    }
                });
            ui.label("level");
            egui::ComboBox::from_id_salt("level")
                .selected_text(uad::LEVELS[self.level])
                .show_ui(ui, |ui| {
                    for (i, name) in uad::LEVELS.iter().enumerate() {
                        ui.selectable_value(&mut self.level, i, *name);
                    }
                });
            if ui.button("scan installed").clicked() {
                let scope = self.scope();
                let level = uad::LEVELS[self.level];
                match uad::scan(&self.adb, scope, level, false) {
                    Ok(h) => {
                        self.say(format!("{} packages flagged as {level}", h.len()));
                        self.hits = h;
                    }
                    Err(e) => self.say(format!("debloat: {e}")),
                }
            }
            if ui.button("refresh database").clicked() {
                match uad::load(true) {
                    Ok(db) => self.say(format!("database: {} packages cached", db.len())),
                    Err(e) => self.say(format!("debloat: {e}")),
                }
            }
        });
        ui.checkbox(
            &mut self.confirm,
            "I understand a disabled app disappears from the launcher (enable restores it)",
        );
        ui.horizontal(|ui| {
            let n = self.hits.len();
            let pkgs: Vec<String> = self.hits.iter().map(|h| h.package.clone()).collect();
            if ui
                .add_enabled(self.confirm && n > 0, egui::Button::new(format!("disable {n}")))
                .clicked()
            {
                let pkgs = pkgs.clone();
                self.act("disable", move |adb| {
                    apps::set_enabled(adb, &pkgs, false).map(|_| format!("disabled {}", pkgs.len()))
                });
            }
            if ui
                .add_enabled(self.confirm && n > 0, egui::Button::new(format!("re-enable {n}")))
                .clicked()
            {
                let pkgs = pkgs.clone();
                self.act("re-enable", move |adb| {
                    apps::set_enabled(adb, &pkgs, true).map(|_| format!("enabled {}", pkgs.len()))
                });
            }
        });
        ui.separator();
        let hits = self.hits.clone();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for h in &hits {
                    ui.horizontal(|ui| {
                        ui.monospace(&h.package);
                        ui.label(egui::RichText::new(format!("{} / {}", h.removal, h.list)).small());
                    });
                    if let Some(first) = h.description.lines().find(|l| !l.trim().is_empty()) {
                        ui.label(egui::RichText::new(first).small());
                    }
                }
            });
    }

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.horizontal(|ui| {
            ui.label("density");
            ui.add(egui::TextEdit::singleline(&mut self.dpi).desired_width(70.0));
            if ui.button("read").clicked() {
                self.act("density", |adb| settings::density(adb));
            }
            if ui.button("set").clicked() {
                match self.dpi.trim().parse::<u32>() {
                    Ok(dpi) => self.act("density", move |adb| {
                        settings::set_density(adb, dpi).map(|_| format!("{dpi} dpi"))
                    }),
                    Err(_) => self.say("density: enter a number of dpi (72 or more)"),
                }
            }
            if ui.button("reset").clicked() {
                self.act("density", |adb| settings::reset_density(adb).map(|_| "reset".to_string()));
            }
        });
        ui.horizontal(|ui| {
            ui.label("font scale");
            ui.add(egui::TextEdit::singleline(&mut self.scale).desired_width(70.0));
            if ui.button("read").clicked() {
                self.act("font scale", |adb| settings::font_scale(adb));
            }
            if ui.button("set").clicked() {
                match self.scale.trim().parse::<f32>() {
                    Ok(s) => self.act("font scale", move |adb| {
                        settings::set_font_scale(adb, s).map(|_| format!("{s}"))
                    }),
                    Err(_) => self.say("font scale: enter a number between 0.5 and 2.0"),
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("AdGuard DNS on").clicked() {
                self.act("adblock", |adb| settings::adblock_on(adb).map(|_| settings::ADGUARD_DNS.to_string()));
            }
            if ui.button("AdGuard DNS off").clicked() {
                self.act("adblock", |adb| settings::adblock_off(adb).map(|_| "off".to_string()));
            }
            if ui.button("DNS state").clicked() {
                self.act("dns", |adb| settings::dns_state(adb));
            }
            if ui.button("block OTA").clicked() {
                self.act("ota", |adb| settings::ota(adb, true).map(|_| "blocked".to_string()));
            }
            if ui.button("OTA state").clicked() {
                self.act("ota", |adb| settings::ota_state(adb));
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("toggles");
            for name in settings::TOGGLES.iter().map(|t| t.name) {
                if ui.small_button(name).clicked() {
                    self.act(name, move |adb| settings::toggle(adb, name, None));
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("screens");
            for name in device::SCREENS.iter().map(|(n, _)| *n) {
                if ui.small_button(name).clicked() {
                    self.act(name, move |adb| {
                        device::screen(adb, name).map(|_| format!("opened {name}"))
                    });
                }
            }
        });
    }

    fn screen_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Screen");
        ui.horizontal(|ui| {
            if ui.button("screenshot").clicked() {
                let dest = std::env::temp_dir().join(SHOT);
                let path = dest.display().to_string();
                match screen::screenshot(&self.adb, &dest) {
                    Ok(()) => self.say(format!("screenshot: saved {path}")),
                    Err(e) => self.say(format!("screenshot: {e}")),
                }
                self.preview(&dest, ui.ctx());
            }
            ui.label("record seconds");
            ui.add(egui::TextEdit::singleline(&mut self.seconds).desired_width(60.0));
            if ui.button("record").clicked() {
                match self.seconds.trim().parse::<u32>() {
                    Ok(secs) => {
                        let dest = std::env::temp_dir().join("atvtools-record.mp4");
                        let path = dest.display().to_string();
                        self.act("record", move |adb| {
                            screen::record(adb, secs, std::path::Path::new(&path))
                                .map(|_| format!("saved {path}"))
                        });
                    }
                    Err(_) => self.say("record: enter a number of seconds"),
                }
            }
            if ui.button("scrcpy (separate window)").clicked() {
                match screen::scrcpy(self.adb.serial.as_deref(), &[]) {
                    Ok(()) => self.say("scrcpy: closed"),
                    Err(e) => self.say(format!("scrcpy: {e}")),
                }
            }
        });
        if let Some(tex) = &self.shot {
            ui.add(egui::Image::new(egui::load::SizedTexture::from_handle(tex)).max_width(420.0));
        }
        ui.separator();
        ui.label("scrcpy mirrors the TV on this machine; it is an external binary (SCRCPY env var or PATH).");
    }

    /// Decode the file we just pulled and put it on screen; the reader only
    /// handles the shapes screencap emits, so an odd PNG just gets a note.
    fn preview(&mut self, path: &std::path::Path, ctx: &egui::Context) {
        self.shot = None;
        let Ok(bytes) = std::fs::read(path) else { return };
        match png_rgba(&bytes) {
            Some((w, h, rgba)) => {
                let image = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                self.shot = Some(ctx.load_texture("shot", image, egui::TextureOptions::LINEAR));
            }
            None => self.say("preview: nothing to draw; open the file instead"),
        }
    }

    fn remote_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Remote");
        ui.horizontal_wrapped(|ui| {
            for (name, _) in device::KEYS {
                if ui.small_button(*name).clicked() {
                    let key = name.to_string();
                    self.act(name, move |adb| {
                        device::key(adb, &[key]).map(|_| "sent".to_string())
                    });
                }
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("text");
            ui.add(egui::TextEdit::singleline(&mut self.text_in).desired_width(320.0));
            if ui.button("send").clicked() {
                let words: Vec<String> = self.text_in.split_whitespace().map(|s| s.to_string()).collect();
                self.act("text", move |adb| {
                    device::text(adb, &words).map(|_| "typed".to_string())
                });
            }
        });
        ui.horizontal(|ui| {
            ui.label("url");
            ui.add(egui::TextEdit::singleline(&mut self.url_in).desired_width(320.0));
            if ui.button("open").clicked() {
                let url = self.url_in.clone();
                self.act("open", move |adb| {
                    device::open_url(adb, &url).map(|_| format!("opened {url}"))
                });
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("wake").clicked() {
                self.act("wake", |adb| device::wake(adb).map(|_| "sent".to_string()));
            }
            if ui.button("sleep").clicked() {
                self.act("sleep", |adb| device::sleep(adb).map(|_| "sent".to_string()));
            }
            if ui.button("reboot").clicked() {
                self.act("reboot", |adb| device::reboot(adb, None).map(|_| "sent".to_string()));
            }
            if ui.button("notifications").clicked() {
                self.act("notifications", |adb| {
                    device::expand_notifications(adb).map(|_| "expanded".to_string())
                });
            }
        });
    }

    fn helpers_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Helpers");
        ui.horizontal_wrapped(|ui| {
            if ui.button("doctor").clicked() {
                self.act("doctor", |adb| crate::doctor::run(adb));
            }
            if ui.button("doctor --fix").clicked() {
                self.act("doctor --fix", |adb| crate::doctor::fix(adb));
            }
            if ui.button("device info").clicked() {
                self.act("info", |adb| {
                    let rows = device::info(adb)?;
                    Ok(rows
                        .iter()
                        .map(|(k, v)| format!("{k}: {v}"))
                        .collect::<Vec<_>>()
                        .join("  "))
                });
            }
            if ui.button("optimize (animations, cache)").clicked() {
                self.act("optimize", |adb| {
                    device::optimize(adb)
                });
            }
            if ui.button("uad gui (universal debloater)").clicked() {
                self.act("uad gui", |adb| uad::launch_gui(adb, false));
            }
            if ui.button("check for updates").clicked() {
                self.act("update", |_adb| {
                    crate::update::check(env!("CARGO_PKG_VERSION"), None)
                });
            }
        });
        ui.separator();
        ui.label("Catalog: download and install on the TV");
        for e in catalog::CATALOG {
            ui.horizontal(|ui| {
                let install = ui.small_button("install").clicked();
                let replace = ui
                    .small_button("replace")
                    .on_hover_text("remove the other build of the same id first (official Shizuku -> fork)")
                    .clicked();
                if install || replace {
                    let url = e.url.to_string();
                    let name = e.name.to_string();
                    let label = name.clone();
                    self.act(&label, move |adb| {
                        let abis = catalog::abis(adb);
                        let abis: Vec<&str> = abis.iter().map(String::as_str).collect();
                        let url = catalog::resolve(&url, &abis)?;
                        let dest = std::env::temp_dir().join("atvtools").join(format!("{name}.apk"));
                        download::fetch(&url, &dest)?;
                        apps::install_with(adb, &[dest], replace)?;
                        Ok(format!("installed {name}{}", if replace { " (replacing)" } else { "" }))
                    });
                }
                ui.monospace(e.name);
                ui.label(egui::RichText::new(e.note).small());
            });
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let tab = self.tab;
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Connect, "Connect");
                ui.selectable_value(&mut self.tab, Tab::Apps, "Apps");
                ui.selectable_value(&mut self.tab, Tab::Files, "Files");
                ui.selectable_value(&mut self.tab, Tab::Debloat, "Debloat");
                ui.selectable_value(&mut self.tab, Tab::Settings, "Settings");
                ui.selectable_value(&mut self.tab, Tab::Screen, "Screen");
                ui.selectable_value(&mut self.tab, Tab::Remote, "Remote");
                ui.selectable_value(&mut self.tab, Tab::Helpers, "Helpers");
            });
        });
        egui::Panel::bottom("activity")
            .resizable(true)
            .default_size(150.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("activity");
                    if ui.small_button("clear").clicked() {
                        self.log.clear();
                    }
                });
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for line in &self.log {
                            ui.monospace(line);
                        }
                    });
            });
        egui::CentralPanel::default().show(ui, |ui| match tab {
            Tab::Connect => self.connect_tab(ui),
            Tab::Apps => self.apps_tab(ui),
            Tab::Files => self.files_tab(ui),
            Tab::Debloat => self.debloat_tab(ui),
            Tab::Settings => self.settings_tab(ui),
            Tab::Screen => self.screen_tab(ui),
            Tab::Remote => self.remote_tab(ui),
            Tab::Helpers => self.helpers_tab(ui),
        });
    }
}

/// Minimal PNG reader for the screenshots we pull: 8-bit RGB or RGBA, not
/// interlaced. Decoding here keeps an image crate out of the build.
fn png_rgba(bytes: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if !bytes.starts_with(&SIG) {
        return None;
    }
    let (mut w, mut h, mut channels) = (0usize, 0usize, 0usize);
    let mut idat: Vec<u8> = Vec::new();
    let mut at = 8;
    while at + 8 <= bytes.len() {
        let len = be32(bytes, at)? as usize;
        let kind = bytes.get(at + 4..at + 8)?;
        let body = bytes.get(at + 8..at + 8 + len)?;
        match kind {
            b"IHDR" => {
                w = be32(body, 0)? as usize;
                h = be32(body, 4)? as usize;
                if body.get(8).copied() != Some(8) {
                    return None; // palette or 16-bit: not what screencap writes
                }
                channels = match body.get(9).copied()? {
                    2 => 3,
                    6 => 4,
                    _ => return None,
                };
                if body.get(12).copied() != Some(0) {
                    return None; // interlaced
                }
            }
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len; // length, kind, body, crc
    }
    if w == 0 || h == 0 {
        return None;
    }
    // IDAT is zlib-framed deflate; the raw variant is what the zip reader needs.
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&idat).ok()?;
    let stride = w * channels;
    let mut out = vec![0u8; w * h * 4];
    let mut prev = vec![0u8; stride];
    for y in 0..h {
        let row = raw.get(y * (stride + 1)..(y + 1) * (stride + 1))?;
        let filter = row[0];
        let mut cur = row[1..].to_vec();
        for i in 0..stride {
            let a = if i >= channels { cur[i - channels] } else { 0 };
            let b = prev[i];
            let c = if i >= channels { prev[i - channels] } else { 0 };
            let left = a as u16;
            let up = b as u16;
            cur[i] = match filter {
                0 => cur[i],
                1 => cur[i].wrapping_add(a),
                2 => cur[i].wrapping_add(b),
                3 => cur[i].wrapping_add(((left + up) / 2) as u8),
                4 => cur[i].wrapping_add(paeth(a, b, c)),
                _ => return None,
            };
        }
        for x in 0..w {
            let (r, g, bl, al) = match channels {
                3 => (cur[x * 3], cur[x * 3 + 1], cur[x * 3 + 2], 255),
                _ => (cur[x * 4], cur[x * 4 + 1], cur[x * 4 + 2], cur[x * 4 + 3]),
            };
            let o = (y * w + x) * 4;
            out[o] = r;
            out[o + 1] = g;
            out[o + 2] = bl;
            out[o + 3] = al;
        }
        prev = cur;
    }
    Some((w, h, out))
}

fn be32(d: &[u8], at: usize) -> Option<u32> {
    let b = d.get(at..at + 4)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let pa = (b as i16 - c as i16).abs();
    let pb = (a as i16 - c as i16).abs();
    let pc = (a as i16 + b as i16 - 2 * c as i16).abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut c = (body.len() as u32).to_be_bytes().to_vec();
        c.extend_from_slice(kind);
        c.extend_from_slice(body);
        c.extend_from_slice(&[0, 0, 0, 0]); // the reader ignores CRCs
        c
    }

    fn tiny_png() -> Vec<u8> {
        let mut ihdr = 2u32.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA, no interlace
        let raw = vec![0u8, 255, 0, 0, 255, 0, 255, 0, 255]; // filter byte + 2 pixels
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        png.extend(chunk(b"IHDR", &ihdr));
        png.extend(chunk(b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6)));
        png.extend(chunk(b"IEND", &[]));
        png
    }

    #[test]
    fn a_screencap_shaped_png_decodes() {
        let (w, h, rgba) = png_rgba(&tiny_png()).unwrap();
        assert_eq!((w, h), (2, 1));
        assert_eq!(rgba, vec![255, 0, 0, 255, 0, 255, 0, 255]);
        assert!(png_rgba(b"not a png").is_none());
    }

    #[test]
    fn the_paeth_predictor_matches_the_spec() {
        assert_eq!(paeth(0, 0, 0), 0);
        // a=10, b=20, c=15: pa=5, pb=5, pc=0, and the tie goes to c.
        assert_eq!(paeth(10, 20, 15), 15);
    }
}
