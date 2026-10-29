# Veyva

**Keep the moment.**

A local Windows replay recorder designed around hardware H.264 encoding and bounded encoded history. Veyva is the selected project name; trademark and domain exclusivity have not been established.

## Desktop interface

Run `veyva.exe` or `veyva.exe gui`. The application arms replay on startup by default; this is configurable in Preferences. Capture shows actual retained history, encoded payload, submitted frames, dropped frames, and session/export errors. Wait for history, then save by button or the configured global shortcut (Alt+F1 by default). Normal recording has separate Start, Pause/Resume, and Finish & Save controls. Saving runs on a separate worker while capture continues. Stop capture from Preferences before editing the profile; the next session starts with empty history.

The interface uses an ink/mint theme, the Veyva icon, Capture/Settings/About navigation, profile presets and custom settings. Idle rendering is event-driven; an active session refreshes status at most about four times per second. There is no live video preview.

The desktop supports system audio from the default Windows output device. Microphone and tray controls are not implemented. The existing 1-60 second `record` CLI remains a silent diagnostic path. Normal GUI recording streams to disk without that duration limit. Closing the window requests stop and waits asynchronously for any export/recording finalization; forced termination or OS shutdown can still interrupt a file.

## Build and checks

Windows, Rust MSVC, Visual Studio C++ tools and the Windows SDK are required. The capture path requires D3D11 and a compatible hardware H.264 encoder. Profile ranges do not guarantee encoder support for every combination. Unsupported configurations fail explicitly rather than using a CPU encoder.

```powershell
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
cargo build --release --offline
.\target\release\veyva.exe gui
```

The GUI uses eframe 0.29.1 with the OpenGL renderer and bundled fonts; the Windows API bindings remain pinned to 0.58.0. Offline builds require cached dependencies. The first GUI build is larger and slower than the earlier CLI-only build. The Windows resource compiler embeds `assets/veyva.ico`; set `VEYVA_RC` to an explicit `rc.exe` path if discovery fails. Runtime GUI memory and game FPS impact need new measurements; earlier CLI measurements do not describe GUI overhead.

## Settings and command line

Settings live at `%LOCALAPPDATA%\Veyva\settings.conf`. A missing file uses Low End defaults; a malformed file is preserved and shown as an error. The GUI provides an explicit reset. Settings saves validate and replace the file after writing/synchronizing a same-directory temporary file.

```powershell
.\target\release\veyva.exe settings show
.\target\release\veyva.exe settings preset balanced
.\target\release\veyva.exe settings set fps 60
.\target\release\veyva.exe settings set history_seconds 45
.\target\release\veyva.exe probe
.\target\release\veyva.exe record --seconds 10 --output "$env:TEMP\veyva-custom-01.mp4" --width 1280 --height 720 --fps 30 --video-kbps 4000
```

Supported saved keys: `width`, `height`, `fps`, `video_kbps`, `history_seconds`, `replay_budget_mib`, `monitor`, `replay_auto_start`, `audio_enabled`, `hotkey_modifiers`, and `hotkey_key`. Presets: `low`, `balanced`, `high`. Record overrides are temporary. GUI output folder selection is currently session-local, defaulting to `%USERPROFILE%\Videos\Veyva`.

Ranges: even widths 320-7680; even heights 180-4320; FPS 1-240; video bitrate 250-100000 kbps; history 5-600 seconds; encoded payload budget 16-512 MiB. The history budget is a limit, not a guarantee of the requested duration. Actual retained seconds can be shorter because of bitrate bursts, keyframe alignment or insufficient memory.

## Capture and resource boundaries

- Selected desktop display -> GPU scaling/NV12 conversion -> hardware H.264 -> keyframe-aligned encoded history.
- Replay history is bounded by time, payload bytes and packet count. Only one session and one export are allowed. Snapshots share immutable owned packets; COM objects remain on their creating workers.
- Export reconstruction can temporarily add payload copies. The history budget is not a total-process memory limit; allow roughly up to three history budgets in transient encoded payload plus graphics, encoder and GUI overhead.
- Frame input uses at most four allocator-owned GPU surfaces. Stale or saturated slots are dropped. No CPU pixel readback or continuous video preview is used.
- Export writes a temporary MP4, finalizes and synchronizes it, then publishes without replacing an existing file.
- Rotated/non-BGRA outputs are rejected. Non-16:9 aspect handling, HDR, cursor composition and automatic device-loss recovery remain incomplete/unvalidated.

Logs: `%LOCALAPPDATA%\Veyva\veyva.log`, rotated at startup after 1 MiB. CLI measurements and timings are documented in `docs/VALIDATION.md`; the CPU/memory collector is described in `docs/PERFORMANCE.md`.

## Project map

- `src/gui.rs`: native control window and settings panels.
- `src/session.rs`: session commands, status and bounded encoded history.
- `src/windows_capture.rs`: Windows capture, encoding and MP4 export.
- `src/settings.rs`: validated persistent profiles.
- `assets/`: SVG source, ICO, PNG preview and design tokens; CSS tokens are a design reference, not a web runtime.
- `docs/BRAND.md`, `docs/PLAN.md`, `docs/VALIDATION.md`: identity, remaining work and evidence.

Selected capture/conversion and MFT negotiation were adapted from the supplied ClipFlow revision. MIT notices remain in source and `docs/CLIPFLOW-LICENSE.txt`. Original Veyva code has no public distribution license selected yet.

## Normal recording, automatic replay and system audio

The desktop capture screen now has two independent flows:

- Normal recording: Start recording, Pause, Resume, Finish & save. The file is streamed to a bounded disk worker, so recording duration is not limited by the replay-history budget or the old 60-second diagnostic CLI limit. Pauses are removed from the saved timeline. Start/resume wait for a decodable video keyframe; an encoder may delay this until its next keyframe.
- Replay: armed automatically on application launch by default, with the selected history duration and memory budget. Save Replay or the configured global shortcut exports the available history while buffering continues. Default shortcut: Alt+F1. Replay can be turned off independently of a normal recording.

Both consume the same hardware video encoder. Normal recording has a bounded 64-packet queue plus a byte budget; a disk that cannot keep up produces a visible recording failure rather than unbounded queue growth. When neither flow needs video, the GPU capture path is skipped. The audio worker may remain initialized while the session is ready.

System audio captures the default Windows render endpoint through WASAPI, converts to 48 kHz stereo PCM, and encodes AAC at 128 kbps. Audio is included in new desktop replay/normal MP4 exports when available. Initialization/runtime audio failures are shown explicitly; affected files may be video-only. Microphone capture is not included. The timed `record` CLI remains a silent diagnostic path.

Settings format version 2 adds `replay_auto_start`, `audio_enabled`, `hotkey_modifiers`, and `hotkey_key`. Boolean CLI values are 0/1. Existing version-1 settings migrate with replay and audio enabled and Alt+F1 selected. Settings editing requires stopping the capture engine; ongoing recordings/exports must finish first.

Figma design and native implementation notes: `docs/DESIGN.md`. New behavior supersedes the older first-milestone limitations described above.
