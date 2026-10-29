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
