# Changelog

Release entries use `## [VERSION] - YYYY-MM-DD`. Move completed changes
from Unreleased into a versioned section before pushing the matching `v` tag.

## [Unreleased]

### Fixed
- Add an optional, default-off preference to show the Capture window once when a new audio capture error occurs while hidden in the tray. Wake the hidden window when audio fails and log the error.

## [0.1.0] - 2026-10-30

First stable release for Windows x64.

### Added
- Hardware H.264 desktop recording with pause/resume and MP4 export.
- Bounded instant replay with global shortcuts and a shared video encoder for compatible recording sessions.
- System output and optional microphone capture mixed into AAC, with source level meters.
- Custom resolution, FPS, bitrate, replay history, payload budget, cursor capture and keyboard shortcuts.
- Optional background tray operation, Windows login startup and replay success sound.
- Optional all-output capture excluding Veyva on supported Windows versions.
- Light/dark appearance, custom window controls and persistent window placement.
- Portable binary and matching source packages with GPL-3.0-only licensing, upstream notices, locked dependency sources and offline build instructions.

### Changed
- Custom output folders are remembered across launches; Videos/Veyva remains the default when no selection is saved.
- System audio and microphone controls share the same toggle, device selector and meter layout. Both toggles are available in Preferences.
- Capture and audio preferences save automatically; compatible recording and replay share capture resources.
- Replay success sound is configured in Preferences and works independently of capture audio selection.
- Clip names use a lowercase product prefix, local date/time and recording type, with collision protection.
- Alt+F4 exits; the custom close button follows the close-to-tray preference. Desktop launches do not open a console.

### Fixed
- Recover desktop duplication after access loss with bounded retries and English capture errors.
- Clear inactive replay/audio status when a session ends.
- Check output disk space before export and periodically during normal recording, attempting early finalization when space is low.
- Handle dependency timestamps outside ZIP's supported range when packaging release sources.

### Known limitations
- Requires compatible graphics drivers, hardware H.264 encoding and Windows media components; there is no automatic software fallback.
- Recovery does not cover every GPU reset, adapter change or display configuration. Low-space checks cannot guarantee finalization after disk exhaustion or drive failure.
- Replay success sounds may appear in later clips or active recordings; microphones may pick up speaker playback even with process exclusion.
- Binaries are unsigned. Preferences are stored in LocalAppData, not beside the executable. History payload limits are not total process memory limits.

## [0.1.0-rc.2] - 2026-10-30

Packaging correction for the first release candidate; no capture behavior changes.
The rc.1 source packaging workflow failed before creating a draft release.

### Fixed
- Clamp out-of-range dependency timestamps to ZIP's supported range while preserving source contents and checksums.

## [0.1.0-rc.1] - 2026-10-30

First release candidate for Windows x64. Hardware compatibility and package
acceptance are still being evaluated; this is not a stable release.

### Added
- Hardware H.264 desktop capture with MP4 export; no automatic software fallback.
- Normal recording with pause/resume and bounded instant replay, sharing an encoder when compatible.
- System output and optional microphone selection, mixed AAC audio and source level meters.
- Optional all-output audio capture excluding Veyva on supported Windows versions.
- Optional replay success sound, independent of capture audio selection.
- Custom resolution, frame rate, bitrate, replay duration, payload budget and cursor capture.
- Configurable global shortcuts, optional close-to-tray and Windows login startup.
- Light/dark appearance, custom window controls and persistent window placement.
- Timestamped lowercase clip names with collision protection.
- Output-space checks, replay export preflight and attempted early recording finalization on low space.
- Portable binary and matching source packaging with locked dependency sources, offline build instructions and checksums.

### Changed
- Capture and audio preferences save automatically after editing.
- Alt+F4 exits while the custom titlebar close button follows close-to-tray.
- Desktop launches use the Windows GUI subsystem without opening an extra console.
- Original code, documentation and assets use GPL-3.0-only; third-party notices remain included.

### Fixed
- Retry desktop duplication after access loss, with a bounded reconnection period and English capture errors.
- Clear stale replay and audio indicators when a capture session ends.

### Known limitations
- Device compatibility, actual low-space recording finalization and source-archive rebuilding require release acceptance testing.
- Desktop access-loss recovery does not cover every GPU reset, adapter change or display configuration.
- Replay success sounds may appear in subsequent clips or active recordings; microphone pickup remains possible even with process exclusion.
- The application is unsigned; preferences remain in LocalAppData rather than beside the portable executable.
- Encoded history limits are not total process memory limits. No measured low-end performance guarantee is made.
