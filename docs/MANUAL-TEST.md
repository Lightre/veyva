# Desktop replay manual checks

Build once with `cargo build --release --offline`, then run `target\release\veyva.exe gui`.

1. Confirm replay is off at startup; the ink/mint window, V icon and readable controls appear. Resize to minimum size and check scrolling, labels and settings access.
2. Start replay. Wait 10 seconds; buffered duration and payload should rise while the window remains responsive. Confirm the displayed profile matches saved settings.
3. Save replay. Capture should continue while saving. Open the output folder and play the MP4, seek in it, and confirm its duration matches available history. Audio is intentionally absent.
4. Save again; files must have different names. A second save during an active export must be disabled or rejected clearly.
5. Stop replay, change FPS/bitrate/history/budget in Settings, save, restart the app and check persistence. Start a new session; history must begin empty.
6. Set a small history budget and run beyond the configured history duration. Payload must stay capped; displayed duration may be shorter than requested. Process memory includes GPU/GUI/export overhead beyond that budget.
7. Close during an export. The app should indicate stopping and remain responsive until export completes, then close.
8. With replay stopped and the window untouched, inspect Task Manager. Report unexpectedly high idle CPU/GPU use. For a running session, compare to the earlier CLI measurements only as separate observations, not equivalent benchmark runs.

If capture or save fails, preserve `%LOCALAPPDATA%\Veyva\veyva.log`, the visible error and relevant profile values. Do not delete a partial MP4 before reporting it. Do not assume an untested high-resolution/high-FPS profile is supported by the encoder.
