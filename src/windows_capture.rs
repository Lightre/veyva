// GPU setup/conversion and MFT negotiation adapted from ClipFlow.
// Copyright (c) 2026 jaimitus. MIT; see docs/CLIPFLOW-LICENSE.txt.
use super::{RResult, RecorderError};
use crate::{diagnostics::Logger, settings::CaptureProfile};
use std::{
    os::windows::ffi::OsStrExt,
    path::Path,
    time::{Duration, Instant},
};
use windows::{
    core::{Interface, PCWSTR, PWSTR},
    Win32::{
        Foundation::{HMODULE, TRUE},
        Graphics::{
            Direct3D::{D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1},
            Direct3D11::*,
            Dxgi::{Common::*, *},
        },
        Media::MediaFoundation::*,
        System::Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED},
    },
};
const HNS_PER_SECOND: i64 = 10_000_000;
fn hr(e: windows::core::Error) -> RecorderError {
    RecorderError::Other(format!("{} (0x{:08X})", e.message(), e.code().0 as u32))
}

struct MfSession;
impl MfSession {
    fn new() -> RResult<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(hr)?;
            if let Err(e) = MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET) {
                CoUninitialize();
                return Err(hr(e));
            }
            Ok(Self)
        }
    }
}
impl Drop for MfSession {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Codec {
    H264,
}
#[derive(Clone, Debug)]
struct EncoderInfo {
    friendly_name: String,
}
struct VideoFormatHeader {
    width: u32,
    height: u32,
    fps_num: u32,
    fps_den: u32,
    bitrate: u32,
    profile: u32,
    sequence_header: Vec<u8>,
}
struct GraphicsStack {
    pub device: ID3D11Device,
    pub context: ID3D11DeviceContext,
    pub duplication: IDXGIOutputDuplication,
    pub adapter_name: String,
    pub vram_mb: u64,
    pub src_width: u32,
    pub src_height: u32,
}

impl GraphicsStack {
    fn create(monitor_index: u32) -> RResult<Self> {
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1().map_err(hr)?;

            // Walk adapters/outputs to find the requested monitor. Index is
            // flattened across adapters so a 2-GPU laptop behaves sanely.
            let mut flat = 0u32;
            let mut adapter_idx = 0u32;
            loop {
                let adapter: IDXGIAdapter1 = match factory.EnumAdapters1(adapter_idx) {
                    Ok(a) => a,
                    Err(_) => break,
                };
                let adesc = adapter.GetDesc1().map_err(hr)?;

                // Skip the Microsoft Basic Render Driver.
                if (adesc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 {
                    adapter_idx += 1;
                    continue;
                }

                let mut output_idx = 0u32;
                loop {
                    let output: IDXGIOutput = match adapter.EnumOutputs(output_idx) {
                        Ok(o) => o,
                        Err(_) => break,
                    };
                    if flat == monitor_index {
                        return Self::build(&adapter, &adesc, &output);
                    }
                    flat += 1;
                    output_idx += 1;
                }
                adapter_idx += 1;
            }
            Err(RecorderError::NoSuchMonitor(monitor_index))
        }
    }

