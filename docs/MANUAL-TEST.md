# Desktop recording and replay manual checks

Build with `cargo build --release --offline`, then run `target\release\veyva.exe gui`.

1. Build the new release and open the GUI. Replay must arm automatically with the configured history. Preferences can disable this startup behavior.
2. Play ordinary audible content through the default Windows output device. Confirm System audio is on; otherwise report the visible audio error.
3. Start normal recording, wait five seconds, Pause, wait five seconds, Resume, record another five seconds, then Finish & save. The saved duration should be about ten seconds plus keyframe/packet boundary effects, not fifteen. Confirm audible content and A/V alignment.
4. While normal recording is running or paused, save replay by button and by Alt+F1 from another foreground application. Replay must continue independently; files should include recent history, not just normal-recording segments.
5. Disable replay and continue normal recording. Finish it; both idle/paused flows should avoid GPU capture work. Turn replay back on and check history starts fresh.
6. In Preferences, stop capture using the explicit action after recording/export completes. Change shortcut modifiers/function key and save. Test the new shortcut and a known occupied shortcut; conflicts must be visible.
7. Close during a normal recording or replay export. The window should show stopping and close after finalization. Verify saved files can be played and seeked. Forced OS/process termination is not covered by graceful close.
8. Report audio errors, drift, silent files, pause gaps or missing final clips with the corresponding Veyva log. Microphone and the timed CLI audio are intentionally outside this implementation.


## Approved C layout and themes

1. Build with `cargo build --release --offline`, then launch `target\release\veyva.exe gui`.
2. With no theme setting, verify dark appearance. Select Light in the inspector, navigate Preferences/About, restart and verify Light persists. Restore Dark and repeat.
3. Edit a capture preference without saving, change appearance, restart and verify the unsaved capture preference was not persisted.
4. Resize from each edge/corner down to the minimum size. Check that transport, replay and inspector remain reachable by scrolling at 100%, 125% and 150% Windows scaling.
5. Drag using the empty titlebar region, double-click it, minimize, maximize/restore and check Windows snap manually.
6. Start recording, pause/resume, finish/save and save a replay. Check states and errors remain visible in both themes. Closing during recording must finalize safely before the window exits.

These are pending runtime checks; compile success is not playback or visual acceptance.


## Editable capture preferences and cursor

- Change resolution/FPS/audio in the inspector while replay is armed, then Apply. Verify a single restart, history reset and resumed buffering. Discard must restore the applied values in both views.
- During recording, pause, finalization and replay export, verify Apply cannot interrupt the operation. Close during a pending apply and confirm no replacement session starts.
- Test invalid/custom dimensions, failed encoder negotiation and retry using a supported preset. Errors must remain visible.
- Record and save replay with Capture cursor enabled and disabled. Check moving pointer, text caret pointer shape, display edges and scaled output. DXGI may already embed a pointer in its desktop image; disabling separate-pointer composition cannot remove an embedded pointer.
- Check scrollbar clearance in both themes at minimum size and 125%/150% DPI.
- Compare capture overhead with cursor on/off on the same workload. Cursor composition adds a GPU desktop copy and a small cursor-region readback/upload; its overhead has not been measured.


## Destination and transport controls

- Use the folder icon to choose a destination using the native folder picker. Cancel must preserve the existing path. The adjacent arrow opens the existing destination in Explorer. Manual path editing remains available.
- Start recording: the main button becomes red Finish & save; Pause becomes Resume after pausing. Finish from either active or paused recording and verify MP4 playback. Saving disables transport until completion.
- Verify repeated clicks cannot enqueue conflicting recording commands. Replay saving remains independent.


## Keyboard shortcuts and filenames

- Preferences / Keyboard shortcuts: click each binding and enter a modifier plus key. Escape cancels; duplicate combinations are rejected. Bindings save automatically; press Delete while listening to clear to None. Restart and verify persistence. OS-reserved combinations must show registration errors.
- Clear all three bindings and verify None persists without duplicate errors. Verify static labels cannot be selected while file path text remains selectable.
- While assigning, verify no recording/replay action fires and typing does not edit other fields. Verify Ctrl+Shift+R starts/finishes recording, Ctrl+Shift+P pauses/resumes, and Alt+F1 saves replay while another application has focus.
- Verify recording status has a drawn dot with no missing-font square. Verify filenames include local calendar date/time and retain unique suffixes.


## Audio devices, microphone and background operation

1. Refresh devices in Capture / Audio. Choose system output and microphone input, enable microphone, Apply, restart and verify selections persist. Compare output-only, microphone-only and mixed recordings/replays. Check synchronization, clipping, device disconnection and unavailable-device errors. Use headphones to avoid acoustic echo.
2. Enable Keep running in the notification area when closed. Close the window, save a replay with the shortcut, then use the tray menu to Show Veyva or Save Replay. Quit must finalize active capture and terminate the process. With close-to-tray disabled, closing must exit normally.
3. Enable Start replay in the background when I sign in. Sign out/in and verify one hidden session with a visible tray icon and working replay shortcut. Disable the setting and verify the Veyva HKCU Run entry is removed. Startup uses the current executable path; moving it requires re-enabling startup.
4. Launch Veyva again while it runs: the existing window should appear without duplicate encoding or shortcut registration. Restart Windows Explorer and verify icon recovery. Tray failure must restore the window and prevent hiding.
5. Measure hidden-mode CPU/GPU/memory on the target PC; hiding the GUI does not remove capture/encode costs. Abrupt Windows session termination/power loss has not been verified for recording finalization.

Background and startup default to disabled; microphone also defaults to disabled. Audio device and microphone changes use Apply. Background toggles save immediately. No Windows service or elevated installation is involved.


## Hidden tray command regression

Hide Veyva using close-to-tray. A single left click or Show Veyva must restore it. Hide again and use Save Replay; the replay must save without revealing the window. Test the replay hotkey while hidden. Quit must restore the closing window if needed, finalize and exit. Repeat with login/background launch and a second executable launch. These checks remain manual; incremental Clippy does not verify native message delivery.


## Tray shutdown deadlock regression

Disable close-to-tray, close the window with replay active, and verify both the window and process exit after finalization. Repeat with replay stopped and via tray Quit. Then re-enable close-to-tray and verify Show, Save Replay and the global shortcut while hidden. Native wake routing must not read a same-process window title from a worker: GetWindowTextW can synchronously wait on the UI while the UI joins that worker. The wake helper now uses PID and pinned winit class metadata only; normal tray shutdown does not wake the exiting UI.
