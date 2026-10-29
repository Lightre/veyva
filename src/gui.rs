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
    style.interaction.selectable_labels = false;
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
    let window_path = Settings::default_path()?.with_file_name("window.ron");
    let first_launch = !window_path.exists();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Veyva")
            .with_decorations(false)
            .with_app_id("Veyva")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([800.0, 400.0])
            .with_icon(icon),
        centered: first_launch,
        persistence_path: Some(window_path),
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
    audio_preferences: crate::audio_preferences::AudioPreferences,
    applied_audio: crate::audio_preferences::AudioPreferences,
    audio_devices: Vec<crate::audio_preferences::AudioDevice>,
    device_scan: Option<
        std::sync::mpsc::Receiver<Result<Vec<crate::audio_preferences::AudioDevice>, String>>,
    >,
    audio_load_error: Option<String>,
    background: crate::background::BackgroundPreferences,
    tray: Option<crate::background::Tray>,
    hidden: bool,
    folder_picker: Option<std::sync::mpsc::Receiver<Result<Option<String>, String>>>,
    page: Page,
    logo: egui::TextureHandle,
    settings: Settings,
    applied: Settings,
    pending_apply: Option<bool>,
    auto_candidate: Option<(
        Settings,
        crate::audio_preferences::AudioPreferences,
        std::time::Instant,
    )>,
    auto_failed: bool,
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
    hotkeys: Vec<(u8, crate::hotkey::Hotkey)>,
    listening_shortcut: Option<u8>,
    restore_shortcuts: bool,
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
        let audio_result = crate::audio_preferences::AudioPreferences::default_path()
            .and_then(|path| crate::audio_preferences::AudioPreferences::load(&path));
        let (audio_preferences, audio_load_error) = match audio_result {
            Ok(value) => (value, None),
            Err(error) => (Default::default(), Some(error)),
        };
        let background_result = crate::background::BackgroundPreferences::load();
        let (background, background_error) = match background_result {
            Ok(value) => (value, None),
            Err(error) => (Default::default(), Some(error)),
        };
        let ctx = cc.egui_ctx.clone();
        let tray_result =
            crate::background::Tray::new(std::sync::Arc::new(move || ctx.request_repaint()));
        let (tray, tray_error) = match tray_result {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(error)),
        };
        Self {
            applied_audio: audio_preferences.clone(),
            audio_preferences,
            audio_devices: Vec::new(),
            device_scan: None,
            audio_load_error,
            background,
            tray,
            hidden: false,
            folder_picker: None,
            page: Page::Capture,
            logo,
            settings,
            applied: settings,
            pending_apply: None,
            auto_candidate: None,
            auto_failed: false,
            pending_command: None,
            pending_saved: None,
            pending_error: None,
            settings_path,
            load_error,
            error: background_error.or(tray_error),
            notice: None,
            output,
            session: None,
            closing: false,
            context: cc.egui_ctx.clone(),
            hotkeys: Vec::new(),
            listening_shortcut: None,
            restore_shortcuts: false,
            shortcut_error: None,
            pending_auto_start: true,
        }
    }

    fn close_window(&mut self, ctx: &egui::Context) {
        if self.background.close_to_tray
            && self.tray.as_ref().is_some_and(|tray| tray.is_available())
            && !self.closing
        {
            self.hidden = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        } else if self.active() {
            self.closing = true;
            self.send(Command::Stop);
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn audio_controls(&mut self, ui: &mut egui::Ui) {
        let levels = if self.active() {
            self.session
                .as_ref()
                .and_then(|session| session.status.lock().ok())
                .filter(|state| state.audio_active)
                .map_or([0.0; 2], |state| state.audio_levels)
        } else {
            [0.0; 2]
        };
        let palette = Palette::for_theme(self.settings.theme);
        ui.add_enabled_ui(
            self.pending_apply.is_none() && !self.closing && self.audio_load_error.is_none(),
            |ui| {
                audio_device_choice(
                    ui,
                    "System output",
                    false,
                    &mut self.audio_preferences.output_device,
                    &self.audio_devices,
                );
                audio_level_meter(ui, levels[0], self.applied.audio_enabled, palette);
                setting_checkbox(
                    ui,
                    &mut self.audio_preferences.microphone_enabled,
                    "Record microphone",
                );
                ui.add_enabled_ui(self.audio_preferences.microphone_enabled, |ui| {
                    audio_device_choice(
                        ui,
                        "Microphone input",
                        true,
                        &mut self.audio_preferences.input_device,
                        &self.audio_devices,
                    );
                    audio_level_meter(
                        ui,
                        levels[1],
                        self.applied_audio.microphone_enabled,
                        palette,
                    );
                });
            },
        );
        if let Some(error) = &self.audio_load_error {
            banner(ui, error, CORAL);
        }
    }

    fn refresh_audio_devices(&mut self, ui: &mut egui::Ui, palette: Palette) {
        let scanning = self.device_scan.is_some();
        let response = ui
            .add_enabled_ui(!scanning, |ui| refresh_icon(ui, palette))
            .inner;
        if response
            .on_hover_text(if scanning {
                "Scanning audio devices..."
            } else {
                "Refresh audio devices"
            })
            .clicked()
        {
            let (sender, receiver) = std::sync::mpsc::channel();
            let ctx = self.context.clone();
            match std::thread::Builder::new()
                .name("veyva-device-scan".into())
                .spawn(move || {
                    let _ = sender.send(crate::audio_preferences::devices());
                    ctx.request_repaint();
                }) {
                Ok(_) => self.device_scan = Some(receiver),
                Err(error) => self.error = Some(error.to_string()),
            }
        }
    }

    fn background_controls(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.label(RichText::new("05 / BACKGROUND & STARTUP").small().strong());
            let mut candidate = self.background;
            let close_changed = setting_checkbox(
                ui,
                &mut candidate.close_to_tray,
                "Keep running in the notification area when closed",
            )
            .changed();
            let startup_changed = setting_checkbox(
                ui,
                &mut candidate.start_with_windows,
                "Start replay in the background when I sign in",
            )
            .changed();
            ui.label(RichText::new("Both options are off by default. Use the Veyva tray menu to open the window, save a replay or quit completely. Background capture uses your selected quality and history budget.").small().color(ui.visuals().weak_text_color()));
            if close_changed || startup_changed {
                if !self.tray.as_ref().is_some_and(|tray| tray.is_available()) {
                    self.error = Some("The notification icon is unavailable; background mode cannot be enabled safely.".into());
                } else {
                    match candidate.save() {
                        Ok(()) => {
                            self.background = candidate;
                            self.notice = Some("Background preferences saved.".into());
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
            }
        });
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
        if let Some(error) = &self.audio_load_error {
            self.error = Some(error.clone());
            return;
        }
        if let Err(error) = self.persist().and_then(|_| {
            crate::audio_preferences::AudioPreferences::default_path()
                .and_then(|path| self.audio_preferences.save(&path))
        }) {
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
            self.applied_audio = self.audio_preferences.clone();
            self.bind_hotkey();
            self.notice = Some("Preferences applied. New captures use these settings.".into());
        }
    }

    fn preferences_changed(&self) -> bool {
        self.settings != self.applied || self.audio_preferences != self.applied_audio
    }

    fn auto_apply_preferences(&mut self, ctx: &egui::Context) {
        if !self.preferences_changed() {
            self.auto_candidate = None;
            self.auto_failed = false;
            return;
        }
        if self.closing || self.pending_apply.is_some() || self.listening_shortcut.is_some() {
            return;
        }
        let changed = self
            .auto_candidate
            .as_ref()
            .is_none_or(|(settings, audio, _)| {
                *settings != self.settings || *audio != self.audio_preferences
            });
        if changed {
            self.auto_candidate = Some((
                self.settings,
                self.audio_preferences.clone(),
                std::time::Instant::now(),
            ));
            self.auto_failed = false;
        }
        if self.auto_failed {
            return;
        }
        let elapsed = self.auto_candidate.as_ref().unwrap().2.elapsed();
        if elapsed < Duration::from_millis(500) {
            ctx.request_repaint_after(Duration::from_millis(500) - elapsed);
            return;
        }
        if ctx.wants_keyboard_input() || ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        let busy = self
            .session
            .as_ref()
            .and_then(|s| s.status.lock().ok())
            .is_some_and(|s| {
                s.recording
                    || s.finalizing
                    || s.saving
                    || s.phase == "Starting"
                    || s.phase.starts_with("Stopping")
            });
        if busy || self.pending_command.is_some() {
            return;
        }
        self.apply_settings();
        self.auto_failed = self.error.is_some();
    }

    fn apply_controls(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        let text = if self.pending_apply.is_some() {
            "Updating capture. Replay history is restarting..."
        } else if self.auto_failed {
            "Preferences could not be saved. Correct the value or retry."
        } else if self.preferences_changed() {
            "Changes save automatically when editing finishes. Active recordings/exports finish first; current microphone capture is unchanged until then. Replay history resets."
        } else {
            "Preferences saved automatically."
        };
        ui.label(
            RichText::new(text)
                .small()
                .color(ui.visuals().weak_text_color()),
        );
        if self.auto_failed && ui.small_button("Retry saving").clicked() {
            self.auto_candidate = None;
            self.auto_failed = false;
            self.error = None;
        }
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
        self.hotkeys.clear();
        self.shortcut_error = None;
        for (kind, modifiers, key) in [
            (
                0,
                self.applied.record_hotkey_modifiers,
                self.applied.record_hotkey_key,
            ),
            (
                1,
                self.applied.pause_hotkey_modifiers,
                self.applied.pause_hotkey_key,
            ),
            (2, self.applied.hotkey_modifiers, self.applied.hotkey_key),
        ] {
            if key == 0 {
                continue;
            }
            let ctx = self.context.clone();
            match crate::hotkey::Hotkey::register(
                modifiers,
                key,
                std::sync::Arc::new(move || {
                    ctx.request_repaint();
                    crate::background::wake_desktop();
                }),
            ) {
                Ok(hotkey) => self.hotkeys.push((kind, hotkey)),
                Err(error) => {
                    let errors = self.shortcut_error.get_or_insert_with(String::new);
                    if !errors.is_empty() {
                        errors.push_str("; ");
                    }
                    errors.push_str(&error);
                }
            }
        }
    }

    fn shortcuts(&mut self, ui: &mut egui::Ui) {
        let palette = Palette::for_theme(self.settings.theme);
        card(ui, |ui| {
            ui.label(
                RichText::new("04 / KEYBOARD SHORTCUTS")
                    .small()
                    .strong()
                    .color(palette.muted),
            );
            ui.label("Click a binding, then press your key combination. Escape cancels.");
            ui.label(RichText::new("Use Ctrl, Alt, Shift or Win with a letter, number, function or navigation key.").small().color(palette.muted));
            for (kind, title, hint, modifiers, key) in [
                (
                    0,
                    "Start / finish recording",
                    "Finish saves the current recording.",
                    self.settings.record_hotkey_modifiers,
                    self.settings.record_hotkey_key,
                ),
                (
                    1,
                    "Pause / resume recording",
                    "Available while recording.",
                    self.settings.pause_hotkey_modifiers,
                    self.settings.pause_hotkey_key,
                ),
                (
                    2,
                    "Save replay",
                    "Save the buffered moments.",
                    self.settings.hotkey_modifiers,
                    self.settings.hotkey_key,
                ),
            ] {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(title).strong());
                        ui.label(RichText::new(hint).small().color(palette.muted));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if self.listening_shortcut == Some(kind) {
                            "Press shortcut...".into()
                        } else {
                            crate::hotkey::label(modifiers, key)
                        };
                        if ui
                            .add_sized([160.0, 36.0], egui::Button::new(label))
                            .clicked()
                        {
                            self.hotkeys.clear();
                            self.listening_shortcut = Some(kind);
                            self.restore_shortcuts = false;
                        }
                    });
                });
            }
            ui.add_space(12.0);
            ui.label(
                RichText::new(
                    "Shortcuts save automatically. Delete clears a binding; Escape cancels.",
                )
                .small()
                .color(palette.muted),
            );
            if let Some(error) = &self.shortcut_error {
                banner(ui, error, CORAL);
            }
        });
    }

    fn listen_for_shortcut(&mut self, ctx: &egui::Context) {
        if let Some(kind) = self.listening_shortcut {
            let events = ctx.input(|i| i.events.clone());
            ctx.input_mut(|i| {
                i.events.retain(|event| {
                    !matches!(event, egui::Event::Key { .. } | egui::Event::Text(_))
                })
            });
            if !ctx.input(|i| i.focused) {
                self.listening_shortcut = None;
                self.restore_shortcuts = true;
            } else {
                for event in events {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        repeat: false,
                        modifiers,
                        ..
                    } = event
                    {
                        if key == egui::Key::Escape {
                            self.listening_shortcut = None;
                            self.restore_shortcuts = true;
                            break;
                        }
                        let mut mods = u32::from(modifiers.alt)
                            | (u32::from(modifiers.ctrl) << 1)
                            | (u32::from(modifiers.shift) << 2)
                            | (u32::from(unsafe {
                                use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
                                GetAsyncKeyState(0x5b) < 0 || GetAsyncKeyState(0x5c) < 0
                            }) << 3);
                        let captured = if key == egui::Key::Delete {
                            mods = 0;
                            Some(0)
                        } else {
                            shortcut_key(key)
                        };
                        if let Some(vk) = captured {
                            let mut candidate = self.settings;
                            match kind {
                                0 => {
                                    candidate.record_hotkey_modifiers = mods;
                                    candidate.record_hotkey_key = vk;
                                }
                                1 => {
                                    candidate.pause_hotkey_modifiers = mods;
                                    candidate.pause_hotkey_key = vk;
                                }
                                _ => {
                                    candidate.hotkey_modifiers = mods;
                                    candidate.hotkey_key = vk;
                                }
                            }
                            let result = (|| -> Result<(), String> {
                                if self.load_error.is_some() {
                                    return Err("Reset unreadable settings first".into());
                                }
                                // Validate only the shortcut update, preserving unrelated drafts.
                                let path = self.settings_path.as_ref().map_err(Clone::clone)?;
                                let mut saved = Settings::load(path)?;
                                copy_shortcuts(&mut saved, &candidate);
                                saved.save(path)
                            })();
                            match result {
                                Ok(()) => {
                                    copy_shortcuts(&mut self.settings, &candidate);
                                    copy_shortcuts(&mut self.applied, &candidate);
                                    self.error = None;
                                    self.listening_shortcut = None;
                                    self.restore_shortcuts = true;
                                    self.notice = Some("Shortcut saved.".into());
                                }
                                Err(error) => self.error = Some(error),
                            }
                        } else {
                            self.error =
                                Some("This key is not supported for global shortcuts.".into());
                        }
                        break;
                    }
                }
            }
        }
        if self.restore_shortcuts
            && ctx.input(|i| i.keys_down.is_empty() && i.modifiers == egui::Modifiers::NONE)
        {
            self.restore_shortcuts = false;
            self.bind_hotkey();
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
        let local = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        Ok(folder.join(format!(
            "Veyva-{kind}-{:04}-{:02}-{:02}_{:02}-{:02}-{:02}-{:03}-{stamp}-{}.mp4",
            local.wYear,
            local.wMonth,
            local.wDay,
            local.wHour,
            local.wMinute,
            local.wSecond,
            local.wMilliseconds,
            std::process::id()
        )))
    }
    fn save_replay(&mut self) {
        if self.preferences_changed() {
            self.error =
                Some("Wait for capture preferences to update before saving replay.".into());
            return;
        }
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
        if self.preferences_changed() || self.pending_apply.is_some() {
            self.error =
                Some("Wait for capture preferences to finish updating before recording.".into());
            return;
        }
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
                            self.close_window(ctx);
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
        if self.auto_failed {
            self.apply_controls(ui);
        }
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Audio").size(18.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.refresh_audio_devices(ui, p)
            });
        });
        let label = if state.audio_active {
            "Audio capture active"
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
        self.audio_controls(ui);
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
        let stopping = self.preferences_changed()
            || state.phase.starts_with("Stopping")
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
            ui.horizontal(|ui| {
                let galley = ui.painter().layout_no_wrap(
                    label.to_owned(),
                    egui::FontId::proportional(14.0),
                    p.muted,
                );
                let width = galley.size().x + 20.0;
                ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                let (rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 16.0), egui::Sense::hover());
                ui.painter().circle_filled(
                    rect.center(),
                    3.5,
                    if state.recording { p.accent } else { p.muted },
                );
                ui.label(label);
            });
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
                crate::hotkey::label(self.applied.hotkey_modifiers, self.applied.hotkey_key),
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
                &format!("Audio capture unavailable: {error}. Files may contain video only."),
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
            RichText::new("Changes save automatically after editing. Replay restarts when capture settings change.")
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
                    field(ui,"History budget (MiB)",&mut self.settings.profile.replay_budget_mib,16..=512,16);
                    field(ui,"Monitor index (starts at 0)",&mut self.settings.monitor,0..=32,1);
                });
            });
            ui.add_space(8.0);
            ui.label(RichText::new("The history budget caps encoded payload, not total process memory. A smaller budget may retain fewer seconds. Audio devices and microphone capture are configured in the Capture inspector.").small().color(palette.muted));
            card(ui, |ui| {
                ui.label(RichText::new("03 / AUDIO & BEHAVIOR").small().strong().color(palette.muted));
                setting_checkbox(ui, &mut self.settings.capture_cursor,"Capture cursor");
                setting_checkbox(ui, &mut self.settings.replay_auto_start,"Arm replay when Veyva opens");
                setting_checkbox(ui, &mut self.settings.audio_enabled,"Record system audio");

            });

        });
        ui.add_space(12.0);
        self.shortcuts(ui);
        ui.add_space(12.0);
        self.background_controls(ui);
        self.apply_controls(ui);
    }
}

