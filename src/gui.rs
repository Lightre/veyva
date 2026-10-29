use crate::session::{self, Command, Session};
use crate::settings::{CaptureProfile, Settings, Theme};
use eframe::egui::{self, Color32, RichText, Vec2};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const CORAL: Color32 = Color32::from_rgb(225, 86, 104);

#[derive(Clone, Copy)]
struct Palette {
    background: Color32,
    surface: Color32,
    border: Color32,
    text: Color32,
    muted: Color32,
    accent: Color32,
    accent_text: Color32,
}

impl Palette {
    fn for_theme(theme: Theme) -> Self {
        let rgb = Color32::from_rgb;
        match theme {
            Theme::Dark => Self {
                background: rgb(20, 21, 23),
                surface: rgb(30, 32, 35),
                border: rgb(48, 51, 55),
                text: rgb(238, 244, 247),
                muted: rgb(169, 183, 198),
                accent: rgb(112, 226, 188),
                accent_text: rgb(16, 21, 28),
            },
            Theme::Light => Self {
                background: rgb(250, 250, 248),
                surface: rgb(240, 242, 239),
                border: rgb(217, 223, 217),
                text: rgb(27, 41, 37),
                muted: rgb(104, 118, 111),
                accent: rgb(36, 91, 72),
                accent_text: Color32::WHITE,
            },
        }
    }
}

fn apply_theme(ctx: &egui::Context, theme: Theme) {
    let p = Palette::for_theme(theme);
    let mut style = (*ctx.style()).clone();
    style.visuals = match theme {
        Theme::Dark => egui::Visuals::dark(),
        Theme::Light => egui::Visuals::light(),
    };
    style.visuals.panel_fill = p.background;
    style.visuals.window_fill = p.surface;
    let input_fill = match theme {
        Theme::Dark => Color32::from_rgb(15, 16, 18),
        Theme::Light => Color32::from_rgb(228, 232, 227),
    };
    style.visuals.extreme_bg_color = input_fill;
    style.visuals.override_text_color = Some(p.text);
    style.visuals.selection.bg_fill = p.accent.gamma_multiply(0.25);
    style.visuals.selection.stroke.color = p.accent;
    style.visuals.widgets.inactive.bg_fill = p.surface;
    style.visuals.widgets.inactive.weak_bg_fill = input_fill;
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    style.visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
    style.visuals.widgets.open.bg_stroke = egui::Stroke::NONE;
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, p.border);
    style.visuals.widgets.hovered.bg_fill = p.accent.gamma_multiply(0.18);
    style.visuals.widgets.hovered.weak_bg_fill = input_fill;
    style.visuals.widgets.active.bg_fill = p.accent.gamma_multiply(0.3);
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    style.spacing.scroll.floating = true;
    style.spacing.scroll.floating_allocated_width = 0.0;
    style.spacing.scroll.bar_width = 6.0;
    style.spacing.scroll.dormant_background_opacity = 0.0;
    style.spacing.scroll.active_background_opacity = 0.0;
    style.spacing.scroll.interact_background_opacity = 0.0;
    style.spacing.item_spacing = Vec2::new(12.0, 12.0);
    style.spacing.button_padding = Vec2::new(16.0, 10.0);
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(24.0));
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
    ctx.set_style(style);
}

