# Changelog

Release entries use `## [VERSION] - YYYY-MM-DD`. Move completed changes
from Unreleased into a versioned section before pushing the matching `v` tag.

## [Unreleased]

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
