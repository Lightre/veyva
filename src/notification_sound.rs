//! Optional confirmation played only after a replay file is published.
static SOUND: &[u8] = include_bytes!("../assets/sounds/replay-saved.wav");

pub fn play() {
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
