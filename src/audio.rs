//! Default render endpoint loopback, converted by WASAPI to 48 kHz stereo PCM.
//! AAC setup adapted from ClipFlow src-tauri/src/media/audio.rs,
//! revision d204de138d503d2eb41fcb2af553aa2430f0a78b (MIT).
//! Copyright (c) 2026 jaimitus. See docs/CLIPFLOW-LICENSE.txt for the MIT license.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
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
}

impl AudioCapture {
    pub fn start(origin: Instant) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(None));
        let (sender, packets) = mpsc::sync_channel(128);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let stop_worker = stop.clone();
        let error_worker = error.clone();
        let worker = std::thread::Builder::new()
            .name("veyva-system-audio".into())
            .spawn(move || {
                let result = unsafe { run(origin, &stop_worker, &sender, &ready_tx) };
                if let Err(message) = result {
                    *error_worker.lock().unwrap_or_else(|e| e.into_inner()) = Some(message.clone());
                    let _ = ready_tx.try_send(Err(message));
                }
            })
            .map_err(|e| format!("Start system audio thread: {e}"))?;
        match ready_rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(())) => Ok(Self {
                stop,
                worker: Some(worker),
                packets,
                error,
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
                    _ => "System audio initialization timed out".into(),
                })
            }
        }
    }
    pub fn drain(&mut self) -> Vec<AudioPacket> {
        self.packets.try_iter().take(128).collect()
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
    stop: &AtomicBool,
    sender: &SyncSender<AudioPacket>,
    ready: &SyncSender<Result<(), String>>,
) -> Result<(), String> {
    CoInitializeEx(None, COINIT_MULTITHREADED)
        .ok()
        .map_err(|e| format!("Initialize audio COM: {e}"))?;
    if let Err(e) = MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET) {
        CoUninitialize();
        return Err(format!("Initialize audio Media Foundation: {e}"));
    }
    let _runtime = Runtime;
    let result = capture(origin, stop, sender, ready);
    result.map_err(|e| format!("System audio: {e}"))
}

unsafe fn capture(
    origin: Instant,
    stop: &AtomicBool,
    sender: &SyncSender<AudioPacket>,
    ready: &SyncSender<Result<(), String>>,
) -> windows::core::Result<()> {
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
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
        AUDCLNT_STREAMFLAGS_LOOPBACK
            | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
            | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
        200_000,
        0,
        &format,
        None,
    )?;
    let capture: IAudioCaptureClient = client.GetService()?;
    let encoder = Encoder::new()?;
    client.Start()?;
    let _endpoint = Endpoint(client);
    let mut qpc = 0i64;
    let mut frequency = 0i64;
    windows::Win32::System::Performance::QueryPerformanceFrequency(&mut frequency)?;
    windows::Win32::System::Performance::QueryPerformanceCounter(&mut qpc)?;
    let origin_hns =
        qpc as i128 * 10_000_000 / frequency as i128 - origin.elapsed().as_nanos() as i128 / 100;
    let _ = ready.try_send(Ok(()));
    let mut pending = Vec::<u8>::with_capacity(192_000);
    let mut frames_written = (origin.elapsed().as_nanos() * 48_000 / 1_000_000_000) as i64;
    while !stop.load(Ordering::Acquire) {
        // Bound both the PCM queue and work per poll, including on a broken driver.
        for _ in 0..32 {
            if stop.load(Ordering::Acquire) || capture.GetNextPacketSize()? == 0 {
                break;
            }
            let mut data = std::ptr::null_mut();
            let mut frames = 0;
            let mut flags = 0;
            let mut qpc_position = 0u64;
            capture.GetBuffer(
                &mut data,
                &mut frames,
                &mut flags,
                None,
                Some(&mut qpc_position),
            )?;
            // WASAPI QPC positions already use 100 ns units (not raw counter ticks).
            let valid_time = flags & AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32 == 0;
            let queued_end = frames_written + (pending.len() / 4) as i64;
            let target = if valid_time {
                (((qpc_position as i128 - origin_hns).max(0) * 48_000) / 10_000_000) as i64
            } else {
                queued_end
            };
            let gap = (target - queued_end).max(0) as usize;
            let trim = (queued_end - target).max(0).min(i64::from(frames)) as usize;
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
        // Loopback may deliver no packets at all while the render device is idle.
        // Fill only after a 50 ms grace period so later sound retains wall time.
        let elapsed_frames = (origin.elapsed().as_nanos() * 48_000 / 1_000_000_000) as i64;
        let missing = elapsed_frames - frames_written - (pending.len() / 4) as i64 - 2400;
        if missing > 0 {
            let fill = (missing as usize).min((192_000 - pending.len()) / 4);
            pending.resize(pending.len() + fill * 4, 0);
        }
        while pending.len() >= 4096 && !stop.load(Ordering::Acquire) {
            encoder.input(&pending[..4096], frames_written * 10_000_000 / 48_000)?;
            pending.drain(..4096);
            frames_written += 1024;
            encoder.output(sender)?;
        }
        std::thread::sleep(Duration::from_millis(5));
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
