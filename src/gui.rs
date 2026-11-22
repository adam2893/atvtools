//! Native desktop window (egui) with non-blocking background workers,
//! anti-slop cool-slate design system, and guided Android TV cleanup.

use crate::adb::{Adb, Device};
use crate::apps::{self, Scope};
use crate::{catalog, debloat, device, download, logs, net, profile, remote, screen, settings, shizuku, transfer, tv_safety, uad, undo};
use anyhow::Result;
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

const SHOT: &str = "atvtools-shot.png";
const SCOPES: [&str; 3] = ["third-party", "system", "all"];

// Locked design tokens: cool-slate neutral family + single blue accent.
const BG_WINDOW: Color32 = Color32::from_rgb(15, 20, 25);
const BG_PANEL: Color32 = Color32::from_rgb(20, 26, 33);
const BG_CARD: Color32 = Color32::from_rgb(27, 34, 44);
const BORDER_SUBTLE: Color32 = Color32::from_rgb(42, 52, 65);
const TEXT_PRIMARY: Color32 = Color32::from_rgb(230, 237, 243);
const TEXT_MUTED: Color32 = Color32::from_rgb(139, 148, 158);
const ACCENT: Color32 = Color32::from_rgb(59, 130, 246);
const STATUS_OK: Color32 = Color32::from_rgb(34, 197, 94);
const STATUS_WARN: Color32 = Color32::from_rgb(245, 158, 11);
const STATUS_ERR: Color32 = Color32::from_rgb(239, 68, 68);

const RADIUS_CARD: CornerRadius = CornerRadius::same(8);
const RADIUS_CONTROL: CornerRadius = CornerRadius::same(6);
const RADIUS_BADGE: CornerRadius = CornerRadius::same(10);

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Connect,
    Cleanup,
    Apps,
    Debloat,
    Settings,
    RemoteScreen,
    FilesCatalog,
}

impl Tab {
    const ALL: [(Self, &str); 7] = [
        (Self::Connect, "Connect"),
        (Self::Cleanup, "TV Cleanup"),
        (Self::Apps, "Apps"),
        (Self::Debloat, "Debloat (UAD)"),
        (Self::Settings, "Settings"),
        (Self::RemoteScreen, "Remote & Screen"),
        (Self::FilesCatalog, "Files & Catalog"),
    ];
}

#[derive(PartialEq, Clone, Copy)]
enum HelperChoice {
    Shizuku,
    Stellar,
}

enum WorkerMsg {
    FoundHosts(Vec<String>),
    Devices(Vec<Device>),
    Packages(Vec<apps::PkgRow>),
    UadHits(Vec<uad::Hit>),
    PrivacyFindings(Vec<tv_safety::PrivacyFinding>),
    Drift(tv_safety::DriftReport),
    Launchers(Vec<String>),
    Brand(String),
    Sizes(Vec<(String, u64, u64)>),
    LogOutput(String),
    Health(tv_safety::SystemHealth),
    Screenshot(PathBuf),
    SetTarget(String),
    Done(String),
}

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
            .with_inner_size([1080.0, 760.0])
            .with_min_inner_size([840.0, 580.0])
            .with_title("atvtools"),
        ..Default::default()
    };
    eframe::run_native(
        "atvtools",
        options,
        Box::new(move |cc| {
            apply_theme(&cc.egui_ctx);
            Ok(Box::new(App::new(serial)) as Box<dyn eframe::App>)
        }),
    )
    .map_err(|e| anyhow::anyhow!("could not start the GUI: {e}"))
}

fn apply_theme(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    let mut v = egui::Visuals::dark();
    v.window_fill = BG_WINDOW;
    v.panel_fill = BG_PANEL;
    v.extreme_bg_color = BG_WINDOW;
    v.faint_bg_color = BG_CARD;
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.noninteractive.bg_fill = BG_CARD;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    v.widgets.noninteractive.corner_radius = RADIUS_CONTROL;
    v.widgets.inactive.bg_fill = BG_CARD;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    v.widgets.inactive.corner_radius = RADIUS_CONTROL;
    v.widgets.hovered.bg_fill = Color32::from_rgb(36, 46, 59);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.hovered.corner_radius = RADIUS_CONTROL;
    v.widgets.active.bg_fill = ACCENT;
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.active.corner_radius = RADIUS_CONTROL;
    style.visuals = v;
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    ctx.set_style_of(egui::Theme::Dark, style);
}

fn card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, BORDER_SUBTLE))
        .corner_radius(RADIUS_CARD)
        .inner_margin(egui::Margin::same(14))
        .show(ui, add_contents);
}

fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.linear_multiply(0.18))
        .stroke(Stroke::new(1.0, color.linear_multiply(0.6)))
        .corner_radius(RADIUS_BADGE)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(color).small().strong());
        });
}

fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(Color32::WHITE).strong())
            .fill(ACCENT)
            .corner_radius(RADIUS_CONTROL),
    )
}

struct App {
    adb: Adb,
    serial_input: String,
    pair_addr: String,
    pair_code: String,
    tab: Tab,
    log: Vec<String>,
    found: Vec<String>,
    attached: Vec<Device>,
    packages: Vec<apps::PkgRow>,
    scope: usize,
    filter: String,
    ticked: Vec<String>,
    hits: Vec<uad::Hit>,
    level: usize,
    confirm: bool,
    privacy_findings: Vec<tv_safety::PrivacyFinding>,
    drift_report: Option<tv_safety::DriftReport>,
    launchers: Vec<String>,
    detected_brand: String,
    pkg_sizes: Vec<(String, u64, u64)>,
    helper_choice: HelperChoice,
    text_in: String,
    url_in: String,
    dpi: String,
    scale: String,
    dns_provider: usize,
    nextdns_id: String,
    log_tag: String,
    log_grep: String,
    diag_output: String,
    fleet_all: bool,
    health: Option<tv_safety::SystemHealth>,
    screensaver_in: String,
    live_preview: bool,
    last_shot_at: Option<std::time::Instant>,
    serving_remote: bool,
    seconds: String,
    remote_path: String,
    local_path: String,
    shot: Option<egui::TextureHandle>,
    shot_size: Option<[usize; 2]>,
    tx: Sender<WorkerMsg>,
    rx: Receiver<WorkerMsg>,
    in_flight: usize,
    busy_label: String,
    show_log: bool,
}

