//! Selected render loopback and optional microphone, mixed as 48 kHz stereo PCM.
//! AAC setup adapted from ClipFlow src-tauri/src/media/audio.rs,
//! revision d204de138d503d2eb41fcb2af553aa2430f0a78b (MIT).
//! Copyright (c) 2026 jaimitus. See THIRD_PARTY_NOTICES.txt for the MIT license.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use windows::Win32::{
    Media::{Audio::*, MediaFoundation::*},
    System::Com::*,
};

pub(crate) struct AudioPacket {
    pub data: Arc<[u8]>,
    pub time: i64,
    pub duration: i64,
}

pub(crate) struct AudioCapture {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    packets: Receiver<AudioPacket>,
    error: Arc<Mutex<Option<String>>>,
    levels: Arc<[AtomicU32; 2]>,
}

impl AudioCapture {
    pub fn start(
        origin: Instant,
        system_enabled: bool,
        preferences: crate::audio_preferences::AudioPreferences,
    ) -> Result<Self, String> {
        let levels = Arc::new([AtomicU32::new(0), AtomicU32::new(0)]);
        let worker_levels = levels.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(None));
        let (sender, packets) = mpsc::sync_channel(128);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let stop_worker = stop.clone();
        let error_worker = error.clone();
        let worker = std::thread::Builder::new()
            .name("veyva-audio".into())
            .spawn(move || {
                let result = unsafe {
                    run(
                        origin,
                        system_enabled,
                        preferences,
                        &stop_worker,
                        &sender,
                        &ready_tx,
                        &worker_levels,
                    )
                };
                for level in worker_levels.iter() {
                    level.store(0, Ordering::Relaxed);
                }
                if let Err(message) = result {
                    *error_worker.lock().unwrap_or_else(|e| e.into_inner()) = Some(message.clone());
                    let _ = ready_tx.try_send(Err(message));
                }
            })
            .map_err(|e| format!("Start audio thread: {e}"))?;
        match ready_rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(())) => Ok(Self {
                stop,
                worker: Some(worker),
                packets,
                error,
                levels,
            }),
            result => {
                stop.store(true, Ordering::Release);
                // A driver initialization call may outlive the startup deadline.
                // Detach in that case; the worker owns its COM state and sees stop.
                if worker.is_finished() {
                    let _ = worker.join();
                }
                Err(match result {
                    Ok(Err(e)) => e,
                    _ => "Audio initialization timed out".into(),
                })
            }
        }
    }
    pub fn drain(&mut self) -> Vec<AudioPacket> {
        self.packets.try_iter().take(128).collect()
    }
    pub fn levels(&self) -> [f32; 2] {
        std::array::from_fn(|index| f32::from_bits(self.levels[index].load(Ordering::Relaxed)))
    }
    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Runtime;
impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}
struct Endpoint(IAudioClient);
impl Drop for Endpoint {
    fn drop(&mut self) {
        unsafe {
            let _ = self.0.Stop();
        }
    }
}

unsafe fn run(
    origin: Instant,
    system_enabled: bool,
    preferences: crate::audio_preferences::AudioPreferences,
    stop: &AtomicBool,
    sender: &SyncSender<AudioPacket>,
    ready: &SyncSender<Result<(), String>>,
    levels: &[AtomicU32; 2],
) -> Result<(), String> {
    CoInitializeEx(None, COINIT_MULTITHREADED)
        .ok()
        .map_err(|e| format!("Initialize audio COM: {e}"))?;
    if let Err(e) = MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET) {
        CoUninitialize();
        return Err(format!("Initialize audio Media Foundation: {e}"));
    }
    let _runtime = Runtime;
    let result = capture(
        origin,
        system_enabled,
        preferences,
        stop,
        sender,
        ready,
        levels,
    );
    result.map_err(|e| format!("Audio capture: {e}"))
}

