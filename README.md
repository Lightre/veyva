<p align="center">
  <img src="assets/preview.png" alt="Veyva icon" width="80" height="80">
</p>
<h1 align="center">Veyva</h1>
<p align="center"><strong>Keep the moment.</strong><br>A native Windows recorder with instant replay and hardware H.264 encoding.</p>
<p align="center">Windows · Rust · H.264 + AAC · MP4</p>

> **Development preview.** Features are implemented, but device compatibility, background operation and long-session performance still need manual acceptance. See [validation evidence](docs/VALIDATION.md).

## Capture your way

- **Instant replay:** keep a bounded history and save the latest moments with a button or global shortcut.
- **Normal recording:** start, pause, resume and finish to a saved MP4. Compatible recording and replay share one video encoder.
- **Audio:** select a system output device and optional microphone input. Sources are mixed into one AAC track; microphone capture defaults off.
- **Your settings:** presets and custom resolution, FPS, bitrate, replay duration, payload budget and cursor capture.
- **Keyboard shortcuts:** click a binding and press a combination. Changes save automatically; Delete clears the binding and Escape cancels.
- **Background operation:** optional close-to-tray and per-user Windows login startup. The tray menu offers Show Veyva, Save Replay and Quit.
- **Native interface:** light/dark appearance, custom window controls and no live video preview.

No account, telemetry or cloud upload is included.

## Build and run

Requires Windows, Rust with the MSVC toolchain, Visual Studio C++ build tools and the Windows SDK. Capture needs D3D11 and a compatible hardware H.264 encoder. Unsupported configurations fail explicitly; the application does not silently switch to software encoding.

```powershell
cargo build --release
.\target\release\veyva.exe gui
```

Use `--offline` with Cargo when dependencies are already cached. The resource compiler embeds the application icon; set `VEYVA_RC` to an explicit `rc.exe` path if automatic discovery fails.

1. Open Veyva and select a capture profile. Replay arms on launch by default.
2. Wait for history to accumulate, then choose **Save replay**, or press **Alt+F1**.
3. For a normal recording, choose **Start recording**, optionally **Pause / Resume**, then **Finish & save**.
4. Edit capture/audio preferences; changes save automatically after editing. Changing capture settings while replay is active clears its history and starts a new session. Active recording/export operations guard this transition.

Default shortcuts: **Ctrl+Shift+R** starts/finishes recording; **Ctrl+Shift+P** pauses/resumes; **Alt+F1** saves replay. Windows-reserved or conflicting combinations may fail registration and show an error.

## Background behavior

Both background options are **off by default**. Enable them in Preferences when you want continuous replay across normal window closure and Windows sign-in.

- **Keep running in the notification area when closed:** closing the window hides it while the existing session continues.
- **Start replay in the background when I sign in:** registers the current executable under the current user's Windows startup settings. No Windows service or administrator installation is required.
- **Alt+F4** or **Quit** from the tray completely stops Veyva. With close-to-tray disabled, closing the window also exits.

A second launch requests the existing window rather than starting another GUI capture session. Keep the executable in a stable location before enabling startup; moving it requires updating the startup setting. Hidden capture still consumes CPU/GPU/memory, and retained history is lost when the process exits unless saved.

## Files and preferences

Clips default to `%USERPROFILE%\Videos\Veyva`. The destination field supports manual editing, a native folder picker and opening the folder in Explorer. Destination selection currently lasts for the application session. Names include local date/time and a uniqueness suffix; existing clips are not overwritten.

Preferences and diagnostics live under `%LOCALAPPDATA%\Veyva`:

- `settings.conf`: capture profile, shortcuts, cursor and appearance.
- `audio.conf`: system output and microphone preferences.
- `background.conf`: close-to-tray and Windows startup preferences.
- `window.ron`: native window position, size and maximized state (eframe persistence).
- `veyva.log`: capture/session diagnostics, rotated at startup after 1 MiB.

## Resource boundaries and current limitations

History is bounded by time, encoded payload bytes and packet count, with decodable keyframe boundaries. The payload budget is **not total process memory** and does not guarantee the full requested duration. GPU surfaces, the encoder, GUI and pending exports require additional memory.

- Hardware/driver support determines valid resolution, FPS and bitrate combinations. No game-FPS or hidden-mode overhead guarantee has been established.
- Cursor composition adds a desktop GPU copy and cursor-region CPU readback. A pointer already embedded by DXGI cannot be removed by disabling separate cursor composition.
- Microphone and system audio share one mixed track. There is no echo cancellation or independent track editing; headphones help prevent acoustic echo.
- Rotated/non-BGRA displays are rejected. HDR, device-loss recovery, sleep/wake and automatic audio endpoint changes need further work/validation.
- Forced termination, Windows logoff or power loss can interrupt an unfinished MP4. Normal close waits for finalization.
- The timed `record` CLI remains a short, silent diagnostic path; desktop recording provides the audio/replay workflow.

## Development

```powershell
cargo fmt --check
cargo clippy --offline -- -D warnings
cargo test --offline
```

A first build/test may compile substantial Windows/GUI dependencies. Compilation is not proof of recording, audio or tray behavior. Follow the [manual acceptance checklist](docs/MANUAL-TEST.md) and record results in [VALIDATION.md](docs/VALIDATION.md).

The interface uses eframe/egui 0.29.1; Windows bindings are pinned to 0.58.0. There is no additional web UI runtime.

## Project guide

- [Design and native interface notes](docs/DESIGN.md)
- [Manual testing](docs/MANUAL-TEST.md)
- [Validation evidence and limitations](docs/VALIDATION.md)
- [Performance measurement](docs/PERFORMANCE.md)
- [Implementation roadmap and history](docs/PLAN.md)
- [Upstream provenance](docs/REFERENCE.md)

## Licensing

A public distribution license has **not yet been selected for original Veyva code**. No open-source license grant for that code is implied by this repository.

Selected capture/conversion, encoder negotiation and AAC setup were adapted from MIT-licensed ClipFlow. Copyright, license text and the exact upstream revision are preserved in [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt) and source annotations. Dependency licenses must also be reviewed before distributing release binaries.
