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
