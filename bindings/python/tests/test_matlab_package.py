"""Rebuilding a source package must remove obsolete ZIP members."""
import os
import shutil
import subprocess
import tempfile
import unittest
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


@unittest.skipUnless(shutil.which("zip"), "ZIP packaging tool required")
class MatlabPackageTest(unittest.TestCase):
    def test_rebuild_is_reproducible_and_drops_stale_members(self):
        with tempfile.TemporaryDirectory(prefix="matlab package ") as directory:
            env = dict(os.environ, SOURCE_DATE_EPOCH="315532800")
            command = ["bash", "scripts/build-matlab-package.sh", directory]
            first = subprocess.run(command, cwd=ROOT, env=env, text=True, capture_output=True, check=True)
            archive = Path(first.stdout.strip())
            expected = archive.read_bytes()
            with zipfile.ZipFile(archive) as package:
                self.assertIn("nirs4all/tests/native_runtime.m", package.namelist())
                self.assertIn("nirs4all/tests/fixtures/workflow_dense.json", package.namelist())
            with zipfile.ZipFile(archive, "a") as package:
                package.writestr("nirs4all/obsolete.m", "obsolete release member")
            subprocess.run(command, cwd=ROOT, env=env, text=True, capture_output=True, check=True)
            self.assertEqual(archive.read_bytes(), expected)
            with zipfile.ZipFile(archive) as package:
                self.assertNotIn("nirs4all/obsolete.m", package.namelist())


if __name__ == "__main__":
    unittest.main()
