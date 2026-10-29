# Manual testing

Build and launch from the repository root:

```powershell
cargo build --release --offline
.\target\release\veyva.exe gui
```

Omit `--offline` on the first build if dependencies are not cached. Recording tests capture desktop contents and selected audio sources. Keep test clips and logs outside version control.

Record the Windows version, GPU/driver, monitor resolution/scaling and capture profile with test results. Check completed files in a media player, including duration, seeking, audio quality and synchronization. A successful build alone does not verify recording behavior.

## Recording and replay

1. Launch with replay-on-start enabled. Wait for history, save with the button and global shortcut, and verify the available duration. Short history must be reported accurately.
2. Start recording, pause for five seconds, resume and finish. The pause must be removed from the saved timeline. Verify the first frames and seeking.
3. Save replay while recording and while paused. Replay must remain independent. Disable replay while recording and verify recording continues.
4. Exercise repeated saves, a small history budget, and a destination that cannot be written. Errors must be visible; existing files must remain unchanged.
5. Compare cursor on/off in both flows, including screen edges and scaled output. DXGI may already include the cursor in desktop pixels; that embedded pointer cannot be removed by disabling separate cursor composition.

## Preferences and shortcuts

1. Change resolution, FPS, bitrate or device selection. Changes must save automatically after editing finishes, with one replay restart after a short quiet interval. No restart should occur for each typed digit or while dragging a value.
2. Change capture preferences during recording or export. Current capture must continue, and the interface must explain that the update is deferred until completion. New recordings must not use pending old settings.
3. Enter invalid values. Verify an actionable error without repeated saves or restarts. Correct the value or retry after resolving an I/O failure.
4. Assign each shortcut by pressing a modifier/key combination. Escape cancels; Delete clears to None. Duplicate bindings and OS registration conflicts must be reported. All bindings, including None, must survive restart.
5. While assigning, verify no recording action fires and text does not enter other fields. Test global shortcuts with another application focused and while Veyva is hidden.
6. Switch themes and restart. Appearance must persist without restarting capture. Static labels must not be selectable; editable paths must remain selectable.

## Audio

1. Refresh devices using the icon beside Audio. Select the output and optional microphone. Verify device selections persist after restart.
2. Test system-only, microphone-only and mixed audio in normal recordings and replay. With microphone disabled, a newly started session must not capture the separate microphone source.
3. Compare playback against the source for crackle, clipping and A/V drift. Use headphones to avoid acoustic echo. Windows microphone monitoring can also route microphone sound into the system output.
4. Disconnect a selected device, select an unavailable endpoint, and test a device with a different sample rate/channel count. Unsupported negotiation or runtime failures must be visible.

## Window and background operation

1. With no saved window state, verify the window fits the monitor work area. Move/resize, close normally, and reopen. Position, size and maximized state must restore.
2. Repeat after changing display scaling or disconnecting a monitor. Check minimum size, scrolling, popup placement and keyboard focus in both themes.
3. Enable close-to-tray. The custom titlebar X must hide the window; a single tray click and Show Veyva must restore it. Save Replay must work while hidden.
4. Alt+F4, taskbar Close and tray Quit must exit, finalizing any recording/export first. With close-to-tray disabled, the titlebar X must also exit. Verify the process terminates.
5. Enable login startup, sign out/in and verify one hidden session, a visible tray icon and working replay shortcut. Disable startup and verify the Veyva entry is removed from the current user's Run registry key.
6. Launch a second instance and verify it opens the existing window. Restart Windows Explorer and verify tray recovery. Tray failure must not leave an inaccessible hidden application.
7. Move the executable and update the login-startup setting. Test lock/unlock and sleep/wake separately; do not assume device recovery is supported.

## Reliability and performance

- Exercise long recording/replay sessions, recording plus replay export, repeated saves, disk-full/write failures and encoder-unavailable errors.
- Compare capture off/on with the same workload. Record CPU/GPU use, memory, dropped frames, frame times and export latency. Measure visible and hidden modes separately.
- Repeat cursor on/off and audio source combinations. History payload limits are not total-process memory limits.
- Forced termination, power loss and Windows logoff may interrupt unfinished files. Keep those cases separate from normal graceful shutdown.

The CLI measurement tool is documented in [PERFORMANCE.md](PERFORMANCE.md). Use reproducible steps and sanitized diagnostics in reports; avoid publishing private clips or personal paths.
