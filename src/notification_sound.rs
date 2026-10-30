//! Requests are consumed only by an audio worker using the self-excluding backend.
use std::sync::atomic::{AtomicBool, Ordering};
static REQUESTED: AtomicBool = AtomicBool::new(false);
static SOUND: &[u8] = include_bytes!("../assets/sounds/replay-saved.wav");

pub fn request() {
    REQUESTED.store(true, Ordering::Release);
}

pub fn poll(allowed: bool) {
    if REQUESTED.swap(false, Ordering::AcqRel) && allowed {
        unsafe {
            use windows::Win32::Media::Audio::*;
            let played = PlaySoundW(
                windows::core::PCWSTR(SOUND.as_ptr().cast()),
                None,
                SND_MEMORY | SND_ASYNC | SND_NODEFAULT | SND_NOSTOP,
            );
            if !played.as_bool() {
                if let Ok((mut log, _)) = crate::diagnostics::Logger::open() {
                    let _ = log.write("Replay notification sound could not be played");
                }
            }
        }
    }
}

pub fn stop() {
    REQUESTED.store(false, Ordering::Release);
    unsafe {
        let _ = windows::Win32::Media::Audio::PlaySoundW(
            windows::core::PCWSTR::null(),
            None,
            windows::Win32::Media::Audio::SND_FLAGS(0),
        );
    }
}
