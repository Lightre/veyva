<p align="center">
  <img src="assets/preview.png" alt="Veyva icon" width="80" height="80">
</p>
<h1 align="center">Veyva</h1>
<p align="center"><strong>Keep the moment.</strong><br>A native Windows recorder with instant replay and hardware H.264 encoding.</p>
<p align="center">Windows · Rust · H.264 + AAC · MP4</p>

> **Development preview.** Features are implemented, but device compatibility, background operation and long-session performance still need manual acceptance. Use the [manual test checklist](docs/MANUAL-TEST.md) to verify a build on your hardware.

## Capture your way

- **Instant replay:** keep a bounded history and save the latest moments with a button or global shortcut.
- **Normal recording:** start, pause, resume and finish to a saved MP4. Compatible recording and replay share one video encoder.
- **Audio:** select a system output device and optional microphone input. Sources are mixed into one AAC track; microphone capture defaults off.
- **Your settings:** presets and custom resolution, FPS, bitrate, replay duration, payload budget and cursor capture.
- **Keyboard shortcuts:** click a binding and press a combination. Changes save automatically; Delete clears the binding and Escape cancels.
- **Background operation:** optional close-to-tray and per-user Windows login startup. The tray menu offers Show Veyva, Save Replay and Quit.
- **Native interface:** light/dark appearance, custom window controls and no live video preview.

No account, telemetry or cloud upload is included.

## Replay success sound

An optional **Replay saved sound** in Preferences confirms a replay only after its MP4 is saved and defaults off. It works with any system output selection, with or without microphone capture, and with audio capture disabled. It cannot enter the replay that just finished saving, but may be captured in subsequent clips, an active normal recording, or through a microphone listening to speakers.

The optional **All outputs / exclude Veyva** mode captures application audio across output devices while excluding Veyva and its child processes. It requires Windows build 20348 or newer; unsupported capture reports an error instead of falling back. This mode does not prevent a microphone from hearing notification playback through speakers. Keep endpoint-specific capture when you want to record a single output; disable the notification sound if it must not appear in later recordings.

Output checks require free space for replay payload, MP4 overhead and a 64 MiB safety reserve. Normal recording checks space periodically on its disk worker and attempts to finalize early when space is low. Checks cannot reserve space against other applications or guarantee recovery from a full or disconnected drive.

## Build and run

Requires Windows, Rust with the MSVC toolchain, Visual Studio C++ build tools and the Windows SDK. Capture needs D3D11 and a compatible hardware H.264 encoder. Unsupported configurations fail explicitly; the application does not silently switch to software encoding.

```powershell
cargo build --release
.\target\release\veyva.exe gui
```

Normal desktop launches do not open a console window. Diagnostic CLI commands attach to the existing parent console when available and preserve redirected output. In PowerShell, use `Start-Process -FilePath .\target\release\veyva.exe -ArgumentList "settings", "show" -NoNewWindow -Wait` when synchronous completion is needed.

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

Clips default to `%USERPROFILE%\Videos\Veyva`. The destination field supports manual editing, a native folder picker and opening the folder in Explorer. The selected destination is remembered across launches. Without a saved selection, the default above is used. An inaccessible destination reports an error rather than redirecting output elsewhere. Names use a lowercase `veyva-` prefix followed by local date/time, for example `veyva-2026-10-29_21-45-08-123-record.mp4` or `veyva-2026-10-29_21-45-08-123-replay.mp4`. A short numeric suffix is added only when a name is already occupied; existing clips are not overwritten.

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

A first build/test may compile substantial Windows/GUI dependencies. Compilation is not proof of recording, audio or tray behavior. Follow the [manual acceptance checklist](docs/MANUAL-TEST.md) and include relevant results when reporting an issue or submitting a change.

The interface uses eframe/egui 0.29.1; Windows bindings are pinned to 0.58.0. There is no additional web UI runtime.

## Project guide

- [Manual testing](docs/MANUAL-TEST.md)
- [Performance measurement](docs/PERFORMANCE.md)
- [Releasing and portable packages](docs/RELEASING.md)
- [Changelog](CHANGELOG.md)
- [Contributing](CONTRIBUTING.md)
- [Third-party notices](THIRD_PARTY_NOTICES.txt)

## License

Copyright (C) 2026 Lightre. Veyva's original code, documentation, and visual and audio assets are licensed under **[GNU GPL version 3 only](LICENSE)** (`GPL-3.0-only`), without warranty. Contributions are accepted under the same license.

Third-party components retain their own copyrights and licenses; see [third-party notices](THIRD_PARTY_NOTICES.txt). The [branding policy](BRANDING.md) explains use of the Veyva name and logo. Selected capture/conversion, encoder negotiation and AAC setup retain their MIT notices and exact upstream source revision.

Release binaries are accompanied by a matching source archive containing the locked dependency sources and offline build instructions. Distributing modified versions requires providing corresponding source as required by the license; private modifications do not have to be published.
