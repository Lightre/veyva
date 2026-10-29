use crate::settings::Settings;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{sync_channel, SyncSender},
        Arc, Mutex,
    },
    thread::JoinHandle,
};

pub enum Command {
    StartRecording(PathBuf),
    PauseRecording,
    ResumeRecording,
    FinishRecording,
    SetReplayEnabled(bool),
    SaveReplay(PathBuf),
    Stop,
}
#[derive(Clone)]
pub struct Status {
    pub recording: bool,
    pub paused: bool,
    pub recorded_seconds: f64,
    pub finalizing: bool,
    pub replay_enabled: bool,
    pub audio_active: bool,
    pub audio_error: Option<String>,
    pub phase: String,
    pub buffered_seconds: f64,
    pub buffer_bytes: u64,
    pub submitted: u64,
    pub dropped: u64,
    pub saving: bool,
    pub last_saved: Option<PathBuf>,
    pub error: Option<String>,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            recording: false,
            paused: false,
            recorded_seconds: 0.0,
            finalizing: false,
            replay_enabled: false,
            audio_active: false,
            audio_error: None,
            phase: "Idle".into(),
            buffered_seconds: 0.0,
            buffer_bytes: 0,
            submitted: 0,
            dropped: 0,
            saving: false,
            last_saved: None,
            error: None,
        }
    }
}
pub struct Session {
    pub status: Arc<Mutex<Status>>,
    commands: SyncSender<Command>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Session {
    pub fn send(&self, command: Command) -> Result<(), String> {
        if matches!(command, Command::Stop) {
            if !self.is_finished() {
                self.status.lock().unwrap_or_else(|e| e.into_inner()).phase =
                    "Stopping / finishing export".into();
            }
            self.stop.store(true, Ordering::Release);
            return Ok(());
        }
        self.commands
            .try_send(command)
            .map_err(|e| format!("Session command unavailable: {e}"))
    }
    pub fn is_finished(&self) -> bool {
        self.worker.as_ref().is_none_or(|w| w.is_finished())
    }
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if self.is_finished() {
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}
pub fn start(settings: Settings) -> Result<Session, String> {
    settings.profile.validate()?;
    static ACTIVE: AtomicBool = AtomicBool::new(false);
    struct ActiveGuard;
    impl Drop for ActiveGuard {
        fn drop(&mut self) {
            ACTIVE.store(false, Ordering::Release);
        }
    }
    if ACTIVE
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("The previous capture session is still running or finishing an export".into());
    }
    let active = ActiveGuard;
    let status = Arc::new(Mutex::new(Status {
        phase: "Starting".into(),
        ..Default::default()
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let (commands, receiver) = sync_channel(4);
    let shared = status.clone();
    let stopping = stop.clone();
    let worker = std::thread::Builder::new()
        .name("veyva-capture".into())
        .spawn(move || {
            let _active = active;
            let result = (|| -> crate::capture::RResult<()> {
                let (mut log, _) = crate::diagnostics::Logger::open()?;
                log.write(&format!("Capture session starting: monitor={}, {}x{} at {} FPS; H.264; audio_requested={}; history_seconds={}; payload_budget_mib={}", settings.monitor, settings.profile.width, settings.profile.height, settings.profile.fps, settings.audio_enabled, settings.profile.history_seconds, settings.profile.replay_budget_mib))?;
                let result = crate::capture::continuous(settings, receiver, stopping, shared.clone());
                match &result {
                    Ok(()) => { log.write("Replay session stopped")?; }
                    Err(error) => { let _ = log.write(&format!("Replay session failed: {error}")); }
                }
                result
            })();
            let mut status = shared.lock().unwrap_or_else(|e| e.into_inner());
            status.phase = if result.is_ok() { "Stopped" } else { "Failed" }.into();
            if let Err(error) = result {
                status.error = Some(error.to_string());
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(Session {
        status,
        commands,
        stop,
        worker: Some(worker),
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Track {
    Video,
    Audio,
}
#[derive(Clone)]
pub(crate) struct Packet {
    pub track: Track,
    pub data: Arc<[u8]>,
    pub time: i64,
    pub duration: i64,
    pub keyframe: bool,
}
pub(crate) struct History {
    packets: VecDeque<Packet>,
    pub bytes: u64,
    byte_limit: u64,
    time_limit: i64,
    newest_video_end: i64,
}
impl History {
    pub fn new(bytes: u64, seconds: u32) -> Self {
        Self {
            packets: VecDeque::new(),
            bytes: 0,
            byte_limit: bytes,
            time_limit: i64::from(seconds) * 10_000_000,
            newest_video_end: 0,
        }
    }
    pub fn push(&mut self, packet: Packet) {
        if self.packets.is_empty() && !(matches!(packet.track, Track::Video) && packet.keyframe) {
            return;
        }
        if matches!(packet.track, Track::Video) {
            self.newest_video_end = self
                .newest_video_end
                .max(packet.time.saturating_add(packet.duration));
        }
        self.bytes += packet.data.len() as u64;
        self.packets.push_back(packet);
        while self.bytes > self.byte_limit
            || self.span() > self.time_limit
            || self.packets.len() > 144_000
        {
            if let Some(packet) = self.packets.pop_front() {
                self.bytes -= packet.data.len() as u64;
            } else {
                break;
            }
            while self
                .packets
                .front()
                .is_some_and(|p| !(matches!(p.track, Track::Video) && p.keyframe))
            {
                self.bytes -= self.packets.pop_front().unwrap().data.len() as u64;
            }
        }
    }
    fn span(&self) -> i64 {
        self.packets
            .front()
            .map_or(0, |first| self.newest_video_end.saturating_sub(first.time))
    }
    pub fn seconds(&self) -> f64 {
        self.span() as f64 / 10_000_000.0
    }
    pub fn snapshot(&self) -> Vec<Packet> {
        let base = self.packets.front().map_or(0, |p| p.time);
        self.packets
            .iter()
            .filter(|p| p.time >= base)
            .cloned()
            .collect()
    }
}
/// Rebase complete recording segments onto one timeline; replay is unaffected.
#[derive(Default)]
pub(crate) struct RecordingClock {
    paused: bool,
    segment_start: Option<i64>,
    output_start: i64,
    pub end: i64,
}
impl RecordingClock {
    pub fn is_paused(&self) -> bool {
        self.paused
    }
    pub fn pause(&mut self) {
        self.paused = true;
    }
    pub fn resume(&mut self) {
        self.paused = false;
        self.segment_start = None;
    }
    pub fn map(&mut self, packet: &Packet) -> Option<i64> {
        if self.paused {
            return None;
        }
        if self.segment_start.is_none() {
            if packet.track != Track::Video || !packet.keyframe {
                return None;
            }
            self.segment_start = Some(packet.time);
            self.output_start = self.end;
        }
        let start = self.segment_start.unwrap();
        if packet.time < start {
            return None;
        }
        let mapped = packet.time - start + self.output_start;
        self.end = self.end.max(mapped.saturating_add(packet.duration));
        Some(mapped)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn packet(n: i64, keyframe: bool) -> Packet {
        Packet {
            track: Track::Video,
            data: Arc::from([1u8; 4]),
            time: n * 10_000_000,
            duration: 10_000_000,
            keyframe,
        }
    }
    #[test]
    fn recording_pause_removes_gap_and_waits_for_keyframe() {
        let mut clock = RecordingClock::default();
        assert_eq!(clock.map(&packet(4, false)), None);
        assert_eq!(clock.map(&packet(5, true)), Some(0));
        clock.pause();
        assert_eq!(clock.map(&packet(6, true)), None);
        clock.resume();
        assert_eq!(clock.map(&packet(20, false)), None);
        assert_eq!(clock.map(&packet(21, true)), Some(10_000_000));
        let mut audio = packet(21, true);
        audio.track = Track::Audio;
        assert_eq!(clock.map(&audio), Some(10_000_000));
    }
    #[test]
    fn audio_respects_history_budget_and_snapshot_start() {
        let mut history = History::new(20, 3);
        for n in 0..8 {
            history.push(packet(n, n % 2 == 0));
            let mut audio = packet(n, false);
            audio.track = Track::Audio;
            history.push(audio);
            assert!(history.bytes <= 20);
        }
        let snapshot = history.snapshot();
        assert!(snapshot
            .first()
            .is_some_and(|p| p.track == Track::Video && p.keyframe));
        assert!(snapshot.iter().all(|p| p.time >= snapshot[0].time));
    }
    #[test]
    fn eviction_preserves_keyframes_and_byte_budget() {
        let mut h = History::new(12, 60);
        for n in 0..8 {
            h.push(packet(n, n % 2 == 0));
            assert!(h.bytes <= 12);
            assert!(h.snapshot().first().is_none_or(|p| p.keyframe));
        }
        assert_eq!(h.snapshot()[0].time, 60_000_000);
    }
    #[test]
    fn time_limit_and_short_history() {
        let mut h = History::new(1000, 3);
        h.push(packet(0, false));
        assert_eq!(h.seconds(), 0.0);
        h.push(packet(1, true));
        assert_eq!(h.seconds(), 1.0);
        let snapshot = h.snapshot();
        for n in 2..9 {
            h.push(packet(n, n % 2 == 0));
        }
        assert!(h.seconds() <= 3.0);
        assert_eq!(snapshot[0].time, 10_000_000);
    }
    #[test]
    fn oversized_gop_is_discarded_until_keyframe() {
        let mut h = History::new(3, 60);
        h.push(packet(0, true));
        h.push(packet(1, false));
        assert!(h.snapshot().is_empty());
        assert_eq!(h.bytes, 0);
    }
}
