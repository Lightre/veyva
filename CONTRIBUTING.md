# Contributing

Keep changes focused and preserve unrelated work. Use a feature or fix branch and open a pull request targeting `main`; changes are merged using squash merge.

Code, identifiers, interface text, documentation, logs and commit messages are written in English. Use concise, lowercase Conventional Commits titles, such as `fix: preserve replay timestamps`.

## Checks

```powershell
cargo fmt --check
cargo clippy --offline -- -D warnings
cargo test --offline
```

Run checks appropriate to the change. Dependency caches are required for offline commands. Describe checks performed, manual verification and remaining limitations in the pull request. Follow [manual testing](docs/MANUAL-TEST.md) for capture, audio, window or background changes; compilation does not establish runtime correctness.

Capture/encode work must remain independent of the settings UI. Keep queues, history and exports bounded; preserve decodable keyframe boundaries and audio/video timestamps. Do not silently enable software encoding. Resource and performance claims require measurements on identified hardware.

Do not commit recordings, runtime logs, local configuration, credentials or build output. Preserve third-party copyright/license notices and document the exact source revision for adapted code in [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt).

The repository has not yet assigned a public distribution license to original Veyva code. Review the licensing section of the README before reusing or distributing it.

## Assets

`assets/mark.svg` is the icon geometry source. Regenerate the PNG/ICO files with `python tools/generate_icon.py` (requires Pillow). Runtime theme colors live in `src/gui.rs`.
