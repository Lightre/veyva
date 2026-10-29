use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Initial product defaults. These are targets, not measured performance results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureProfile {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub video_kbps: u32,
    pub audio_kbps: u32,
    pub history_seconds: u32,
    pub replay_budget_mib: u32,
}

impl CaptureProfile {
    pub const fn low_end() -> Self {
        Self {
            width: 1280,
            height: 720,
            fps: 30,
            video_kbps: 3000,
            audio_kbps: 128,
            history_seconds: 30,
            replay_budget_mib: 32,
        }
    }

    pub const fn balanced() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 30,
            video_kbps: 6000,
            audio_kbps: 128,
            history_seconds: 60,
            replay_budget_mib: 96,
        }
    }

    pub const fn high() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 60,
            video_kbps: 12000,
            audio_kbps: 128,
            history_seconds: 60,
            replay_budget_mib: 192,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(320..=7680).contains(&self.width) || !self.width.is_multiple_of(2) {
            return Err("width must be even and between 320 and 7680".into());
        }
        if !(180..=4320).contains(&self.height) || !self.height.is_multiple_of(2) {
            return Err("height must be even and between 180 and 4320".into());
        }
        if !(1..=240).contains(&self.fps) {
            return Err("fps must be between 1 and 240".into());
        }
        if !(250..=100_000).contains(&self.video_kbps) {
            return Err("video_kbps must be between 250 and 100000".into());
        }
        if self.audio_kbps != 128 {
            return Err("audio_kbps is fixed at 128 for the current system-audio encoder".into());
        }
        if !(5..=600).contains(&self.history_seconds) {
            return Err("history_seconds must be between 5 and 600".into());
        }
        if !(16..=512).contains(&self.replay_budget_mib) {
            return Err("replay_budget_mib must be between 16 and 512".into());
        }
        Ok(())
    }

    /// Nominal compressed A/V payload, excluding allocator, GPU and muxer overhead.
    pub fn payload_mib(self) -> f64 {
        (f64::from(self.video_kbps) + f64::from(self.audio_kbps))
            * 1000.0
            * f64::from(self.history_seconds)
            / 8.0
            / 1_048_576.0
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

impl Theme {
    fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub profile: CaptureProfile,
    pub theme: Theme,
    pub monitor: u32,
    pub replay_auto_start: bool,
    pub audio_enabled: bool,
    pub capture_cursor: bool,
    pub hotkey_modifiers: u32,
    pub hotkey_key: u32,
    pub record_hotkey_modifiers: u32,
    pub record_hotkey_key: u32,
    pub pause_hotkey_modifiers: u32,
    pub pause_hotkey_key: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            profile: CaptureProfile::low_end(),
            theme: Theme::default(),
            monitor: 0,
            replay_auto_start: true,
            audio_enabled: true,
            capture_cursor: true,
            hotkey_modifiers: 1,
            hotkey_key: 0x70,
            record_hotkey_modifiers: 6,
            record_hotkey_key: 0x52,
            pause_hotkey_modifiers: 6,
            pause_hotkey_key: 0x50,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        self.profile.validate()?;
        let shortcuts = [
            (self.hotkey_modifiers, self.hotkey_key),
            (self.record_hotkey_modifiers, self.record_hotkey_key),
            (self.pause_hotkey_modifiers, self.pause_hotkey_key),
        ];
        for (index, &(modifiers, key)) in shortcuts.iter().enumerate() {
            if !valid_shortcut(modifiers, key) {
                return Err("Shortcuts require a modifier and a supported key".into());
            }
            if shortcuts[..index].contains(&(modifiers, key)) {
                return Err("Shortcuts must use distinct key combinations".into());
            }
        }
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        match fs::read_to_string(path) {
            Ok(contents) => Self::from_text(&contents),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(format!("could not read settings: {error}")),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|e| format!("could not create settings directory: {e}"))?;

        static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);
        let file_name = path
            .file_name()
            .ok_or_else(|| "settings path has no file name".to_string())?;
        let (mut file, selected_path) = loop {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let mut name = file_name.to_os_string();
            name.push(format!(".{}.{}.tmp", std::process::id(), id));
            let temp_path = parent.join(name);
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)
            {
                Ok(file) => break (file, temp_path.clone()),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!("could not create temporary settings file: {error}"))
                }
            }
        };

        let result = (|| {
            file.write_all(self.to_text().as_bytes())
                .map_err(|e| format!("could not write settings: {e}"))?;
            file.sync_all()
                .map_err(|e| format!("could not sync settings: {e}"))?;
            drop(file);
            fs::rename(&selected_path, path)
                .map_err(|e| format!("could not replace settings file: {e}"))
        })();
        if result.is_err() {
            let _ = fs::remove_file(selected_path);
        }
        result
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let mut candidate = *self;
        if key == "theme" {
            candidate.theme = match value {
                "dark" => Theme::Dark,
                "light" => Theme::Light,
                _ => return Err("theme must be dark or light".into()),
            };
            *self = candidate;
            return Ok(());
        }
        let parsed = value
            .parse::<u32>()
            .map_err(|_| format!("{key} must be an unsigned integer"))?;
        match key {
            "width" => candidate.profile.width = parsed,
            "height" => candidate.profile.height = parsed,
            "fps" => candidate.profile.fps = parsed,
            "video_kbps" => candidate.profile.video_kbps = parsed,
            "history_seconds" => candidate.profile.history_seconds = parsed,
            "replay_budget_mib" => candidate.profile.replay_budget_mib = parsed,
            "monitor" => candidate.monitor = parsed,
            "replay_auto_start" if parsed <= 1 => candidate.replay_auto_start = parsed == 1,
            "audio_enabled" if parsed <= 1 => candidate.audio_enabled = parsed == 1,
            "capture_cursor" if parsed <= 1 => candidate.capture_cursor = parsed == 1,
            "hotkey_modifiers" => candidate.hotkey_modifiers = parsed,
            "hotkey_key" => candidate.hotkey_key = parsed,
            "record_hotkey_modifiers" => candidate.record_hotkey_modifiers = parsed,
            "record_hotkey_key" => candidate.record_hotkey_key = parsed,
            "pause_hotkey_modifiers" => candidate.pause_hotkey_modifiers = parsed,
            "pause_hotkey_key" => candidate.pause_hotkey_key = parsed,
            "audio_kbps" => {
                return Err("The current system-audio encoder uses a fixed 128 kbps bitrate".into())
            }
            _ => return Err(format!("unknown setting: {key}")),
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn to_text(self) -> String {
        format!(
            "version=2\nwidth={}\nheight={}\nfps={}\nvideo_kbps={}\naudio_kbps={}\nhistory_seconds={}\nreplay_budget_mib={}\nmonitor={}\nreplay_auto_start={}\naudio_enabled={}\ncapture_cursor={}\nhotkey_modifiers={}\nhotkey_key={}\ntheme={}\n",
            self.profile.width,
            self.profile.height,
            self.profile.fps,
            self.profile.video_kbps,
            self.profile.audio_kbps,
            self.profile.history_seconds,
            self.profile.replay_budget_mib,
            self.monitor,
            u32::from(self.replay_auto_start),
            u32::from(self.audio_enabled),
            u32::from(self.capture_cursor),
            self.hotkey_modifiers,
            self.hotkey_key,
            self.theme.as_str(),
        ) + &format!(
            "record_hotkey_modifiers={}\nrecord_hotkey_key={}\npause_hotkey_modifiers={}\npause_hotkey_key={}\n",
            self.record_hotkey_modifiers, self.record_hotkey_key,
            self.pause_hotkey_modifiers, self.pause_hotkey_key,
        )
    }

    fn from_text(contents: &str) -> Result<Self, String> {
        let mut version = None;
        let mut width = None;
        let mut height = None;
        let mut fps = None;
        let mut video_kbps = None;
        let mut audio_kbps = None;
        let mut history_seconds = None;
        let mut replay_budget_mib = None;
        let mut monitor = None;
        let mut replay_auto_start = None;
        let mut audio_enabled = None;
        let mut capture_cursor = None;
        let mut hotkey_modifiers = None;
        let mut hotkey_key = None;
        let mut record_hotkey_modifiers = None;
        let mut record_hotkey_key = None;
        let mut pause_hotkey_modifiers = None;
        let mut pause_hotkey_key = None;
        let mut theme = None;

        for (index, line) in contents.lines().enumerate() {
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!("malformed settings line {}", index + 1))?;
            if key.is_empty() || value.is_empty() || value.contains('=') {
                return Err(format!("malformed settings line {}", index + 1));
            }
            let slot = match key {
                "version" => &mut version,
                "width" => &mut width,
                "height" => &mut height,
                "fps" => &mut fps,
                "video_kbps" => &mut video_kbps,
                "audio_kbps" => &mut audio_kbps,
                "history_seconds" => &mut history_seconds,
                "replay_budget_mib" => &mut replay_budget_mib,
                "monitor" => &mut monitor,
                "replay_auto_start" => &mut replay_auto_start,
                "audio_enabled" => &mut audio_enabled,
                "capture_cursor" => &mut capture_cursor,
                "hotkey_modifiers" => &mut hotkey_modifiers,
                "hotkey_key" => &mut hotkey_key,
                "record_hotkey_modifiers" => &mut record_hotkey_modifiers,
                "record_hotkey_key" => &mut record_hotkey_key,
                "pause_hotkey_modifiers" => &mut pause_hotkey_modifiers,
                "pause_hotkey_key" => &mut pause_hotkey_key,
                "theme" => {
                    if theme.is_some() {
                        return Err("duplicate setting: theme".into());
                    }
                    theme = Some(match value {
                        "dark" => Theme::Dark,
                        "light" => Theme::Light,
                        _ => return Err("invalid theme: expected dark or light".into()),
                    });
                    continue;
                }
                _ => return Err(format!("unknown setting: {key}")),
            };
            if slot.is_some() {
                return Err(format!("duplicate setting: {key}"));
            }
            *slot = Some(
                value
                    .parse::<u32>()
                    .map_err(|_| format!("invalid integer for {key}"))?,
            );
        }

        match version {
            Some(1 | 2) => {}
            Some(_) => return Err("unsupported settings version".into()),
            None => return Err("missing settings version".into()),
        }
        let profile = CaptureProfile {
            width: required(width, "width")?,
            height: required(height, "height")?,
            fps: required(fps, "fps")?,
            video_kbps: required(video_kbps, "video_kbps")?,
            audio_kbps: required(audio_kbps, "audio_kbps")?,
            history_seconds: required(history_seconds, "history_seconds")?,
            replay_budget_mib: required(replay_budget_mib, "replay_budget_mib")?,
        };
        profile.validate()?;
        let legacy = version == Some(1);
        let auto = if legacy {
            replay_auto_start.unwrap_or(1)
        } else {
            required(replay_auto_start, "replay_auto_start")?
        };
        let audio = if legacy {
            audio_enabled.unwrap_or(1)
        } else {
            required(audio_enabled, "audio_enabled")?
        };
        let cursor = capture_cursor.unwrap_or(1);
        if auto > 1 || audio > 1 || cursor > 1 {
            return Err("Boolean settings must be 0 or 1".into());
        }
        let result = Self {
            record_hotkey_modifiers: record_hotkey_modifiers.unwrap_or(6),
            record_hotkey_key: record_hotkey_key.unwrap_or(0x52),
            pause_hotkey_modifiers: pause_hotkey_modifiers.unwrap_or(6),
            pause_hotkey_key: pause_hotkey_key.unwrap_or(0x50),
            profile,
            theme: theme.unwrap_or_default(),
            monitor: required(monitor, "monitor")?,
            replay_auto_start: auto == 1,
            audio_enabled: audio == 1,
            capture_cursor: cursor == 1,
            hotkey_modifiers: if legacy {
                hotkey_modifiers.unwrap_or(1)
            } else {
                required(hotkey_modifiers, "hotkey_modifiers")?
            },
            hotkey_key: if legacy {
                hotkey_key.unwrap_or(0x70)
            } else {
                required(hotkey_key, "hotkey_key")?
            },
        };
        result.validate()?;
        Ok(result)
    }

    pub fn default_path() -> Result<PathBuf, String> {
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .ok_or_else(|| "LOCALAPPDATA is not set".to_string())?;
        Ok(PathBuf::from(local_app_data)
            .join("Veyva")
            .join("settings.conf"))
    }
}