    unsafe fn build(
        adapter: &IDXGIAdapter1,
        adesc: &DXGI_ADAPTER_DESC1,
        output: &IDXGIOutput,
    ) -> RResult<Self> {
        let levels = [D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0];
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        let mut level = D3D_FEATURE_LEVEL::default();

        D3D11CreateDevice(
            adapter,
            // Must be UNKNOWN when an adapter is supplied.
            windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
            Some(&levels),
            D3D11_SDK_VERSION,
            Some(&mut device),
            Some(&mut level),
            Some(&mut context),
        )
        .map_err(hr)?;

        let device = device.ok_or_else(|| RecorderError::Other("no d3d11 device".into()))?;
        let context = context.ok_or_else(|| RecorderError::Other("no d3d11 context".into()))?;

        // Desktop Duplication + MF both call into the device from several
        // threads; without this the driver will happily corrupt itself.
        let mt: ID3D11Multithread = device.cast().map_err(hr)?;
        let _ = mt.SetMultithreadProtected(TRUE);

        let output1: IDXGIOutput1 = output.cast().map_err(hr)?;
        let duplication = output1.DuplicateOutput(&device).map_err(|e| {
                if e.code() == DXGI_ERROR_UNSUPPORTED {
                    RecorderError::Other(
                        "Desktop Duplication is unavailable on this output (hybrid-GPU: run ClipFlow on the GPU driving the display)".into(),
                    )
                } else {
                    hr(e)
                }
            })?;

        let desc = duplication.GetDesc();
        if desc.Rotation != DXGI_MODE_ROTATION_IDENTITY
            || desc.ModeDesc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
        {
            return Err(RecorderError::Other(
                "Rotated or non-BGRA displays are not supported in this milestone".into(),
            ));
        }

        let adapter_name = String::from_utf16_lossy(
            &adesc.Description[..adesc
                .Description
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(adesc.Description.len())],
        );

        let _ = adapter; // adapter kept alive through `device`

        Ok(Self {
            src_width: desc.ModeDesc.Width,
            src_height: desc.ModeDesc.Height,
            device,
            context,
            duplication,
            adapter_name,
            vram_mb: (adesc.DedicatedVideoMemory / (1024 * 1024)) as u64,
        })
    }
}

// --------------------------------------------- BGRA ➜ NV12 on the GPU
struct ColorConverter {
    vdevice: ID3D11VideoDevice,
    vcontext: ID3D11VideoContext,
    processor: ID3D11VideoProcessor,
    enumerator: ID3D11VideoProcessorEnumerator,
    pub nv12: ID3D11Texture2D,
    out_view: ID3D11VideoProcessorOutputView,
    _dst_w: u32,
    _dst_h: u32,
}

impl ColorConverter {
    fn new(
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        src_w: u32,
        src_h: u32,
        dst_w: u32,
        dst_h: u32,
        fps: u32,
    ) -> RResult<Self> {
        unsafe {
            let vdevice: ID3D11VideoDevice = device.cast().map_err(hr)?;
            let vcontext: ID3D11VideoContext = context.cast().map_err(hr)?;

            let content_desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
                InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
                InputFrameRate: DXGI_RATIONAL {
                    Numerator: fps,
                    Denominator: 1,
                },
                InputWidth: src_w,
                InputHeight: src_h,
                OutputFrameRate: DXGI_RATIONAL {
                    Numerator: fps,
                    Denominator: 1,
                },
                OutputWidth: dst_w,
                OutputHeight: dst_h,
                Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
            };

            let enumerator = vdevice
                .CreateVideoProcessorEnumerator(&content_desc)
                .map_err(hr)?;
            let processor = vdevice.CreateVideoProcessor(&enumerator, 0).map_err(hr)?;

            // Game capture is full-range RGB going to limited-range NV12;
            // getting this wrong is the classic "washed out clip" bug.
            let in_cs = D3D11_VIDEO_PROCESSOR_COLOR_SPACE {
                _bitfield: 0, // Full-range RGB input.
            };
            let out_cs = D3D11_VIDEO_PROCESSOR_COLOR_SPACE {
                _bitfield: 0x14, // BT.709, limited-range NV12 output.
            };
            vcontext.VideoProcessorSetStreamColorSpace(&processor, 0, &in_cs);
            vcontext.VideoProcessorSetOutputColorSpace(&processor, &out_cs);
            vcontext.VideoProcessorSetStreamFrameFormat(
                &processor,
                0,
                D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            );

            let nv12_desc = D3D11_TEXTURE2D_DESC {
                Width: dst_w,
                Height: dst_h,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_NV12,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
            };
            let mut nv12: Option<ID3D11Texture2D> = None;
            device
                .CreateTexture2D(&nv12_desc, None, Some(&mut nv12))
                .map_err(hr)?;
            let nv12 = nv12.ok_or_else(|| RecorderError::Other("no nv12 texture".into()))?;

            let ovd = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
                },
            };
            let mut out_view: Option<ID3D11VideoProcessorOutputView> = None;
            vdevice
                .CreateVideoProcessorOutputView(&nv12, &enumerator, &ovd, Some(&mut out_view))
                .map_err(hr)?;
            let out_view =
                out_view.ok_or_else(|| RecorderError::Other("no vp output view".into()))?;

            Ok(Self {
                vdevice,
                vcontext,
                processor,
                enumerator,
                nv12,
                out_view,
                _dst_w: dst_w,
                _dst_h: dst_h,
            })
        }
    }

    /// One `VideoProcessorBlt` = colour convert + scale + (optional) crop.
    fn convert(&self, src: &ID3D11Texture2D) -> RResult<()> {
        unsafe {
            let ivd = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                FourCC: 0,
                ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPIV {
                        MipSlice: 0,
                        ArraySlice: 0,
                    },
                },
            };
            let mut in_view: Option<ID3D11VideoProcessorInputView> = None;
            self.vdevice
                .CreateVideoProcessorInputView(src, &self.enumerator, &ivd, Some(&mut in_view))
                .map_err(hr)?;
            let in_view = in_view.ok_or_else(|| RecorderError::Other("no vp input view".into()))?;

            let mut streams = [D3D11_VIDEO_PROCESSOR_STREAM {
                Enable: TRUE,
                OutputIndex: 0,
                InputFrameOrField: 0,
                PastFrames: 0,
                FutureFrames: 0,
                pInputSurface: std::mem::ManuallyDrop::new(Some(in_view)),
                ..Default::default()
            }];

            let result =
                self.vcontext
                    .VideoProcessorBlt(&self.processor, &self.out_view, 0, &streams);
            std::mem::ManuallyDrop::drop(&mut streams[0].pInputSurface);

            result.map_err(hr)
        }
    }
}

struct HardwareEncoder {
    transform: IMFTransform,
    events: Option<IMFMediaEventGenerator>,
    _dxgi_manager: IMFDXGIDeviceManager,
    info: EncoderInfo,
    header: VideoFormatHeader,
    async_mode: bool,
    pending_input: u32,
    fps: u32,
    pending_output: u32,
    drain_complete: bool,
}

