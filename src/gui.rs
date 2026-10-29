use crate::session::{self, Command, Session};
use crate::settings::{CaptureProfile, Settings};
use eframe::egui::{self, Color32, RichText, Vec2};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const INK: Color32 = Color32::from_rgb(16, 21, 28);
const SLATE: Color32 = Color32::from_rgb(24, 34, 45);
const MINT: Color32 = Color32::from_rgb(112, 226, 188);
const MUTED: Color32 = Color32::from_rgb(147, 165, 184);
const CORAL: Color32 = Color32::from_rgb(255, 125, 135);

pub fn run() -> Result<(), String> {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/preview.png"))
        .map_err(|e| format!("Could not load app icon: {e}"))?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Veyva")
            .with_inner_size([1080.0, 820.0])
            .with_min_inner_size([860.0, 680.0])
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
    context: egui::Context,
    hotkey: Option<crate::hotkey::Hotkey>,
    shortcut_error: Option<String>,
    pending_auto_start: bool,
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
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(28.0));
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
            context: cc.egui_ctx.clone(),
            hotkey: None,
            shortcut_error: None,
            pending_auto_start: true,
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
        self.settings.validate()?;
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

    fn bind_hotkey(&mut self) {
        self.hotkey = None;
        self.shortcut_error = None;
        let ctx = self.context.clone();
        match crate::hotkey::Hotkey::register(
            self.settings.hotkey_modifiers,
            self.settings.hotkey_key,
            std::sync::Arc::new(move || ctx.request_repaint()),
        ) {
            Ok(key) => self.hotkey = Some(key),
            Err(error) => self.shortcut_error = Some(error),
        }
    }
    fn output_path(&self, kind: &str) -> Result<PathBuf, String> {
        let folder = PathBuf::from(self.output.trim());
        if !folder.is_absolute() {
            return Err("Choose an absolute output folder first".into());
        }
        std::fs::create_dir_all(&folder)
            .map_err(|e| format!("Could not create output folder: {e}"))?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        Ok(folder.join(format!("Veyva-{kind}-{stamp}-{}.mp4", std::process::id())))
    }
    fn save_replay(&mut self) {
        let ready = self
            .session
            .as_ref()
            .and_then(|s| s.status.lock().ok())
            .is_some_and(|s| s.replay_enabled && !s.saving && s.buffered_seconds > 0.0);
        if !ready {
            self.error = Some("Replay is disabled, still warming up, or already saving".into());
            return;
        }
        match self.output_path("replay") {
            Ok(path) => self.send(Command::SaveReplay(path)),
            Err(error) => self.error = Some(error),
        }
    }
    fn start_recording(&mut self) {
        let path = match self.output_path("recording") {
            Ok(path) => path,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        if !self.active() {
            self.start();
        }
        if self.active() {
            self.send(Command::StartRecording(path));
        }
    }
    fn capture(&mut self, ui: &mut egui::Ui) {
        ui.heading("Capture");
        ui.label(
            RichText::new("Two ways to keep the moment. One shared capture engine.").color(MUTED),
        );
        ui.add_space(18.0);
        let state = self
            .session
            .as_ref()
            .and_then(|s| s.status.lock().ok())
            .map(|s| s.clone())
            .unwrap_or_default();
        let active = self.active();
        let stopping = state.phase.starts_with("Stopping");
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("NORMAL RECORDING")
                        .small()
                        .strong()
                        .color(MUTED),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.colored_label(
                        if state.recording { MINT } else { MUTED },
                        if state.finalizing {
                            "Finalizing file..."
                        } else if state.paused {
                            "Paused"
                        } else if state.recording {
                            "Recording"
                        } else {
                            "Ready"
                        },
                    );
                });
            });
            let secs = state.recorded_seconds.max(0.0) as u64;
            ui.label(
                RichText::new(format!(
                    "{:02}:{:02}:{:02}",
                    secs / 3600,
                    (secs / 60) % 60,
                    secs % 60
                ))
                .size(42.0)
                .strong(),
            );
            ui.label(
                RichText::new(
                    "Start, pause, resume, then finish and save. Replay stays independent.",
                )
                .color(MUTED),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !state.recording
                            && !state.finalizing
                            && !stopping
                            && !self.closing
                            && self.load_error.is_none(),
                        egui::Button::new(RichText::new("Start recording").color(INK)).fill(MINT),
                    )
                    .clicked()
                {
                    self.start_recording();
                }
                if ui
                    .add_enabled(
                        state.recording && !state.finalizing && !self.closing,
                        egui::Button::new(if state.paused { "Resume" } else { "Pause" }),
                    )
                    .clicked()
                {
                    self.send(if state.paused {
                        Command::ResumeRecording
                    } else {
                        Command::PauseRecording
                    });
                }
                if ui
                    .add_enabled(
                        state.recording && !state.finalizing && !self.closing,
                        egui::Button::new("Finish & save"),
                    )
                    .clicked()
                {
                    self.send(Command::FinishRecording);
                }
            });
        });
        ui.add_space(16.0);
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("INSTANT REPLAY")
                        .small()
                        .strong()
                        .color(MUTED),
                );
                ui.colored_label(
                    if state.replay_enabled { MINT } else { MUTED },
                    if state.replay_enabled { "Armed" } else { "Off" },
                );
            });
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{:.1}s", state.buffered_seconds))
                        .size(34.0)
                        .strong(),
                );
                ui.label(
                    RichText::new(format!(
                        "of {} seconds configured history",
                        self.settings.profile.history_seconds
                    ))
                    .color(MUTED),
                );
            });
            ui.add(
                egui::ProgressBar::new(
                    (state.buffered_seconds / f64::from(self.settings.profile.history_seconds))
                        .clamp(0.0, 1.0) as f32,
                )
                .fill(MINT)
                .desired_height(4.0),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        active
                            && state.replay_enabled
                            && state.buffered_seconds > 0.0
                            && !state.saving
                            && !self.closing
                            && !stopping,
                        egui::Button::new(
                            RichText::new(if state.saving {
                                "Saving..."
                            } else {
                                "Save replay"
                            })
                            .color(INK),
                        )
                        .fill(MINT),
                    )
                    .clicked()
                {
                    self.save_replay();
                }
                ui.label(
                    RichText::new(crate::hotkey::label(
                        self.settings.hotkey_modifiers,
                        self.settings.hotkey_key,
                    ))
                    .color(MINT)
                    .strong(),
                );
                if ui
                    .add_enabled(
                        !self.closing && !stopping,
                        egui::Button::new(if state.replay_enabled {
                            "Turn off replay"
                        } else {
                            "Turn on replay"
                        }),
                    )
                    .clicked()
                {
                    if !active {
                        self.start();
                    }
                    if self.active() {
                        self.send(Command::SetReplayEnabled(!state.replay_enabled));
                    }
                }
            });
            ui.label(
                RichText::new(format!(
                    "{:.1} MiB encoded history / {} MiB budget   |   {} frames   |   {} skipped",
                    state.buffer_bytes as f64 / 1_048_576.0,
                    self.settings.profile.replay_budget_mib,
                    state.submitted,
                    state.dropped
                ))
                .small()
                .color(MUTED),
            );
        });
        ui.add_space(14.0);
        let p = self.settings.profile;
        ui.horizontal(|ui| {
            ui.colored_label(
                if state.audio_active { MINT } else { MUTED },
                if state.audio_active {
                    "System audio: on"
                } else if self.settings.audio_enabled && active {
                    "System audio: unavailable / starting"
                } else {
                    "System audio: off"
                },
            );
            ui.separator();
            ui.label(
                RichText::new(format!(
                    "{} x {} / {} fps / {:.1} Mbps",
                    p.width,
                    p.height,
                    p.fps,
                    p.video_kbps as f64 / 1000.0
                ))
                .color(MUTED),
            );
            if ui.small_button("Preferences").clicked() {
                self.page = Page::Settings;
            }
        });
        if let Some(error) = state.audio_error {
            banner(
                ui,
                &format!("System audio unavailable: {error}. Files may contain video only."),
                CORAL,
            );
        }
        ui.add_space(14.0);
        ui.label(RichText::new("SAVE LOCATION").small().strong().color(MUTED));
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.output)
                    .desired_width((ui.available_width() - 120.0).max(100.0)),
            );
            if ui.button("Open folder").clicked() {
                let path = PathBuf::from(self.output.trim());
                if path.is_dir() {
                    if let Err(error) = std::process::Command::new("explorer.exe").arg(path).spawn()
                    {
                        self.error = Some(error.to_string());
                    }
                } else {
                    self.error = Some(
                        "The folder is created when you start recording or save a replay".into(),
                    );
                }
            }
        });
        if let Some(path) = state.last_saved {
            ui.label(
                RichText::new(format!("Saved: {}", path.display()))
                    .small()
                    .color(MINT),
            );
        }
        if let Some(error) = state.error {
            banner(ui, &error, CORAL);
        }
        if let Some(error) = &self.shortcut_error {
            banner(ui, error, CORAL);
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
                "Stop capture to edit settings. Replay history will be cleared.",
                MINT,
            );
            let busy = self
                .session
                .as_ref()
                .and_then(|s| s.status.lock().ok())
                .is_some_and(|s| s.recording || s.finalizing || s.saving);
            if ui
                .add_enabled(!busy, egui::Button::new("Stop capture to edit"))
                .clicked()
            {
                self.send(Command::Stop);
            }
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
            ui.label(RichText::new("The history budget caps encoded payload, not total process memory. A smaller budget may retain fewer seconds. System audio uses the default Windows output device. Microphone capture is not included.").small().color(MUTED));
            card(ui, |ui| {
                ui.checkbox(&mut self.settings.replay_auto_start,"Arm replay when Veyva opens");
                ui.checkbox(&mut self.settings.audio_enabled,"Record system audio");
                ui.label(RichText::new("SAVE REPLAY SHORTCUT").small().strong().color(MUTED));
                ui.horizontal(|ui|{
                    for (flag,label) in [(1,"Alt"),(2,"Ctrl"),(4,"Shift"),(8,"Win")] {
                        let mut enabled=self.settings.hotkey_modifiers&flag!=0;
                        if ui.checkbox(&mut enabled,label).changed(){if enabled{self.settings.hotkey_modifiers|=flag;}else{self.settings.hotkey_modifiers&=!flag;}}
                    }
                    egui::ComboBox::from_id_salt("hotkey-key").selected_text(format!("F{}",self.settings.hotkey_key-0x70+1)).show_ui(ui,|ui|{
                        for key in 0x70..=0x7B {ui.selectable_value(&mut self.settings.hotkey_key,key,format!("F{}",key-0x70+1));}
                    });
                });
            });
            if ui.button("Save settings").clicked() {
                match self.persist() { Ok(()) => { self.bind_hotkey(); self.error = None; self.notice = Some("Settings saved. A new session uses the updated settings.".into()); if self.settings.replay_auto_start {self.start();} }, Err(e) => self.error = Some(e) }
            }
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.pending_auto_start {
            self.pending_auto_start = false;
            if self.load_error.is_none() {
                self.bind_hotkey();
                if self.settings.replay_auto_start {
                    self.start();
                }
            }
        }
        if self
            .hotkey
            .as_ref()
            .is_some_and(|hotkey| hotkey.take_pressed())
            && !self.closing
        {
            self.save_replay();
        }
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
                        match result { Ok(()) => { self.settings = Settings::default(); self.load_error = None; self.error = None; self.pending_auto_start = true; }, Err(e) => self.error = Some(e) }
                    }
                    ui.add_space(12.0);
                }
                match self.page {
                    Page::Capture => self.capture(ui),
                    Page::Settings => self.settings(ui),
                    Page::About => {
                        ui.heading("Made for the moment.");
                        ui.label(RichText::new("Veyva 0.1.0").color(MUTED));
                        ui.add_space(20.0);
                        card(ui, |ui| {
                            ui.label(RichText::new("Recording and replay, together.").size(22.0).strong());
                            ui.label("One hardware encoder powers normal recording and replay history. Replay arms when Veyva opens unless disabled in Preferences.");
                            ui.separator();
                            ui.label("System audio and configurable replay shortcuts are supported. Microphone capture and tray controls are not included.");
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
        .inner_margin(24.0)
        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(39, 51, 66)))
        .show(ui, |ui| {
            ui.set_min_width((ui.available_width()).max(0.0_f32));
            contents(ui);
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
