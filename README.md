# Veyra Replay

**Keep the moment. Keep your frames.**

A Windows replay recorder designed around modest PCs, bounded memory and hardware encoding.
Veyra is a provisional working name, not a cleared public brand.

## Current status

This repository is a development template. The executable prints the proposed Low End profile and a nominal compressed-payload estimate. It does not capture video, record audio, save clips, install shortcuts or create a tray icon. No recorder performance has been measured.

## Development

Install Rust with the MSVC toolchain and the matching Visual Studio C++ build tools.

```powershell
cargo run --offline
cargo fmt --check
cargo check --offline
```

The initial scaffold has no external dependencies. Windows capture dependencies will be added only with the capture implementation.

## First release scope

- One selected display; H.264 hardware encoding; system audio with optional microphone.
- Replay history with time and byte limits; save to a seekable MP4 without re-encoding.
- Manual recording using the same active capture/encode session where settings match.
- Global save shortcut, tray controls, explicit capture status and local diagnostics.
- Low End default: 720p, 30 FPS, 3 Mbps video, 30 seconds, 32 MiB replay-payload budget.

Hardware encoder support must be detected. Machines without a compatible encoder may require reduced quality or may be unsupported in the first release. Low overhead on every old PC is not a promise.

## Project map

- `src/`: minimal Rust entry point and profile model.
- `docs/BRAND.md`: name, palette, voice and interface direction.
- `docs/PLAN.md`: implementation order and performance validation.
- `docs/REFERENCE.md`: ClipFlow source review and provenance.
- `assets/mark.svg`: editable draft app mark.
- `AGENTS.md`: project constraints for future development.

No upstream code has been copied. The ClipFlow checkout remains unchanged.