pub fn run() -> Result<(), String> {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/preview.png"))
        .map_err(|e| format!("Could not load app icon: {e}"))?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Veyva")
            .with_decorations(false)
            .with_inner_size([1180.0, 860.0])
            .with_min_inner_size([1000.0, 740.0])
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
    folder_picker: Option<std::sync::mpsc::Receiver<Result<Option<String>, String>>>,
    page: Page,
    logo: egui::TextureHandle,
    settings: Settings,
    applied: Settings,
    pending_apply: Option<bool>,
    pending_command: Option<(u8, bool)>,
    pending_saved: Option<PathBuf>,
    pending_error: Option<String>,
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
        let settings_path = Settings::default_path();
        let loaded = settings_path
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|p| Settings::load(p));
        let (settings, load_error) = match loaded {
            Ok(s) => (s, None),
            Err(e) => (Settings::default(), Some(e)),
        };
        apply_theme(&cc.egui_ctx, settings.theme);
        let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/preview.png"))
            .expect("embedded app icon was validated before opening the window");
        let logo = cc.egui_ctx.load_texture(
            "veyva-mark",
            egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            ),
            egui::TextureOptions::LINEAR,
        );
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
            folder_picker: None,
            page: Page::Capture,
            logo,
            settings,
            applied: settings,
            pending_apply: None,
            pending_command: None,
            pending_saved: None,
            pending_error: None,
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
        let result = session::start(self.applied);
        match result {
            Ok(s) => {
                self.session = Some(s);
                self.notice = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn send(&mut self, command: Command) {
        if (self.pending_apply.is_some() || self.pending_command.is_some())
            && !matches!(command, Command::Stop)
        {
            return;
        }
        let pending = match &command {
            Command::StartRecording(_) => Some((0, true)),
            Command::PauseRecording => Some((1, true)),
            Command::ResumeRecording => Some((1, false)),
            Command::FinishRecording => Some((0, false)),
            Command::SetReplayEnabled(value) => Some((2, *value)),
            Command::SaveReplay(_) => Some((3, true)),
            Command::Stop => None,
        };
        if let Some(s) = &self.session {
            self.pending_error = s.status.lock().ok().and_then(|s| s.error.clone());
            match s.send(command) {
                Ok(()) => self.pending_command = pending,
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn apply_settings(&mut self) {
        let state = self
            .session
            .as_ref()
            .and_then(|s| s.status.lock().ok())
            .map(|s| s.clone())
            .unwrap_or_default();
        if self.pending_apply.is_some()
            || self.pending_command.is_some()
            || state.phase == "Starting"
            || self.closing
            || state.recording
            || state.finalizing
            || state.saving
            || state.phase.starts_with("Stopping")
        {
            return;
        }
        if let Err(error) = self.persist() {
            self.error = Some(error);
            return;
        }
        self.error = None;
        if self.active() {
            self.pending_apply = Some(state.replay_enabled);
            self.notice = Some(
                "Applying preferences: stopping capture safely. Replay history will be cleared."
                    .into(),
            );
            self.send(Command::Stop);
        } else {
            self.applied = self.settings;
            self.bind_hotkey();
            self.notice = Some("Preferences applied. New captures use these settings.".into());
        }
    }

    fn apply_controls(&mut self, ui: &mut egui::Ui) {
        let state = self
            .session
            .as_ref()
            .and_then(|s| s.status.lock().ok())
            .map(|s| s.clone())
            .unwrap_or_default();
        let busy = self.pending_command.is_some()
            || state.phase == "Starting"
            || self.pending_apply.is_some()
            || self.closing
            || state.recording
            || state.finalizing
            || state.saving
            || state.phase.starts_with("Stopping");
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !busy && self.load_error.is_none() && self.settings != self.applied,
                    egui::Button::new("Apply changes"),
                )
                .clicked()
            {
                self.apply_settings();
            }
            if ui
                .add_enabled(
                    !busy && self.settings != self.applied,
                    egui::Button::new("Discard changes"),
                )
                .clicked()
            {
                self.settings = self.applied;
            }
        });
        ui.label(
            RichText::new(if self.pending_apply.is_some() {
                "Applying preferences..."
            } else if busy {
                "Apply after recording and exports finish."
            } else if self.settings != self.applied {
                "Unsaved changes. Applying restarts active replay and clears its history."
            } else {
                "All preferences applied."
            })
            .small()
            .color(ui.visuals().weak_text_color()),
        );
    }

    fn quick_capture_settings(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new(ui.id().with("quick-capture"))
            .num_columns(2)
            .spacing([16.0, 14.0])
            .show(ui, |ui| {
                field(ui, "Width", &mut self.settings.profile.width, 320..=7680, 2);
                field(
                    ui,
                    "Height",
                    &mut self.settings.profile.height,
                    180..=4320,
                    2,
                );
                field(ui, "Frame rate", &mut self.settings.profile.fps, 1..=240, 1);
            });
        setting_checkbox(ui, &mut self.settings.audio_enabled, "Record system audio");
        setting_checkbox(ui, &mut self.settings.capture_cursor, "Capture cursor");
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
        if self.pending_apply.is_some() || self.pending_command.is_some() || self.closing {
            return;
        }
        let ready = self
            .session
            .as_ref()
            .and_then(|s| s.status.lock().ok())
            .is_some_and(|s| s.replay_enabled && !s.saving && s.buffered_seconds > 0.0);
        if !ready {
            self.error = Some("Replay is disabled, still warming up, or already saving".into());
            return;
        }
        self.pending_saved = self
            .session
            .as_ref()
            .and_then(|s| s.status.lock().ok())
            .and_then(|s| s.last_saved.clone());
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
    fn appearance(&mut self, ui: &mut egui::Ui) {
        let previous = self.settings.theme;
        ui.label("Appearance");
        egui::ComboBox::from_id_salt("appearance")
            .width(ui.available_width() - 12.0)
            .selected_text(match previous {
                Theme::Dark => "Dark",
                Theme::Light => "Light",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.settings.theme, Theme::Dark, "Dark");
                ui.selectable_value(&mut self.settings.theme, Theme::Light, "Light");
            });
        if self.settings.theme != previous {
            // Save only appearance; unsaved capture preferences stay a draft.
            let result = self
                .settings_path
                .as_ref()
                .map_err(Clone::clone)
                .and_then(|path| {
                    if self.load_error.is_some() {
                        return Err("Reset unreadable settings before saving appearance".into());
                    }
                    let mut saved = Settings::load(path)?;
                    saved.theme = self.settings.theme;
                    saved.save(path)
                });
            match result {
                Ok(()) => {
                    self.applied.theme = self.settings.theme;
                    apply_theme(&self.context, self.settings.theme);
                }
                Err(error) => {
                    self.settings.theme = previous;
                    self.error = Some(error);
                }
            }
        }
    }

    fn titlebar(&mut self, ctx: &egui::Context) {
        let p = Palette::for_theme(self.settings.theme);
        egui::TopBottomPanel::top("titlebar")
            .exact_height(44.0)
            .frame(
                egui::Frame::none()
                    .fill(p.background)
                    .inner_margin(egui::Margin {
                        left: 22.0,
                        right: 0.0,
                        top: 0.0,
                        bottom: 0.0,
                    }),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.image((self.logo.id(), Vec2::splat(32.0)));
                    ui.label(RichText::new("Veyva").size(25.0).strong());
                    ui.add_space(28.0);
                    for (page, label) in [
                        (Page::Capture, "Capture"),
                        (Page::Settings, "Preferences"),
                        (Page::About, "About"),
                    ] {
                        let selected = self.page == page;
                        let response = ui.add(
                            egui::Button::new(RichText::new(label).color(if selected {
                                p.accent
                            } else {
                                p.muted
                            }))
                            .frame(false),
                        );
                        if selected {
                            ui.painter().hline(
                                response.rect.x_range(),
                                ui.max_rect().bottom() - 1.5,
                                egui::Stroke::new(3.0_f32, p.accent),
                            );
                        }
                        if response.clicked() {
                            self.page = page;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        if window_button(ui, WindowControl::Close, p).clicked() {
                            if self.active() {
                                self.closing = true;
                                self.send(Command::Stop);
                            } else {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                        let control = if maximized {
                            WindowControl::Restore
                        } else {
                            WindowControl::Maximize
                        };
                        if window_button(ui, control, p).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                        }
                        if window_button(ui, WindowControl::Minimize, p).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                        let drag = ui.allocate_response(
                            Vec2::new(ui.available_width(), 44.0),
                            egui::Sense::click_and_drag(),
                        );
                        if drag.double_clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                        } else if drag.drag_started_by(egui::PointerButton::Primary) {
                            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }
                    });
                });
            });
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        let p = Palette::for_theme(self.settings.theme);
        let state = self
            .session
            .as_ref()
            .and_then(|s| s.status.lock().ok())
            .map(|s| s.clone())
            .unwrap_or_default();
        ui.label(RichText::new("Capture settings").size(18.0).strong());
        ui.label(
            RichText::new("H.264 hardware encoding")
                .small()
                .color(p.muted),
        );
        ui.add_enabled_ui(
            self.pending_apply.is_none() && !self.closing && self.load_error.is_none(),
            |ui| self.quick_capture_settings(ui),
        );
        self.apply_controls(ui);
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(RichText::new("Audio").size(18.0).strong());
        let label = if state.audio_active {
            "● System audio active"
        } else if self.active() && self.settings.audio_enabled {
            "System audio starting / unavailable"
        } else if self.settings.audio_enabled {
            "System audio enabled"
        } else {
            "System audio off"
        };
        ui.colored_label(
            if state.audio_active {
                p.accent
            } else {
                p.muted
            },
            label,
        );
        ui.label(
            RichText::new("Default Windows output device")
                .small()
                .color(p.muted),
        );
        ui.add_enabled(
            false,
            egui::Checkbox::new(&mut false, "Microphone · Coming later"),
        );
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label("File destination");
        let (choose_folder, open_folder) = egui::Frame::none()
            .fill(ui.visuals().extreme_bg_color)
            .rounding(6.0)
            .inner_margin(egui::Margin::symmetric(8.0, 4.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 80.0).max(40.0);
                    ui.add_sized(
                        [width, 28.0],
                        egui::TextEdit::singleline(&mut self.output).frame(false),
                    );
                    (
                        folder_button(ui, p, false).clicked(),
                        folder_button(ui, p, true).clicked(),
                    )
                })
                .inner
            })
            .inner;
        if choose_folder && self.folder_picker.is_none() {
            let (sender, receiver) = std::sync::mpsc::channel();
            let ctx = self.context.clone();
            match std::thread::Builder::new()
                .name("veyva-folder-picker".into())
                .spawn(move || {
                    let _ = sender.send(choose_output_folder());
                    ctx.request_repaint();
                }) {
                Ok(_) => self.folder_picker = Some(receiver),
                Err(error) => self.error = Some(error.to_string()),
            }
        }
        if open_folder {
            let path = PathBuf::from(self.output.trim());
            if path.is_dir() {
                if let Err(error) = std::process::Command::new("explorer.exe").arg(path).spawn() {
                    self.error = Some(error.to_string());
                }
            } else {
                self.error =
                    Some("The folder is created when a recording or replay is saved".into());
            }
        }
        ui.label(
            RichText::new("Folder choice applies to this session.")
                .small()
                .color(p.muted),
        );
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        self.appearance(ui);
    }

    fn capture(&mut self, ui: &mut egui::Ui) {
        let p = Palette::for_theme(self.settings.theme);
        let state = self
            .session
            .as_ref()
            .and_then(|s| s.status.lock().ok())
            .map(|s| s.clone())
            .unwrap_or_default();
        let active = self.active();
        let stopping = state.phase.starts_with("Stopping")
            || self.pending_apply.is_some()
            || self.pending_command.is_some();
        ui.heading("Normal recording");
        ui.label(RichText::new("Record your screen and system audio.").color(p.muted));
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            let secs = state.recorded_seconds.max(0.0) as u64;
            let size = (ui.available_width() / 7.5).clamp(48.0, 76.0);
            ui.label(
                RichText::new(format!(
                    "{:02}:{:02}:{:02}",
                    secs / 3600,
                    (secs / 60) % 60,
                    secs % 60
                ))
                .size(size),
            );
            let label = if state.finalizing {
                "Finalizing file..."
            } else if state.paused {
                "Paused"
            } else if state.recording {
                "Recording"
            } else {
                "Ready to record"
            };
            ui.label(
                RichText::new(format!("●  {label}")).color(if state.recording {
                    p.accent
                } else {
                    p.muted
                }),
            );
        });
        ui.add_space(42.0);
        ui.horizontal(|ui| {
            let width = ((ui.available_width() - 12.0) / 2.0).max(130.0);
            let busy = state.finalizing || matches!(self.pending_command, Some((0, false)));
            let enabled = !busy && !stopping && !self.closing && self.load_error.is_none();
            let mut primary = p;
            if state.recording {
                primary.accent = Color32::from_rgb(190, 48, 65);
                primary.accent_text = Color32::WHITE;
            }
            let label = if busy {
                "Saving..."
            } else if state.recording {
                "Finish & save"
            } else {
                "Start recording"
            };
            if transport_button(
                ui,
                label,
                width,
                enabled,
                Some(primary),
                if state.recording || busy { 1 } else { 0 },
            )
            .clicked()
            {
                if state.recording {
                    self.send(Command::FinishRecording);
                } else {
                    self.start_recording();
                }
            }
            if transport_button(
                ui,
                if state.paused { "Resume" } else { "Pause" },
                width,
                state.recording && !busy && !stopping && !self.closing,
                None,
                if state.paused { 3 } else { 2 },
            )
            .clicked()
            {
                self.send(if state.paused {
                    Command::ResumeRecording
                } else {
                    Command::PauseRecording
                });
            }
        });
        ui.add_space(22.0);
        ui.separator();
        ui.add_space(16.0);
        ui.columns(2, |columns| {
            readout(
                &mut columns[0],
                "Record display",
                &format!("Monitor {}", u64::from(self.settings.monitor) + 1),
            );
            readout(&mut columns[1], "Recording area", "Entire display");
        });
        ui.add_space(18.0);
        ui.separator();
        ui.add_space(18.0);
        ui.label(RichText::new("Instant replay").size(22.0).strong());
        ui.label(
            RichText::new("Keep the last moments. Save independently of normal recording.")
                .color(p.muted),
        );
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let mut armed = state.replay_enabled;
            if ui
                .add_enabled(
                    !self.closing && !stopping && self.load_error.is_none(),
                    egui::Checkbox::new(&mut armed, "Armed"),
                )
                .changed()
            {
                if !active {
                    self.start();
                }
                if self.active() {
                    self.send(Command::SetReplayEnabled(armed));
                }
            }
            ui.label(format!(
                "{:.1}s / {}s",
                state.buffered_seconds, self.applied.profile.history_seconds
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if action(
                    ui,
                    if state.saving {
                        "Saving..."
                    } else {
                        "Save replay"
                    },
                    150.0,
                    active
                        && state.replay_enabled
                        && state.buffered_seconds > 0.0
                        && !state.saving
                        && !self.closing
                        && self.pending_apply.is_none()
                        && !stopping,
                    Some(p),
                )
                .clicked()
                {
                    self.save_replay();
                }
            });
        });
        ui.add(
            egui::ProgressBar::new(
                (state.buffered_seconds / f64::from(self.applied.profile.history_seconds))
                    .clamp(0.0, 1.0) as f32,
            )
            .fill(p.accent)
            .desired_height(3.0),
        );
        ui.label(
            RichText::new(format!(
                "{}  ·  {:.1} / {} MiB history  ·  {} skipped frames",
                crate::hotkey::label(self.settings.hotkey_modifiers, self.settings.hotkey_key),
                state.buffer_bytes as f64 / 1_048_576.0,
                self.applied.profile.replay_budget_mib,
                state.dropped
            ))
            .small()
            .color(p.muted),
        );
        if let Some(path) = state.last_saved {
            ui.label(
                RichText::new(format!("Saved: {}", path.display()))
                    .small()
                    .color(p.accent),
            );
        }
        if let Some(error) = state.audio_error {
            banner(
                ui,
                &format!("System audio unavailable: {error}. Files may contain video only."),
                CORAL,
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
        let palette = Palette::for_theme(self.settings.theme);
        ui.heading("Preferences");
        self.appearance(ui);
        ui.add_space(20.0);
        ui.label(RichText::new("Choose a profile that fits your machine.").color(palette.muted));
        ui.add_space(16.0);
        ui.label(
            RichText::new("Changes stay in a shared draft until you apply them.")
                .small()
                .color(palette.muted),
        );
        ui.add_enabled_ui(self.pending_apply.is_none() && !self.closing && self.load_error.is_none(), |ui| {
            card(ui, |ui| {
                ui.label(RichText::new("01 / QUALITY PRESET").small().strong().color(palette.muted));
                ui.horizontal(|ui| {
                    for (name, profile) in [("Low impact",CaptureProfile::low_end()),("Balanced",CaptureProfile::balanced()),("High quality",CaptureProfile::high())] {
                        if ui.selectable_label(self.settings.profile == profile,name).clicked() { self.settings.profile = profile; }
                    }
                    if ![CaptureProfile::low_end(),CaptureProfile::balanced(),CaptureProfile::high()].contains(&self.settings.profile) { ui.label(RichText::new("Custom").color(palette.accent)); }
                });
                ui.label(RichText::new("Presets are targets. Actual performance depends on your hardware.").small().color(palette.muted));
            });
            ui.add_space(10.0);
            card(ui, |ui| {
                ui.label(RichText::new("02 / CAPTURE & REPLAY").small().strong().color(palette.muted));
                egui::Grid::new("capture-settings").num_columns(2).spacing([32.0,18.0]).show(ui, |ui| {
                    field(ui,"Width (pixels)",&mut self.settings.profile.width,320..=7680,2);
                    field(ui,"Height (pixels)",&mut self.settings.profile.height,180..=4320,2);
                    field(ui,"Frame rate (fps)",&mut self.settings.profile.fps,1..=240,1);
                    field(ui,"Video bitrate (kbps)",&mut self.settings.profile.video_kbps,250..=100_000,250);
                    field(ui,"Replay history (seconds)",&mut self.settings.profile.history_seconds,5..=600,5);
                    field(ui,"History budget (MiB)",&mut self.applied.profile.replay_budget_mib,16..=512,16);
                    field(ui,"Monitor index (starts at 0)",&mut self.settings.monitor,0..=32,1);
                });
            });
            ui.add_space(8.0);
            ui.label(RichText::new("The history budget caps encoded payload, not total process memory. A smaller budget may retain fewer seconds. System audio uses the default Windows output device. Microphone capture is not included.").small().color(palette.muted));
            card(ui, |ui| {
                ui.label(RichText::new("03 / AUDIO & BEHAVIOR").small().strong().color(palette.muted));
                setting_checkbox(ui, &mut self.settings.capture_cursor,"Capture cursor");
                setting_checkbox(ui, &mut self.settings.replay_auto_start,"Arm replay when Veyva opens");
                setting_checkbox(ui, &mut self.settings.audio_enabled,"Record system audio");
                ui.label(RichText::new("SAVE REPLAY SHORTCUT").small().strong().color(palette.muted));
                ui.horizontal(|ui|{
                    for (flag,label) in [(1,"Alt"),(2,"Ctrl"),(4,"Shift"),(8,"Win")] {
                        let mut enabled=self.settings.hotkey_modifiers&flag!=0;
                        if setting_checkbox(ui, &mut enabled,label).changed(){if enabled{self.settings.hotkey_modifiers|=flag;}else{self.settings.hotkey_modifiers&=!flag;}}
                    }
                    egui::ComboBox::from_id_salt("hotkey-key").selected_text(format!("F{}",self.settings.hotkey_key-0x70+1)).show_ui(ui,|ui|{
                        for key in 0x70..=0x7B {ui.selectable_value(&mut self.settings.hotkey_key,key,format!("F{}",key-0x70+1));}
                    });
                });
            });

        });
        self.apply_controls(ui);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some((kind, target)) = self.pending_command {
            let state = self
                .session
                .as_ref()
                .and_then(|s| s.status.lock().ok())
                .map(|s| s.clone())
                .unwrap_or_default();
            let acknowledged = match kind {
                0 => state.recording == target,
                1 => state.paused == target,
                2 => state.replay_enabled == target,
                _ => state.saving || state.last_saved != self.pending_saved,
            };
            if acknowledged || !self.active() || state.error != self.pending_error {
                self.pending_command = None;
            }
        }
        if let Some(receiver) = &self.folder_picker {
            match receiver.try_recv() {
                Ok(result) => {
                    self.folder_picker = None;
                    match result {
                        Ok(Some(path)) => self.output = path,
                        Ok(None) => {}
                        Err(error) => self.error = Some(error),
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.folder_picker = None;
                    self.error = Some("Folder picker stopped unexpectedly".into());
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.pending_apply.is_some() && !self.active() && !self.closing {
            let replay_enabled = self.pending_apply.take().unwrap_or(false);
            let stop_error = self
                .session
                .as_ref()
                .and_then(|s| s.status.lock().ok())
                .and_then(|s| s.error.clone());
            self.session = None;
            self.applied = self.settings;
            self.bind_hotkey();
            if let Some(error) = stop_error {
                self.error = Some(format!("Capture stopped with an error: {error}. Preferences were saved; arm replay to retry."));
                self.notice = None;
            } else {
                self.start();
                if self.active() {
                    self.send(Command::SetReplayEnabled(replay_enabled));
                    self.notice =
                        Some("Preferences saved. Capture is starting; replay history will warm up again.".into());
                }
            }
        }
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
            && self.pending_apply.is_none()
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
        if self.active() || self.closing || self.pending_apply.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        self.titlebar(ctx);
        let palette = Palette::for_theme(self.settings.theme);
        if self.page == Page::Capture {
            egui::SidePanel::right("inspector")
                .exact_width(380.0)
                .resizable(false)
                .frame(
                    egui::Frame::none()
                        .fill(palette.background)
                        .inner_margin(egui::Margin {
                            left: 26.0,
                            right: 4.0,
                            top: 26.0,
                            bottom: 26.0,
                        })
                        .stroke(egui::Stroke::new(1.0_f32, palette.border)),
                )
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                        )
                        .show(ui, |ui| {
                            scroll_content(ui, 22.0, |ui| self.inspector(ui));
                        });
                });
        }
        egui::CentralPanel::default().frame(egui::Frame::none().fill(palette.background).inner_margin(egui::Margin { left: 30.0, right: 4.0, top: 30.0, bottom: 30.0 })).show(ctx, |ui| {
            egui::ScrollArea::vertical().scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded).show(ui, |ui| {
                scroll_content(ui, 26.0, |ui| {
                if self.closing { banner(ui,"Finishing capture and closing safely...", palette.accent); }
                if let Some(e) = self.load_error.clone() {
                    banner(ui,&format!("Settings could not be loaded: {e}"),CORAL);
                    if ui.button("Reset settings to defaults").clicked() {
                        let result = self.settings_path.as_ref().map_err(Clone::clone).and_then(|p| Settings::default().save(p));
                        match result { Ok(()) => { self.settings = Settings::default(); self.applied = self.settings; apply_theme(ctx, self.settings.theme); self.load_error = None; self.error = None; self.pending_auto_start = true; }, Err(e) => self.error = Some(e) }
                    }
                    ui.add_space(12.0);
                }
                match self.page {
                    Page::Capture => self.capture(ui),
                    Page::Settings => self.settings(ui),
                    Page::About => {
                        ui.heading("Made for the moment.");
                        ui.label(RichText::new("Veyva 0.1.0").color(palette.muted));
                        ui.add_space(20.0);
                        card(ui, |ui| {
                            ui.label(RichText::new("Recording and replay, together.").size(22.0).strong());
                            ui.label("One hardware encoder powers normal recording and replay history. Replay arms when Veyva opens unless disabled in Preferences.");
                            ui.separator();
                            ui.label("System audio and configurable replay shortcuts are supported. Microphone capture and tray controls are not included.");
                            ui.label(RichText::new("No account. No telemetry. No cloud upload.").color(palette.accent));
                        });
                    }
                }
                if let Some(e) = &self.error { banner(ui,e,CORAL); }
                if let Some(n) = &self.notice { banner(ui,n,palette.accent); }
                });
            });
        });
        resize_edges(ctx);
    }
}

