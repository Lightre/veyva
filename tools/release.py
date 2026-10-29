"""Validate tagged versions and package a Windows portable build; no publication."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def release_version(tag, manifest, changelog):
    pattern = r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?"
    if not re.fullmatch(pattern, tag):
        raise ValueError("Expected a vMAJOR.MINOR.PATCH tag with an optional prerelease suffix")
    version = tag[1:]
    if manifest["package"]["version"] != version:
        raise ValueError("Tag does not match Cargo.toml package version")
    sections = list(re.finditer(r"^## \[([^\]]+)\].*$", changelog, re.MULTILINE))
    matches = [i for i, item in enumerate(sections) if item.group(1) == version]
    if len(matches) != 1:
        raise ValueError("CHANGELOG.md must contain exactly one section for the tagged version")
    i = matches[0]
    end = sections[i + 1].start() if i + 1 < len(sections) else len(changelog)
    notes = changelog[sections[i].end():end].strip()
    if not notes or not re.search(r"^[-*] \S", notes, re.MULTILINE):
        raise ValueError("The release changelog section needs at least one entry")
    return version, notes


def validate_lock(version, lock):
    packages = [p for p in lock["package"] if p["name"] == "veyva"]
    if len(packages) != 1 or packages[0]["version"] != version:
        raise ValueError("Cargo.lock root package version does not match the tag")


def validate_project(tag):
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    version, notes = release_version(tag, manifest, (ROOT / "CHANGELOG.md").read_text(encoding="utf-8"))
    validate_lock(version, tomllib.loads((ROOT / "Cargo.lock").read_text(encoding="utf-8")))
    return version, notes


def imported_dlls(path):
    data = Path(path).read_bytes()
    if data[:2] != b"MZ":
        raise ValueError("Not a Windows executable")
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\x00\x00":
        raise ValueError("Invalid PE signature")
    machine, count = struct.unpack_from("<HH", data, pe + 4)
    if machine != 0x8664:
        raise ValueError("Release executable must be Windows x64")
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    if struct.unpack_from("<H", data, optional)[0] != 0x20B:
        raise ValueError("Expected PE32+ executable")
    directory = optional + 112
    import_rva = struct.unpack_from("<I", data, directory + 8)[0]
    sections = []
    for index in range(count):
        start = optional + optional_size + index * 40
        virtual_size, address, raw_size, offset = struct.unpack_from("<IIII", data, start + 8)
        sections.append((address, max(virtual_size, raw_size), offset))

    def file_offset(rva):
        for address, size, offset in sections:
            if address <= rva < address + size:
                return offset + rva - address
        raise ValueError("Invalid PE import address")

    if not import_rva:
        return []
    cursor = file_offset(import_rva)
    names = []
    while any(struct.unpack_from("<IIIII", data, cursor)):
        name_rva = struct.unpack_from("<I", data, cursor + 12)[0]
        start = file_offset(name_rva)
        names.append(data[start:data.index(b"\x00", start)].decode("ascii"))
        cursor += 20
    return sorted(set(names), key=str.lower)


def validate_desktop_subsystem(path):
    data = Path(path).read_bytes()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    # imported_dlls validates the x64 PE headers before packaging reaches here.
    subsystem = struct.unpack_from("<H", data, pe + 24 + 68)[0]
    if subsystem != 2:
        raise ValueError("Release executable must use the Windows GUI subsystem")


def dependency_notices(metadata):
    blocks = ["Veyva dependency license inventory", "Generated from locked Cargo metadata; review before distribution."]
    for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
        if package["name"] == "veyva":
            continue
        directory = Path(package["manifest_path"]).parent
        blocks.append(f"\n{'=' * 72}\n{package['name']} {package['version']}\nLicense: {package.get('license') or 'See upstream license file'}\nSource: {package.get('repository') or package.get('source') or 'Local source'}")
        files = {p for p in directory.iterdir() if p.is_file() and p.name.upper().startswith(("LICENSE", "COPYING", "NOTICE"))}
        if package.get("license_file"):
            explicit = directory / package["license_file"]
            if not explicit.is_file():
                raise ValueError(f"Missing license file for {package['name']}")
            files.add(explicit)
        for path in sorted(files):
            blocks.append(f"\n--- {path.name} ---\n{path.read_text(encoding='utf-8')}")
        if not files:
            blocks.append("No license text at the crate root. Review the declared license and upstream source before publishing.")
    return "\n".join(blocks) + "\n"


def package_release(tag):
    _, notes = validate_project(tag)
    binary = ROOT / "target/release/veyva.exe"
    imports = imported_dlls(binary)
    validate_desktop_subsystem(binary)
    if any(name.lower().startswith(("vcruntime", "msvcp")) for name in imports):
        raise ValueError("Portable release unexpectedly depends on a dynamic MSVC runtime")
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--offline", "--format-version", "1"], cwd=ROOT, text=True))
    folder = f"veyva-{tag}-windows-x64"
    output = ROOT / "dist"
    output.mkdir(exist_ok=True)
    archive = output / f"{folder}.zip"
    if archive.exists():
        raise ValueError("Release archive already exists; refusing to overwrite")
    notices = dependency_notices(metadata)
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
        for source, name in [(binary, "veyva.exe"), (ROOT / "README.md", "README.md"), (ROOT / "THIRD_PARTY_NOTICES.txt", "THIRD_PARTY_NOTICES.txt")]:
            bundle.write(source, f"{folder}/{name}")
        bundle.writestr(f"{folder}/DEPENDENCY_LICENSES.txt", notices)
        bundle.writestr(f"{folder}/IMPORTS.txt", "\n".join(imports) + "\n")
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    (output / "SHA256SUMS.txt").write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
    (output / "release-notes.md").write_text(notes + "\n", encoding="utf-8")
    print(archive.name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["validate", "package", "imports"])
    parser.add_argument("--tag")
    parser.add_argument("--exe", type=Path, default=ROOT / "target/release/veyva.exe")
    args = parser.parse_args()
    if args.command == "imports":
        print("\n".join(imported_dlls(args.exe)))
    elif args.command == "package":
        package_release(args.tag or "")
    else:
        version, _ = validate_project(args.tag or "")
        print(version)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"Release preparation failed: {error}")
