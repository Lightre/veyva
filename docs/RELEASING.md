# Releases

## Distribution

The initial release format is a Windows x64 portable ZIP. Extract it into a stable directory and run `veyva.exe`; there is no MSI installation. Preferences remain under `%LOCALAPPDATA%\Veyva`, and clips default to the user's Videos directory. This is installation-free distribution, not a mode that stores all settings beside the executable.

A compatible D3D11 GPU, hardware H.264 encoder, OpenGL support and Windows media components are still required. The workflow statically links the MSVC runtime and rejects normal PE imports of dynamic VCRUNTIME/MSVCP libraries. Packaging also rejects a console-subsystem executable. This check does not prove compatibility on every Windows installation; media components, drivers and DLLs loaded indirectly still require a clean-machine smoke test. Binaries are unsigned.

MSI packaging can be added when managed installation, uninstall registration or machine-wide deployment is needed. It is not necessary for the current per-user application.

## Version preparation

1. Finish acceptance testing and merge the version preparation PR into `main` with passing Windows CI.
2. Update `Cargo.toml` and the root package version in `Cargo.lock` together. Use the same SemVer version for both and the changelog heading. Do not retag an existing release.
3. Move release entries from Unreleased into a dated section such as `## [0.1.0] - YYYY-MM-DD`. Leave Unreleased for later changes.
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

The portable ZIP contains the executable, README, GPL license, copyright and branding notices, upstream notices, a Cargo dependency license inventory with available crate-root license texts, and a normal-import DLL inventory. The dependency inventory is deliberately conservative and may include build-only or other-platform crates; it does not replace a distribution license review.

A separate `veyva-vVERSION-source.zip` contains the tracked project inputs, assets, build scripts, lockfile, full locked dependency sources in `vendor/`, an offline Cargo configuration, `SOURCE-README.txt` with build instructions and tool/revision details, and `SOURCE-SHA256SUMS`. Git metadata, ignored files and `AGENTS.md` are excluded. Both release archives are covered by `SHA256SUMS.txt`; GitHub's automatic source ZIP does not replace the bundled dependency source package.

Source packaging requires a clean checkout, refuses to overwrite an existing archive, uses `cargo vendor --locked --versioned-dirs` and validates dependency resolution with `cargo metadata --frozen`. Vendoring can download dependency sources on the release runner. Locally, after binary packaging, use `python tools/package_source.py --tag vVERSION`. The package builds with `cargo build --release --frozen --target x86_64-pc-windows-msvc`; general-purpose tools and Windows system libraries are not bundled. Full source-archive building still needs verification before publication.

Release notes start with the exact matching changelog section. GitHub-generated release notes are appended beneath it. Missing/empty/duplicate changelog sections or mismatched versions stop the workflow. Existing release assets are not silently overwritten; investigate a partial draft before rerunning.

The result is a **draft release**, not an automatically published release. A prerelease tag also marks the draft as a prerelease. No tag or release is created merely by merging the workflow PR.

Before publishing:

- Original Veyva code and assets use GPL-3.0-only. Review the dependency license inventory and preserve third-party notices.
- Provide equivalent access to the matching source ZIP alongside the binary ZIP. A private repository link alone does not provide source access to binary recipients. The draft must remain unpublished until both archives are available.
- Download the ZIP, verify SHA256SUMS.txt and test on a clean Windows machine, including recording/audio, tray, shortcuts and startup.
- Review changelog/generated notes, then publish the draft from GitHub Releases.

For local validation without a build: `python -m unittest discover -s tools/tests`. `python tools/release.py validate --tag v0.1.0` validates a prepared version without creating a package or release. `python tools/release.py imports --exe PATH` inspects a built PE file without running it.