#[derive(Default)]
struct AudioDiagnostics {
    packets: u64,
    discontinuities: u64,
    invalid_timestamps: u64,
    gap_frames: u64,
    trimmed_frames: u64,
    idle_fill_frames: u64,
    backlog_polls: u64,
    max_poll_ms: u128,
    events: Vec<(u128, &'static str, u64)>,
}
impl AudioDiagnostics {
    fn event(&mut self, origin: Instant, kind: &'static str, frames: u64) {
        // Preserve startup context and the most recent glitches, with no strings
        // or log writes in the packet loop.
        if self.events.len() == 24 {
            self.events.remove(8);
        }
        self.events
            .push((origin.elapsed().as_millis(), kind, frames));
    }
}
impl Drop for AudioDiagnostics {
    fn drop(&mut self) {
        // Writes happen after capture has stopped, including error unwinding.
        if let Ok((mut log, _)) = crate::diagnostics::Logger::open() {
            let _=log.write(&format!("Audio summary: packets={}, discontinuities={}, invalid_timestamps={}, gap_frames={}, trimmed_frames={}, idle_fill_frames={}, backlog_polls={}, max_poll_ms={}",self.packets,self.discontinuities,self.invalid_timestamps,self.gap_frames,self.trimmed_frames,self.idle_fill_frames,self.backlog_polls,self.max_poll_ms));
            for event in &self.events {
                let _ = log.write(&format!(
                    "Audio event: elapsed_ms={}, kind={}, frames={}",
                    event.0, event.1, event.2
                ));
            }
        }
    }
}

struct Source {
    meter_index: usize,
    _endpoint: Endpoint,
    capture: IAudioCaptureClient,
    pending: Vec<u8>,
    packet_clock: crate::audio_timeline::PacketClock,
    frames_written: i64,
    diagnostics: AudioDiagnostics,
    last_packet: Instant,
}
impl Source {
    unsafe fn new(
        id: Option<&str>,
        is_input: bool,
        first_frame: i64,
    ) -> windows::core::Result<Self> {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = match id {
            Some(id) => enumerator.GetDevice(windows::core::PCWSTR(
                windows::core::HSTRING::from(id).as_ptr(),
            ))?,
            None => enumerator
                .GetDefaultAudioEndpoint(if is_input { eCapture } else { eRender }, eConsole)?,
        };
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
        // Explicit conversion avoids interpreting arbitrary endpoint channel layouts
        // or treating a 44.1 kHz mix as 48 kHz. Unsupported conversion fails startup.
        let format = WAVEFORMATEX {
            wFormatTag: 1,
            nChannels: 2,
            nSamplesPerSec: 48_000,
            nAvgBytesPerSec: 192_000,
            nBlockAlign: 4,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            (if is_input {
                0
            } else {
                AUDCLNT_STREAMFLAGS_LOOPBACK
            }) | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
            1_000_000, // 100 ms capacity tolerates brief scheduling stalls; poll remains 5 ms.
            0,
            &format,
            None,
        )?;
        if let Ok(mix) = client.GetMixFormat() {
            let native = std::ptr::read_unaligned(mix);
            let (tag, rate, channels, bits) = (
                native.wFormatTag,
                native.nSamplesPerSec,
                native.nChannels,
                native.wBitsPerSample,
            );
            CoTaskMemFree(Some(mix.cast()));
            if let Ok((mut log, _)) = crate::diagnostics::Logger::open() {
                let _=log.write(&format!("System audio format: requested PCM16 48000 Hz stereo; native mix tag={tag}, rate={rate}, channels={channels}, bits={bits}; sample-position packet alignment; WASAPI buffer request=100 ms"));
            }
        }
        let capture: IAudioCaptureClient = client.GetService()?;
        client.Start()?;
        Ok(Self {
            meter_index: usize::from(is_input),
            _endpoint: Endpoint(client),
            capture,
            pending: Vec::with_capacity(192_000),
            packet_clock: Default::default(),
            frames_written: first_frame,
            diagnostics: Default::default(),
            last_packet: Instant::now(),
        })
    }
    unsafe fn poll(
        &mut self,
        origin: Instant,
        origin_hns: i128,
        stop: &AtomicBool,
    ) -> windows::core::Result<bool> {
        let mut drained_frames = 0u64;
        let capture = &self.capture;
        let pending = &mut self.pending;
        let diagnostics = &mut self.diagnostics;
        // Bound both the PCM queue and work per poll, including on a broken driver.
        for _ in 0..32 {
            if stop.load(Ordering::Acquire) || capture.GetNextPacketSize()? == 0 {
                break;
            }
            let mut data = std::ptr::null_mut();
            let mut frames = 0;
            let mut flags = 0;
            let mut qpc_position = 0u64;
            let mut device_position = 0u64;
            capture.GetBuffer(
                &mut data,
                &mut frames,
                &mut flags,
                Some(&mut device_position),
                Some(&mut qpc_position),
            )?;
            // WASAPI QPC positions already use 100 ns units (not raw counter ticks).
            let valid_time = flags & AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32 == 0;
            let queued_end = self.frames_written + (pending.len() / 4) as i64;
            let qpc_frame =
                (((qpc_position as i128 - origin_hns).max(0) * 48_000) / 10_000_000) as i64;
            let target =
                self.packet_clock
                    .target(device_position, qpc_frame, valid_time, queued_end);
            diagnostics.packets += 1;
            if frames > 0 {
                self.last_packet = Instant::now();
                drained_frames += u64::from(frames);
            }
            if flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0 {
                diagnostics.discontinuities += 1;
                diagnostics.event(origin, "device_discontinuity", u64::from(frames));
            }
            if !valid_time {
                diagnostics.invalid_timestamps += 1;
            }
            let gap = (target - queued_end).max(0) as usize;
            let trim = (queued_end - target).max(0).min(i64::from(frames)) as usize;
            diagnostics.gap_frames += gap as u64;
            diagnostics.trimmed_frames += trim as u64;
            if gap > 0 {
                diagnostics.event(origin, "alignment_gap", gap as u64);
            }
            if trim > 0 {
                diagnostics.event(origin, "alignment_trim", trim as u64);
            }
            let bytes = (frames as usize - trim) * 4;
            let overflow = gap > 48_000 || pending.len() + gap * 4 + bytes > 192_000;
            if !overflow {
                pending.resize(pending.len() + gap * 4, 0);
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                    pending.resize(pending.len() + bytes, 0);
                } else {
                    pending
                        .extend_from_slice(std::slice::from_raw_parts(data.add(trim * 4), bytes));
                }
            }
            capture.ReleaseBuffer(frames)?;
            if overflow {
                return Err(windows::core::Error::new(
                    windows::core::HRESULT(0x80004005u32 as i32),
                    "Audio PCM queue exceeded one second",
                ));
            }
        }
        let backlogged = capture.GetNextPacketSize()? > 0;
        if backlogged {
            diagnostics.backlog_polls += 1;
        }
        // A temporarily late capture thread is not an idle render device.
        // Drain real packets first; synthesize silence only after confirmed idle.
        let elapsed_frames = (origin.elapsed().as_nanos() * 48_000 / 1_000_000_000) as i64;
        let missing = elapsed_frames - self.frames_written - (pending.len() / 4) as i64 - 2400;
        if missing > 0
            && crate::audio_timeline::should_fill_silence(
                backlogged,
                self.last_packet.elapsed().as_millis(),
            )
        {
            let fill = (missing as usize).min((192_000 - pending.len()) / 4);
            pending.resize(pending.len() + fill * 4, 0);
            diagnostics.idle_fill_frames += fill as u64;
        }

