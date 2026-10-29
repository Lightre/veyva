use crate::session::{self, Command, Session};
use crate::settings::{CaptureProfile, Settings};
use eframe::egui::{self, Color32, RichText, Vec2};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const INK: Color32 = Color32::from_rgb(16, 21, 28);
const SLATE: Color32 = Color32::from_rgb(26, 35, 48);
const MINT: Color32 = Color32::from_rgb(112, 226, 188);
const MUTED: Color32 = Color32::from_rgb(169, 183, 198);
const CORAL: Color32 = Color32::from_rgb(255, 125, 135);

pub fn run() -> Result<(), String> {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/preview.png"))
        .map_err(|e| format!("Could not load app icon: {e}"))?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Veyva")
            .with_inner_size([980.0, 700.0])
            .with_min_inner_size([820.0, 620.0])
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native("Veyva", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
        .map_err(|e| format!("Could not open Veyva: {e}"))
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Page {
    Capture,
    Settings,
    About,
}

struct App {
    page: Page,
    settings: Settings,
    settings_path: Result<PathBuf, String>,
    load_error: Option<String>,
    error: Option<String>,
    notice: Option<String>,
    output: String,
    session: Option<Session>,
    closing: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = INK;
        style.visuals.window_fill = SLATE;
        style.visuals.extreme_bg_color = INK;
        style.visuals.selection.bg_fill = MINT.gamma_multiply(0.25);
        style.visuals.selection.stroke.color = MINT;
        style.visuals.widgets.inactive.bg_fill = SLATE;
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(43, 59, 73);
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(51, 78, 79);
        style.spacing.item_spacing = Vec2::new(12.0, 12.0);
        style.spacing.button_padding = Vec2::new(18.0, 11.0);
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(30.0));
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        cc.egui_ctx.set_style(style);
        let settings_path = Settings::default_path();
        let loaded = settings_path
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|p| Settings::load(p));
        let (settings, load_error) = match loaded {
            Ok(s) => (s, None),
            Err(e) => (Settings::default(), Some(e)),
        };
        let output = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .map(|p| {
                p.join("Videos")
                    .join("Veyva")
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_default();
        Self {
            page: Page::Capture,
            settings,
            settings_path,
            load_error,
            error: None,
            notice: None,
            output,
            session: None,
            closing: false,
        }
    }

    fn active(&self) -> bool {
        self.session.as_ref().is_some_and(|s| !s.is_finished())
    }

    fn persist(&mut self) -> Result<(), String> {
        if self.load_error.is_some() {
            return Err(
                "Reset unreadable settings explicitly before saving or starting replay.".into(),
            );
        }
        self.settings.profile.validate()?;
        self.settings
            .save(self.settings_path.as_ref().map_err(Clone::clone)?)
    }

    fn start(&mut self) {
        self.error = None;
        let result = self.persist().and_then(|_| session::start(self.settings));
        match result {
            Ok(s) => {
                self.session = Some(s);
                self.notice = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn send(&mut self, command: Command) {
        if let Some(s) = &self.session {
            if let Err(e) = s.send(command) {
                self.error = Some(e);
            }
        }
    }

    fn save_replay(&mut self) {
        let result = (|| {
            if self.output.trim().is_empty() {
                return Err("Choose an output folder first.".to_string());
            }
            let folder = PathBuf::from(self.output.trim());
            if !folder.is_absolute() {
                return Err("Use an absolute output folder path.".into());
            }
            std::fs::create_dir_all(&folder)
                .map_err(|e| format!("Could not create output folder: {e}"))?;
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos();
            let path = folder.join(format!("Veyva-replay-{stamp}-{}.mp4", std::process::id()));
            if path.exists() {
                return Err("Output filename already exists. Try saving again.".into());
            }
            Ok(path)
        })();
        match result {
            Ok(path) => self.send(Command::SaveReplay(path)),
            Err(e) => self.error = Some(e),
        }
    }

    fn capture(&mut self, ui: &mut egui::Ui) {
        ui.heading("Keep the moment.");
        ui.label(RichText::new("A rolling replay buffer. Save only what matters.").color(MUTED));
        ui.add_space(16.0);
        let active = self.active();
        let snapshot = self
            .session
            .as_ref()
            .and_then(|s| s.status.lock().ok())
            .map(|s| {
                (
                    s.phase.clone(),
                    s.buffered_seconds,
                    s.buffer_bytes,
                    s.submitted,
                    s.dropped,
                    s.saving,
                    s.last_saved.clone(),
                    s.error.clone(),
                )
            });
        let stopping = snapshot
            .as_ref()
            .is_some_and(|s| s.0.starts_with("Stopping"));
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(if active {
                        "REPLAY SESSION"
                    } else {
                        "REPLAY OFF"
                    })
                    .color(if active { MINT } else { MUTED })
                    .strong()
                    .size(12.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(
                            snapshot
                                .as_ref()
                                .map(|s| s.0.as_str())
                                .unwrap_or("Ready when you are"),
                        )
                        .color(MUTED),
                    );
                });
            });
            ui.add_space(18.0);
            let seconds = snapshot.as_ref().map(|s| s.1).unwrap_or(0.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{seconds:.1}")).size(56.0).strong());
                ui.label(
                    RichText::new(format!(
                        "/ {} seconds",
                        self.settings.profile.history_seconds
                    ))
                    .color(MUTED),
                );
            });
            ui.add(
                egui::ProgressBar::new(
                    (seconds / f64::from(self.settings.profile.history_seconds)).clamp(0.0, 1.0)
                        as f32,
                )
                .fill(MINT)
                .desired_height(5.0),
            );
            ui.add_space(8.0);
            let (bytes, frames, dropped) = snapshot
                .as_ref()
                .map(|s| (s.2, s.3, s.4))
                .unwrap_or_default();
            ui.horizontal(|ui| {
                metric(
                    ui,
                    "ENCODED PAYLOAD",
                    format!("{:.1} MiB", bytes as f64 / 1_048_576.0),
                );
                ui.separator();
                metric(ui, "SUBMITTED FRAMES", frames.to_string());
                ui.separator();
                metric(ui, "DROPPED FRAMES", dropped.to_string());
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if !active {
                    if ui
                        .add_enabled(
                            self.load_error.is_none() && !self.closing,
                            egui::Button::new(RichText::new("Start replay").color(INK)).fill(MINT),
                        )
                        .clicked()
                    {
                        self.start();
                    }
                } else if ui
                    .add_enabled(!self.closing, egui::Button::new("Stop replay"))
                    .clicked()
                {
                    self.send(Command::Stop);
                }
                let saving = snapshot.as_ref().is_some_and(|s| s.5);
                if ui
                    .add_enabled(
                        active && !stopping && seconds > 0.0 && !saving && !self.closing,
                        egui::Button::new(if saving {
                            "Saving replay...".to_string()
                        } else {
                            format!(
                                "Save last {} seconds",
                                self.settings.profile.history_seconds
                            )
                        }),
                    )
                    .clicked()
                {
                    self.save_replay();
                }
            });
            ui.label(
                RichText::new(
                    "History starts fresh each session. Saving keeps the replay buffer running.",
                )
                .small()
                .color(MUTED),
            );
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.label(
                RichText::new("CAPTURE PROFILE")
                    .small()
                    .color(MUTED)
                    .strong(),
            );
            let p = self.settings.profile;
            ui.label(
                RichText::new(format!(
                    "{} x {}   /   {} fps   /   {:.1} Mbps",
                    p.width,
                    p.height,
                    p.fps,
                    p.video_kbps as f64 / 1000.0
                ))
                .size(20.0),
            );
            ui.label(
                RichText::new(format!(
                    "Monitor {}  -  {} MiB history budget  -  Video only",
                    self.settings.monitor, p.replay_budget_mib
                ))
                .color(MUTED),
            );
            if ui.small_button("Edit capture settings").clicked() {
                self.page = Page::Settings;
            }
        });
        ui.add_space(10.0);
        ui.label(RichText::new("SAVE LOCATION").small().strong().color(MUTED));
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.output)
                    .desired_width((ui.available_width() - 132.0).max(100.0)),
            );
            if ui.button("Open folder").clicked() {
                let path = PathBuf::from(self.output.trim());
                if path.is_dir() {
                    if let Err(e) = std::process::Command::new("explorer.exe").arg(path).spawn() {
                        self.error = Some(format!("Could not open folder: {e}"));
                    }
                } else {
                    self.error = Some(
                        "The output folder will be created when you save your first replay.".into(),
                    );
                }
            }
        });
        if let Some((_, _, _, _, _, _, saved, error)) = snapshot {
            if let Some(e) = error {
                banner(ui, &e, CORAL);
            }
            if let Some(p) = saved {
                ui.label(
                    RichText::new(format!("Saved: {}", p.display()))
                        .small()
                        .color(MINT),
                );
            }
        }
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Capture settings");
        ui.label(RichText::new("Choose a profile that fits your machine.").color(MUTED));
        ui.add_space(16.0);
        let active = self.active();
        if active {
            banner(
                ui,
                "Stop replay to edit settings. Changes apply to a fresh buffer on next start.",
                MINT,
            );
        }
        ui.add_enabled_ui(!active && !self.closing && self.load_error.is_none(), |ui| {
            card(ui, |ui| {
                ui.label(RichText::new("QUALITY PRESET").small().strong().color(MUTED));
                ui.horizontal(|ui| {
                    for (name, profile) in [("Low impact",CaptureProfile::low_end()),("Balanced",CaptureProfile::balanced()),("High quality",CaptureProfile::high())] {
                        if ui.selectable_label(self.settings.profile == profile,name).clicked() { self.settings.profile = profile; }
                    }
                    if ![CaptureProfile::low_end(),CaptureProfile::balanced(),CaptureProfile::high()].contains(&self.settings.profile) { ui.label(RichText::new("Custom").color(MINT)); }
                });
                ui.label(RichText::new("Presets are targets. Actual performance depends on your hardware.").small().color(MUTED));
            });
            ui.add_space(10.0);
            card(ui, |ui| {
                egui::Grid::new("capture-settings").num_columns(2).spacing([32.0,18.0]).show(ui, |ui| {
                    field(ui,"Width (pixels)",&mut self.settings.profile.width,320..=7680,2);
                    field(ui,"Height (pixels)",&mut self.settings.profile.height,180..=4320,2);
                    field(ui,"Frame rate (fps)",&mut self.settings.profile.fps,1..=240,1);
                    field(ui,"Video bitrate (kbps)",&mut self.settings.profile.video_kbps,250..=100_000,250);
                    field(ui,"Replay history (seconds)",&mut self.settings.profile.history_seconds,5..=600,5);
                    field(ui,"History budget (MiB)",&mut self.settings.profile.replay_budget_mib,16..=512,16);
                    field(ui,"Monitor index (starts at 0)",&mut self.settings.monitor,0..=32,1);
                });
            });
            ui.add_space(8.0);
            ui.label(RichText::new("The history budget caps encoded payload, not total process memory. A smaller budget may retain fewer seconds. Audio capture is not available in this milestone.").small().color(MUTED));
            if ui.button("Save settings").clicked() {
                match self.persist() { Ok(()) => { self.error = None; self.notice = Some("Settings saved. They apply on the next replay start.".into()); }, Err(e) => self.error = Some(e) }
            }
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) && self.active() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.closing {
                self.closing = true;
                self.send(Command::Stop);
            }
        }
        if self.closing && !self.active() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.active() || self.closing {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        egui::SidePanel::left("navigation")
            .exact_width(184.0)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(13, 18, 24))
                    .inner_margin(24.0),
            )
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("V /  Veyva").size(25.0).strong().color(MINT));
                ui.label(RichText::new("YOUR MOMENT, KEPT.").size(9.0).color(MUTED));
                ui.add_space(40.0);
                for (page, label) in [
                    (Page::Capture, "Capture"),
                    (Page::Settings, "Settings"),
                    (Page::About, "About"),
                ] {
                    ui.selectable_value(&mut self.page, page, label);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.label(RichText::new("LOCAL BY DESIGN").size(10.0).color(MINT));
                    ui.label(RichText::new("Veyva 0.1.0").small().color(MUTED));
                });
            });
        egui::CentralPanel::default().frame(egui::Frame::none().fill(INK).inner_margin(32.0)).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if self.closing { banner(ui,"Finishing capture and closing safely...", MINT); }
                if let Some(e) = self.load_error.clone() {
                    banner(ui,&format!("Settings could not be loaded: {e}"),CORAL);
                    if ui.button("Reset settings to defaults").clicked() {
                        let result = self.settings_path.as_ref().map_err(Clone::clone).and_then(|p| Settings::default().save(p));
                        match result { Ok(()) => { self.settings = Settings::default(); self.load_error = None; self.error = None; }, Err(e) => self.error = Some(e) }
                    }
                    ui.add_space(12.0);
                }
                match self.page {
                    Page::Capture => self.capture(ui),
                    Page::Settings => self.settings(ui),
                    Page::About => {
                        ui.heading("Made for the moment.");
                        ui.label(RichText::new("Veyva 0.1.0 - First milestone").color(MUTED));
                        ui.add_space(20.0);
                        card(ui, |ui| {
                            ui.label(RichText::new("A small, local replay recorder.").size(22.0).strong());
                            ui.label("Desktop capture and H.264 encoding run in the background only after you start replay. Save the recent buffer to an MP4 on your computer.");
                            ui.separator();
                            ui.label("This milestone captures video only. Audio, hotkeys and tray controls are not available. Timed recording remains available through the command line.");
                            ui.label(RichText::new("No account. No telemetry. No cloud upload.").color(MINT));
                        });
                    }
                }
                if let Some(e) = &self.error { banner(ui,e,CORAL); }
                if let Some(n) = &self.notice { banner(ui,n,MINT); }
            });
        });
    }
}

fn card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(SLATE)
        .rounding(12.0)
        .inner_margin(22.0)
        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(39, 51, 66)))
        .show(ui, |ui| {
            ui.set_min_width((ui.available_width()).max(0.0_f32));
            contents(ui);
        });
}

fn metric(ui: &mut egui::Ui, label: &str, value: String) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).size(10.0).color(MUTED));
        ui.label(RichText::new(value).size(20.0));
    });
}

fn banner(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::none()
        .fill(color.gamma_multiply(0.08))
        .rounding(8.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.colored_label(color, text);
        });
}

fn field(
    ui: &mut egui::Ui,
    name: &str,
    value: &mut u32,
    range: std::ops::RangeInclusive<u32>,
    speed: u32,
) {
    ui.label(name);
    ui.add(egui::DragValue::new(value).range(range).speed(speed));
    ui.end_row();
}
