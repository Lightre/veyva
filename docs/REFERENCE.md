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

No reference source code or artwork is included in this initial scaffold. Do not copy upstream updater endpoints, signing keys, application identifiers, branding or release configuration. This repository currently has no distribution license assigned to its original code; select one before public distribution.
