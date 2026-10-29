# Implementation plan

## Architecture decision

Use a Rust core with a compact native Windows UI. The UI framework is intentionally undecided until idle memory and redraw cost are compared. Do not import the full Tauri/React application solely to reuse the capture algorithm.

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
