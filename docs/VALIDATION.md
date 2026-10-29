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

Capture interval diagnostics are buffered in memory (at most 64 reports) and written after capture and encoder drain, so interval logging does not write to the terminal or log file while capturing. Each report uses capture-relative `start_ms`/`end_ms`, frame counts and drop reasons, plus sum/max CPU-call wall times for sleep wait, encoder drain, acquire/convert/release, allocator/copy, and encoder submit. These timings measure time spent in the CPU calls; they do not measure asynchronous GPU execution. Totals are written after the interval reports. The buffering and timing changes have not been checked against a live capture.

## Still unverified

No controlled game FPS/CPU/GPU benchmark, long-duration memory test, independent color-chart comparison or manual player UI inspection has been completed. HDR, non-16:9 layouts, cursor composition and other GPU vendors are not validated. Audio, replay history, UI/tray and automatic device-loss recovery remain future work. Compilation and a short local encode do not establish support for every old PC.

Private test recordings and scratch verification scripts are under ignored `work/`; they are not committed.

## Focused review follow-up

The encoded sample collection counted current payload bytes but retained original encoder buffers. In the caller-allocated output path each buffer was at least 1 MiB, so small packets could retain much more memory than the 32 MiB payload budget suggested. Retention now copies the compressed bytes into an exact-sized buffer and releases the original sample, also avoiding retention of a hardware encoder's output pool. Sample attributes, flags, timestamps and durations are preserved.

Validation: five tests passed, including a synthetic Media Foundation regression retaining a 16-byte payload from a 1 MiB source allocation and checking capacity, timestamps and keyframe metadata. Formatting and Clippy with warnings denied passed. No screen recording was started for this review. The existing release executable predates this fix; the corrected capture path has not been exercised against a live encoder. The extra compressed-payload copy needs performance measurement in a future recording run.

## Frame pacing update

Frame pacing now uses a high-resolution Windows waitable timer, with handle cleanup and standard-sleep fallback when unavailable. Runtime timer failure disables further timer attempts and is reported after the capture loop. A synthetic creation/1 ms wait test passed on this machine, along with formatting, Clippy and the release build; no screen recording was started. Reduced live frame drops have not yet been verified.

The provisional icon was replaced with a simple two-tone V. SVG geometry is the source used to generate both PNG and ICO, avoiding separate arc/stroke implementations. The final brand name and icon remain undecided.

## Branding update

The project brand is now Veyva. Historical validation entries above retain the names and paths observed during those earlier runs; those references describe the prior executable and log locations and were not changed by this rename. The new runtime log location is `%LOCALAPPDATA%\Veyva\veyva.log`.

## Thirty-second desktop measurement

On 2026-10-29, the measurement tool ran a 5-second pre-capture baseline and a 30-second selected-display recording on the previously identified Vega 8 system (4 logical processors). The 31-second process observation includes startup and MP4 export. All 62 process samples succeeded; no timeout occurred.

- Encoded 893 of 900 target frame slots; 7 skips (5 scheduler-late, 2 encoder-busy); no missing-first-frame or allocator-empty skips.
- Process CPU averaged 0.718% of total logical CPU capacity (0.0287 core equivalents).
- Peak sampled working set: 48,005,120 bytes (45.8 MiB); peak sampled private bytes: 49,168,384 bytes (46.9 MiB). Sampling may miss brief peaks.
- Total system CPU averaged 20.93% before capture and 33.23% during the process observation. These sequential intervals are not a matched causal comparison; the difference cannot be attributed entirely to Veyva.
- FFprobe read 893 H.264 frames at 1280x720 and reported exactly 30.000 seconds. MP4 size: 11,172,425 bytes; logged export time: 65 ms.

Raw CSV/JSON, process log and private footage are in ignored `work/measurement-20261029T005702Z/`. The observed desktop workload was not scripted. No game FPS, GPU utilization, driver CPU attribution or long-session memory stability was measured. This run does not establish the cause of earlier intermittent stalls or prove low overhead in games.
