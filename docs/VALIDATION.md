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

## Persistent settings foundation

Added a validated, versioned settings model shared by the CLI and native capture backend. Unit tests cover strict parsing, profile validation, atomic in-memory updates, and save/load/replacement in a temporary directory. Eleven tests passed during integration. No live recording with custom profiles has been performed; supported ranges are configuration bounds and require encoder negotiation. Replay history configuration is persisted but replay capture is not implemented yet.

## Native GUI and continuous replay source validation

The native GUI and continuous replay backend pass the integrated offline compile and Clippy check with warnings denied. Three standalone History tests passed for GOP-aligned byte eviction, time/short-history/snapshot retention, and oversized-GOP rejection. The complete GUI-linked test suite was not run because the first dependency code generation is lengthy; a release build is delegated to the user. No new GUI or continuous replay capture was launched by the agent.

Manual acceptance is in `docs/MANUAL-TEST.md`. Prior CLI playback, memory and CPU measurements do not verify the new GUI/replay behavior. Audio, global shortcuts, tray and unlimited manual recording remain unavailable.

## Dual-flow source checks

Added independent normal recording and replay commands sharing a capture encoder, a bounded recording writer, system audio loopback/AAC and configurable replay hotkeys. Ten fast isolated tests passed: five settings/persistence/migration cases and five A/V history/pause-clock cases. The complete integrated application passes incremental compile and Clippy checks. A release build, actual hotkey registration, native window visual QA, real audio playback/synchronization, pause/resume export playback, and concurrent recording/replay runtime behavior have not been exercised by the agent.

The Figma Capture frame and the separate recording/replay panel screenshots were inspected. This verifies the design artifact, not the native rendered UI. Prior silent CLI recordings do not verify the new A/V path. No screen or system-audio capture was launched for this change.

## Audio crackle investigation

A user reported continuous crackle in a desktop A/V recording. The supplied 22.33-second MP4 decoded as H.264 plus 48 kHz stereo AAC-LC without decoder errors; decoded peak level was approximately -13.9 dBFS, so simple full-scale clipping was not evident. The source sound was not available for comparison.

The packet assembler previously recomputed an integer PCM-frame target from every WASAPI QPC timestamp. Even one-frame timestamp rounding/jitter could insert a zero frame or remove a PCM frame at each packet boundary. Packet alignment now anchors QPC once and follows the device sample position; large wall-clock discontinuities or device-position resets re-anchor, while ordinary packet timestamp jitter does not edit PCM. Real missing device frames still create a gap. Audio startup now logs the requested PCM16/48 kHz/stereo format and the native mix-format fields once.

Four isolated alignment tests passed (timestamp jitter, genuine missing frames, silence/resume, reset/invalid timestamp), and integrated Clippy passed. No live audio capture or listening test was performed after this change. This addresses a concrete source of discontinuities but does not establish that it explains all of the reported crackle; manual comparison against the same source audio is required.

## Remaining intermittent audio glitches

The user reported that the sample-position change greatly reduced crackle but did not eliminate small intermittent artifacts in normal and replay recordings. The new startup log identifies a native 48 kHz stereo 32-bit extensible mix format; the requested capture format is explicitly converted PCM16/48 kHz/stereo.

The WASAPI buffer capacity request was increased from 20 ms to 100 ms to tolerate brief scheduling delays; reads still poll every 5 ms. Idle-silence synthesis now waits for 150 ms without captured packets and is prohibited while real packets remain queued. Backlogged packets are drained in bounded batches without an extra 5 ms sleep between productive batches. This prevents a late capture worker from treating queued source sound as silence.

Deferred audio diagnostics keep counters plus at most 24 numeric events in memory (startup context and recent glitches). On shutdown/error they log device-discontinuity flags, invalid timestamps, inserted/trimmed frame totals, idle-fill totals, backlog observations and maximum poll interval. No packet-loop terminal/file writes were introduced.

Five isolated audio alignment/silence-policy tests and integrated Clippy passed. No new live recording, listening or release build was performed. The residual crackle remains unconfirmed until the same source is tested; the counters should distinguish genuine capture discontinuities from application alignment/silence insertion.


## Approved C desktop redesign

The native layout now uses horizontal navigation, separate recording/replay controls, a right inspector and a custom titlebar. Dark/light palettes and immediate appearance persistence were added without capture engine changes. Seven isolated settings tests passed, including old-file migration, both theme roundtrips, invalid/duplicate theme rejection, atomic updates and save/replace behavior. Incremental offline compilation and Clippy with warnings denied passed.

Both editable Figma theme frames were visually inspected. No release build, GUI launch, recording or native screenshot comparison was performed for this change. Windows snap, custom resize handles, DPI scaling, safe close while saving, theme persistence across restart, and the rendered layout remain manual checks. Prior capture/audio evidence is unchanged.


## Editable preferences and cursor capture

Inspector and Preferences share a draft with Apply/Discard. Replay changes use asynchronous stop/restart; recording, finalization, export and pending commands guard application. Scrollbars reserve layout space. Cursor capture is persisted (default on for older files), connected to both capture paths and composed before scaling using DXGI pointer metadata.

Incremental offline check and all-targets Clippy with warnings denied passed. The isolated cursor-setting compatibility test passed. Four cursor shape/clipping/blending unit tests compile but were not executed: dependency test compilation was stopped to respect the requested build budget. No release build, live GUI/capture test or overhead benchmark was performed. Embedded DXGI pointers cannot be removed by disabling separate pointer composition; cursor-region readback may introduce GPU synchronization cost.


## Audio device and background foundation

Added persistent WASAPI render/capture endpoint preferences, a bounded timestamp-aligned stereo PCM mixer feeding one AAC track, and microphone-only capture. Microphone defaults off. Background operation uses a native notification icon/message thread and the same desktop capture session; close-to-tray and per-user login startup default off. A named mutex prevents duplicate GUI sessions and a second launch requests the existing window. Hidden rendering skips page composition; capture/hotkey processing remains active.

Release build, actual audio playback/device negotiation, hidden-window hotkeys, Windows login startup and Explorer recovery require the manual checks above. No runtime overhead claim is made. The mixed track has no echo cancellation or independently editable microphone track. Windows logoff/power loss finalization remains unverified.

Combined incremental Clippy with warnings denied passed after integration. Seven standalone audio timing/mixing tests passed. No application, recorder or login task was launched by the agent.