fn required(value: Option<u32>, key: &str) -> Result<u32, String> {
    value.ok_or_else(|| format!("missing setting: {key}"))
}

pub fn valid_shortcut(modifiers: u32, key: u32) -> bool {
    (1..=15).contains(&modifiers)
        && matches!(key, 0x30..=0x39 | 0x41..=0x5A | 0x70..=0x87
            | 0x09 | 0x0D | 0x20..=0x28 | 0x2D | 0x2E)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_shortcuts_round_trip_and_legacy_files_receive_defaults() {
        let settings = Settings {
            record_hotkey_modifiers: 3,
            record_hotkey_key: 0x39,
            pause_hotkey_modifiers: 5,
            pause_hotkey_key: 0x87,
            ..Default::default()
        };
        assert_eq!(Settings::from_text(&settings.to_text()).unwrap(), settings);
        let old = Settings::default()
            .to_text()
            .lines()
            .filter(|line| {
                !line.starts_with("record_hotkey_") && !line.starts_with("pause_hotkey_")
            })
            .collect::<Vec<_>>()
            .join("\n");
        for version in ["version=1", "version=2"] {
            assert_eq!(
                Settings::from_text(&old.replace("version=2", version)).unwrap(),
                Settings::default()
            );
        }
    }

    #[test]
    fn shortcuts_reject_duplicates_and_invalid_values() {
        let defaults = Settings::default();
        for (modifiers, key) in [(0, 0x41), (16, 0x41), (1, 0), (1, 0x88), (1, 0x40)] {
            assert!(!valid_shortcut(modifiers, key));
            let invalid = Settings {
                pause_hotkey_modifiers: modifiers,
                pause_hotkey_key: key,
                ..defaults
            };
            assert!(invalid.validate().is_err());
            assert!(Settings::from_text(&invalid.to_text()).is_err());
        }
        for key in [
            0x30, 0x39, 0x41, 0x5A, 0x70, 0x87, 0x09, 0x0D, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25,
            0x26, 0x27, 0x28, 0x2D, 0x2E,
        ] {
            assert!(valid_shortcut(15, key));
        }
        for (modifiers, key) in [
            (defaults.hotkey_modifiers, defaults.hotkey_key),
            (defaults.record_hotkey_modifiers, defaults.record_hotkey_key),
        ] {
            let duplicate = Settings {
                pause_hotkey_modifiers: modifiers,
                pause_hotkey_key: key,
                ..defaults
            };
            assert!(duplicate.validate().is_err());
            assert!(Settings::from_text(&duplicate.to_text()).is_err());
        }
        let duplicate = Settings {
            record_hotkey_modifiers: defaults.hotkey_modifiers,
            record_hotkey_key: defaults.hotkey_key,
            ..defaults
        };
        assert!(duplicate.validate().is_err());
        let mut settings = defaults;
        assert!(settings.set("pause_hotkey_key", "82").is_err());
        assert_eq!(settings, defaults);
        assert!(
            Settings::from_text(&format!("{}record_hotkey_key=82\n", defaults.to_text())).is_err()
        );
    }

    #[test]
    fn cursor_setting_round_trips_defaults_and_rejects_invalid_values() {
        let mut settings = Settings::default();
        assert!(settings.capture_cursor);
        settings.set("capture_cursor", "0").unwrap();
        assert!(
            !Settings::from_text(&settings.to_text())
                .unwrap()
                .capture_cursor
        );
        let legacy = Settings::default()
            .to_text()
            .replace("capture_cursor=1\n", "");
        assert!(Settings::from_text(&legacy).unwrap().capture_cursor);
        assert!(
            Settings::from_text(&legacy.replace("version=2", "version=1"))
                .unwrap()
                .capture_cursor
        );
        assert!(settings.set("capture_cursor", "2").is_err());
        assert!(Settings::from_text(&format!("{legacy}capture_cursor=2\n")).is_err());
        assert!(
            Settings::from_text(&format!("{legacy}capture_cursor=1\ncapture_cursor=0\n")).is_err()
        );
    }

    #[test]
    fn saves_and_replaces_in_requested_directory() {
        let dir = std::env::temp_dir().join(format!("veyva-settings-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.conf");
        let mut settings = Settings::default();
        settings.save(&path).unwrap();
        settings.set("fps", "60").unwrap();
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), settings);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }
    #[test]
    fn legacy_settings_enable_replay_audio_and_alt_f1() {
        let original = Settings::default().to_text();
        let legacy = original
            .lines()
            .filter(|line| {
                !line.starts_with("replay_auto_start=")
                    && !line.starts_with("audio_enabled=")
                    && !line.starts_with("hotkey_")
            })
            .collect::<Vec<_>>()
            .join("\n")
            .replace("version=2", "version=1");
        let loaded = Settings::from_text(&legacy).unwrap();
        assert!(loaded.replay_auto_start && loaded.audio_enabled);
        assert_eq!((loaded.hotkey_modifiers, loaded.hotkey_key), (1, 0x70));
        let mut changed = loaded;
        assert!(changed.set("hotkey_modifiers", "0").is_err());
        changed.set("audio_enabled", "0").unwrap();
        assert!(!changed.audio_enabled);
    }
    #[test]
    fn settings_text_round_trips() {
        let settings = Settings {
            theme: Theme::Light,
            ..Default::default()
        };
        assert_eq!(Settings::from_text(&settings.to_text()).unwrap(), settings);
    }

    #[test]
    fn legacy_and_v2_settings_default_missing_theme_to_dark() {
        let current = Settings::default().to_text();
        let without_theme = current
            .lines()
            .filter(|line| !line.starts_with("theme="))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            Settings::from_text(&without_theme).unwrap().theme,
            Theme::Dark
        );

        let legacy = without_theme.replace("version=2", "version=1");
        assert_eq!(Settings::from_text(&legacy).unwrap().theme, Theme::Dark);
    }

    #[test]
    fn rejects_invalid_or_duplicate_theme_and_sets_named_values() {
        let valid = Settings::default().to_text();
        assert!(Settings::from_text(&valid.replace("theme=dark", "theme=sepia")).is_err());
        assert!(Settings::from_text(&format!("{valid}theme=light\n")).is_err());

        let mut settings = Settings::default();
        settings.set("theme", "light").unwrap();
        assert_eq!(settings.theme, Theme::Light);
        settings.set("theme", "dark").unwrap();
        assert_eq!(settings.theme, Theme::Dark);
        assert!(settings.set("theme", "0").is_err());
    }

    #[test]
    fn rejects_corrupt_settings() {
        let valid = Settings::default().to_text();
        assert!(Settings::from_text(&valid.replace("version=2", "version=99")).is_err());
        assert!(Settings::from_text(&format!("{valid}width=1280\n")).is_err());
        assert!(Settings::from_text(&format!("{valid}unknown=1\n")).is_err());
        assert!(Settings::from_text(&valid.replace("height=720", "height=721")).is_err());
        assert!(Settings::from_text("version=2\nwidth=1280\n").is_err());
    }

    #[test]
    fn set_is_atomic_and_rejects_unsupported_audio_bitrate() {
        let mut settings = Settings::default();
        let original = settings;
        assert!(settings.set("width", "321").is_err());
        assert_eq!(settings, original);
        assert!(settings.set("audio_kbps", "192").is_err());
        assert_eq!(settings, original);
        settings.set("monitor", "2").unwrap();
        assert_eq!(settings.monitor, 2);
    }
}
