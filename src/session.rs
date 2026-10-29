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
    SaveReplay(PathBuf),
    Stop,
}
pub struct Status {
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
                log.write(&format!("Replay session starting: monitor={}, {}x{} at {} FPS; silent H.264; history_seconds={}; payload_budget_mib={}", settings.monitor, settings.profile.width, settings.profile.height, settings.profile.fps, settings.profile.history_seconds, settings.profile.replay_budget_mib))?;
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

#[derive(Clone)]
pub(crate) struct Packet {
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
}
impl History {
    pub fn new(bytes: u64, seconds: u32) -> Self {
        Self {
            packets: VecDeque::new(),
            bytes: 0,
            byte_limit: bytes,
            time_limit: i64::from(seconds) * 10_000_000,
        }
    }
    pub fn push(&mut self, packet: Packet) {
        if self.packets.is_empty() && !packet.keyframe {
            return;
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
            while self.packets.front().is_some_and(|p| !p.keyframe) {
                self.bytes -= self.packets.pop_front().unwrap().data.len() as u64;
            }
        }
    }
    fn span(&self) -> i64 {
        self.packets
            .front()
            .zip(self.packets.back())
            .map_or(0, |(a, b)| {
                b.time.saturating_add(b.duration).saturating_sub(a.time)
            })
    }
    pub fn seconds(&self) -> f64 {
        self.span() as f64 / 10_000_000.0
    }
    pub fn snapshot(&self) -> Vec<Packet> {
        self.packets.iter().cloned().collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn packet(n: i64, keyframe: bool) -> Packet {
        Packet {
            data: Arc::from([1u8; 4]),
            time: n * 10_000_000,
            duration: 10_000_000,
            keyframe,
        }
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
