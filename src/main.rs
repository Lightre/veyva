mod capture;
mod diagnostics;
mod settings;
use std::path::PathBuf;

#[derive(Debug, PartialEq)]
struct Options {
    probe: bool,
    seconds: u32,
    monitor: u32,
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
        monitor: 0,
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
                options.monitor = pair[1].parse().map_err(|_| "Invalid monitor index")?
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
fn main() -> std::process::ExitCode {
    match execute() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
fn execute() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let Some(options) = parse(&args)? else {
        println!("Veyva - native Windows capture milestone\n\nUsage:\n  veyva probe [--monitor 0]\n  veyva record --output <file.mp4> [--seconds 10] [--monitor 0]\n\nSilent 720p/30 FPS H.264. Hardware encoder required. Recording never starts automatically.");
        let profile = settings::CaptureProfile::low_end();
        println!(
            "Future replay profile: {} seconds; nominal A/V payload {:.1} MiB",
            profile.history_seconds,
            profile.payload_mib()
        );
        return Ok(());
    };
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
        options.monitor,
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

#[cfg(test)]
mod tests {
    use super::*;
    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
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