impl App {
    fn new(serial: Option<String>) -> Self {
        let serial_input = serial.clone().unwrap_or_default();
        let (tx, rx) = mpsc::channel();
        let mut app = App {
            adb: Adb::new(serial),
            serial_input,
            pair_addr: String::new(),
            pair_code: String::new(),
            tab: Tab::Connect,
            log: Vec::new(),
            found: Vec::new(),
            attached: Vec::new(),
            packages: Vec::new(),
            scope: 0,
            filter: String::new(),
            ticked: Vec::new(),
            hits: Vec::new(),
            level: 0,
            confirm: false,
            privacy_findings: Vec::new(),
            drift_report: None,
            launchers: Vec::new(),
            detected_brand: String::new(),
            pkg_sizes: Vec::new(),
            helper_choice: HelperChoice::Shizuku,
            text_in: String::new(),
            url_in: String::new(),
            dpi: String::new(),
            scale: String::new(),
            dns_provider: 0,
            nextdns_id: String::new(),
            log_tag: String::new(),
            log_grep: String::new(),
            diag_output: String::new(),
            fleet_all: false,
            health: None,
            screensaver_in: "com.neilturner.aerialviews/.ui.screensaver.DreamActivity".to_string(),
            live_preview: false,
            last_shot_at: None,
            serving_remote: false,
            seconds: "20".to_string(),
            remote_path: transfer::DOWNLOADS.to_string(),
            local_path: String::new(),
            shot: None,
            shot_size: None,
            tx,
            rx,
            in_flight: 0,
            busy_label: String::new(),
            show_log: true,
        };
        app.say("Ready. Connect to a TV in the Connect tab or select an attached device.");
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

    fn spawn(
        &mut self,
        ctx: &egui::Context,
        label: impl Into<String>,
        work: impl FnOnce(Adb, Sender<WorkerMsg>) -> Result<String> + Send + 'static,
    ) {
        let label = label.into();
        self.in_flight += 1;
        self.busy_label = label.clone();
        let adb = self.adb.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let msg = match work(adb, tx.clone()) {
                Ok(v) => WorkerMsg::Done(format!("{label}: {v}")),
                Err(e) => WorkerMsg::Done(format!("{label}: {e}")),
            };
            let _ = tx.send(msg);
            ctx.request_repaint();
        });
    }

    fn drain_worker(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                WorkerMsg::FoundHosts(hosts) => self.found = hosts,
                WorkerMsg::Devices(devs) => self.attached = devs,
                WorkerMsg::Packages(rows) => {
                    self.packages = rows;
                    self.ticked.clear();
                }
                WorkerMsg::UadHits(hits) => self.hits = hits,
                WorkerMsg::PrivacyFindings(f) => self.privacy_findings = f,
                WorkerMsg::Drift(d) => self.drift_report = Some(d),
                WorkerMsg::Launchers(l) => self.launchers = l,
                WorkerMsg::Brand(b) => self.detected_brand = b,
                WorkerMsg::Sizes(s) => self.pkg_sizes = s,
                WorkerMsg::LogOutput(s) => self.diag_output = s,
                WorkerMsg::Health(h) => self.health = Some(h),
                WorkerMsg::Screenshot(path) => self.preview(&path, ctx),
                WorkerMsg::SetTarget(serial) => {
                    self.serial_input = serial.clone();
                    self.adb = Adb::new(if serial.is_empty() { None } else { Some(serial) });
                }
                WorkerMsg::Done(line) => {
                    self.in_flight = self.in_flight.saturating_sub(1);
                    if self.in_flight == 0 {
                        self.busy_label.clear();
                    }
                    self.say(line);
                }
            }
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

    fn connect_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Connect");
        ui.label(RichText::new("Discover your Android TV on the local network or pair with Wireless Debugging.").color(TEXT_MUTED));
        ui.add_space(6.0);

        card(ui, |ui| {
            ui.strong("Target Device");
            ui.horizontal(|ui| {
                ui.label("TV address (host:port)");
                ui.add(egui::TextEdit::singleline(&mut self.serial_input).desired_width(220.0).hint_text("192.168.1.50:5555"));
                if primary_button(ui, "Connect").clicked() {
                    let s = self.serial_input.trim().to_string();
                    if s.is_empty() {
                        self.adb = Adb::new(None);
                        self.say("Target set to adb default");
                    } else {
                        self.spawn(ctx, "connect", move |adb, tx| {
                            let name = crate::connect_to(&adb, &s)?;
                            let _ = tx.send(WorkerMsg::SetTarget(s));
                            Ok(name)
                        });
                    }
                }
                if ui.button("Scan Network").clicked() {
                    self.spawn(ctx, "scan", |adb, tx| {
                        let hosts = net::candidates(&adb);
                        let answers = net::probe(&adb, &hosts);
                        let count = answers.len();
                        let _ = tx.send(WorkerMsg::FoundHosts(answers));
                        Ok(format!("scanned {} hosts, {count} answered on {}", hosts.len(), net::ADB_PORT))
                    });
                }
                if ui.button("Refresh Devices").clicked() {
                    self.spawn(ctx, "devices", |adb, tx| {
                        let d = adb.devices()?;
                        let count = d.len();
                        let _ = tx.send(WorkerMsg::Devices(d));
                        Ok(format!("{count} attached"))
                    });
                }
                if ui.button("USB -> Wi-Fi (5555)").clicked() {
                    self.spawn(ctx, "usb->wifi", |adb, _| tv_safety::usb_to_wifi(&adb));
                }
                if ui.button("Wake-on-LAN").clicked() {
                    self.spawn(ctx, "wake-lan", |adb, _| device::wake_lan(&adb));
                }
                if ui.button("System Health").clicked() {
                    self.spawn(ctx, "health", |adb, tx| {
                        let h = tv_safety::system_health(&adb)?;
                        let summary = format!("Android {} (SDK {}), {} free", h.android, h.sdk, h.storage_free);
                        let _ = tx.send(WorkerMsg::Health(h));
                        Ok(summary)
                    });
                }
            });
            if let Some(h) = &self.health {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    badge(ui, &format!("Android {} (SDK {})", h.android, h.sdk), ACCENT);
                    if let Some(mb) = h.ram_avail_mb {
                        badge(ui, &format!("{mb} MB RAM free"), if mb < 250 { STATUS_WARN } else { STATUS_OK });
                    }
                    badge(ui, &format!("{} /data free", h.storage_free), STATUS_OK);
                    if !h.uptime.is_empty() {
                        ui.label(RichText::new(&h.uptime).color(TEXT_MUTED).small());
                    }
                });
            }
        });

        ui.add_space(6.0);
        card(ui, |ui| {
            ui.strong("Wireless Debugging Pairing (Android 11+)");
            ui.horizontal(|ui| {
                ui.label("Pairing host:port");
                ui.add(egui::TextEdit::singleline(&mut self.pair_addr).desired_width(180.0).hint_text("192.168.1.50:37123"));
                ui.label("6-digit code");
                ui.add(egui::TextEdit::singleline(&mut self.pair_code).desired_width(90.0).hint_text("123456"));
                if ui.button("Pair").clicked() {
                    let addr = self.pair_addr.trim().to_string();
                    let code = self.pair_code.trim().to_string();
                    self.spawn(ctx, "pair", move |adb, _| adb.pair(&addr, &code));
                }
            });
        });

        ui.add_space(6.0);
        if !self.found.is_empty() || !self.attached.is_empty() {
            card(ui, |ui| {
                ui.strong("Discovered & Attached TVs");
                let mut pick: Option<String> = None;
                for host in &self.found {
                    ui.horizontal(|ui| {
                        ui.monospace(host);
                        if ui.small_button("Connect").clicked() {
                            pick = Some(host.clone());
                        }
                    });
                }
                if let Some(host) = pick {
                    self.spawn(ctx, "connect", move |adb, tx| {
                        let name = crate::connect_to(&adb, &host)?;
                        let _ = tx.send(WorkerMsg::SetTarget(host));
                        Ok(name)
                    });
                }
                let mut use_this: Option<String> = None;
                for d in &self.attached {
                    ui.horizontal(|ui| {
                        let color = if d.state == "device" { STATUS_OK } else { STATUS_WARN };
                        badge(ui, &d.state, color);
                        ui.monospace(&d.serial);
                        ui.label(RichText::new(d.model.as_deref().unwrap_or("unknown model")).color(TEXT_MUTED));
                        if ui.small_button("Use").clicked() {
                            use_this = Some(d.serial.clone());
                        }
                    });
                }
                if let Some(serial) = use_this {
                    self.serial_input = serial.clone();
                    self.adb = Adb::new(Some(serial.clone()));
                    self.say(format!("Target set to {serial}"));
                }
            });
        } else {
            card(ui, |ui| {
                ui.label(RichText::new("No devices listed yet. Click 'Scan Network' or 'Refresh Devices' above.").color(TEXT_MUTED));
            });
        }
    }

    fn cleanup_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("TV Cleanup");
        ui.label(RichText::new("Safe-guarded Android TV cleanup with brick protection, Wi-Fi ownership checks, and undo.").color(TEXT_MUTED));
        ui.add_space(6.0);

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // Step 1: Privacy & ACR Audit
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("1. Privacy, ACR & Telemetry Audit");
                    if primary_button(ui, "Scan TV").clicked() {
                        self.spawn(ctx, "privacy audit", |adb, tx| {
                            let f = tv_safety::audit_privacy(&adb)?;
                            let count = f.len();
                            let _ = tx.send(WorkerMsg::PrivacyFindings(f));
                            Ok(format!("{count} known privacy/telemetry package(s) found"))
                        });
                    }
                    let enabled_privacy: Vec<String> = self
                        .privacy_findings
                        .iter()
                        .filter(|f| f.enabled)
                        .map(|f| f.package.clone())
                        .collect();
                    if ui
                        .add_enabled(!enabled_privacy.is_empty(), egui::Button::new(format!("Disable {} Flagged", enabled_privacy.len())))
                        .clicked()
                    {
                        self.spawn(ctx, "privacy disable", move |adb, _| {
                            apps::set_enabled(&adb, &enabled_privacy, false)?;
                            let _ = undo::note("privacy-audit", adb.serial.as_deref(), &enabled_privacy);
                            Ok(format!("disabled {} package(s) (undo saved)", enabled_privacy.len()))
                        });
                    }
                });
                if self.privacy_findings.is_empty() {
                    ui.label(RichText::new("Run 'Scan TV' to detect Inscape/Samba ACR, voice assistants, and OEM telemetry.").color(TEXT_MUTED).small());
                } else {
                    for f in &self.privacy_findings {
                        ui.horizontal(|ui| {
                            badge(ui, f.category.label(), if f.enabled { STATUS_WARN } else { STATUS_OK });
                            ui.monospace(&f.package);
                            ui.label(RichText::new(if f.enabled { "enabled" } else { "disabled" }).color(TEXT_MUTED).small());
                        });
                        ui.label(RichText::new(&f.note).color(TEXT_MUTED).small());
                    }
                }
            });

            ui.add_space(8.0);

            // Step 2: Presets, Drift Check & Undo
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("2. Ad-Free & Brand Presets, OTA Drift & Snapshots");
                    if ui.small_button("Detect TV Brand").clicked() {
                        self.spawn(ctx, "brand", |adb, tx| {
                            let b = tv_safety::device_brand(&adb);
                            let _ = tx.send(WorkerMsg::Brand(b.clone()));
                            Ok(if b.is_empty() { "unknown brand".into() } else { b })
                        });
                    }
                    if !self.detected_brand.is_empty() {
                        badge(ui, &self.detected_brand, ACCENT);
                    }
                    ui.checkbox(&mut self.fleet_all, "Apply to all attached TVs");
                });
                let presets = if self.detected_brand.is_empty() {
                    debloat::PRESETS.iter().collect::<Vec<_>>()
                } else {
                    debloat::presets_for_brand(&self.detected_brand)
                };
                let all_tvs = self.fleet_all;
                ui.horizontal_wrapped(|ui| {
                    for preset in presets {
                        if ui.button(format!("Apply {}", preset.title)).clicked() {
                            let pkgs: Vec<String> = preset.packages.iter().map(|s| (*s).to_string()).collect();
                            let title = preset.title.to_string();
                            self.spawn(ctx, format!("preset {title}"), move |adb, _| {
                                if all_tvs {
                                    let devs: Vec<_> = adb.devices()?.into_iter().filter(|d| d.state == "device").collect();
                                    let count = devs.len();
                                    for d in devs {
                                        let dev_adb = Adb { serial: Some(d.serial), bin: adb.bin.clone() };
                                        apps::set_enabled(&dev_adb, &pkgs, false)?;
                                        let _ = undo::note(&format!("preset {title}"), dev_adb.serial.as_deref(), &pkgs);
                                    }
                                    Ok(format!("applied {title} to {count} TV(s)"))
                                } else {
                                    apps::set_enabled(&adb, &pkgs, false)?;
                                    let _ = undo::note(&format!("preset {title}"), adb.serial.as_deref(), &pkgs);
                                    Ok(format!("applied {title} (safe-guarded)"))
                                }
                            });
                        }
                    }
                    if ui.button("Check OTA Drift").clicked() {
                        self.spawn(ctx, "drift check", |adb, tx| {
                            let expected = tv_safety::preset_packages(None)?;
                            let report = tv_safety::check_drift(&adb, &expected)?;
                            let n = report.reenabled.len();
                            let _ = tx.send(WorkerMsg::Drift(report));
                            Ok(if n == 0 { "no drift detected".into() } else { format!("{n} package(s) re-enabled by OTA") })
                        });
                    }
                    if ui.button("Fix Drift").clicked() {
                        self.spawn(ctx, "drift fix", |adb, _| {
                            let expected = tv_safety::preset_packages(None)?;
                            let (guard, _) = tv_safety::fix_drift(&adb, &expected, &tv_safety::SafetyOptions::default())?;
                            Ok(format!("re-disabled {} drifted package(s)", guard.allowed.len()))
                        });
                    }
                    if ui.button("Undo Last Batch").clicked() {
                        self.spawn(ctx, "undo", |adb, _| {
                            let Some(rec) = undo::last(None) else {
                                return Ok("no undo entries found".into());
                            };
                            apps::set_enabled(&adb, &rec.packages, true)?;
                            undo::retire(&rec.stamp);
                            Ok(format!("re-enabled {} package(s) from '{}'", rec.packages.len(), rec.what))
                        });
                    }
                    if ui.button("Save Snapshot").clicked() {
                        self.spawn(ctx, "snapshot save", |adb, _| {
                            let p = profile::save_snapshot(&adb)?;
                            Ok(format!("saved {}", p.display()))
                        });
                    }
                    if ui.button("Restore Latest Snapshot").clicked() {
                        self.spawn(ctx, "snapshot restore", |adb, _| {
                            let Some(path) = profile::latest_snapshot() else {
                                return Ok("no snapshots found in ~/.atvtools/snapshots".into());
                            };
                            let prof = profile::load(&path)?;
                            profile::apply(&adb, &prof)?;
                            Ok(format!("restored {}", path.display()))
                        });
                    }
                });
                if let Some(d) = &self.drift_report {
                    if d.is_clean() {
                        badge(ui, "In sync: no OTA drift", STATUS_OK);
                    } else {
                        badge(ui, &format!("{} drifted package(s)", d.reenabled.len()), STATUS_WARN);
                        for p in &d.reenabled {
                            ui.monospace(p);
                        }
                    }
                }
            });

            ui.add_space(8.0);

            // Step 3: Launcher & Shizuku / Stellar
            card(ui, |ui| {
                ui.strong("3. Custom Launcher & Privileged Helper (Shizuku / Stellar)");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Optional launchers:");
                    for (cat, label) in [
                        ("tvos-launcher", "Install tvOS Launcher"),
                        ("flauncher", "Install FLauncher"),
                        ("projectivy", "Install Projectivy"),
                    ] {
                        if ui.button(label).clicked() {
                            let name = cat.to_string();
                            self.spawn(ctx, format!("install {name}"), move |adb, _| {
                                download::install_catalog_with(&adb, &name, false)?;
                                Ok(format!("installed {name}"))
                            });
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Detect Launchers").clicked() {
                        self.spawn(ctx, "launchers", |adb, tx| {
                            let l = tv_safety::query_home_launchers(&adb)?;
                            let n = l.len();
                            let _ = tx.send(WorkerMsg::Launchers(l));
                            Ok(format!("{n} launcher(s) found"))
                        });
                    }
                    for comp in self.launchers.clone() {
                        let pkg = comp.split('/').next().unwrap_or(&comp).to_string();
                        if ui.small_button(format!("Set Home: {pkg}")).clicked() {
                            self.spawn(ctx, "set home", move |adb, _| tv_safety::set_home_activity(&adb, &pkg));
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Privileged helper:");
                    ui.selectable_value(&mut self.helper_choice, HelperChoice::Shizuku, "fork-Shizuku");
                    ui.selectable_value(&mut self.helper_choice, HelperChoice::Stellar, "Stellar (optional)");
                    let prov = match self.helper_choice {
                        HelperChoice::Shizuku => shizuku::Provider::Shizuku,
                        HelperChoice::Stellar => shizuku::Provider::Stellar,
                    };
                    if ui.button("Install").clicked() {
                        self.spawn(ctx, format!("install {}", prov.label()), move |adb, _| {
                            download::install_catalog_with(&adb, prov.catalog_name(), true)?;
                            Ok(format!("installed {}", prov.catalog_name()))
                        });
                    }
                    if ui.button("Start").clicked() {
                        self.spawn(ctx, format!("start {}", prov.label()), move |adb, _| {
                            shizuku::start_for(&adb, prov)?;
                            Ok(format!("{} start requested", prov.label()))
                        });
                    }
                    if ui.button("Status").clicked() {
                        self.spawn(ctx, format!("status {}", prov.label()), move |adb, _| {
                            let inst = shizuku::installed_for(&adb, prov)?;
                            let run = shizuku::running_for(&adb, prov)?;
                            Ok(format!("installed={inst}, running={run}"))
                        });
                    }
                });
            });

            ui.add_space(8.0);

            // Step 4: Finish & Lock Down ADB
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("4. Finish & Lock Down ADB");
                    if ui.button("Security Audit").clicked() {
                        self.spawn(ctx, "security audit", |adb, _| Ok(tv_safety::security_audit(&adb)));
                    }
                    if ui.button("Open Developer Options on TV").clicked() {
                        self.spawn(ctx, "dev options", |adb, _| {
                            device::screen(&adb, "developer").map(|_| "opened Developer Options".into())
                        });
                    }
                });
                ui.label(RichText::new("When done cleaning your TV, turn off Network/Wireless debugging in Developer Options so port 5555 is not left open on your LAN.").color(TEXT_MUTED).small());
            });
        });
    }

    fn apps_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Apps");
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Scope");
                egui::ComboBox::from_id_salt("scope")
                    .selected_text(SCOPES[self.scope])
                    .show_ui(ui, |ui| {
                        for (i, name) in SCOPES.iter().enumerate() {
                            ui.selectable_value(&mut self.scope, i, *name);
                        }
                    });
                if primary_button(ui, "Reload").clicked() {
                    let scope = self.scope();
                    self.spawn(ctx, "apps", move |adb, tx| {
                        let rows = apps::list_rows(&adb, scope, true, true)?;
                        let n = rows.len();
                        let _ = tx.send(WorkerMsg::Packages(rows));
                        Ok(format!("{n} packages listed"))
                    });
                }
                ui.label("Filter");
                ui.add(egui::TextEdit::singleline(&mut self.filter).desired_width(180.0).hint_text("package name..."));
            });
            ui.horizontal_wrapped(|ui| {
                badge(ui, &format!("{} selected", self.ticked.len()), ACCENT);
                let has_sel = !self.ticked.is_empty();
                if ui.add_enabled(has_sel, egui::Button::new("Disable (Safe)")).clicked() {
                    let pkgs = std::mem::take(&mut self.ticked);
                    self.spawn(ctx, "disable", move |adb, _| {
                        apps::set_enabled(&adb, &pkgs, false)?;
                        let _ = undo::note("gui-apps", adb.serial.as_deref(), &pkgs);
                        Ok(format!("processed {} package(s)", pkgs.len()))
                    });
                }
                if ui.add_enabled(has_sel, egui::Button::new("Enable")).clicked() {
                    let pkgs = std::mem::take(&mut self.ticked);
                    self.spawn(ctx, "enable", move |adb, _| {
                        apps::set_enabled(&adb, &pkgs, true)?;
                        Ok(format!("enabled {} package(s)", pkgs.len()))
                    });
                }
                if ui.add_enabled(has_sel, egui::Button::new("Uninstall (User 0)")).clicked() {
                    let pkgs = std::mem::take(&mut self.ticked);
                    self.spawn(ctx, "uninstall", move |adb, _| {
                        apps::uninstall(&adb, &pkgs)?;
                        Ok(format!("uninstalled {} package(s)", pkgs.len()))
                    });
                }
                if ui.add_enabled(has_sel, egui::Button::new("Launch")).clicked() {
                    let pkgs = std::mem::take(&mut self.ticked);
                    self.spawn(ctx, "launch", move |adb, _| {
                        for p in &pkgs {
                            apps::launch(&adb, p)?;
                        }
                        Ok(format!("launched {}", pkgs.len()))
                    });
                }
                if ui.add_enabled(has_sel, egui::Button::new("Force Stop")).clicked() {
                    let pkgs = std::mem::take(&mut self.ticked);
                    self.spawn(ctx, "force stop", move |adb, _| {
                        for p in &pkgs {
                            apps::force_stop(&adb, p)?;
                        }
                        Ok(format!("stopped {}", pkgs.len()))
                    });
                }
                if ui.add_enabled(has_sel, egui::Button::new("Info")).clicked() {
                    let pkgs = std::mem::take(&mut self.ticked);
                    self.spawn(ctx, "info", move |adb, _| {
                        let mut out = String::new();
                        for p in &pkgs {
                            out.push_str(&apps::info(&adb, p)?);
                        }
                        Ok(out)
                    });
                }
                if ui.button("Analyze Sizes (Top 15)").clicked() {
                    let scope = self.scope();
                    self.spawn(ctx, "sizes", move |adb, tx| {
                        let rows = apps::sizes(&adb, scope, 15)?;
                        let n = rows.len();
                        let _ = tx.send(WorkerMsg::Sizes(rows));
                        Ok(format!("loaded top {n} package sizes"))
                    });
                }
                if ui.button("Trim Caches").clicked() {
                    self.spawn(ctx, "trim caches", |adb, _| apps::trim_caches(&adb));
                }
                if ui.add_enabled(has_sel, egui::Button::new("Grant Storage")).clicked() {
                    let pkgs = std::mem::take(&mut self.ticked);
                    self.spawn(ctx, "grant storage", move |adb, _| {
                        for p in &pkgs {
                            apps::appop(&adb, p, "MANAGE_EXTERNAL_STORAGE", true)?;
                        }
                        Ok(format!("granted storage to {}", pkgs.len()))
                    });
                }
                if ui.add_enabled(has_sel, egui::Button::new("Grant Write Settings")).clicked() {
                    let pkgs = std::mem::take(&mut self.ticked);
                    self.spawn(ctx, "grant write_settings", move |adb, _| {
                        for p in &pkgs {
                            apps::appop(&adb, p, "WRITE_SETTINGS", true)?;
                        }
                        Ok(format!("granted WRITE_SETTINGS to {}", pkgs.len()))
                    });
                }
            });
        });
        if !self.pkg_sizes.is_empty() {
            ui.add_space(4.0);
            card(ui, |ui| {
                ui.strong("Largest Packages (APK + Data)");
                for (pkg, code, data) in &self.pkg_sizes {
                    ui.horizontal(|ui| {
                        ui.monospace(pkg);
                        ui.label(RichText::new(format!("{} app + {} data", apps::mb(*code), apps::mb(*data))).color(TEXT_MUTED).small());
                    });
                }
            });
        }
        ui.add_space(6.0);
        if self.packages.is_empty() {
            card(ui, |ui| {
                ui.label(RichText::new("No packages loaded. Click 'Reload' above to fetch installed packages from your TV.").color(TEXT_MUTED));
            });
            return;
        }
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
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (package, installer) in rows {
                ui.horizontal(|ui| {
                    let mut on = self.tick(&package);
                    if ui.checkbox(&mut on, "").changed() {
                        self.toggle_tick(&package);
                    }
                    ui.monospace(&package);
                    if tv_safety::is_core_package(&package) {
                        badge(ui, "core", STATUS_ERR);
                    }
                    if !installer.is_empty() {
                        ui.label(RichText::new(installer).color(TEXT_MUTED).small());
                    }
                });
            }
        });
    }

    fn debloat_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Debloat (UAD Database)");
        ui.label(RichText::new("Powered by Universal Android Debloater lists + CleanYourTV core brick protection.").color(TEXT_MUTED));
        ui.add_space(6.0);

        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Scope");
                egui::ComboBox::from_id_salt("debloat-scope")
                    .selected_text(SCOPES[self.scope])
                    .show_ui(ui, |ui| {
                        for (i, name) in SCOPES.iter().enumerate() {
                            ui.selectable_value(&mut self.scope, i, *name);
                        }
                    });
                ui.label("Level");
                egui::ComboBox::from_id_salt("level")
                    .selected_text(uad::LEVELS[self.level])
                    .show_ui(ui, |ui| {
                        for (i, name) in uad::LEVELS.iter().enumerate() {
                            ui.selectable_value(&mut self.level, i, *name);
                        }
                    });
                if primary_button(ui, "Scan Installed").clicked() {
                    let scope = self.scope();
                    let level = uad::LEVELS[self.level];
                    self.spawn(ctx, "uad scan", move |adb, tx| {
                        let hits = uad::scan(&adb, scope, level, false)?;
                        let n = hits.len();
                        let _ = tx.send(WorkerMsg::UadHits(hits));
                        Ok(format!("{n} packages flagged at '{level}'"))
                    });
                }
                if ui.button("Refresh Database").clicked() {
                    self.spawn(ctx, "uad refresh", |_, _| {
                        let db = uad::load(true)?;
                        Ok(format!("{} packages cached", db.len()))
                    });
                }
            });
            ui.checkbox(
                &mut self.confirm,
                "I understand disabled apps disappear from the launcher (Undo / Enable restores them)",
            );
            ui.horizontal(|ui| {
                let n = self.hits.len();
                let pkgs: Vec<String> = self.hits.iter().map(|h| h.package.clone()).collect();
                if ui
                    .add_enabled(self.confirm && n > 0, egui::Button::new(format!("Disable {n} (Safe)")))
                    .clicked()
                {
                    self.spawn(ctx, "uad disable", move |adb, _| {
                        apps::set_enabled(&adb, &pkgs, false)?;
                        let _ = undo::note("uad-gui", adb.serial.as_deref(), &pkgs);
                        Ok(format!("processed {} package(s)", pkgs.len()))
                    });
                }
                if ui
                    .add_enabled(self.confirm && n > 0, egui::Button::new(format!("Re-enable {n}")))
                    .clicked()
                {
                    let pkgs: Vec<String> = self.hits.iter().map(|h| h.package.clone()).collect();
                    self.spawn(ctx, "uad enable", move |adb, _| {
                        apps::set_enabled(&adb, &pkgs, true)?;
                        Ok(format!("enabled {} package(s)", pkgs.len()))
                    });
                }
            });
        });

        ui.add_space(6.0);
        if self.hits.is_empty() {
            card(ui, |ui| {
                ui.label(RichText::new("No UAD scan results yet. Select a safety level and click 'Scan Installed'.").color(TEXT_MUTED));
            });
            return;
        }
        let hits = self.hits.clone();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for h in &hits {
                ui.horizontal(|ui| {
                    let color = match h.removal.as_str() {
                        "Recommended" => STATUS_OK,
                        "Advanced" => ACCENT,
                        "Expert" => STATUS_WARN,
                        _ => STATUS_ERR,
                    };
                    badge(ui, &h.removal, color);
                    ui.monospace(&h.package);
                    ui.label(RichText::new(&h.list).color(TEXT_MUTED).small());
                });
                if let Some(first) = h.description.lines().find(|l| !l.trim().is_empty()) {
                    ui.label(RichText::new(first).color(TEXT_MUTED).small());
                }
            }
        });
    }

    fn settings_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Settings");
        ui.add_space(4.0);
        card(ui, |ui| {
            ui.strong("Display & Typography");
            ui.horizontal(|ui| {
                ui.label("Density (DPI)");
                ui.add(egui::TextEdit::singleline(&mut self.dpi).desired_width(70.0));
                if ui.button("Read").clicked() {
                    self.spawn(ctx, "density", |adb, _| settings::density(&adb));
                }
                if ui.button("Set").clicked() {
                    match self.dpi.trim().parse::<u32>() {
                        Ok(dpi) => self.spawn(ctx, "density", move |adb, _| {
                            settings::set_density(&adb, dpi).map(|_| format!("{dpi} dpi"))
                        }),
                        Err(_) => self.say("density: enter a number >= 72"),
                    }
                }
                if ui.button("Reset").clicked() {
                    self.spawn(ctx, "density", |adb, _| settings::reset_density(&adb).map(|_| "reset".into()));
                }
            });
            ui.horizontal(|ui| {
                ui.label("Font scale");
                ui.add(egui::TextEdit::singleline(&mut self.scale).desired_width(70.0));
                if ui.button("Read").clicked() {
                    self.spawn(ctx, "font scale", |adb, _| settings::font_scale(&adb));
                }
                if ui.button("Set").clicked() {
                    match self.scale.trim().parse::<f32>() {
                        Ok(s) => self.spawn(ctx, "font scale", move |adb, _| {
                            settings::set_font_scale(&adb, s).map(|_| format!("{s}"))
                        }),
                        Err(_) => self.say("font scale: enter 0.5 - 2.0"),
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Screen size");
                if ui.button("Read Size").clicked() {
                    self.spawn(ctx, "screen size", |adb, _| settings::screen_size(&adb));
                }
                if ui.button("Reset Size").clicked() {
                    self.spawn(ctx, "screen size", |adb, _| settings::reset_screen_size(&adb).map(|_| "reset".into()));
                }
                ui.separator();
                ui.label("Animations");
                for (scale, label) in [(0.0, "0x Instant"), (0.5, "0.5x Snappy"), (1.0, "1.0x Stock")] {
                    if ui.small_button(label).clicked() {
                        self.spawn(ctx, "animations", move |adb, _| settings::set_animation_scales(&adb, scale));
                    }
                }
            });
        });
        ui.add_space(6.0);
        card(ui, |ui| {
            ui.strong("Private DNS & OTA Updates");
            ui.horizontal_wrapped(|ui| {
                ui.label("Provider");
                let prov_name = settings::DNS_PROVIDERS[self.dns_provider].0;
                egui::ComboBox::from_id_salt("dns-provider")
                    .selected_text(prov_name)
                    .show_ui(ui, |ui| {
                        for (i, (name, _)) in settings::DNS_PROVIDERS.iter().enumerate() {
                            ui.selectable_value(&mut self.dns_provider, i, *name);
                        }
                    });
                if prov_name == "nextdns" {
                    ui.label("Profile ID");
                    ui.add(egui::TextEdit::singleline(&mut self.nextdns_id).desired_width(90.0).hint_text("abc123"));
                }
                if primary_button(ui, "Apply DNS").clicked() {
                    let id = self.nextdns_id.clone();
                    self.spawn(ctx, "dns", move |adb, _| {
                        let host = settings::dns_host(prov_name, Some(&id))?;
                        settings::set_dns(&adb, &host)?;
                        Ok(host)
                    });
                }
                if ui.button("Private DNS Off").clicked() {
                    self.spawn(ctx, "adblock", |adb, _| settings::adblock_off(&adb).map(|_| "off".into()));
                }
                if ui.button("DNS State").clicked() {
                    self.spawn(ctx, "dns", |adb, _| settings::dns_state(&adb));
                }
                if ui.button("Block OTA").clicked() {
                    self.spawn(ctx, "ota", |adb, _| settings::ota(&adb, true).map(|_| "blocked".into()));
                }
                if ui.button("OTA State").clicked() {
                    self.spawn(ctx, "ota", |adb, _| settings::ota_state(&adb));
                }
            });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label("Screensaver");
                ui.add(egui::TextEdit::singleline(&mut self.screensaver_in).desired_width(280.0));
                if ui.button("Set").clicked() {
                    let comp = self.screensaver_in.trim().to_string();
                    self.spawn(ctx, "screensaver", move |adb, _| {
                        settings::screensaver(&adb, &comp)?;
                        Ok(format!("set {comp}"))
                    });
                }
                if ui.button("Off").clicked() {
                    self.spawn(ctx, "screensaver", |adb, _| {
                        settings::screensaver(&adb, "off")?;
                        Ok("disabled".into())
                    });
                }
            });
        });
        ui.add_space(6.0);
        card(ui, |ui| {
            ui.strong("System Toggles & TV Settings Screens");
            ui.horizontal_wrapped(|ui| {
                for name in settings::TOGGLES.iter().map(|t| t.name) {
                    if ui.small_button(name).clicked() {
                        self.spawn(ctx, name, move |adb, _| settings::toggle(&adb, name, None));
                    }
                }
            });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                for name in device::SCREENS.iter().map(|(n, _)| *n) {
                    if ui.small_button(name).clicked() {
                        self.spawn(ctx, name, move |adb, _| {
                            device::screen(&adb, name).map(|_| format!("opened {name}"))
                        });
                    }
                }
            });
        });
    }

    fn remote_screen_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Remote & Screen");
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            card(ui, |ui| {
                ui.strong("D-Pad & Media Keys");
                ui.horizontal_wrapped(|ui| {
                    for (name, _) in device::KEYS {
                        if ui.small_button(*name).clicked() {
                            let key = name.to_string();
                            self.spawn(ctx, *name, move |adb, _| {
                                device::key(&adb, &[key]).map(|_| "sent".into())
                            });
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Text");
                    ui.add(egui::TextEdit::singleline(&mut self.text_in).desired_width(280.0));
                    if ui.button("Send").clicked() {
                        let words: Vec<String> = self.text_in.split_whitespace().map(str::to_string).collect();
                        self.spawn(ctx, "text", move |adb, _| device::text(&adb, &words).map(|_| "typed".into()));
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("URL");
                    ui.add(egui::TextEdit::singleline(&mut self.url_in).desired_width(280.0));
                    if ui.button("Open").clicked() {
                        let url = self.url_in.clone();
                        self.spawn(ctx, "open", move |adb, _| device::open_url(&adb, &url).map(|_| format!("opened {url}")));
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Wake").clicked() {
                        self.spawn(ctx, "wake", |adb, _| device::wake(&adb).map(|_| "sent".into()));
                    }
                    if ui.button("Sleep").clicked() {
                        self.spawn(ctx, "sleep", |adb, _| device::sleep(&adb).map(|_| "sent".into()));
                    }
                    if ui.button("Sync & Reboot").clicked() {
                        self.spawn(ctx, "reboot", |adb, _| {
                            let _ = tv_safety::pre_reboot_sync(&adb);
                            device::reboot(&adb, None).map(|_| "synced and rebooted".into())
                        });
                    }
                    if ui.button("Inspect Remote Key (4s)").clicked() {
                        self.spawn(ctx, "key inspector", |adb, _| tv_safety::read_last_key_event(&adb));
                    }
                    if ui.add_enabled(!self.serving_remote, egui::Button::new("Browser Remote (:8080)")).clicked() {
                        self.serving_remote = true;
                        let adb = self.adb.clone();
                        let tx = self.tx.clone();
                        let ctx_bg = ctx.clone();
                        std::thread::spawn(move || {
                            let _ = remote::serve(&adb, "0.0.0.0:8080");
                            let _ = tx.send(WorkerMsg::Done("browser remote: stopped".into()));
                            ctx_bg.request_repaint();
                        });
                        self.say("Browser Remote listening on http://0.0.0.0:8080 (open on your phone)");
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Inputs");
                    for (label, code) in [
                        ("TV Input", "178"),
                        ("HDMI 1", "243"),
                        ("HDMI 2", "244"),
                        ("HDMI 3", "245"),
                        ("HDMI 4", "246"),
                    ] {
                        if ui.small_button(label).clicked() {
                            let k = code.to_string();
                            self.spawn(ctx, label, move |adb, _| {
                                device::key(&adb, &[k]).map(|_| "sent".into())
                            });
                        }
                    }
                });
            });
            ui.add_space(6.0);
            card(ui, |ui| {
                ui.strong("Screen Capture & Mirroring");
                ui.horizontal_wrapped(|ui| {
                    if primary_button(ui, "Screenshot").clicked() {
                        let dest = std::env::temp_dir().join(SHOT);
                        self.spawn(ctx, "screenshot", move |adb, tx| {
                            screen::screenshot(&adb, &dest)?;
                            let p = dest.display().to_string();
                            let _ = tx.send(WorkerMsg::Screenshot(dest));
                            Ok(format!("saved {p}"))
                        });
                    }
                    ui.checkbox(&mut self.live_preview, "Live Preview (2.5s)");
                    ui.label("Seconds");
                    ui.add(egui::TextEdit::singleline(&mut self.seconds).desired_width(50.0));
                    if ui.button("Record").clicked() {
                        if let Ok(secs) = self.seconds.trim().parse::<u32>() {
                            let dest = std::env::temp_dir().join("atvtools-record.mp4");
                            self.spawn(ctx, "record", move |adb, _| {
                                screen::record(&adb, secs, &dest)?;
                                Ok(format!("saved {}", dest.display()))
                            });
                        } else {
                            self.say("record: enter a number of seconds");
                        }
                    }
                    if ui.button("Launch scrcpy").clicked() {
                        self.spawn(ctx, "scrcpy", |adb, _| {
                            screen::scrcpy(adb.serial.as_deref(), &[]).map(|_| "closed".into())
                        });
                    }
                });
                if self.live_preview && self.in_flight == 0 {
                    let due = self
                        .last_shot_at
                        .map(|t| t.elapsed() >= std::time::Duration::from_millis(2500))
                        .unwrap_or(true);
                    if due {
                        self.last_shot_at = Some(std::time::Instant::now());
                        let dest = std::env::temp_dir().join(SHOT);
                        self.spawn(ctx, "live shot", move |adb, tx| {
                            screen::screenshot(&adb, &dest)?;
                            let _ = tx.send(WorkerMsg::Screenshot(dest));
                            Ok("refreshed".into())
                        });
                    } else {
                        ctx.request_repaint_after(std::time::Duration::from_millis(400));
                    }
                }
                if let Some(tex) = &self.shot {
                    ui.add_space(6.0);
                    ui.label(RichText::new("Click anywhere on the screenshot to tap that spot on the TV.").color(TEXT_MUTED).small());
                    let resp = ui.add(
                        egui::Image::new(egui::load::SizedTexture::from_handle(tex))
                            .max_width(520.0)
                            .sense(egui::Sense::click()),
                    );
                    if resp.clicked() {
                        if let (Some(pos), Some([tw, th])) = (resp.interact_pointer_pos(), self.shot_size) {
                            let rect = resp.rect;
                            let rx = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                            let ry = ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
                            let tv_x = (rx * tw as f32).round() as u32;
                            let tv_y = (ry * th as f32).round() as u32;
                            self.spawn(ctx, format!("tap {tv_x},{tv_y}"), move |adb, _| {
                                adb.shell(&format!("input tap {tv_x} {tv_y}"))?;
                                Ok("tapped".into())
                            });
                        }
                    }
                }
            });
        });
    }

    fn files_catalog_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Files & App Catalog");
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            card(ui, |ui| {
                ui.strong("File Transfer");
                ui.horizontal(|ui| {
                    ui.label("TV path");
                    ui.add(egui::TextEdit::singleline(&mut self.remote_path).desired_width(280.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Local path");
                    ui.add(egui::TextEdit::singleline(&mut self.local_path).desired_width(280.0));
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.button("List TV Folder").clicked() {
                        let remote = self.remote_path.clone();
                        self.spawn(ctx, "ls", move |adb, _| Ok(transfer::ls(&adb, &remote)?.trim().to_string()));
                    }
                    if ui.button("Send to TV").clicked() {
                        let remote = self.remote_path.clone();
                        let local = PathBuf::from(self.local_path.clone());
                        self.spawn(ctx, "push", move |adb, _| {
                            transfer::push(&adb, &local, &remote)?;
                            Ok(format!("sent {}", local.display()))
                        });
                    }
                    if ui.button("Copy from TV").clicked() {
                        let remote = self.remote_path.clone();
                        let local = PathBuf::from(self.local_path.clone());
                        self.spawn(ctx, "pull", move |adb, _| {
                            transfer::pull(&adb, &remote, &local)?;
                            Ok(format!("copied to {}", local.display()))
                        });
                    }
                });
            });

            ui.add_space(6.0);
            card(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.strong("Diagnostics & Utilities");
                    if ui.button("Doctor").clicked() {
                        self.spawn(ctx, "doctor", |adb, _| crate::doctor::run(&adb));
                    }
                    if ui.button("Doctor --fix").clicked() {
                        self.spawn(ctx, "doctor --fix", |adb, _| crate::doctor::fix(&adb));
                    }
                    if ui.button("Device Info").clicked() {
                        self.spawn(ctx, "info", |adb, _| {
                            let rows = device::info(&adb)?;
                            Ok(rows.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("  "))
                        });
                    }
                    if ui.button("Optimize").clicked() {
                        self.spawn(ctx, "optimize", |adb, _| device::optimize(&adb));
                    }
                    if ui.button("Check Updates").clicked() {
                        self.spawn(ctx, "update", |_, _| crate::update::check(env!("CARGO_PKG_VERSION"), None));
                    }
                });
            });

            ui.add_space(6.0);
            card(ui, |ui| {
                ui.strong("Logs & Crash Viewer");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Tag");
                    ui.add(egui::TextEdit::singleline(&mut self.log_tag).desired_width(110.0).hint_text("optional"));
                    ui.label("Grep");
                    ui.add(egui::TextEdit::singleline(&mut self.log_grep).desired_width(130.0).hint_text("filter text"));
                    if ui.button("Dump Logcat (200)").clicked() {
                        let tag = (!self.log_tag.trim().is_empty()).then(|| self.log_tag.trim().to_string());
                        let grep = (!self.log_grep.trim().is_empty()).then(|| self.log_grep.trim().to_string());
                        self.spawn(ctx, "logcat", move |adb, tx| {
                            let out = logs::logcat(&adb, tag.as_deref(), grep.as_deref(), false, 200)?;
                            let lines = out.lines().count();
                            let _ = tx.send(WorkerMsg::LogOutput(out));
                            Ok(format!("loaded {lines} line(s)"))
                        });
                    }
                    if ui.button("Recent Crashes").clicked() {
                        let grep = (!self.log_grep.trim().is_empty()).then(|| self.log_grep.trim().to_string());
                        self.spawn(ctx, "crashes", move |adb, tx| {
                            let out = logs::crash(&adb, grep.as_deref(), 200)?;
                            let _ = tx.send(WorkerMsg::LogOutput(out));
                            Ok("loaded crash dropbox".into())
                        });
                    }
                    if ui.button("Save Bugreport").clicked() {
                        let dir = std::env::temp_dir().join("atvtools-bugreport");
                        self.spawn(ctx, "bugreport", move |adb, _| logs::bugreport(&adb, &dir));
                    }
                });
                if !self.diag_output.is_empty() {
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                        ui.monospace(RichText::new(&self.diag_output).small());
                    });
                }
            });

            ui.add_space(6.0);
            card(ui, |ui| {
                ui.strong("APK Catalog");
                for e in catalog::CATALOG {
                    ui.horizontal(|ui| {
                        let install = ui.small_button("Install").clicked();
                        let replace = ui
                            .small_button("Replace")
                            .on_hover_text("Remove conflicting signature copy first")
                            .clicked();
                        if install || replace {
                            let name = e.name.to_string();
                            self.spawn(ctx, name.clone(), move |adb, _| {
                                download::install_catalog_with(&adb, &name, replace)?;
                                Ok(format!("installed {name}"))
                            });
                        }
                        ui.monospace(e.name);
                        ui.label(RichText::new(e.note).color(TEXT_MUTED).small());
                    });
                }
            });
        });
    }

    fn preview(&mut self, path: &std::path::Path, ctx: &egui::Context) {
        self.shot = None;
        self.shot_size = None;
        let Ok(bytes) = std::fs::read(path) else { return };
        match png_rgba(&bytes) {
            Some((w, h, rgba)) => {
                let image = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                self.shot = Some(ctx.load_texture("shot", image, egui::TextureOptions::LINEAR));
                self.shot_size = Some([w, h]);
            }
            None => self.say("preview: unsupported PNG format; open file directly"),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain_worker(&ctx);

        // Drag-and-drop APK sideloading
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| {
                    p.extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("apk"))
                })
                .collect()
        });
        if !dropped.is_empty() {
            let count = dropped.len();
            self.spawn(&ctx, format!("sideload {count} apk(s)"), move |adb, _| {
                apps::install_with(&adb, &dropped, false)?;
                Ok(format!("installed {count} APK(s)"))
            });
        }

        // Top status bar
        egui::Panel::top("header").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("atvtools").strong());
                let target = if self.serial_input.trim().is_empty() {
                    "adb default"
                } else {
                    self.serial_input.trim()
                };
                badge(ui, target, if self.serial_input.trim().is_empty() { TEXT_MUTED } else { STATUS_OK });
                if self.in_flight > 0 {
                    ui.spinner();
                    ui.label(RichText::new(&self.busy_label).color(ACCENT).small());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let toggle_label = if self.show_log { "Hide Log" } else { "Show Log" };
                    if ui.small_button(toggle_label).clicked() {
                        self.show_log = !self.show_log;
                    }
                });
            });
        });

        // Left navigation sidebar
        egui::Panel::left("nav").default_size(165.0).resizable(false).show(ui, |ui| {
            ui.add_space(6.0);
            for (tab, label) in Tab::ALL {
                let selected = self.tab == tab;
                let text = if selected {
                    RichText::new(label).color(Color32::WHITE).strong()
                } else {
                    RichText::new(label).color(TEXT_MUTED)
                };
                if ui.selectable_label(selected, text).clicked() {
                    self.tab = tab;
                }
            }
        });

        // Bottom activity drawer
        if self.show_log {
            egui::Panel::bottom("activity")
                .resizable(true)
                .default_size(135.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong("Activity");
                        if ui.small_button("Clear").clicked() {
                            self.log.clear();
                        }
                    });
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for line in &self.log {
                                ui.monospace(RichText::new(line).small());
                            }
                        });
                });
        }

        // Main content
        let tab = self.tab;
        egui::CentralPanel::default().show(ui, |ui| match tab {
            Tab::Connect => self.connect_tab(ui, &ctx),
            Tab::Cleanup => self.cleanup_tab(ui, &ctx),
            Tab::Apps => self.apps_tab(ui, &ctx),
            Tab::Debloat => self.debloat_tab(ui, &ctx),
            Tab::Settings => self.settings_tab(ui, &ctx),
            Tab::RemoteScreen => self.remote_screen_tab(ui, &ctx),
            Tab::FilesCatalog => self.files_catalog_tab(ui, &ctx),
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
                    return None;
                }
                channels = match body.get(9).copied()? {
                    2 => 3,
                    6 => 4,
                    _ => return None,
                };
                if body.get(12).copied() != Some(0) {
                    return None;
                }
            }
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    if w == 0 || h == 0 {
        return None;
    }
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
        c.extend_from_slice(&[0, 0, 0, 0]);
        c
    }

    fn tiny_png() -> Vec<u8> {
        let mut ihdr = 2u32.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        let raw = vec![0u8, 255, 0, 0, 255, 0, 255, 0, 255];
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
        assert_eq!(paeth(10, 20, 15), 15);
    }
}
