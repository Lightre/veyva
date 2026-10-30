"""Package matching GPL source and locked dependencies; never publish a release."""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile
import zipfile

from release import ROOT, validate_project


def copy_project(root, destination):
    files = subprocess.check_output(["git", "ls-files", "-z"], cwd=root).decode("utf-8").split("\0")
    for relative in filter(None, files):
        if Path(relative).name == "AGENTS.md":
            continue
        if Path(relative).is_absolute() or ".." in Path(relative).parts:
            raise ValueError("Unsafe tracked source path")
        source = root / relative
        if source.is_symlink() or not source.is_file():
            raise ValueError(f"Unsupported or missing tracked source: {relative}")
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)


def write_archive(directory, archive):
    if archive.exists():
        raise ValueError("Source archive already exists; refusing to overwrite")
    files = sorted(path for path in directory.rglob("*") if path.is_file())
    sums = "".join(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.relative_to(directory).as_posix()}\n" for path in files)
    manifest = directory / "SOURCE-SHA256SUMS"
    manifest.write_text(sums, encoding="utf-8", newline="\n")
    # Registry sources may carry timestamps outside ZIP's 1980-2107 range.
    # Clamp archive metadata without changing vendored file contents or checksums.
    with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED, strict_timestamps=False) as bundle:
        for path in [*files, manifest]:
            bundle.write(path, f"{directory.name}/{path.relative_to(directory).as_posix()}")


def package_source(tag):
    version, _ = validate_project(tag)
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT).strip():
        raise ValueError("Source packaging requires a clean working tree")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    output = ROOT / "dist"
    output.mkdir(exist_ok=True)
    archive = output / f"veyva-{tag}-source.zip"
    if archive.exists():
        raise ValueError("Source archive already exists; refusing to overwrite")
    with tempfile.TemporaryDirectory(prefix="veyva-source-") as temporary:
        directory = Path(temporary) / f"veyva-{tag}-source"
        directory.mkdir()
        copy_project(ROOT, directory)
        config = subprocess.check_output(["cargo", "vendor", "--locked", "--versioned-dirs", "vendor"], cwd=directory, text=True)
        cargo_config = directory / ".cargo/config.toml"
        cargo_config.parent.mkdir(exist_ok=True)
        with cargo_config.open("a", encoding="utf-8") as stream:
            stream.write("\n" + config)
            stream.write('\n[target.x86_64-pc-windows-msvc]\nrustflags = ["-C", "target-feature=+crt-static"]\n')
        subprocess.run(["cargo", "metadata", "--frozen", "--format-version", "1"], cwd=directory, stdout=subprocess.DEVNULL, check=True)
        rust = subprocess.check_output(["rustc", "--version"], text=True).strip()
        cargo = subprocess.check_output(["cargo", "--version"], text=True).strip()
        instructions = f"""Veyva {version} source

Git revision: {revision}
Preparation tools: {rust}; {cargo}

This archive contains the matching source, assets, build scripts, Cargo.lock,
and locked dependency sources in vendor/. Their original notices are preserved.
See LICENSE, COPYRIGHT, BRANDING.md, and THIRD_PARTY_NOTICES.txt.

Building on Windows x64
1. Install Rust with the x86_64-pc-windows-msvc toolchain and Visual Studio
   C++ build tools, including the Windows SDK and resource compiler.
2. Open a terminal in the extracted directory containing Cargo.toml.
3. Run: cargo build --release --frozen --target x86_64-pc-windows-msvc
4. Find target/x86_64-pc-windows-msvc/release/veyva.exe.

.cargo/config.toml uses vendor/ and statically links the MSVC runtime.
--frozen uses Cargo.lock and prevents network access. General-purpose build
 tools and Windows system libraries must already be installed and are not bundled.
Tool versions identify the preparation environment; identical binary hashes
are not promised. SOURCE-SHA256SUMS covers the archive inputs except itself.

Distribute this archive alongside the matching portable binary package with
equivalent access. Preserve notices and identify changes in modified versions.
"""
        (directory / "SOURCE-README.txt").write_text(instructions, encoding="utf-8")
        write_archive(directory, archive)
    with archive.open("rb") as source:
        checksum = f"{hashlib.file_digest(source, 'sha256').hexdigest()}  {archive.name}\n"
    with (output / "SHA256SUMS.txt").open("a", encoding="utf-8") as stream:
        stream.write(checksum)
    print(archive.name)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True)
    args = parser.parse_args()
    try:
        package_source(args.tag)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"Source packaging failed: {error}")