impl eframe::App for App {
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(receiver) = &self.device_scan {
            match receiver.try_recv() {
                Ok(result) => {
                    self.device_scan = None;
                    match result {
                        Ok(devices) => self.audio_devices = devices,
                        Err(error) => self.error = Some(error),
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.device_scan = None;
                    self.error = Some("Audio device scan stopped unexpectedly".into());
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.hidden && !self.tray.as_ref().is_some_and(|tray| tray.is_available()) {
            self.hidden = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            self.error = Some("Notification icon unavailable; the window was restored.".into());
        }
        let events = self
            .tray
            .as_ref()
            .map(|tray| tray.take_events())
            .unwrap_or_default();
        for event in events {
            match event {
                crate::background::TrayEvent::Show => {
                    self.hidden = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                crate::background::TrayEvent::SaveReplay => self.save_replay(),
                crate::background::TrayEvent::Quit => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    self.hidden = false;
                    self.closing = true;
                    self.send(Command::Stop);
                }
            }
        }
        if self.pending_auto_start
            && std::env::args().any(|arg| arg == "--background")
            && self.tray.as_ref().is_some_and(|tray| tray.is_available())
        {
            self.hidden = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

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
            self.applied_audio = self.audio_preferences.clone();
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
            crate::background::fit_desktop_to_work_area();
            self.pending_auto_start = false;
            if self.load_error.is_none() {
                self.bind_hotkey();
                if self.settings.replay_auto_start
                    || std::env::args().any(|arg| arg == "--background")
                {
                    self.start();
                    if std::env::args().any(|arg| arg == "--background") && self.active() {
                        self.send(Command::SetReplayEnabled(true));
                    }
                }
            }
        }
        self.listen_for_shortcut(ctx);
        let pressed: Vec<u8> = self
            .hotkeys
            .iter()
            .filter_map(|(kind, key)| key.take_pressed().then_some(*kind))
            .collect();
        if self.listening_shortcut.is_none()
            && !self.restore_shortcuts
            && !self.closing
            && self.pending_apply.is_none()
            && self.pending_command.is_none()
        {
            if let Some(kind) = pressed.first() {
                let state = self
                    .session
                    .as_ref()
                    .and_then(|s| s.status.lock().ok())
                    .map(|s| s.clone())
                    .unwrap_or_default();
                if !state.finalizing
                    && state.phase != "Starting"
                    && !state.phase.starts_with("Stopping")
                {
                    match kind {
                        0 => {
                            if state.recording {
                                self.send(Command::FinishRecording);
                            } else {
                                self.start_recording();
                            }
                        }
                        1 if state.recording => self.send(if state.paused {
                            Command::ResumeRecording
                        } else {
                            Command::PauseRecording
                        }),
                        2 => self.save_replay(),
                        _ => {}
                    }
                }
            }
        }
        // Custom titlebar close follows close-to-tray; native close requests
        // (Alt+F4 or taskbar Close) explicitly exit, finalizing asynchronously.
        if ctx.input(|i| i.viewport().close_requested()) && self.active() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.closing {
                self.closing = true;
                self.hidden = false;
                self.send(Command::Stop);
            }
        }
        if self.closing && !self.active() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.active() || self.closing || self.pending_apply.is_some() {
            ctx.request_repaint_after(Duration::from_millis(if self.hidden { 1000 } else { 250 }));
        }
        if self.hidden
            && self
                .session
                .as_ref()
                .and_then(|session| session.status.lock().ok())
                .is_some_and(|state| state.error.is_some())
        {
            self.hidden = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }
        if self.hidden {
            self.auto_apply_preferences(ctx);
            return;
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
                            ui.label("System audio, microphone capture, configurable shortcuts and background tray controls are supported.");
                            ui.label(RichText::new("No account. No telemetry. No cloud upload.").color(palette.accent));
                        });
                    }
                }
                if let Some(e) = &self.error { banner(ui,e,CORAL); }
                if let Some(n) = &self.notice { banner(ui,n,palette.accent); }
                });
            });
        });
        self.auto_apply_preferences(ctx);
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

