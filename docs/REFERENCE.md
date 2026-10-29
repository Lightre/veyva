# ClipFlow reference review

Local checkout: `C:\Projects\.tools\clipflow`
Revision inspected: `d204de138d503d2eb41fcb2af553aa2430f0a78b`
Manifest version: `1.4.2`
Review date: 2026-10-29
Upstream README link: https://github.com/jaimitus/ClipFlow

## Observed source structure

`src-tauri/Cargo.toml` uses Rust, Tauri 2 and windows 0.58. The frontend manifest uses React and Vite. `src-tauri/src/media/recorder.rs` contains the DXGI/D3D11 capture, Media Foundation encoding, encoded-packet history and MP4 save path. `media/audio.rs`, `hotkeys.rs` and `settings.rs` provide focused follow-up references.

Useful concepts: GPU capture and color conversion, hardware H.264 encoding, shared encoded-packet ownership and keyframe-aligned eviction. The reference ring has time and byte pruning; its exact edge cases need independent validation before reuse. Its budget helper allows up to 1536 MiB, so its defaults should not be adopted as a low-end resource policy.

The README advertises sub-50ms saves and low RAM usage. We have not built or run the reference and have not independently verified those claims. Enqueue/snapshot latency must be distinguished from complete durable MP4 export time.

## Reuse policy

The checked-out LICENSE is MIT, copyright (c) 2026 jaimitus. Preserve that copyright and the full license text with any copied or substantially adapted source. The original license is retained in `docs/CLIPFLOW-LICENSE.txt` for future reuse.

The initial scaffold included no reference source code or artwork; the native implementation now adapts the sections documented below. Do not copy upstream updater endpoints, signing keys, application identifiers, branding or release configuration. This repository currently has no distribution license assigned to its original code; select one before public distribution.
# Source and implementation notes

The native backend was added after the initial scaffold. `src/windows_capture.rs` adapts GPU setup, video processing and Media Foundation encoder negotiation from the supplied ClipFlow revision listed above. The original MIT copyright/license is retained in source and `docs/CLIPFLOW-LICENSE.txt`.

The new standalone path uses adapter-specific MFTEnum2 enumeration, hardware-only encoding, bounded allocator-owned input surfaces, nonblocking event reads, explicit drain/finalization, bounded encoded sample retention, and non-overwriting temporary-file publication. Audio and the reference application's Tauri/React UI were not imported. This is a short-recording backend, not the completed replay engine.

API references consulted:

- https://learn.microsoft.com/en-us/windows/win32/api/mfapi/nf-mfapi-mftenum2
- https://learn.microsoft.com/en-us/windows/win32/medfound/mf-sa-d3d11-aware

The newer MF_READWRITE_USE_ONLY_HARDWARE_TRANSFORMS sink-writer flag requires Windows 11 25H2. This implementation instead activates a hardware-only MFT explicitly and gives already-encoded H.264 to the MP4 writer, avoiding that newer OS requirement.
