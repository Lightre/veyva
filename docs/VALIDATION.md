# Validation

Date: 2026-10-29. Local Windows desktop, AMD Radeon Vega 8 Graphics, 496 MiB reported dedicated VRAM, 1920x1080 display, AMDh264Encoder hardware MFT.

## Checks completed

- Rust formatting, four targeted CLI/timestamp tests and Clippy with warnings denied passed.
- Probe selected the hardware H.264 encoder associated with the capture adapter.
- Corrected short capture: 5.000 seconds, H.264, 1280x720, 149 decoded frames, 1 skipped frame slot, 1,864,813-byte MP4. The 30 FPS target preserves timing gaps when frames are skipped; this run averaged 29.8 FPS.
- FFmpeg decoded the entire corrected file without reported errors and decoded a frame after seeking to three seconds.
- Logged MP4 export/finalization took 29 ms in that single run. This is not a general save-latency benchmark and does not measure future replay export.
- Existing-file rejection was exercised and the existing MP4 SHA-256 remained unchanged.
- Invalid display index 999 failed with a clear error.
- Windows resource lookup found the embedded app icon group in the built executable.
- SVG/ICO preview was visually inspected. ICO contains 16, 32, 48, 64, 128 and 256px sizes.

An earlier recording exposed compressed initial frame timing; the capture loop now timestamps scheduled frame slots using integer rational arithmetic. The corrected file passed the decode/seek checks above.

## Still unverified

No controlled game FPS/CPU/GPU benchmark, long-duration memory test, independent color-chart comparison or manual player UI inspection has been completed. HDR, non-16:9 layouts, cursor composition and other GPU vendors are not validated. Audio, replay history, UI/tray and automatic device-loss recovery remain future work. Compilation and a short local encode do not establish support for every old PC.

Private test recordings and scratch verification scripts are under ignored `work/`; they are not committed.