impl HardwareEncoder {
    fn create(
        device: &ID3D11Device,
        width: u32,
        height: u32,
        fps: u32,
        bitrate_kbps: u32,
        codec: Codec,
    ) -> RResult<Self> {
        unsafe {
            let subtype = match codec {
                Codec::H264 => MFVideoFormat_H264,
            };

            // Enumerate hardware encoders. Device-manager negotiation rejects incompatible devices.
            let in_info = MFT_REGISTER_TYPE_INFO {
                guidMajorType: MFMediaType_Video,
                guidSubtype: MFVideoFormat_NV12,
            };
            let out_info = MFT_REGISTER_TYPE_INFO {
                guidMajorType: MFMediaType_Video,
                guidSubtype: subtype,
            };

            let dxgi: IDXGIDevice = device.cast().map_err(hr)?;
            let luid = dxgi
                .GetAdapter()
                .map_err(hr)?
                .GetDesc()
                .map_err(hr)?
                .AdapterLuid;
            let mut enumeration_attrs = None;
            MFCreateAttributes(&mut enumeration_attrs, 1).map_err(hr)?;
            let enumeration_attrs = enumeration_attrs.unwrap();
            let luid_bytes =
                ((u64::from(luid.HighPart as u32) << 32) | u64::from(luid.LowPart)).to_le_bytes();
            enumeration_attrs
                .SetBlob(&MFT_ENUM_ADAPTER_LUID, &luid_bytes)
                .map_err(hr)?;
            let mut activates: *mut Option<IMFActivate> = std::ptr::null_mut();
            let mut count: u32 = 0;
            MFTEnum2(
                MFT_CATEGORY_VIDEO_ENCODER,
                MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
                Some(&in_info),
                Some(&out_info),
                &enumeration_attrs,
                &mut activates,
                &mut count,
            )
            .map_err(hr)?;

            let mut chosen: Option<(IMFActivate, String)> = None;
            if count > 0 && !activates.is_null() {
                let slice = std::slice::from_raw_parts_mut(activates, count as usize);
                for act in slice.iter().flatten() {
                    let mut name = PWSTR::null();
                    let mut len = 0u32;
                    let friendly = if act
                        .GetAllocatedString(&MFT_FRIENDLY_NAME_Attribute, &mut name, &mut len)
                        .is_ok()
                    {
                        let s = name.to_string().unwrap_or_default();
                        windows::Win32::System::Com::CoTaskMemFree(Some(
                            name.0 as *const std::ffi::c_void,
                        ));
                        s
                    } else {
                        "Hardware Encoder".to_string()
                    };
                    if chosen.is_none() {
                        chosen = Some((act.clone(), friendly));
                    }
                }
                for entry in slice {
                    drop(entry.take());
                }
                CoTaskMemFree(Some(activates.cast()));
            }

            let (act, friendly) = chosen.ok_or(RecorderError::NoEncoder)?;
            let transform: IMFTransform = act.ActivateObject().map_err(hr)?;

            // 3. Async unlock + low latency BEFORE any type negotiation.
            let attrs = transform.GetAttributes().ok();
            let mut async_mode = false;
            if let Some(a) = attrs.as_ref() {
                if a.GetUINT32(&MF_TRANSFORM_ASYNC).unwrap_or(0) == 1 {
                    a.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1).map_err(hr)?;
                    async_mode = true;
                }
                let _ = a.SetUINT32(&MF_LOW_LATENCY, 1);
                // Query D3D11 support; this attribute is read-only.
                if a.GetUINT32(&MF_SA_D3D11_AWARE).unwrap_or(0) == 0 {
                    return Err(RecorderError::Other(
                        "Hardware encoder does not support D3D11 surface input".into(),
                    ));
                }
            }

            // 4. Bind our D3D11 device so the MFT reads the NV12 texture in
            //    place (true zero-copy; no staging, no PCIe round trip).
            let mut reset_token = 0u32;
            let mut manager: Option<IMFDXGIDeviceManager> = None;
            MFCreateDXGIDeviceManager(&mut reset_token, &mut manager).map_err(hr)?;
            let manager = manager.ok_or_else(|| RecorderError::Other("no dxgi manager".into()))?;
            manager.ResetDevice(device, reset_token).map_err(hr)?;
            transform
                .ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, manager.as_raw() as usize)
                .map_err(hr)?;