        Ok(backlogged && drained_frames > 0)
    }
}

unsafe fn capture(
    origin: Instant,
    system_enabled: bool,
    preferences: crate::audio_preferences::AudioPreferences,
    stop: &AtomicBool,
    sender: &SyncSender<AudioPacket>,
    ready: &SyncSender<Result<(), String>>,
    levels: &[AtomicU32; 2],
) -> windows::core::Result<()> {
    let first_frame = (origin.elapsed().as_nanos() * 48_000 / 1_000_000_000) as i64;
    let mut sources = Vec::with_capacity(2);
    if system_enabled {
        sources.push(Source::new(
            preferences.output_device.as_deref(),
            false,
            first_frame,
        )?);
    }
    if preferences.microphone_enabled {
        sources.push(Source::new(
            preferences.input_device.as_deref(),
            true,
            first_frame,
        )?);
    }
    let encoder = Encoder::new()?;
    let mut qpc = 0i64;
    let mut frequency = 0i64;
    windows::Win32::System::Performance::QueryPerformanceFrequency(&mut frequency)?;
    windows::Win32::System::Performance::QueryPerformanceCounter(&mut qpc)?;
    let origin_hns =
        qpc as i128 * 10_000_000 / frequency as i128 - origin.elapsed().as_nanos() as i128 / 100;
    let _ = ready.try_send(Ok(()));
    let mut frames_written = first_frame;
    let mut previous_poll = Instant::now();
    let mut envelope = [0.0_f32; 2];
    while !stop.load(Ordering::Acquire) {
        let now = Instant::now();
        let mut backlogged = false;
        for source in &mut sources {
            source.diagnostics.max_poll_ms = source
                .diagnostics
                .max_poll_ms
                .max(now.duration_since(previous_poll).as_millis());
            backlogged |= source.poll(origin, origin_hns, stop)?;
        }
        let elapsed = now.duration_since(previous_poll).as_secs_f32();
        previous_poll = now;
        let mut peaks = [0_u16; 2];
        // Each source owns its clock and bounded PCM history. Mix only when
        // both cover the same frame range; idle sources synthesize silence.
        while !sources.is_empty()
            && sources.iter().all(|source| source.pending.len() >= 4096)
            && !stop.load(Ordering::Acquire)
        {
            let mut mixed = [0u8; 4096];
            for index in (0..4096).step_by(2) {
                let sample = crate::audio_timeline::mix_pcm16(sources.iter().map(|source| {
                    let sample =
                        i16::from_le_bytes([source.pending[index], source.pending[index + 1]]);
                    peaks[source.meter_index] =
                        peaks[source.meter_index].max(sample.unsigned_abs());
                    sample
                }));
                mixed[index..index + 2].copy_from_slice(&sample.to_le_bytes());
            }
            encoder.input(&mixed, frames_written * 10_000_000 / 48_000)?;
            for source in &mut sources {
                source.pending.drain(..4096);
                source.frames_written += 1024;
            }
            frames_written += 1024;
            encoder.output(sender)?;
        }
        for index in 0..2 {
            envelope[index] =
                crate::audio_timeline::meter_envelope(envelope[index], peaks[index], elapsed);
            levels[index].store(envelope[index].to_bits(), Ordering::Relaxed);
        }
        if !backlogged {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    encoder
        .transform
        .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0)?;
    encoder
        .transform
        .ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0)?;
    encoder.output(sender)?;
    Ok(())
}