fn copy_shortcuts(target: &mut Settings, source: &Settings) {
    target.hotkey_modifiers = source.hotkey_modifiers;
    target.hotkey_key = source.hotkey_key;
    target.record_hotkey_modifiers = source.record_hotkey_modifiers;
    target.record_hotkey_key = source.record_hotkey_key;
    target.pause_hotkey_modifiers = source.pause_hotkey_modifiers;
    target.pause_hotkey_key = source.pause_hotkey_key;
}

fn shortcut_key(key: egui::Key) -> Option<u32> {
    let name = key.name();
    if name.len() == 1 {
        let byte = name.as_bytes()[0];
        if byte.is_ascii_uppercase() || byte.is_ascii_digit() {
            return Some(u32::from(byte));
        }
    }
    if let Some(number) = name.strip_prefix('F').and_then(|s| s.parse::<u32>().ok()) {
        if (1..=24).contains(&number) {
            return Some(0x6f + number);
        }
    }
    Some(match key {
        egui::Key::Space => 0x20,
        egui::Key::Enter => 0x0d,
        egui::Key::Tab => 0x09,
        egui::Key::ArrowLeft => 0x25,
        egui::Key::ArrowUp => 0x26,
        egui::Key::ArrowRight => 0x27,
        egui::Key::ArrowDown => 0x28,
        egui::Key::Insert => 0x2d,
        egui::Key::Delete => 0x2e,
        egui::Key::Home => 0x24,
        egui::Key::End => 0x23,
        egui::Key::PageUp => 0x21,
        egui::Key::PageDown => 0x22,
        _ => return None,
    })
}

