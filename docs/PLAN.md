# Implementation plan

## Architecture decision

Use a Rust core with a compact native Windows UI. The native interface now uses eframe/egui with the OpenGL renderer; idle memory and redraw cost still need runtime measurement. Do not import the full Tauri/React application solely to reuse the capture algorithm.

Planned pipeline: selected display -> DXGI/D3D11 -> GPU scale/color conversion -> Media Foundation hardware H.264 -> timestamped encoded history -> MP4 export worker. WASAPI audio must share a monotonic timing model with video. An active manual recording should consume the same encoded stream when compatible.

## Milestones

1. Detect display adapters and compatible hardware encoders. Capture one display to a short seekable MP4 and verify playback, duration and colors. Report unsupported hardware explicitly.
2. Add an encoded replay ring with hard byte/time budgets and GOP-aligned eviction. Bound exports and retained snapshot memory as well as active history. Save through a temporary file and rename only after successful finalization.
3. Add system audio, then optional microphone. Verify silence, device changes, timestamp discontinuities and long-session drift.
4. Add shortcut, tray and settings window. Persist settings and rotate local logs. Default startup recording to off. Handle shortcut conflicts and clean shutdown.
5. Add manual recording sharing the encoder where possible. Test concurrent replay saves, disk-full errors, slow disks and device loss.

## Resource policy

Low End starts at 720p/30 FPS, H.264 3000 kbps, AAC 128 kbps and 30 seconds. Nominal A/V payload is about 11.2 MiB; this excludes GPU surfaces, process memory, packet metadata, temporary export retention and bitrate bursts. The proposed ring payload ceiling is 32 MiB. Export snapshots need a separate bounded allowance; allow one pending export initially.

Use short bounded frame queues and drop stale frames under pressure. Request periodic keyframes; if a GOP cannot fit the memory budget, discard that GOP and resume at a usable keyframe. Show the real retained duration. Do not silently switch to a CPU encoder.

## Validation gates

Record CPU, GPU, hardware encoder, RAM, Windows build, driver, game/workload and output profile. Compare capture off/on with the same scene: average FPS, p95/p99 frame time, process private bytes, GPU encoder use, dropped frames, audio discontinuities, save-to-complete latency and file size. Measure UI open, tray, idle, buffering and export separately.

Initial investigation goals, not guarantees: less than 3% average FPS loss at Low End, bounded memory over a 30-minute run, decodable clips with seeking, and no growing A/V drift. Use repeated comparable runs and report variation. Test an older integrated GPU as well as a discrete GPU before claiming low-end support.

Mandatory cases: first keyframe, full ring, repeated save, recording plus replay, encoder unavailable, monitor resolution change, lock/unlock, sleep/wake, audio-device change, disk full and quit during export.

## Current implementation update

Milestone 1 has a command-line implementation: selected-display GPU capture, adapter-specific hardware H.264 negotiation, silent short MP4 export and persistent diagnostics. See `VALIDATION.md` for actual observations. The initial scaffold's no-dependency state has been superseded by the pinned Windows API binding dependency.

Milestone 2 now has a dedicated GOP-aligned replay ring and bounded snapshot export separate from the short-recording sample collection. The desktop replay implementation needs manual validation before it is considered runtime verified.

## Desktop replay implementation update

The source now includes an explicit-start native eframe interface and continuous replay session. Encoded history uses keyframe-aligned time/byte/packet eviction; one independent export worker writes a snapshot while capture continues. Session shutdown waits for export on the capture worker while the GUI polls asynchronously. Manual timed CLI capture remains available; unlimited manual recording, audio, tray and hotkeys remain later milestones.

Obsolete one-off scaffolding scripts were archived under ignored `work/archive/scaffolding/`; the obsolete `target/release/veyra.exe` was removed. Private MP4 recordings, measurement CSV/JSON and prior validation evidence were preserved. `assets/theme.css` remains design reference data, not an additional UI runtime.

The initial GUI source/check does not establish visual acceptance or runtime capture correctness. The next user validation must include idle/start/stop, repeated replay saves, playback/seeking, buffer limits, settings persistence, and closing while saving. GUI process memory and gameplay performance must be measured separately from earlier CLI runs.

## Shared recording/replay implementation

The corrected application model now shares one hardware encoder between normal recording and automatic replay. Normal recording uses Start/Pause/Resume/Finish with an independent bounded streaming writer; replay has an independent enable state and snapshot exporter. The GUI defaults to replay-on at launch and provides configurable Alt+F1 replay saving. System audio capture and AAC pass-through muxing are implemented for both GUI flows; microphone capture and tray remain future work.

The design reference is editable in Figma; see `DESIGN.md`. The timed CLI still serves as a silent diagnostic path. Next acceptance focuses on simultaneous replay+recording, pause-gap removal, audio/video sync, hotkey conflicts, stream finalization, and memory under long recordings.