fn card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(ui.visuals().window_fill)
        .rounding(12.0)
        .inner_margin(24.0)
        .stroke(egui::Stroke::NONE)
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
    ui.scope(|ui| {
        ui.spacing_mut().interact_size = Vec2::new(104.0, 34.0);
        ui.visuals_mut().widgets.inactive.bg_fill = ui.visuals().extreme_bg_color;
        ui.visuals_mut().widgets.inactive.rounding = egui::Rounding::same(6.0);
        ui.add(egui::DragValue::new(value).range(range).speed(speed));
    });
    ui.end_row();
}

fn action(
    ui: &mut egui::Ui,
    label: &str,
    width: f32,
    enabled: bool,
    accent: Option<Palette>,
) -> egui::Response {
    let mut button = egui::Button::new(RichText::new(label).size(15.0))
        .min_size(Vec2::new(width, 48.0))
        .rounding(7.0);
    if let Some(p) = accent {
        button = egui::Button::new(RichText::new(label).size(15.0).color(p.accent_text))
            .fill(p.accent)
            .min_size(Vec2::new(width, 48.0))
            .rounding(7.0);
    }
    ui.add_enabled(enabled, button.stroke(egui::Stroke::NONE))
}

fn readout(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(
        RichText::new(label)
            .small()
            .color(ui.visuals().weak_text_color()),
    );
    egui::Frame::none()
        .fill(ui.visuals().extreme_bg_color)
        .rounding(5.0)
        .stroke(egui::Stroke::NONE)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_min_width((ui.available_width() - 2.0).max(0.0));
            ui.label(value);
        });
}

