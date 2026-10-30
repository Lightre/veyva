import hashlib
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock
import zipfile

TOOLS = Path(__file__).parents[1]
sys.path.insert(0, str(TOOLS))
import package_source


class SourcePackageTests(unittest.TestCase):
    def test_copies_only_tracked_inputs_and_omits_agent_guidance(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "project"
            destination = Path(temporary) / "source"
            root.mkdir()
            for relative in ["Cargo.toml", "LICENSE", "assets/sounds/replay-saved.wav", "AGENTS.md", "target/private.log"]:
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("fixture", encoding="utf-8")
            tracked = b"Cargo.toml\0LICENSE\0assets/sounds/replay-saved.wav\0AGENTS.md\0"
            with mock.patch.object(package_source.subprocess, "check_output", return_value=tracked):
                package_source.copy_project(root, destination)
            self.assertTrue((destination / "assets/sounds/replay-saved.wav").is_file())
            self.assertTrue((destination / "LICENSE").is_file())
            self.assertFalse((destination / "AGENTS.md").exists())
            self.assertFalse((destination / "target").exists())

    def test_archive_includes_hidden_config_vendor_notices_and_verified_checksums(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary) / "veyva-source"
            archive = Path(temporary) / "source.zip"
            inputs = {".cargo/config.toml": b"config", "vendor/crate/LICENSE": b"notice", "LICENSE": b"gpl", "assets/sounds/replay-saved.wav": b"sound"}
            for relative, content in inputs.items():
                path = directory / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(content)
            package_source.write_archive(directory, archive)
            with zipfile.ZipFile(archive) as bundle:
                for relative, content in inputs.items():
                    self.assertEqual(bundle.read(f"veyva-source/{relative}"), content)
                sums = bundle.read("veyva-source/SOURCE-SHA256SUMS").decode()
                for relative, content in inputs.items():
                    self.assertIn(f"{hashlib.sha256(content).hexdigest()}  {relative}\n", sums)
            previous = archive.read_bytes()
            with self.assertRaisesRegex(ValueError, "refusing to overwrite"):
                package_source.write_archive(directory, archive)
            self.assertEqual(archive.read_bytes(), previous)

    def test_pre_1980_dependency_timestamp_is_clamped_without_changing_contents(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary) / "source"
            dependency = directory / "vendor/crate/LICENSE"
            dependency.parent.mkdir(parents=True)
            content = b"upstream license notice"
            dependency.write_bytes(content)
            os.utime(dependency, (0, 0))
            archive = Path(temporary) / "source.zip"
            package_source.write_archive(directory, archive)
            self.assertEqual(dependency.read_bytes(), content)
            with zipfile.ZipFile(archive) as bundle:
                info = bundle.getinfo("source/vendor/crate/LICENSE")
                self.assertEqual(info.date_time, (1980, 1, 1, 0, 0, 0))
                self.assertEqual(bundle.read(info), content)
                sums = bundle.read("source/SOURCE-SHA256SUMS").decode()
                self.assertIn(hashlib.sha256(content).hexdigest(), sums)

    def test_dirty_checkout_is_rejected_before_vendoring(self):
        with mock.patch.object(package_source, "validate_project", return_value=("0.1.0", "notes")), mock.patch.object(package_source.subprocess, "check_output", return_value=b" M Cargo.toml"), self.assertRaisesRegex(ValueError, "clean working tree"):
            package_source.package_source("v0.1.0")

    def test_unsafe_tracked_paths_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            with mock.patch.object(package_source.subprocess, "check_output", return_value=b"../outside\0"), self.assertRaisesRegex(ValueError, "Unsafe tracked"):
                package_source.copy_project(Path(temporary), Path(temporary) / "source")


if __name__ == "__main__":
    unittest.main()
