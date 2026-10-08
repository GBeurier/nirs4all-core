"""The Python Archive V2 facade only delegates to the native aggregate."""

from __future__ import annotations

from collections.abc import Iterator
from contextlib import contextmanager
import json
from pathlib import Path
import sys
import types
import unittest
from unittest.mock import patch

from nirs4all_core import (
    NativeArchiveUnavailableError,
    read_archive_v3_view,
    inspect_methods_archive_v2_predictors,
    predict_methods_archive_v2_matrix,
    read_portable_predictor_package_v2,
    read_portable_refit_package_v3,
    replay_methods_archive_v2,
    replay_methods_archive_v2_conformal_presentation_v1,
    replay_methods_archive_v2_conformal_presentation_v2,
    replay_methods_archive_v3,
    write_archive_v2_from_native_payloads,
    write_archive_v3_from_native_payloads,
)


# These paths are mock inputs; use the host OS representation in exact assertions.
MODEL_PATH = str(Path("/tmp/model.n4a"))
REFIT_PATH = str(Path("/tmp/refit.n4a"))
CALIBRATED_PATH = str(Path("/tmp/calibrated.n4a"))
METHODS_LIBRARY_PATH = str(Path("/opt/lib/libn4m.so"))


@contextmanager
def _mock_native_bridge(module: object) -> Iterator[None]:
    """Isolate both Python caches without unloading an existing native bridge."""
    package = sys.modules["nirs4all_core"]
    absent = object()
    original = package.__dict__.pop("_native", absent)
    try:
        with patch.dict(sys.modules, {"nirs4all_core._native": module}):
            yield
    finally:
        package.__dict__.pop("_native", None)
        if original is not absent:
            package.__dict__["_native"] = original