fn resize_edges(ctx: &egui::Context) {
    if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
        return;
    }
    let rect = ctx.screen_rect();
    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
        return;
    };
    let left = pos.x < rect.left() + 5.0;
    let right = pos.x > rect.right() - 5.0;
    let top = pos.y < rect.top() + 5.0;
    let bottom = pos.y > rect.bottom() - 5.0;
    use egui::viewport::ResizeDirection as D;
    let direction = match (left, right, top, bottom) {
        (true, _, true, _) => Some((D::NorthWest, egui::CursorIcon::ResizeNwSe)),
        (_, true, _, true) => Some((D::SouthEast, egui::CursorIcon::ResizeNwSe)),
        (_, true, true, _) => Some((D::NorthEast, egui::CursorIcon::ResizeNeSw)),
        (true, _, _, true) => Some((D::SouthWest, egui::CursorIcon::ResizeNeSw)),
        (true, _, _, _) => Some((D::West, egui::CursorIcon::ResizeHorizontal)),
        (_, true, _, _) => Some((D::East, egui::CursorIcon::ResizeHorizontal)),
        (_, _, true, _) => Some((D::North, egui::CursorIcon::ResizeVertical)),
        (_, _, _, true) => Some((D::South, egui::CursorIcon::ResizeVertical)),
        _ => None,
    };
    if let Some((direction, cursor)) = direction {
        ctx.set_cursor_icon(cursor);
        if ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
        }
    }
}

