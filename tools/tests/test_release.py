import importlib.util
from pathlib import Path
import unittest
import tempfile
import struct

spec = importlib.util.spec_from_file_location("release", Path(__file__).parents[1] / "release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseTests(unittest.TestCase):
    def test_extracts_only_matching_version(self):
        version, notes = release.release_version("v0.1.0", {"package": {"version": "0.1.0"}}, "## [Unreleased]\n- Later\n## [0.1.0] - 2026-10-29\n### Fixed\n- Example\n## [0.0.1]\n- Older\n")
        self.assertEqual(version, "0.1.0")
        self.assertEqual(notes, "### Fixed\n- Example")

    def test_rejects_mismatch_missing_empty_and_duplicate_notes(self):
        for tag, version, notes in [
            ("v0.2.0", "0.1.0", "## [0.2.0]\n- Entry"),
            ("v0.1.0", "0.1.0", "## [Unreleased]\n- Entry"),
            ("v0.1.0", "0.1.0", "## [0.1.0]\n"),
            ("v0.1.0", "0.1.0", "## [0.1.0]\n- A\n## [0.1.0]\n- B"),
            ("v01.1.0", "01.1.0", "## [01.1.0]\n- Entry"),
        ]:
            with self.subTest(tag=tag, notes=notes), self.assertRaises(ValueError):
                release.release_version(tag, {"package": {"version": version}}, notes)

    def test_prerelease(self):
        self.assertEqual(release.release_version("v0.1.0-rc.1", {"package": {"version": "0.1.0-rc.1"}}, "## [0.1.0-rc.1]\n- Candidate")[0], "0.1.0-rc.1")

    def test_lock_version_must_match(self):
        release.validate_lock("0.1.0", {"package": [{"name": "veyva", "version": "0.1.0"}]})
        with self.assertRaises(ValueError):
            release.validate_lock("0.1.0", {"package": [{"name": "veyva", "version": "0.2.0"}]})

    def test_reads_imports_without_executing_binary(self):
        data = bytearray(0x600)
        data[:2] = b"MZ"
        struct.pack_into("<I", data, 0x3c, 0x80)
        data[0x80:0x84] = b"PE\x00\x00"
        struct.pack_into("<HH", data, 0x84, 0x8664, 1)
        struct.pack_into("<H", data, 0x94, 240)
        struct.pack_into("<H", data, 0x98, 0x20b)
        struct.pack_into("<I", data, 0x98 + 112 + 8, 0x1000)
        struct.pack_into("<IIII", data, 0x98 + 240 + 8, 0x400, 0x1000, 0x400, 0x200)
        struct.pack_into("<I", data, 0x20c, 0x1060)
        name = b"VCRUNTIME140.dll\x00"
        data[0x260:0x260 + len(name)] = name
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "fixture.exe"
            path.write_bytes(data)
            self.assertEqual(release.imported_dlls(path), ["VCRUNTIME140.dll"])
            struct.pack_into("<H", data, 0x84, 0x14c)
            path.write_bytes(data)
            with self.assertRaises(ValueError):
                release.imported_dlls(path)


if __name__ == "__main__":
    unittest.main()
