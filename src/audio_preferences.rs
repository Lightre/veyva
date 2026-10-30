use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const EXCLUDE_SELF: &str = "veyva:all-outputs-exclude-self";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioPreferences {
    pub output_device: Option<String>,
    pub input_device: Option<String>,
    pub microphone_enabled: bool,
    pub replay_sound: bool,
}

impl AudioPreferences {
    pub fn sound_available(&self, system_enabled: bool) -> bool {
        system_enabled
            && self.output_device.as_deref() == Some(EXCLUDE_SELF)
            && !self.microphone_enabled
    }

    pub fn default_path() -> Result<PathBuf, String> {
        Ok(crate::settings::Settings::default_path()?.with_file_name("audio.conf"))
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("Read audio preferences: {e}")),
        };
        let mut result = Self::default();
        let mut seen = std::collections::HashSet::new();
        for line in content.lines().filter(|line| !line.trim().is_empty()) {
            let (key, value) = line.split_once('=').ok_or("Invalid audio preference")?;
            if !seen.insert(key) {
                return Err(format!("Duplicate audio preference: {key}"));
            }
            match key {
                "output_device" => {
                    result.output_device = (!value.is_empty()).then(|| value.to_owned())
                }
                "input_device" => {
                    result.input_device = (!value.is_empty()).then(|| value.to_owned())
                }
                "replay_sound" => {
                    result.replay_sound = value
                        .parse()
                        .map_err(|_| "Invalid replay sound preference")?;
                }
                "microphone_enabled" => {
                    result.microphone_enabled =
                        value.parse().map_err(|_| "Invalid microphone preference")?
                }
                _ => return Err(format!("Unknown audio preference: {key}")),
            }
        }
        Ok(result)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        for id in [&self.output_device, &self.input_device]
            .into_iter()
            .flatten()
        {
            if id.contains(['\r', '\n']) {
                return Err("Invalid audio device identifier".into());
            }
        }
        let content = format!(
            "output_device={}\ninput_device={}\nmicrophone_enabled={}\nreplay_sound={}\n",
            self.output_device.as_deref().unwrap_or(""),
            self.input_device.as_deref().unwrap_or(""),
            self.microphone_enabled,
            self.replay_sound
        );
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|e| format!("could not create audio preferences directory: {e}"))?;

        static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);
        let file_name = path
            .file_name()
            .ok_or_else(|| "audio preferences path has no file name".to_string())?;
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
                    return Err(format!(
                        "could not create temporary audio preferences file: {error}"
                    ))
                }
            }
        };

        let result = (|| {
            file.write_all(content.as_bytes())
                .map_err(|e| format!("could not write audio preferences: {e}"))?;
            file.sync_all()
                .map_err(|e| format!("could not sync audio preferences: {e}"))?;
            drop(file);
            fs::rename(&selected_path, path)
                .map_err(|e| format!("could not replace audio preferences file: {e}"))
        })();
        if result.is_err() {
            let _ = fs::remove_file(selected_path);
        }
        result
    }
}

#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_input: bool,
}

pub fn devices() -> Result<Vec<AudioDevice>, String> {
    // Enumerate on a dedicated COM apartment; the settings UI may use another model.
    std::thread::spawn(|| unsafe {
        use windows::Win32::{
            Devices::FunctionDiscovery::PKEY_Device_FriendlyName, Media::Audio::*,
            System::Com::StructuredStorage::PropVariantToStringAlloc, System::Com::*,
        };
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|e| e.to_string())?;
        let result = (|| -> windows::core::Result<Vec<AudioDevice>> {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let mut result = Vec::new();
            for (flow, is_input) in [(eRender, false), (eCapture, true)] {
                let list = enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)?;
                for index in 0..list.GetCount()? {
                    let device = list.Item(index)?;
                    let id_ptr = device.GetId()?;
                    let id = id_ptr.to_string();
                    CoTaskMemFree(Some(id_ptr.0.cast()));
                    let id = id?;
                    let properties = device.OpenPropertyStore(STGM_READ)?;
                    let value = properties.GetValue(&PKEY_Device_FriendlyName)?;
                    let name_ptr = PropVariantToStringAlloc(&value)?;
                    let name = name_ptr.to_string();
                    CoTaskMemFree(Some(name_ptr.0.cast()));
                    result.push(AudioDevice {
                        id,
                        name: name?,
                        is_input,
                    });
                }
            }
            Ok(result)
        })();
        CoUninitialize();
        result.map_err(|e| format!("Enumerate audio devices: {e}"))
    })
    .join()
    .map_err(|_| "Audio device enumeration thread failed".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sound_requires_self_exclusion_and_no_microphone() {
        let mut preferences = AudioPreferences::default();
        assert!(!preferences.sound_available(true));
        preferences.output_device = Some(EXCLUDE_SELF.into());
        assert!(preferences.sound_available(true));
        assert!(!preferences.sound_available(false));
        preferences.microphone_enabled = true;
        assert!(!preferences.sound_available(true));
    }
    #[test]
    fn replay_sound_round_trip_and_legacy_default() {
        let path = std::env::temp_dir().join(format!(
            "veyva-audio-preferences-{}-{}.conf",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let preferences = AudioPreferences {
            output_device: Some(EXCLUDE_SELF.into()),
            replay_sound: true,
            ..Default::default()
        };
        preferences.save(&path).unwrap();
        assert_eq!(AudioPreferences::load(&path).unwrap(), preferences);
        std::fs::write(&path, "microphone_enabled=false\n").unwrap();
        assert!(!AudioPreferences::load(&path).unwrap().replay_sound);
        std::fs::remove_file(path).unwrap();
    }
}