fn audio_device_choice(
    ui: &mut egui::Ui,
    label: &str,
    input: bool,
    selected: &mut Option<String>,
    devices: &[crate::audio_preferences::AudioDevice],
) {
    ui.label(label);
    let text = selected
        .as_ref()
        .map(|id| {
            devices
                .iter()
                .find(|device| &device.id == id)
                .map(|device| device.name.as_str())
                .unwrap_or("Saved device (refresh to resolve)")
        })
        .unwrap_or("Windows default");
    egui::ComboBox::from_id_salt(label)
        .width((ui.available_width() - 12.0).max(100.0))
        .selected_text(text)
        .show_ui(ui, |ui| {
            ui.selectable_value(selected, None, "Windows default");
            for device in devices.iter().filter(|device| device.is_input == input) {
                ui.selectable_value(selected, Some(device.id.clone()), &device.name);
            }
        });
}

fn refresh_icon(ui: &mut egui::Ui, palette: Palette) -> egui::Response {
    let response = ui.add_sized([28.0, 28.0], egui::Button::new("").frame(false));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            "Refresh audio devices",
        )
    });
    if response.hovered() || response.has_focus() {
        ui.painter()
            .rect_filled(response.rect, 5.0, palette.surface);
    }
    let color = if !ui.is_enabled() {
        palette.muted.gamma_multiply(0.5)
    } else if response.hovered() || response.has_focus() {
        palette.accent
    } else {
        palette.muted
    };
    let center = response.rect.center();
    let radius = 6.0;
    let points: Vec<_> = (0..=24)
        .map(|step| {
            let angle =
                std::f32::consts::FRAC_PI_2 + step as f32 / 24.0 * std::f32::consts::PI * 1.65;
            center + Vec2::angled(angle) * radius
        })
        .collect();
    let tip = *points.last().unwrap();
    let tangent = (tip - points[points.len() - 2]).normalized();
    let normal = Vec2::new(-tangent.y, tangent.x);
    ui.painter()
        .add(egui::Shape::line(points, egui::Stroke::new(1.5_f32, color)));
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            tip + tangent * 2.0,
            tip - tangent * 3.0 + normal * 3.0,
            tip - tangent * 3.0 - normal * 3.0,
        ],
        color,
        egui::Stroke::NONE,
    ));
    response
}

fn audio_level_meter(ui: &mut egui::Ui, amplitude: f32, enabled: bool, palette: Palette) {
    let amplitude = if enabled {
        amplitude.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let normalized = if amplitude > 0.001 {
        ((20.0 * amplitude.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 4.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, palette.surface);
    if normalized > 0.0 {
        let fill = egui::Rect::from_min_size(
            rect.min,
            Vec2::new(rect.width() * normalized, rect.height()),
        );
        ui.painter().rect_filled(
            fill,
            2.0,
            if amplitude >= 0.95 {
                CORAL
            } else {
                palette.accent
            },
        );
    }
    let label = if !enabled {
        "Audio source disabled".into()
    } else if amplitude <= 0.001 {
        "No captured signal".into()
    } else {
        format!("Captured peak: {:.1} dBFS", 20.0 * amplitude.log10())
    };
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::ProgressIndicator, ui.is_enabled(), &label)
    });
    response.on_hover_text(label);
}