            // 5. Output type first - that is the MFT contract.
            let out_type = MFCreateMediaType().map_err(hr)?;
            out_type
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(hr)?;
            out_type.SetGUID(&MF_MT_SUBTYPE, &subtype).map_err(hr)?;
            out_type
                .SetUINT32(&MF_MT_AVG_BITRATE, bitrate_kbps * 1000)
                .map_err(hr)?;
            out_type
                .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                .map_err(hr)?;
            out_type
                .SetUINT32(&MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32)
                .map_err(hr)?;
            out_type
                .SetUINT32(&MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32)
                .map_err(hr)?;
            out_type
                .SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)
                .map_err(hr)?;
            out_type
                .SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)
                .map_err(hr)?;
            set_attr_size(&out_type, &MF_MT_FRAME_SIZE, width, height)?;
            set_attr_ratio(&out_type, &MF_MT_FRAME_RATE, fps, 1)?;
            set_attr_ratio(&out_type, &MF_MT_PIXEL_ASPECT_RATIO, 1, 1)?;
            if codec == Codec::H264 {
                out_type
                    .SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32)
                    .map_err(hr)?;
            }
            // 1 key frame per second: fine-grained ring eviction + fast seek.
            let _ = out_type.SetUINT32(&MF_MT_MAX_KEYFRAME_SPACING, fps);
            transform.SetOutputType(0, &out_type, 0).map_err(hr)?;

            // 6. Input type: NV12 straight from the video processor.
            let in_type = MFCreateMediaType().map_err(hr)?;
            in_type
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(hr)?;
            in_type
                .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)
                .map_err(hr)?;
            in_type
                .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                .map_err(hr)?;
            in_type
                .SetUINT32(&MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32)
                .map_err(hr)?;
            in_type
                .SetUINT32(&MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32)
                .map_err(hr)?;
            in_type
                .SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)
                .map_err(hr)?;
            in_type
                .SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)
                .map_err(hr)?;
            set_attr_size(&in_type, &MF_MT_FRAME_SIZE, width, height)?;
            set_attr_ratio(&in_type, &MF_MT_FRAME_RATE, fps, 1)?;
            set_attr_ratio(&in_type, &MF_MT_PIXEL_ASPECT_RATIO, 1, 1)?;
            transform.SetInputType(0, &in_type, 0).map_err(hr)?;

            // 7. Rate control - CBR keeps VBV predictable, which keeps the
            //    ring buffer's memory ceiling predictable.
            if let Ok(codec_api) = transform.cast::<ICodecAPI>() {
                let _ = set_codec_u32(&codec_api, &CODECAPI_AVEncCommonRateControlMode, 0);
                let _ = set_codec_u32(
                    &codec_api,
                    &CODECAPI_AVEncCommonMeanBitRate,
                    bitrate_kbps * 1000,
                );
                let _ = set_codec_u32(&codec_api, &CODECAPI_AVEncCommonLowLatency, 1);
                let _ = set_codec_u32(&codec_api, &CODECAPI_AVEncMPVGOPSize, fps);
                let _ = set_codec_u32(&codec_api, &CODECAPI_AVEncVideoMaxNumRefFrame, 1);
                let _ = set_codec_u32(&codec_api, &CODECAPI_AVEncMPVDefaultBPictureCount, 0);
            }

            let events: Option<IMFMediaEventGenerator> = if async_mode {
                Some(transform.cast().map_err(hr)?)
            } else {
                None
            };

            transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
                .map_err(hr)?;
            transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
                .map_err(hr)?;

            // Grab SPS/PPS for the muxer (present after type negotiation).
            let negotiated = transform.GetOutputCurrentType(0).map_err(hr)?;
            let mut sequence_header = Vec::new();
            if let Ok(size) = negotiated.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER) {
                if size > 0 {
                    sequence_header.resize(size as usize, 0u8);
                    let _ =
                        negotiated.GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut sequence_header, None);
                }
            }

            Ok(Self {
                transform,
                events,
                _dxgi_manager: manager,
                info: EncoderInfo {
                    friendly_name: friendly,
                },
                header: VideoFormatHeader {
                    width,
                    height,
                    fps_num: fps,
                    fps_den: 1,
                    bitrate: bitrate_kbps * 1000,
                    profile: eAVEncH264VProfile_High.0 as u32,
                    sequence_header,
                },
                async_mode,
                pending_input: 0,
                fps,
                pending_output: 0,
                drain_complete: false,
            })
        }
    }

    // All event reads are nonblocking. Saturation is reported as a dropped frame.
    unsafe fn pump_events(&mut self) -> RResult<()> {
        let Some(events) = self.events.as_ref() else {
            return Ok(());
        };
        for _ in 0..128 {
            let event = match events.GetEvent(MF_EVENT_FLAG_NO_WAIT) {
                Ok(event) => event,
                Err(e) if e.code() == MF_E_NO_EVENTS_AVAILABLE => return Ok(()),
                Err(e) => return Err(hr(e)),
            };
            event.GetStatus().map_err(hr)?.ok().map_err(hr)?;
            match event.GetType().map_err(hr)? {
                601 => self.pending_input = self.pending_input.saturating_add(1),
                602 => self.pending_output = self.pending_output.saturating_add(1),
                603 => self.drain_complete = true,
                _ => {}
            }
        }
        Err(RecorderError::Other(
            "Encoder event queue exceeded its safety limit".into(),
        ))
    }

    fn submit(&mut self, sample: &IMFSample) -> RResult<bool> {
        unsafe {
            self.pump_events()?;
            if self.async_mode && self.pending_input == 0 {
                return Ok(false);
            }
            match self.transform.ProcessInput(0, sample, 0) {
                Ok(()) => {
                    if self.async_mode {
                        self.pending_input -= 1;
                    }
                    Ok(true)
                }
                Err(e) if e.code() == MF_E_NOTACCEPTING => Ok(false),
                Err(e) => Err(hr(e)),
            }
        }
    }

    fn drain(&mut self, packets: &mut Packets) -> RResult<()> {
        unsafe {
            self.pump_events()?;
            for _ in 0..128 {
                if self.async_mode && self.pending_output == 0 {
                    return Ok(());
                }
                let info = self.transform.GetOutputStreamInfo(0).map_err(hr)?;
                let supplied = if info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 == 0 {
                    let sample = MFCreateSample().map_err(hr)?;
                    let buffer = MFCreateMemoryBuffer(info.cbSize.max(1024 * 1024)).map_err(hr)?;
                    sample.AddBuffer(&buffer).map_err(hr)?;
                    Some(sample)
                } else {
                    None
                };
                let mut output = [MFT_OUTPUT_DATA_BUFFER {
                    pSample: std::mem::ManuallyDrop::new(supplied),
                    ..Default::default()
                }];
                let mut status = 0;
                let result = self.transform.ProcessOutput(0, &mut output, &mut status);
                let sample = std::mem::ManuallyDrop::take(&mut output[0].pSample);
                drop(std::mem::ManuallyDrop::take(&mut output[0].pEvents));
                match result {
                    Ok(()) => {
                        if self.async_mode {
                            self.pending_output -= 1;
                        }
                        if let Some(sample) = sample {
                            packets.push(sample, self.fps)?;
                        }
                    }
                    Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                        let media_type = self.transform.GetOutputAvailableType(0, 0).map_err(hr)?;
                        self.transform
                            .SetOutputType(0, &media_type, 0)
                            .map_err(hr)?;
                    }
                    Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => {
                        self.pending_output = 0;
                        return Ok(());
                    }
                    Err(e) => return Err(hr(e)),
                }
            }
            Err(RecorderError::Other(
                "Encoder output exceeded its safety limit".into(),
            ))
        }
    }

    fn finish(&mut self, packets: &mut Packets) -> RResult<()> {
        unsafe {
            self.transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0)
                .map_err(hr)?;
            self.transform
                .ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0)
                .map_err(hr)?;
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            self.drain(packets)?;
            if !self.async_mode || self.drain_complete {
                break;
            }
            if Instant::now() >= deadline {
                return Err(RecorderError::Other("Encoder drain timed out".into()));
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        unsafe {
            let media_type = self.transform.GetOutputCurrentType(0).map_err(hr)?;
            if let Ok(size) = media_type.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER) {
                if size > 1024 * 1024 {
                    return Err(RecorderError::Other("Invalid codec header size".into()));
                }
                self.header.sequence_header.resize(size as usize, 0);
                media_type
                    .GetBlob(
                        &MF_MT_MPEG_SEQUENCE_HEADER,
                        &mut self.header.sequence_header,
                        None,
                    )
                    .map_err(hr)?;
            }
        }
        Ok(())
    }
}