#[derive(Clone, Copy)]
enum WindowControl {
    Minimize,
    Maximize,
    Restore,
    Close,
}

fn window_button(ui: &mut egui::Ui, control: WindowControl, palette: Palette) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(46.0, 44.0), egui::Sense::click());
    let close = matches!(control, WindowControl::Close);
    if response.hovered() || response.is_pointer_button_down_on() {
        ui.painter().rect_filled(
            rect,
            0.0,
            if close {
                Color32::from_rgb(196, 43, 58)
            } else {
                palette.surface
            },
        );
    }
    let stroke = egui::Stroke::new(
        1.25_f32,
        if close && response.hovered() {
            Color32::WHITE
        } else {
            palette.text
        },
    );
    let icon = egui::Rect::from_center_size(rect.center(), Vec2::splat(10.0));
    match control {
        WindowControl::Minimize => {
            ui.painter().hline(icon.x_range(), icon.center().y, stroke);
        }
        WindowControl::Maximize => {
            ui.painter().rect_stroke(icon, 0.0, stroke);
        }
        WindowControl::Restore => {
            let back = icon.translate(Vec2::new(2.0, -2.0));
            ui.painter().rect_stroke(back, 0.0, stroke);
            ui.painter().rect_filled(
                icon,
                0.0,
                if response.hovered() {
                    palette.surface
                } else {
                    palette.background
                },
            );
            ui.painter().rect_stroke(icon, 0.0, stroke);
        }
        WindowControl::Close => {
            ui.painter()
                .line_segment([icon.left_top(), icon.right_bottom()], stroke);
            ui.painter()
                .line_segment([icon.right_top(), icon.left_bottom()], stroke);
        }
    }
    response.on_hover_text(match control {
        WindowControl::Minimize => "Minimize",
        WindowControl::Maximize => "Maximize",
        WindowControl::Restore => "Restore",
        WindowControl::Close => "Close",
    })
}

