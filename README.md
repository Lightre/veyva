# Veyra Replay

**Keep the moment. Keep your frames.**

A Windows replay recorder being developed for modest PCs, bounded memory and hardware encoding. Veyra is a provisional working name, not a cleared public brand.

## Current milestone

The native command-line backend detects the selected display adapter and its hardware H.264 encoder, then records a short silent 720p/30 FPS MP4. No UI, audio, replay history, tray or global shortcuts are implemented yet. Running without arguments prints help; it never starts recording.

```powershell
cargo run --offline -- probe
cargo run --offline -- record --seconds 10 --monitor 0 --output "C:\Clips\test.mp4"
```

Create the destination directory first. Display indices start at zero across physical adapters. Duration is limited to 1-60 seconds. The output must end in `.mp4`; existing final or temporary files are rejected. The filename ending in `.partial.mp4` is published only after MP4 finalization and file synchronization. A failed export removes its temporary file when possible; forced process termination may leave it behind.

## Development

Requirements: Windows 10 version 1703 or newer, a D3D11-capable display adapter with a compatible hardware H.264 encoder, Rust MSVC and Visual Studio C++ tools/Windows SDK. This milestone is locally tested on one AMD Vega 8 system; other adapters and Windows versions need validation. Unsupported hardware is reported rather than silently switching to CPU encoding.

```powershell
cargo fmt --check
cargo test --offline
cargo build --release --offline
```

`windows` 0.58.0 is the sole direct runtime dependency, pinned to the reference binding version. Offline builds require its transitive packages to be cached. The Windows SDK resource compiler embeds `assets/veyra.ico`; set `VEYRA_RC` to an explicit `rc.exe` path if SDK discovery fails.

Logs: `%LOCALAPPDATA%\Veyra\veyra.log`, rotated at startup after 1 MiB to `veyra.previous.log`. Logs include the adapter, encoder, submitted/encoded/skipped frame counts, encoded payload size and MP4 export time. Timestamp prefixes use Unix milliseconds. No footage is uploaded.

## Resource limits and boundaries

- Hardware H.264 only, 1280x720, 30 FPS target and 3 Mbps nominal video bitrate.
- At most four allocator-owned GPU input surfaces; no CPU pixel readback.
- Encoded payload is retained for this short-recording milestone, capped at 32 MiB and 3600 samples. This is not a replay ring and is not the total process-memory limit.
- Nonblocking encoder event reads, a three-second drain deadline and sleeping frame pacing. Late or saturated frame slots are dropped rather than queued without bound.
- Capture and export currently run in sequence. A future continuous recorder needs a bounded export worker.
- One selected desktop display; rotated/non-BGRA outputs are rejected. The display is scaled to 16:9; non-16:9 aspect handling and HDR are not validated. Cursor composition is not implemented.
- Device loss aborts this milestone with a logged error. Automatic recovery, audio and shared replay/manual recording sessions are later work.

## Project map

- `src/windows_capture.rs`: GPU capture/conversion, hardware encoder and MP4 export.
- `src/main.rs`: explicit probe/record CLI and argument validation.
- `src/diagnostics.rs`: local logs.
- `docs/BRAND.md`: brand and future UI direction; `assets/`: icon and theme tokens.
- `docs/PLAN.md`: remaining implementation and performance validation.
- `docs/REFERENCE.md`: source review/provenance; `docs/VALIDATION.md`: observed checks.

Selected GPU setup/conversion and encoder negotiation were adapted from the supplied ClipFlow revision. Required MIT notices are retained in source and `docs/CLIPFLOW-LICENSE.txt`. Its application UI, branding and updater configuration were not copied. Original Veyra code has no distribution license selected yet.