impl Drop for HardwareEncoder {
    fn drop(&mut self) {
        unsafe {
            let _ = self.transform.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
            let _ = self
                .transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
        }
    }
}

fn set_attr_size(t: &IMFMediaType, key: &windows::core::GUID, a: u32, b: u32) -> RResult<()> {
    unsafe { t.SetUINT64(key, ((a as u64) << 32) | b as u64).map_err(hr) }
}
fn set_attr_ratio(t: &IMFMediaType, key: &windows::core::GUID, a: u32, b: u32) -> RResult<()> {
    set_attr_size(t, key, a, b)
}
unsafe fn set_codec_u32(
    api: &ICodecAPI,
    key: &windows::core::GUID,
    value: u32,
) -> windows::core::Result<()> {
    api.SetValue(key, &windows::core::VARIANT::from(value))
}

// Retain encoded samples only, with an explicit short-recording byte ceiling.
struct Packets {
    samples: Vec<IMFSample>,
    bytes: u64,
    limit: u64,
}
impl Packets {
    fn push(&mut self, sample: IMFSample, fps: u32) -> RResult<()> {
        unsafe {
            let size = u64::from(sample.GetTotalLength().map_err(hr)?);
            if self.bytes + size > self.limit || self.samples.len() >= 3600 {
                return Err(RecorderError::Other(
                    "Short-recording payload budget exceeded".into(),
                ));
            }
            // No B frames: presentation order must match submission order.
            let pts = sample.GetSampleTime().map_err(hr)?;
            if self
                .samples
                .last()
                .is_some_and(|last| last.GetSampleTime().unwrap_or(-1) >= pts)
            {
                return Err(RecorderError::Other(
                    "Encoder produced reordered timestamps; B frames are unsupported".into(),
                ));
            }
            if sample.GetSampleDuration().unwrap_or(0) <= 0 {
                sample
                    .SetSampleDuration(HNS_PER_SECOND / i64::from(fps))
                    .map_err(hr)?;
            }
            // Encoder-owned samples can retain oversized buffers or a finite output
            // pool. Keep only a compact payload copy, releasing that ownership now.
            let buffer = MFCreateMemoryBuffer(size as u32).map_err(hr)?;
            sample.CopyToBuffer(&buffer).map_err(hr)?;
            let retained = MFCreateSample().map_err(hr)?;
            sample.CopyAllItems(&retained).map_err(hr)?;
            retained.SetSampleTime(pts).map_err(hr)?;
            retained
                .SetSampleDuration(sample.GetSampleDuration().map_err(hr)?)
                .map_err(hr)?;
            retained
                .SetSampleFlags(sample.GetSampleFlags().map_err(hr)?)
                .map_err(hr)?;
            retained.AddBuffer(&buffer).map_err(hr)?;
            self.bytes += size;
            self.samples.push(retained);
            Ok(())
        }
    }
}

fn allocator(
    manager: &IMFDXGIDeviceManager,
    profile: CaptureProfile,
) -> RResult<IMFVideoSampleAllocatorEx> {
    unsafe {
        let mut raw = std::ptr::null_mut();
        MFCreateVideoSampleAllocatorEx(&IMFVideoSampleAllocatorEx::IID, &mut raw).map_err(hr)?;
        let allocator = IMFVideoSampleAllocatorEx::from_raw(raw);
        allocator.SetDirectXManager(manager).map_err(hr)?;
        let media_type = MFCreateMediaType().map_err(hr)?;
        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(hr)?;
        media_type
            .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)
            .map_err(hr)?;
        media_type
            .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .map_err(hr)?;
        set_attr_size(
            &media_type,
            &MF_MT_FRAME_SIZE,
            profile.width,
            profile.height,
        )?;
        let mut attrs = None;
        MFCreateAttributes(&mut attrs, 2).map_err(hr)?;
        let attrs = attrs.unwrap();
        attrs
            .SetUINT32(&MF_SA_D3D11_BINDFLAGS, D3D11_BIND_RENDER_TARGET.0 as u32)
            .map_err(hr)?;
        allocator
            .InitializeSampleAllocatorEx(2, 4, &attrs, &media_type)
            .map_err(hr)?;
        Ok(allocator)
    }
}

const MAX_INTERVAL_REPORTS: usize = 64;

#[derive(Clone, Copy, Default)]
struct CallTiming {
    sum: Duration,
    max: Duration,
}

impl CallTiming {
    fn record(&mut self, elapsed: Duration) {
        self.sum += elapsed;
        self.max = self.max.max(elapsed);
    }
}

#[derive(Clone, Copy, Default)]
struct IntervalTimings {
    sleep_wait: CallTiming,
    encoder_drain: CallTiming,
    acquire_convert_release: CallTiming,
    allocator_copy: CallTiming,
    encoder_submit: CallTiming,
}

