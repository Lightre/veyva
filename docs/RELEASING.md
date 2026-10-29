# Releases

## Distribution

The initial release format is a Windows x64 portable ZIP. Extract it into a stable directory and run `veyva.exe`; there is no MSI installation. Preferences remain under `%LOCALAPPDATA%\Veyva`, and clips default to the user's Videos directory. This is installation-free distribution, not a mode that stores all settings beside the executable.

A compatible D3D11 GPU, hardware H.264 encoder, OpenGL support and Windows media components are still required. The workflow statically links the MSVC runtime and rejects normal PE imports of dynamic VCRUNTIME/MSVCP libraries. This check does not prove compatibility on every Windows installation; media components, drivers and DLLs loaded indirectly still require a clean-machine smoke test. Binaries are unsigned.

MSI packaging can be added when managed installation, uninstall registration or machine-wide deployment is needed. It is not necessary for the current per-user application.

## Version preparation

1. Finish acceptance testing and merge the version preparation PR into `main` with passing Windows CI.
2. Update `Cargo.toml` and the root package version in `Cargo.lock` together. Use the same SemVer version for both and the changelog heading. Do not retag an existing release.
3. Move release entries from Unreleased into a dated section such as `## [0.1.0] - YYYY-MM-DD`. Leave Unreleased for later changes. The initial changelog intentionally has no published version section.
4. After merge, create an annotated `vMAJOR.MINOR.PATCH` tag on that commit and push that specific tag. Optional prerelease suffixes such as `v0.1.0-rc.1` are supported; set the Cargo version and changelog to `0.1.0-rc.1` as well.

Example commands, only after the corresponding version is prepared:

```powershell
git switch main
git pull --ff-only
git tag -a v0.1.0 -m "release: v0.1.0"
git push origin v0.1.0
```

## Automation

The Portable release workflow verifies that the tag is contained in `main`, checks its version against Cargo.toml and the changelog, tests the packaging helpers, builds a locked static-runtime x64 executable and creates a ZIP plus SHA-256 checksum file.

The ZIP contains the executable, README, upstream notices, a Cargo dependency license inventory with available crate-root license texts, and a normal-import DLL inventory. The dependency inventory is deliberately conservative and may include build-only or other-platform crates; it does not replace a distribution license review.

Release notes start with the exact matching changelog section. GitHub-generated release notes are appended beneath it. Missing/empty/duplicate changelog sections or mismatched versions stop the workflow. Existing release assets are not silently overwritten; investigate a partial draft before rerunning.

The result is a **draft release**, not an automatically published release. A prerelease tag also marks the draft as a prerelease. No tag or release is created merely by merging the workflow PR.

Before publishing:

- Review the original-code distribution license, dependency license inventory and notices. Original Veyva code has not yet been assigned a public distribution license.
- Download the ZIP, verify SHA256SUMS.txt and test on a clean Windows machine, including recording/audio, tray, shortcuts and startup.
- Review changelog/generated notes, then publish the draft from GitHub Releases.

For local validation without a build: `python -m unittest discover -s tools/tests`. `python tools/release.py validate --tag v0.1.0` validates a prepared version without creating a package or release. `python tools/release.py imports --exe PATH` inspects a built PE file without running it.
