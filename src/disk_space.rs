//! Output-space checks run on disk workers, never in the capture loop.
use std::{os::windows::ffi::OsStrExt, path::Path};
use windows::{core::PCWSTR, Win32::Storage::FileSystem::GetDiskFreeSpaceExW};

pub const RESERVE: u64 = 64 * 1024 * 1024;

pub fn required_space(payload: u64, packets: u64) -> u64 {
    // Leave room for MP4 indexes, headers and finalization as well as payload.
    payload
        .saturating_add(payload / 10)
        .saturating_add(packets.saturating_mul(64))
        .saturating_add(RESERVE)
}

pub fn available(output: &Path) -> Result<u64, String> {
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let wide: Vec<u16> = parent.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut bytes = 0;
    unsafe { GetDiskFreeSpaceExW(PCWSTR(wide.as_ptr()), Some(&mut bytes), None, None) }.map_err(
        |e| {
            format!(
                "Cannot check output disk space (0x{:08X})",
                e.code().0 as u32
            )
        },
    )?;
    Ok(bytes)
}

pub fn ensure(output: &Path, required: u64) -> Result<(), String> {
    let free = available(output)?;
    if free < required {
        return Err(format!(
            "Not enough disk space: {:.1} MiB available, {:.1} MiB required",
            free as f64 / 1048576.0,
            required as f64 / 1048576.0
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn includes_payload_indexes_and_finalization_reserve() {
        assert_eq!(required_space(1000, 10), RESERVE + 1100 + 640);
        assert_eq!(required_space(0, 0), RESERVE);
    }
    #[test]
    fn checks_available_space_without_creating_output() {
        let folder = std::env::temp_dir();
        assert!(available(&folder.join("veyva-space-probe.mp4")).is_ok());
        assert!(ensure(&folder.join("veyva-space-probe.mp4"), u64::MAX)
            .unwrap_err()
            .starts_with("Not enough disk space:"));
    }

    #[test]
    fn large_estimates_saturate_instead_of_wrapping() {
        assert_eq!(required_space(u64::MAX, u64::MAX), u64::MAX);
    }
}