#[derive(Clone, Copy)]
struct IntervalReport {
    start_ms: u128,
    end_ms: u128,
    submitted: u64,
    encoded: usize,
    dropped: u64,
    scheduler_late: u64,
    no_frame: u64,
    allocator_empty: u64,
    encoder_busy: u64,
    timings: IntervalTimings,
}

#[derive(Default)]
struct IntervalReports {
    reports: Vec<IntervalReport>,
}

impl IntervalReports {
    fn push(&mut self, report: IntervalReport) {
        if self.reports.len() < MAX_INTERVAL_REPORTS {
            self.reports.push(report);
        }
    }

    fn flush(&self, log: &mut Logger) -> RResult<()> {
        for report in &self.reports {
            let t = report.timings;
            log.write(&format!(
                "Capture interval: start_ms={}, end_ms={}, submitted={}, encoded={}, dropped={}, scheduler_late={}, no_frame={}, allocator_empty={}, encoder_busy={}, call_wall_ms_sum/max(sleep_wait)={}/{}, call_wall_ms_sum/max(encoder_drain)={}/{}, call_wall_ms_sum/max(acquire_convert_release)={}/{}, call_wall_ms_sum/max(allocator_copy)={}/{}, call_wall_ms_sum/max(encoder_submit)={}/{}; CPU call wall time, GPU execution may continue asynchronously",
                report.start_ms, report.end_ms, report.submitted, report.encoded,
                report.dropped, report.scheduler_late, report.no_frame,
                report.allocator_empty, report.encoder_busy,
                t.sleep_wait.sum.as_micros() as f64 / 1000.0, t.sleep_wait.max.as_micros() as f64 / 1000.0,
                t.encoder_drain.sum.as_micros() as f64 / 1000.0, t.encoder_drain.max.as_micros() as f64 / 1000.0,
                t.acquire_convert_release.sum.as_micros() as f64 / 1000.0, t.acquire_convert_release.max.as_micros() as f64 / 1000.0,
                t.allocator_copy.sum.as_micros() as f64 / 1000.0, t.allocator_copy.max.as_micros() as f64 / 1000.0,
                t.encoder_submit.sum.as_micros() as f64 / 1000.0, t.encoder_submit.max.as_micros() as f64 / 1000.0,
            ))?;
        }
        Ok(())
    }
}

fn mux(path: &Path, header: &VideoFormatHeader, packets: &Packets) -> RResult<()> {
    unsafe {
        let first = packets
            .samples
            .first()
            .ok_or_else(|| RecorderError::Other("No encoded frames received".into()))?;
        if first.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) == 0 {
            return Err(RecorderError::Other(
                "Recording does not start with a keyframe".into(),
            ));
        }
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut attrs = None;
        MFCreateAttributes(&mut attrs, 2).map_err(hr)?;
        let attrs = attrs.unwrap();
        attrs
            .SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4)
            .map_err(hr)?;
        attrs
            .SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)
            .map_err(hr)?;
        let writer = MFCreateSinkWriterFromURL(PCWSTR(wide.as_ptr()), None, &attrs).map_err(hr)?;
        let media_type = MFCreateMediaType().map_err(hr)?;
        media_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(hr)?;
        media_type
            .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)
            .map_err(hr)?;
        media_type
            .SetUINT32(&MF_MT_AVG_BITRATE, header.bitrate)
            .map_err(hr)?;
        media_type
            .SetUINT32(&MF_MT_MPEG2_PROFILE, header.profile)
            .map_err(hr)?;
        media_type
            .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .map_err(hr)?;
        media_type
            .SetUINT32(&MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32)
            .map_err(hr)?;
        media_type
            .SetUINT32(&MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32)
            .map_err(hr)?;
        media_type
            .SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)
            .map_err(hr)?;
        media_type
            .SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)
            .map_err(hr)?;
        set_attr_size(&media_type, &MF_MT_FRAME_SIZE, header.width, header.height)?;
        set_attr_ratio(
            &media_type,
            &MF_MT_FRAME_RATE,
            header.fps_num,
            header.fps_den,
        )?;
        set_attr_ratio(&media_type, &MF_MT_PIXEL_ASPECT_RATIO, 1, 1)?;
        if !header.sequence_header.is_empty() {
            media_type
                .SetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &header.sequence_header)
                .map_err(hr)?;
        }
        let stream = writer.AddStream(&media_type).map_err(hr)?;
        writer
            .SetInputMediaType(stream, &media_type, None)
            .map_err(hr)?;
        writer.BeginWriting().map_err(hr)?;
        let base = first.GetSampleTime().map_err(hr)?;
        for sample in &packets.samples {
            sample
                .SetSampleTime(sample.GetSampleTime().map_err(hr)? - base)
                .map_err(hr)?;
            writer.WriteSample(stream, sample).map_err(hr)?;
        }
        writer.Finalize().map_err(hr)?;
        Ok(())
    }
}