struct Encoder {
    transform: IMFTransform,
}
impl Encoder {
    unsafe fn new() -> windows::core::Result<Self> {
        let transform: IMFTransform = CoCreateInstance(&AACMFTEncoder, None, CLSCTX_INPROC_SERVER)?;
        let input = MFCreateMediaType()?;
        input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        input.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
        input.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, 48_000)?;
        input.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2)?;
        input.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        input.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4)?;
        input.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 192_000)?;
        let output = MFCreateMediaType()?;
        output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        output.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
        output.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, 48_000)?;
        output.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2)?;
        output.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        output.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 16_000)?;
        output.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?;
        output.SetUINT32(&MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, 0x29)?;
        transform.SetOutputType(0, &output, 0)?;
        transform.SetInputType(0, &input, 0)?;
        transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
        transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
        Ok(Self { transform })
    }
    unsafe fn input(&self, pcm: &[u8], time: i64) -> windows::core::Result<()> {
        let buffer = MFCreateMemoryBuffer(pcm.len() as u32)?;
        let mut ptr = std::ptr::null_mut();
        buffer.Lock(&mut ptr, None, None)?;
        std::ptr::copy_nonoverlapping(pcm.as_ptr(), ptr, pcm.len());
        buffer.Unlock()?;
        buffer.SetCurrentLength(pcm.len() as u32)?;
        let sample = MFCreateSample()?;
        sample.AddBuffer(&buffer)?;
        sample.SetSampleTime(time)?;
        sample.SetSampleDuration(1024 * 10_000_000 / 48_000)?;
        self.transform.ProcessInput(0, &sample, 0)
    }
    unsafe fn output(&self, sender: &SyncSender<AudioPacket>) -> windows::core::Result<()> {
        for _ in 0..128 {
            let info = self.transform.GetOutputStreamInfo(0)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&MFCreateMemoryBuffer(info.cbSize.max(4096))?)?;
            let mut output = [MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: 0,
                pSample: std::mem::ManuallyDrop::new(Some(sample.clone())),
                dwStatus: 0,
                pEvents: std::mem::ManuallyDrop::new(None),
            }];
            let mut status = 0;
            let result = self.transform.ProcessOutput(0, &mut output, &mut status);
            let actual = std::mem::ManuallyDrop::take(&mut output[0].pSample);
            let _events = std::mem::ManuallyDrop::take(&mut output[0].pEvents);
            match result {
                Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(()),
                Err(e) => return Err(e),
                Ok(()) => {}
            }
            let sample = actual.unwrap_or(sample);
            let time = sample.GetSampleTime()?;
            let duration = sample.GetSampleDuration()?;
            let buffer = sample.ConvertToContiguousBuffer()?;
            let mut ptr = std::ptr::null_mut();
            let mut length = 0;
            buffer.Lock(&mut ptr, None, Some(&mut length))?;
            let data: Arc<[u8]> = Arc::from(std::slice::from_raw_parts(ptr, length as usize));
            buffer.Unlock()?;
            if !data.is_empty() {
                sender
                    .try_send(AudioPacket {
                        data,
                        time,
                        duration,
                    })
                    .map_err(|_| {
                        windows::core::Error::new(
                            windows::core::HRESULT(0x80004005u32 as i32),
                            "Encoded audio queue full or disconnected",
                        )
                    })?;
            }
        }
        Err(windows::core::Error::new(
            windows::core::HRESULT(0x80004005u32 as i32),
            "AAC output exceeded bounded drain",
        ))
    }
}