class ArchiveFacadeTests(unittest.TestCase):
    def test_v2_native_predictor_inspection_returns_only_native_descriptor_json(self) -> None:
        observed: list[str] = []
        descriptor = {
            "descriptor_type": "dagml.native_predictor_descriptor.v1",
            "schema_version": 1,
            "artifact_sha256": "a" * 64,
            "owner_controller": "controller:methods.pls",
            "format": "N4MM",
            "format_version": 2,
            "writer_abi": {"major": 2, "minor": 5, "patch": 0},
            "storage_algorithm": 0,
            "capabilities": 3,
            "dimensions": {
                "training_samples": 4,
                "n_features": 2,
                "n_targets": 1,
                "n_components": 1,
            },
            "pipeline": {
                "pipeline_type": "n4m.snv_savgol_smooth.v1",
                "schema_version": 1,
                "operator_count": 2,
                "raw_n_features": 2,
                "model_n_features": 2,
                "fingerprint_algorithm": "fnv1a64.v1",
                "native_fingerprint": "0123456789abcdef",
                "savgol_window": 11,
                "savgol_poly_degree": 2,
            },
            "descriptor_fingerprint": "b" * 64,
        }

        def inspect(path: str, library: str, sha256: str) -> str:
            observed.extend([path, library, sha256])
            return json.dumps([descriptor])

        module = types.SimpleNamespace(
            inspect_methods_archive_v2_predictors_json=inspect
        )
        with _mock_native_bridge(module):
            result = inspect_methods_archive_v2_predictors(
                MODEL_PATH,
                methods_library_path=METHODS_LIBRARY_PATH,
                methods_library_sha256="c" * 64,
            )

        self.assertEqual(result, [descriptor])
        self.assertEqual(
            observed,
            [MODEL_PATH, METHODS_LIBRARY_PATH, "c" * 64],
        )

    def test_returns_exact_native_package_bytes(self) -> None:
        observed: list[str] = []

        def read(path: str) -> bytes:
            observed.append(path)
            return b'{"schema_version":2}'

        module = types.SimpleNamespace(read_portable_predictor_package_v2=read)
        with _mock_native_bridge(module):
            result = read_portable_predictor_package_v2(MODEL_PATH)

        self.assertEqual(result, b'{"schema_version":2}')
        self.assertEqual(observed, [MODEL_PATH])

    def test_missing_native_bridge_fails_closed(self) -> None:
        with _mock_native_bridge(None):
            with self.assertRaises(NativeArchiveUnavailableError):
                read_portable_predictor_package_v2(MODEL_PATH)

    def test_returns_exact_native_refit_package_v3_bytes(self) -> None:
        observed: list[str] = []

        def read(path: str) -> bytes:
            observed.append(path)
            return b'{"schema_version":3}'

        module = types.SimpleNamespace(read_portable_refit_package_v3=read)
        with _mock_native_bridge(module):
            result = read_portable_refit_package_v3(MODEL_PATH)

        self.assertEqual(result, b'{"schema_version":3}')
        self.assertEqual(observed, [MODEL_PATH])

    def test_v3_view_missing_native_bridge_fails_closed(self) -> None:
        with _mock_native_bridge(None):
            with self.assertRaises(NativeArchiveUnavailableError):
                read_archive_v3_view(MODEL_PATH)

    def test_returns_native_v3_host_view_without_python_archive_logic(self) -> None:
        observed: list[str] = []
        expected = {
            "archive_id": "archive:v3-test",
            "schema_version": 3,
            "replay": {"execution_status": "requires_native_artifact_executor"},
            "methods": {"n4mm": []},
        }

        def view(path: str) -> dict[str, object]:
            observed.append(path)
            return expected

        module = types.SimpleNamespace(read_archive_v3_view=view)
        with _mock_native_bridge(module):
            result = read_archive_v3_view(MODEL_PATH)

        self.assertEqual(result, expected)
        self.assertEqual(observed, [MODEL_PATH])

    def test_v2_replay_forwards_strict_json_without_callbacks(self) -> None:
        observed: list[object] = []

        def replay(*args: object) -> str:
            observed.extend(args)
            return '{"outcome_id":"outcome:predict","schema_version":2}'

        module = types.SimpleNamespace(replay_methods_archive_v2_json=replay)
        with _mock_native_bridge(module):
            outcome = replay_methods_archive_v2(
                MODEL_PATH,
                {"schema_version": 1, "request_id": "request:predict"},
                {"node.input": {"schema_version": 2}},
                {
                    "node.input": {
                        "sample_ids": ["sample:1"],
                        "x": [[1.0]],
                        "target_names": ["y"],
                    }
                },
                methods_library_path=METHODS_LIBRARY_PATH,
                outcome_id="outcome:predict",
                run_id="run:predict",
                warnings=["portable"],
                diagnostics={"source": "test"},
            )

        self.assertEqual(outcome["outcome_id"], "outcome:predict")
        self.assertEqual(observed[0], MODEL_PATH)
        self.assertEqual(
            observed[1],
            '{"request_id":"request:predict","schema_version":1}',
        )
        self.assertEqual(observed[4:7], [METHODS_LIBRARY_PATH, "outcome:predict", "run:predict"])
        self.assertEqual(observed[7], '["portable"]')
        self.assertEqual(observed[8], '{"source":"test"}')

    def test_v2_matrix_prediction_delegates_one_closed_native_contract(self) -> None:
        observed: list[object] = []

        def predict(*args: object) -> str:
            observed.extend(args)
            return '{"outcome_id":"outcome:matrix","schema_version":3}'

        module = types.SimpleNamespace(predict_methods_archive_v2_matrix_json=predict)
        with _mock_native_bridge(module):
            outcome = predict_methods_archive_v2_matrix(
                MODEL_PATH,
                ["sample:1", "sample:2"],
                [[1.0, 2.0], [3.0, 4.0]],
                ["protein", "moisture"],
                methods_library_path=METHODS_LIBRARY_PATH,
                methods_library_sha256="a" * 64,
                request_id="request:matrix",
                outcome_id="outcome:matrix",
                run_id="run:matrix",
                diagnostics={"source": "test"},
            )

        self.assertEqual(outcome["outcome_id"], "outcome:matrix")
        self.assertEqual(observed[0], MODEL_PATH)
        self.assertEqual(
            json.loads(str(observed[1])),
            {
                "sample_ids": ["sample:1", "sample:2"],
                "x": [[1.0, 2.0], [3.0, 4.0]],
                "expected_target_names": ["protein", "moisture"],
                "methods_library_path": METHODS_LIBRARY_PATH,
                "methods_library_sha256": "a" * 64,
                "request_id": "request:matrix",
                "outcome_id": "outcome:matrix",
                "run_id": "run:matrix",
                "warnings": [],
                "diagnostics": {"source": "test"},
            },
        )

    def test_v2_matrix_prediction_refuses_non_finite_x_before_native_call(self) -> None:
        called = False

        def predict(*_: object) -> str:
            nonlocal called
            called = True
            return "{}"

        module = types.SimpleNamespace(predict_methods_archive_v2_matrix_json=predict)
        with _mock_native_bridge(module):
            with self.assertRaises(TypeError):
                predict_methods_archive_v2_matrix(
                    MODEL_PATH,
                    ["sample:1"],
                    [[float("nan")]],
                    ["protein"],
                    methods_library_path=METHODS_LIBRARY_PATH,
                    methods_library_sha256="a" * 64,
                    request_id="request:matrix",
                    outcome_id="outcome:matrix",
                    run_id="run:matrix",
                )
        self.assertFalse(called)

    def test_v3_replay_uses_distinct_native_entry_point(self) -> None:
        observed: list[object] = []

        def replay(*args: object) -> str:
            observed.extend(args)
            return '{"outcome_id":"outcome:refit","schema_version":3}'

        module = types.SimpleNamespace(replay_methods_archive_v3_json=replay)
        with _mock_native_bridge(module):
            outcome = replay_methods_archive_v3(
                REFIT_PATH,
                "{}",
                "{}",
                "{}",
                methods_library_path=METHODS_LIBRARY_PATH,
                outcome_id="outcome:refit",
                run_id="run:refit",
            )

        self.assertEqual(outcome, {"outcome_id": "outcome:refit", "schema_version": 3})
        self.assertEqual(observed[0], REFIT_PATH)
        self.assertEqual(observed[7:], ["[]", "{}"])

    def test_conformal_presentation_uses_distinct_native_entry_point(self) -> None:
        observed: list[object] = []

        def replay(*args: object) -> str:
            observed.extend(args)
            return '{"schema_version":1,"presentation_fingerprint":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}'

        module = types.SimpleNamespace(
            replay_methods_archive_v2_conformal_presentation_v1_json=replay
        )
        with _mock_native_bridge(module):
            presentation = replay_methods_archive_v2_conformal_presentation_v1(
                CALIBRATED_PATH,
                {"schema_version": 1},
                {"node.input": {"schema_version": 1}},
                {"node.input": {"sample_ids": ["sample:1"], "x": [[1.0]], "target_names": ["y"]}},
                methods_library_path=METHODS_LIBRARY_PATH,
                outcome_id="outcome:conformal.predict",
                run_id="run:conformal.predict",
            )

        self.assertEqual(presentation["schema_version"], 1)
        self.assertEqual(observed[0], CALIBRATED_PATH)
        self.assertEqual(observed[4:7], [METHODS_LIBRARY_PATH, "outcome:conformal.predict", "run:conformal.predict"])

    def test_conformal_presentation_missing_native_entry_point_fails_closed(self) -> None:
        module = types.SimpleNamespace(replay_methods_archive_v2_json=lambda *_: "{}")
        with _mock_native_bridge(module):
            with self.assertRaises(NativeArchiveUnavailableError):
                replay_methods_archive_v2_conformal_presentation_v1(
                    CALIBRATED_PATH,
                    {},
                    {},
                    {},
                    methods_library_path=METHODS_LIBRARY_PATH,
                    outcome_id="outcome:conformal.predict",
                    run_id="run:conformal.predict",
                )

    def test_conformal_presentation_v2_uses_native_projection(self) -> None:
        observed: list[object] = []

        def replay(*args: object) -> str:
            observed.extend(args)
            return '{"schema_version":2,"archive_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","presentation_fingerprint":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}'

        module = types.SimpleNamespace(
            replay_methods_archive_v2_conformal_presentation_v2_json=replay
        )
        with _mock_native_bridge(module):
            presentation = replay_methods_archive_v2_conformal_presentation_v2(
                CALIBRATED_PATH,
                {"schema_version": 1},
                {"node.input": {"schema_version": 1}},
                {
                    "node.input": {
                        "sample_ids": ["sample:1"],
                        "x": [[1.0]],
                        "target_names": ["a", "b"],
                    }
                },
                methods_library_path=METHODS_LIBRARY_PATH,
                outcome_id="outcome:conformal.v2.predict",
                run_id="run:conformal.v2.predict",
            )

        self.assertEqual(presentation["schema_version"], 2)
        self.assertEqual(observed[0], CALIBRATED_PATH)
        self.assertEqual(
            observed[4:7],
            [
                METHODS_LIBRARY_PATH,
                "outcome:conformal.v2.predict",
                "run:conformal.v2.predict",
            ],
        )

    def test_replay_missing_native_entry_point_fails_closed(self) -> None:
        module = types.SimpleNamespace(read_portable_predictor_package_v2=lambda _: b"{}")
        with _mock_native_bridge(module):
            with self.assertRaises(NativeArchiveUnavailableError):
                replay_methods_archive_v2(
                    MODEL_PATH,
                    {},
                    {},
                    {},
                    methods_library_path=METHODS_LIBRARY_PATH,
                    outcome_id="outcome:predict",
                    run_id="run:predict",
                )

    def test_replay_refuses_non_finite_host_json_before_native_call(self) -> None:
        called = False

        def replay(*_: object) -> str:
            nonlocal called
            called = True
            return "{}"

        module = types.SimpleNamespace(replay_methods_archive_v2_json=replay)
        with _mock_native_bridge(module):
            with self.assertRaises(TypeError):
                replay_methods_archive_v2(
                    MODEL_PATH,
                    {},
                    {},
                    {"node.input": {"x": [[float("nan")]]}},
                    methods_library_path=METHODS_LIBRARY_PATH,
                    outcome_id="outcome:predict",
                    run_id="run:predict",
                )
        self.assertFalse(called)

    def test_writer_forwards_opaque_dagml_members_without_zip_logic(self) -> None:
        observed: list[object] = []

        def write(path: str, manifest: dict[str, object], members: list[tuple[str, bytes]]) -> tuple[str, str]:
            observed.extend([path, manifest, members])
            return ("archive:v2", "a" * 64)

        module = types.SimpleNamespace(write_archive_v2_from_native_payloads=write)
        with _mock_native_bridge(module):
            reference = write_archive_v2_from_native_payloads(
                MODEL_PATH,
                {"schema_version": 2},
                {"dagml/package.json": bytearray([1, 2]), "methods/model.n4mm": b"raw"},
            )

        self.assertEqual(reference, {"archive_id": "archive:v2", "archive_sha256": "a" * 64})
        self.assertEqual(observed[0], MODEL_PATH)
        self.assertEqual(observed[1], {"schema_version": 2})
        self.assertEqual(
            observed[2],
            [("dagml/package.json", b"\x01\x02"), ("methods/model.n4mm", b"raw")],
        )

    def test_writer_rejects_non_bytes_before_native_call(self) -> None:
        with self.assertRaises(TypeError, msg="non-byte payload must be refused"):
            write_archive_v2_from_native_payloads(
                MODEL_PATH, {"schema_version": 2}, {"member": "not-bytes"}
            )

    def test_v3_writer_forwards_opaque_dagml_members_without_zip_logic(self) -> None:
        observed: list[object] = []

        def write(path: str, manifest: dict[str, object], members: list[tuple[str, bytes]]) -> tuple[str, str]:
            observed.extend([path, manifest, members])
            return ("archive:v3", "b" * 64)

        module = types.SimpleNamespace(write_archive_v3_from_native_payloads=write)
        with _mock_native_bridge(module):
            reference = write_archive_v3_from_native_payloads(
                REFIT_PATH,
                {"schema_version": 3},
                {"dagml/refit.json": memoryview(b"refit"), "methods/model.n4mm": b"raw"},
            )

        self.assertEqual(reference, {"archive_id": "archive:v3", "archive_sha256": "b" * 64})
        self.assertEqual(observed[0], REFIT_PATH)
        self.assertEqual(observed[1], {"schema_version": 3})
        self.assertEqual(
            observed[2],
            [("dagml/refit.json", b"refit"), ("methods/model.n4mm", b"raw")],
        )

    def test_v3_writer_rejects_non_bytes_before_native_call(self) -> None:
        with self.assertRaises(TypeError, msg="non-byte payload must be refused"):
            write_archive_v3_from_native_payloads(
                REFIT_PATH, {"schema_version": 3}, {"member": "not-bytes"}
            )