pub fn run(
    seconds: u32,
    monitor: u32,
    output: Option<&Path>,
    probe: bool,
    log: &mut Logger,
) -> RResult<()> {
    let _session = MfSession::new()?;
    let gfx = GraphicsStack::create(monitor)?;
    log.write(&format!(
        "Adapter: {}; VRAM: {} MiB; display: {} ({}x{})",
        gfx.adapter_name, gfx.vram_mb, monitor, gfx.src_width, gfx.src_height
    ))?;
    let profile = CaptureProfile::low_end();
    let mut encoder = HardwareEncoder::create(
        &gfx.device,
        profile.width,
        profile.height,
        profile.fps,
        profile.video_kbps,
        Codec::H264,
    )?;
    log.write(&format!("Hardware encoder: {}", encoder.info.friendly_name))?;
    if probe {
        return Ok(());
    }
    let converter = ColorConverter::new(
        &gfx.device,
        &gfx.context,
        gfx.src_width,
        gfx.src_height,
        profile.width,
        profile.height,
        profile.fps,
    )?;
    let pool = allocator(&encoder._dxgi_manager, profile)?;
    let mut packets = Packets {
        samples: Vec::new(),
        bytes: 0,
        limit: u64::from(profile.replay_budget_mib) * 1024 * 1024,
    };
    let interval = Duration::from_secs_f64(1.0 / f64::from(profile.fps));
    let mut submitted = 0u64;
    let mut dropped = 0u64;
    let mut scheduler_late = 0u64;
    let mut no_frame = 0u64;
    let mut allocator_empty = 0u64;
    let mut encoder_busy = 0u64;
    let mut unchanged = 0u64;
    let mut reported_submitted = 0u64;
    let mut reported_encoded = 0usize;
    let mut reported_scheduler_late = 0u64;
    let mut reported_no_frame = 0u64;
    let mut reported_allocator_empty = 0u64;
    let mut reported_encoder_busy = 0u64;
    let mut interval_timings = IntervalTimings::default();
    let mut interval_reports = IntervalReports::default();
    let mut have_frame = false;
    let mut frame_slot = 0u64;
    log.write(&format!(
        "Capture started: {} seconds, {}x{} at {} FPS, silent H.264",
        seconds, profile.width, profile.height, profile.fps
    ))?;
    let start = Instant::now();
    let mut deadline = start;
    let mut report_start = start;
    let capture_result = (|| -> RResult<()> {
        while start.elapsed() < Duration::from_secs(u64::from(seconds)) {
            let report_elapsed = report_start.elapsed();
            if report_elapsed >= Duration::from_secs(1) {
                let encoded = packets.samples.len();
                let end_ms = start.elapsed().as_millis();
                interval_reports.push(IntervalReport {
                    start_ms: end_ms.saturating_sub(report_elapsed.as_millis()),
                    end_ms,
                    submitted: submitted - reported_submitted,
                    encoded: encoded - reported_encoded,
                    dropped: scheduler_late - reported_scheduler_late + no_frame
                        - reported_no_frame
                        + allocator_empty
                        - reported_allocator_empty
                        + encoder_busy
                        - reported_encoder_busy,
                    scheduler_late: scheduler_late - reported_scheduler_late,
                    no_frame: no_frame - reported_no_frame,
                    allocator_empty: allocator_empty - reported_allocator_empty,
                    encoder_busy: encoder_busy - reported_encoder_busy,
                    timings: interval_timings,
                });
                report_start = Instant::now();
                interval_timings = IntervalTimings::default();
                reported_submitted = submitted;
                reported_encoded = encoded;
                reported_scheduler_late = scheduler_late;
                reported_no_frame = no_frame;
                reported_allocator_empty = allocator_empty;
                reported_encoder_busy = encoder_busy;
            }
            if let Some(delay) = deadline.checked_duration_since(Instant::now()) {
                let call_start = Instant::now();
                std::thread::sleep(delay);
                interval_timings.sleep_wait.record(call_start.elapsed());
            }
            if start.elapsed() >= Duration::from_secs(u64::from(seconds)) {
                break;
            }
            let now = Instant::now();
            if now.duration_since(deadline) >= interval {
                let skipped =
                    (now.duration_since(deadline).as_nanos() / interval.as_nanos()) as u64;
                dropped += skipped;
                scheduler_late += skipped;
                frame_slot += skipped;
                deadline += interval * skipped as u32;
            }
            deadline += interval;
            let slot = frame_slot;
            frame_slot += 1;
            let call_start = Instant::now();
            encoder.drain(&mut packets)?;
            interval_timings.encoder_drain.record(call_start.elapsed());
            unsafe {
                let call_start = Instant::now();
                let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
                let mut resource = None;
                match gfx
                    .duplication
                    .AcquireNextFrame(0, &mut frame_info, &mut resource)
                {
                    Ok(()) => {
                        // Convert before releasing the borrowed desktop surface. Only our NV12 output is retained.
                        let converted = (|| {
                            let resource = resource.ok_or_else(|| {
                                RecorderError::Other("Empty desktop frame".into())
                            })?;
                            let texture: ID3D11Texture2D = resource.cast().map_err(hr)?;
                            converter.convert(&texture)
                        })();
                        let released = gfx.duplication.ReleaseFrame().map_err(hr);
                        converted?;
                        released?;
                        have_frame = true;
                    }
                    Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => {
                        unchanged += 1;
                    }
                    Err(e) => return Err(hr(e)),
                }
                interval_timings
                    .acquire_convert_release
                    .record(call_start.elapsed());
                if !have_frame {
                    dropped += 1;
                    no_frame += 1;
                    continue;
                }
                let call_start = Instant::now();
                let sample = match pool.AllocateSample() {
                    Ok(sample) => sample,
                    Err(e) if e.code() == MF_E_SAMPLEALLOCATOR_EMPTY => {
                        interval_timings.allocator_copy.record(call_start.elapsed());
                        dropped += 1;
                        allocator_empty += 1;
                        continue;
                    }
                    Err(e) => return Err(hr(e)),
                };
                let buffer = sample.GetBufferByIndex(0).map_err(hr)?;
                let dxgi: IMFDXGIBuffer = buffer.cast().map_err(hr)?;
                let mut raw = std::ptr::null_mut();
                dxgi.GetResource(&ID3D11Texture2D::IID, &mut raw)
                    .map_err(hr)?;
                let target = ID3D11Texture2D::from_raw(raw);
                gfx.context.CopyResource(&target, &converter.nv12);
                interval_timings.allocator_copy.record(call_start.elapsed());
                let pts = super::frame_time(slot, profile.fps);
                sample.SetSampleTime(pts).map_err(hr)?;
                sample
                    .SetSampleDuration(super::frame_time(slot + 1, profile.fps) - pts)
                    .map_err(hr)?;
                let call_start = Instant::now();
                let accepted = encoder.submit(&sample)?;
                interval_timings.encoder_submit.record(call_start.elapsed());
                if accepted {
                    submitted += 1;
                } else {
                    dropped += 1;
                    encoder_busy += 1;
                }
            }
        }
        Ok(())
    })();
    let report_elapsed = report_start.elapsed();
    if report_elapsed > Duration::ZERO {
        let encoded = packets.samples.len();
        let end_ms = start.elapsed().as_millis();
        interval_reports.push(IntervalReport {
            start_ms: end_ms.saturating_sub(report_elapsed.as_millis()),
            end_ms,
            submitted: submitted - reported_submitted,
            encoded: encoded - reported_encoded,
            dropped: scheduler_late - reported_scheduler_late + no_frame - reported_no_frame
                + allocator_empty
                - reported_allocator_empty
                + encoder_busy
                - reported_encoder_busy,
            scheduler_late: scheduler_late - reported_scheduler_late,
            no_frame: no_frame - reported_no_frame,
            allocator_empty: allocator_empty - reported_allocator_empty,
            encoder_busy: encoder_busy - reported_encoder_busy,
            timings: interval_timings,
        });
    }
    if let Err(error) = capture_result {
        let _ = interval_reports.flush(log);
        return Err(error);
    }
    let finish_result = encoder.finish(&mut packets);
    let flush_result = interval_reports.flush(log);
    finish_result?;
    flush_result?;
    log.write(&format!(
        "Capture totals: scheduler_late={scheduler_late}, no_frame={no_frame}, allocator_empty={allocator_empty}, encoder_busy={encoder_busy}"
    ))?;
    log.write(&format!("Capture finished: submitted={submitted}, encoded={}, dropped={dropped}, unchanged={unchanged}, payload_bytes={}", packets.samples.len(), packets.bytes))?;
    if packets.samples.len() < 2 {
        return Err(RecorderError::Other("Insufficient encoded frames".into()));
    }
    let output = output.ok_or_else(|| RecorderError::Other("Missing output path".into()))?;
    let temporary = output.with_extension("partial.mp4");
    // Reserve both names before writing. Never overwrite an existing user file.
    if output.exists() {
        return Err(RecorderError::Other("Output file already exists".into()));
    }
    let reservation = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    drop(reservation);
    let save_start = Instant::now();
    let saved = mux(&temporary, &encoder.header, &packets).and_then(|()| {
        let file = std::fs::OpenOptions::new().write(true).open(&temporary)?;
        file.sync_all()?;
        // No REPLACE_EXISTING flag: concurrent user files are never overwritten.
        let source: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
        let destination: Vec<u16> = output.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            windows::Win32::Storage::FileSystem::MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(destination.as_ptr()),
                windows::Win32::Storage::FileSystem::MOVE_FILE_FLAGS(0),
            )
            .map_err(hr)?;
        }
        Ok(())
    });
    if saved.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    saved?;
    log.write(&format!(
        "MP4 finalized: {}; bytes={}; export_ms={}",
        output.display(),
        std::fs::metadata(output)?.len(),
        save_start.elapsed().as_millis()
    ))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interval_reports_are_bounded() {
        let mut reports = IntervalReports::default();
        for end_ms in 0..(MAX_INTERVAL_REPORTS + 1) as u128 {
            reports.push(IntervalReport {
                start_ms: end_ms,
                end_ms,
                submitted: 0,
                encoded: 0,
                dropped: 0,
                scheduler_late: 0,
                no_frame: 0,
                allocator_empty: 0,
                encoder_busy: 0,
                timings: IntervalTimings::default(),
            });
        }
        assert_eq!(reports.reports.len(), MAX_INTERVAL_REPORTS);
        assert_eq!(
            reports.reports.last().unwrap().end_ms,
            (MAX_INTERVAL_REPORTS - 1) as u128
        );
    }

    #[test]
    fn retained_payload_releases_oversized_encoder_buffer() {
        let _session = MfSession::new().unwrap();
        unsafe {
            let source = MFCreateSample().unwrap();
            let buffer = MFCreateMemoryBuffer(1024 * 1024).unwrap();
            buffer.SetCurrentLength(16).unwrap();
            source.AddBuffer(&buffer).unwrap();
            source.SetSampleTime(123).unwrap();
            source.SetSampleDuration(333333).unwrap();
            source.SetUINT32(&MFSampleExtension_CleanPoint, 1).unwrap();
            let mut packets = Packets {
                samples: Vec::new(),
                bytes: 0,
                limit: 16,
            };
            packets.push(source, 30).unwrap();
            let stored = &packets.samples[0];
            assert_eq!(
                stored.GetBufferByIndex(0).unwrap().GetMaxLength().unwrap(),
                16
            );
            assert_eq!(stored.GetSampleTime().unwrap(), 123);
            assert_eq!(stored.GetSampleDuration().unwrap(), 333333);
            assert_eq!(stored.GetUINT32(&MFSampleExtension_CleanPoint).unwrap(), 1);
            assert_eq!(packets.bytes, 16);
        }
    }
}
