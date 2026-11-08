"""Native bridge transport over unchanged DAG native scientific fixtures.

Run with the coordinator's rebuilt extension; no Methods runtime is required.
These tests intentionally exercise the native bridge directly so facade policy
and optional imports cannot mask bridge signature or handle-lifecycle defects.
"""

from __future__ import annotations

import hashlib
import os
import tempfile
import unittest
from pathlib import Path

try:
    from nirs4all_core import _native
except ImportError:
    _native = None

if _native is None and os.environ.get("NIRS4ALL_REQUIRE_CORE_N4A") == "1":
    raise ImportError("native Package2/3 transport requires the rebuilt Core extension")

FIXTURES = Path(__file__).resolve().parents[3] / "bindings/rust/nirs4all/tests/fixtures/native_package_transport_v1"


@unittest.skipIf(_native is None, "rebuilt nirs4all_core._native is unavailable")
class NativePackageTransportTests(unittest.TestCase):
    def test_retained_native_proof_reuses_snapshot_and_checks_binding(self) -> None:
        outcome = (FIXTURES / "training_outcome.json").read_bytes()
        package = (FIXTURES / "predictor_package.json").read_bytes()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "retained.n4a"
            _native.write_native_predictor_package(path, outcome, package, container_id="python:retained")
            with _native.open_n4a_file(path) as source:
                retained = _native.open_native_package(source)
                calibration = _native.open_native_package(source, operation="calibrate-external")
            self.assertIsInstance(retained, _native.NativePackage)
            self.assertEqual(retained.view()["package_schema_version"], 2)
            self.assertEqual(calibration.view()["operation"], "calibrate-external")
            self.assertEqual(retained.package_bytes(), package)
            self.assertEqual(retained.training_outcome_bytes(), outcome)
            self.assertEqual(retained.payloads()["package"], package)
            self.assertEqual(_native.read_native_package_view(path), retained.view())
            with self.assertRaises(ValueError):
                _native.open_native_package(retained, operation="calibrate-external")
            with self.assertRaises(ValueError):
                _native.read_portable_predictor_package_v2(calibration)
            with self.assertRaises(ValueError):
                _native.read_native_package_view(retained, entrypoint="other-entry")
            with self.assertRaises(ValueError):
                _native.read_portable_predictor_package_v2(retained, memory_budget_bytes=0)
            refused = Path(directory) / "refused-calibration.n4a"
            with self.assertRaises(ValueError):
                _native.write_calibrated_native_package(refused, retained, outcome, package, container_id="refused:binding")
            self.assertFalse(refused.exists())
            # The process-local proof owns its already-verified selected bytes.
            # Altering the original file must not cause a second closure read
            # when the caller explicitly reuses this native admission.
            path.write_bytes(b"changed after native admission")
            reused = _native.open_native_package(retained, entrypoint="predict-external")
            self.assertEqual(reused.package_bytes(), package)
            self.assertEqual(_native.read_portable_predictor_package_v2(reused), package)
            self.assertEqual(_native.read_archive_v2_payloads(reused), retained.payloads())
            self.assertEqual(_native.read_native_package_view(reused), retained.view())
            self.assertEqual(_native.read_native_package_view(calibration, operation="calibrate-external"), calibration.view())

    def test_exact_package_bytes_shared_file_receipt_and_budget(self) -> None:
        outcome = (FIXTURES / "training_outcome.json").read_bytes()
        package = (FIXTURES / "predictor_package.json").read_bytes()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "model.n4a"
            archive_id, digest = _native.write_native_predictor_package(path, outcome, package, container_id="python:role")
            self.assertEqual(archive_id, "python:role")
            self.assertEqual(digest, hashlib.sha256(path.read_bytes()).hexdigest())
            with _native.open_n4a_file(path) as handle:
                self.assertEqual(_native.read_portable_predictor_package_v2(handle), package)
                payloads = _native.read_archive_v2_payloads(handle, entrypoint="predict-external")
                self.assertEqual(payloads["package"], package)
                self.assertEqual(payloads["training_outcome"], outcome)
                self.assertEqual(payloads["manifest"]["schema"], "nirs4all.n4a.container.v1")
                self.assertEqual(payloads["view"]["archive_sha256"], digest)
                root_path = payloads["view"]["package_member"]
                self.assertEqual(payloads["members"][root_path], package)
                self.assertNotIn("dagml/portable_predictor_package.json", payloads["members"])
                with self.assertRaises(ValueError):
                    _native.read_portable_predictor_package_v2(handle, memory_budget_bytes=0)
                with self.assertRaises(ValueError):
                    _native.read_portable_predictor_package_v2(handle, entrypoint="missing")
                # Refusal must not close or replace the retained source.
                self.assertEqual(_native.read_portable_predictor_package_v2(handle), package)
            with self.assertRaises(ValueError):
                _native.read_portable_predictor_package_v2(handle)
            original = path.read_bytes()
            with self.assertRaises(ValueError):
                _native.write_native_predictor_package(path, outcome, package, container_id="replace:refused")
            self.assertEqual(path.read_bytes(), original)

    def test_standalone_child_reader_view_and_family_refusal(self) -> None:
        package = (FIXTURES / "child_refit_v3.json").read_bytes()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "child.n4a"
            _native.write_native_refit_package(path, package, container_id="python:child")
            with _native.open_n4a_file(path) as handle:
                retained = _native.open_native_package(handle)
                self.assertEqual(_native.read_portable_refit_package_v3(retained), package)
                self.assertEqual(retained.package_bytes(), package)
                self.assertIsNone(retained.payloads()["training_outcome"])
                with self.assertRaises(ValueError):
                    retained.training_outcome_bytes()
                view = _native.read_archive_v3_view(retained)
                self.assertEqual(view["schema_version"], 1)
                self.assertEqual(view["package_schema_version"], 3)
                self.assertEqual(view["root_artifact"], "full_refit")
                self.assertEqual(view["archive_sha256"], hashlib.sha256(path.read_bytes()).hexdigest())
                self.assertFalse(any(link["relation"] == "refit_of" for link in view["manifest"]["links"]))
                with self.assertRaises(ValueError):
                    _native.read_portable_predictor_package_v2(handle)

