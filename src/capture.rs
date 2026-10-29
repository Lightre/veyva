use crate::diagnostics::Logger;
use std::{fmt, path::Path};

#[derive(Debug)]
pub enum RecorderError {
    Other(String),
    NoEncoder,
    NoSuchMonitor(u32),
}
impl fmt::Display for RecorderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Other(message) => f.write_str(message),
            Self::NoEncoder => f.write_str(
                "No compatible hardware H.264 encoder found; software encoding is disabled",
            ),
            Self::NoSuchMonitor(index) => write!(f, "Display index {index} does not exist"),
        }
    }
}
impl std::error::Error for RecorderError {}
impl From<std::io::Error> for RecorderError {
    fn from(e: std::io::Error) -> Self {
        Self::Other(e.to_string())
    }
}
pub type RResult<T> = Result<T, RecorderError>;
#[cfg(windows)]
#[path = "windows_capture.rs"]
mod windows_capture;

pub fn run(
    seconds: u32,
    profile: crate::settings::CaptureProfile,
    monitor: u32,
    output: Option<&Path>,
    probe: bool,
    log: &mut Logger,
) -> RResult<()> {
    #[cfg(windows)]
    {
        windows_capture::run(seconds, profile, monitor, output, probe, log)
    }
    #[cfg(not(windows))]
    {
        let _ = (seconds, profile, monitor, output, probe, log);
        Err(RecorderError::Other(
            "Capture is supported on Windows only".into(),
        ))
    }
}

/// Rational frame timestamps avoid rounding jitter and preserve skipped slots.
fn frame_time(slot: u64, fps: u32) -> i64 {
    ((u128::from(slot) * 10_000_000) / u128::from(fps)) as i64
}
#[cfg(test)]
mod tests {
    use super::frame_time;
    #[test]
    fn timestamps_preserve_exact_seconds_and_skipped_frames() {
        assert_eq!(frame_time(30, 30), 10_000_000);
        assert_eq!(frame_time(1800, 30), 600_000_000);
        assert_eq!(frame_time(3, 30) - frame_time(0, 30), 1_000_000);
        for n in 0..1800 {
            assert!(frame_time(n + 1, 30) > frame_time(n, 30));
        }
    }
}