// A permanent gutter keeps the layout stable when a floating scrollbar appears.
fn scroll_content(ui: &mut egui::Ui, gutter: f32, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .inner_margin(egui::Margin {
            right: gutter,
            ..Default::default()
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            contents(ui);
        });
}

fn folder_button(ui: &mut egui::Ui, palette: Palette, open: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(28.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            if open { "Open folder" } else { "Choose folder" },
        )
    });
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 4.0, palette.surface);
    }
    let r = egui::Rect::from_center_size(rect.center(), Vec2::new(16.0, 12.0));
    let stroke = egui::Stroke::new(
        1.4_f32,
        if response.hovered() || response.has_focus() {
            palette.accent
        } else {
            palette.muted
        },
    );
    let points = vec![
        r.left_bottom(),
        r.left_top(),
        r.left_top() + Vec2::new(6.0, 0.0),
        r.left_top() + Vec2::new(8.0, 3.0),
        r.right_top() + Vec2::new(0.0, 3.0),
        r.right_bottom(),
    ];
    if open {
        ui.painter()
            .line_segment([r.left_bottom(), r.right_top()], stroke);
        ui.painter()
            .line_segment([r.right_top() - Vec2::new(7.0, 0.0), r.right_top()], stroke);
        ui.painter()
            .line_segment([r.right_top(), r.right_top() + Vec2::new(0.0, 7.0)], stroke);
    } else {
        ui.painter().add(egui::Shape::closed_line(points, stroke));
    }
    response.on_hover_text(if open { "Open folder" } else { "Choose folder" })
}

