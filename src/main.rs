#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

#[cfg(windows)]
mod audio;
#[cfg(windows)]
mod audio_preferences;
#[cfg(windows)]
mod audio_timeline;
#[cfg(windows)]
mod background;
mod capture;
mod diagnostics;
#[cfg(windows)]
mod gui;
#[cfg(windows)]
mod hotkey;
#[cfg(windows)]
mod output;
mod session;
mod settings;
use std::path::PathBuf;

#[derive(Debug, PartialEq)]
struct Options {
    probe: bool,
    seconds: u32,
    monitor: Option<u32>,
    overrides: Vec<(String, String)>,
    output: Option<PathBuf>,
}
fn parse(args: &[String]) -> Result<Option<Options>, String> {
    if args.is_empty() || args == ["--help"] || args == ["-h"] {
        return Ok(None);
    }
    let probe = match args[0].as_str() {
        "probe" => true,
        "record" => false,
        _ => return Err("Expected 'probe' or 'record'; use --help".into()),
    };
    let mut options = Options {
        probe,
        seconds: 10,
        monitor: None,
        overrides: Vec::new(),
        output: None,
    };
    let mut seen = std::collections::HashSet::new();
    for pair in args[1..].chunks(2) {
        if pair.len() != 2 {
            return Err(format!("Missing value for {}", pair[0]));
        }
        if !seen.insert(pair[0].as_str()) {
            return Err(format!("Duplicate option: {}", pair[0]));
        }
        match pair[0].as_str() {
            "--monitor" => {
                options.monitor = Some(pair[1].parse().map_err(|_| "Invalid monitor index")?)
            }
            "--width" | "--height" | "--fps" | "--video-kbps" | "--buffer-seconds"
            | "--buffer-mib"
                if !probe =>
            {
                let key = match pair[0].as_str() {
                    "--video-kbps" => "video_kbps",
                    "--buffer-seconds" => "history_seconds",
                    "--buffer-mib" => "replay_budget_mib",
                    other => other.trim_start_matches("--"),
                };
                options.overrides.push((key.to_string(), pair[1].clone()));
            }
            "--seconds" if !probe => {
                options.seconds = pair[1].parse().map_err(|_| "Invalid duration")?
            }
            "--output" if !probe => options.output = Some(PathBuf::from(&pair[1])),
            _ => return Err(format!("Unknown option: {}", pair[0])),
        }
    }
    if !(1..=60).contains(&options.seconds) {
        return Err("Duration must be between 1 and 60 seconds".into());
    }
    if !probe
        && options.output.as_ref().is_none_or(|p| {
            p.extension()
                .is_none_or(|ext| !ext.eq_ignore_ascii_case("mp4"))
        })
    {
        return Err("Recording requires --output with an .mp4 path".into());
    }
    Ok(Some(options))
}
fn desktop_requested(args: &[String]) -> bool {
    args.is_empty() || args == ["gui"] || args == ["gui", "--background"]
}

#[cfg(windows)]
fn attach_cli_console() {
    use windows::Win32::{
        Storage::FileSystem::{GetFileType, FILE_TYPE_DISK, FILE_TYPE_PIPE},
        System::Console::*,
    };
    unsafe {
        // Preserve inherited pipes/files: attaching to the parent may replace
        // standard handles, breaking redirected CLI output and measurement tools.
        let redirected: Vec<_> = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE]
            .into_iter()
            .filter_map(|kind| {
                let handle = GetStdHandle(kind).ok()?;
                let file_type = GetFileType(handle);
                (!handle.is_invalid()
                    && (file_type == FILE_TYPE_DISK || file_type == FILE_TYPE_PIPE))
                    .then_some((kind, handle))
            })
            .collect();
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        for (kind, handle) in redirected {
            let _ = SetStdHandle(kind, handle);
        }
    }
}

fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    #[cfg(windows)]
    if !desktop_requested(&args) {
        attach_cli_console();
    }
    match execute(&args) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            #[cfg(windows)]
            if desktop_requested(&args) {
                if let Ok((mut log, _)) = diagnostics::Logger::open() {
                    let _ = log.write(&format!("Desktop startup failed: {e}"));
                }
                unsafe {
                    use windows::{
                        core::{w, HSTRING},
                        Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK},
                    };
                    let message = HSTRING::from(format!("Veyva could not start.\n\n{e}"));
                    let _ = MessageBoxW(None, &message, w!("Veyva"), MB_OK | MB_ICONERROR);
                }
            }
            std::process::ExitCode::FAILURE
        }
    }
}
fn execute(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if desktop_requested(args) {
        #[cfg(windows)]
        {
            let Some(_instance) = background::SingleInstance::acquire()? else {
                return Ok(());
            };
            return gui::run().map_err(Into::into);
        }
        #[cfg(not(windows))]
        {
            return Err("The desktop interface requires Windows".into());
        }
    }
    if args.first().is_some_and(|arg| arg == "settings") {
        return settings_command(&args[1..]);
    }
    let Some(options) = parse(args)? else {
        println!("Veyva - native Windows capture\n\nUsage:\n  veyva gui\n  veyva probe [--monitor 0]\n  veyva record --output <file.mp4> [--seconds 10] [--monitor 0]\n    [--width 1280] [--height 720] [--fps 30] [--video-kbps 3000]\n    [--buffer-seconds 30] [--buffer-mib 32]\n  veyva settings show\n  veyva settings set <key> <value>\n  veyva settings preset <low|balanced|high>\n\nRecord overrides apply only to that invocation. Silent hardware H.264; recording duration remains limited to 1-60 seconds. Open the desktop interface for continuous replay, system audio and microphone capture.");
        return Ok(());
    };
    let mut settings = settings::Settings::load(&settings::Settings::default_path()?)?;
    // Overrides are validated before capture; persisted settings remain unchanged.
    for (key, value) in &options.overrides {
        settings.set(key, value)?;
    }
    if let Some(monitor) = options.monitor {
        settings.monitor = monitor;
    }
    settings.profile.validate()?;
    if let Some(path) = &options.output {
        if path.exists() || path.with_extension("partial.mp4").exists() {
            return Err("Output or temporary file already exists".into());
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."));
        if !parent.is_dir() {
            return Err("Output directory does not exist".into());
        }
    }
    let (mut log, path) = diagnostics::Logger::open()?;
    log.write(&format!(
        "Veyva {}; log: {}",
        env!("CARGO_PKG_VERSION"),
        path.display()
    ))?;
    let result = capture::run(
        options.seconds,
        settings.profile,
        settings.monitor,
        settings.capture_cursor,
        options.output.as_deref(),
        options.probe,
        &mut log,
    );
    if let Err(e) = &result {
        let _ = log.write(&format!("ERROR: {e}"));
    }
    result?;
    Ok(())
}

fn settings_command(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = settings::Settings::default_path()?;
    let mut current = settings::Settings::load(&path)?;
    match args {
        [command] if command == "show" => {}
        [command, key, value] if command == "set" => {
            current.set(key, value)?;
            current.save(&path)?;
        }
        [command, preset] if command == "preset" => {
            current.profile = match preset.as_str() {
                "low" => settings::CaptureProfile::low_end(),
                "balanced" => settings::CaptureProfile::balanced(),
                "high" => settings::CaptureProfile::high(),
                _ => return Err("Unknown preset: use low, balanced or high".into()),
            };
            current.save(&path)?;
        }
        _ => return Err(
            "Use settings show, settings set <key> <value>, or settings preset <low|balanced|high>"
                .into(),
        ),
    }
    println!("Settings file: {}\n{}", path.display(), current.to_text());
    let estimate = current.profile.payload_mib();
    println!("Nominal replay A/V payload: {estimate:.1} MiB; budget: {} MiB. Excludes overhead and bitrate bursts.", current.profile.replay_budget_mib);
    if estimate > f64::from(current.profile.replay_budget_mib) {
        println!(
            "Warning: nominal payload exceeds the memory budget; replay may retain less history."
        );
    }
    println!("Replay settings apply when starting a desktop replay session. Desktop audio is configured in the application. Actual encoder support must be negotiated on the target hardware.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn desktop_launches_are_distinct_from_cli_commands() {
        for input in [vec![], vec!["gui"], vec!["gui", "--background"]] {
            assert!(desktop_requested(&args(&input)));
        }
        for input in [
            vec!["--help"],
            vec!["settings", "show"],
            vec!["probe"],
            vec!["gui", "invalid"],
        ] {
            assert!(!desktop_requested(&args(&input)));
        }
    }

    #[test]
    fn no_arguments_never_records() {
        assert_eq!(parse(&[]).unwrap(), None);
    }
    #[test]
    fn invalid_recording_requests_are_rejected() {
        for input in [
            vec!["record"],
            vec!["record", "--output", "a.mp4", "--seconds", "0"],
            vec!["record", "--output", "a.mp4", "--seconds", "61"],
            vec!["record", "--output", "a.mp4", "--monitor", "-1"],
            vec!["record", "--output", "a.mp4", "--seconds"],
            vec!["record", "--output", "a.mp4", "--output", "b.mp4"],
        ] {
            assert!(parse(&args(&input)).is_err(), "{input:?}");
        }
    }
    #[test]
    fn explicit_recording_accepts_unicode_paths() {
        let result = parse(&args(&[
            "record",
            "--output",
            "C:\\Clips\\example clip.mp4",
            "--seconds",
            "5",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(result.seconds, 5);
        assert!(!result.probe);
    }
}
