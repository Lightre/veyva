/// Initial product defaults. These are targets, not measured performance results.
#[derive(Debug, Clone, Copy)]
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

    /// Nominal compressed A/V payload, excluding allocator, GPU and muxer overhead.
    pub fn payload_mib(self) -> f64 {
        (f64::from(self.video_kbps) + f64::from(self.audio_kbps))
            * 1000.0
            * f64::from(self.history_seconds)
            / 8.0
            / 1_048_576.0
    }
}