fn setting_checkbox(ui: &mut egui::Ui, value: &mut bool, label: &str) -> egui::Response {
    ui.scope(|ui| {
        let border = ui.visuals().widgets.noninteractive.bg_stroke;
        let accent = ui.visuals().selection.stroke;
        ui.visuals_mut().widgets.inactive.bg_stroke = border;
        ui.visuals_mut().widgets.hovered.bg_stroke = accent;
        ui.visuals_mut().widgets.active.bg_stroke = accent;
        ui.visuals_mut().widgets.inactive.fg_stroke = accent;
        ui.add(egui::Checkbox::new(value, label))
    })
    .inner
}

fn transport_button(
    ui: &mut egui::Ui,
    label: &str,
    width: f32,
    enabled: bool,
    palette: Option<Palette>,
    icon: u8,
) -> egui::Response {
    let response = action(ui, label, width, enabled, palette);
    let center = egui::pos2(response.rect.left() + 23.0, response.rect.center().y);
    let color = if !enabled {
        ui.visuals().weak_text_color()
    } else {
        palette.map_or(ui.visuals().text_color(), |p| p.accent_text)
    };
    let painter = ui.painter();
    match icon {
        0 => {
            painter.circle_filled(center, 5.0, color);
        }
        1 => {
            painter.rect_filled(
                egui::Rect::from_center_size(center, Vec2::splat(10.0)),
                1.0,
                color,
            );
        }
        2 => {
            for x in [-3.0, 3.0] {
                painter.rect_filled(
                    egui::Rect::from_center_size(center + Vec2::new(x, 0.0), Vec2::new(3.0, 12.0)),
                    0.0,
                    color,
                );
            }
        }
        _ => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + Vec2::new(-4.0, -6.0),
                    center + Vec2::new(6.0, 0.0),
                    center + Vec2::new(-4.0, 6.0),
                ],
                color,
                egui::Stroke::NONE,
            ));
        }
    }
    response
}

fn choose_output_folder() -> Result<Option<String>, String> {
    use windows::Win32::{System::Com::*, UI::Shell::*};
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|e| e.to_string())?;
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe {
                    CoUninitialize();
                }
            }
        }
        let _apartment = Apartment;
        let result = (|| -> windows::core::Result<Option<String>> {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            dialog.SetOptions(
                dialog.GetOptions()? | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_NOCHANGEDIR,
            )?;
            if let Err(error) = dialog.Show(None) {
                if error.code().0 as u32 == 0x800704c7 {
                    return Ok(None);
                }
                return Err(error);
            }
            let path = dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
            let text = path.to_string();
            CoTaskMemFree(Some(path.0.cast()));
            Ok(Some(text?))
        })();
        result.map_err(|e| format!("Could not choose folder: {e}"))
    }
}
