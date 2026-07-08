import json
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from scripts.e2e import consume_repository_descriptor as consumer


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests" / "parity" / "fixtures" / "portable_methods_pipeline.json"


def _resolution() -> dict[str, object]:
    dataset = {
        "kind": "provider_materialized_csv_nirs_matrix",
        "X": [[1.0, 2.0], [3.0, 4.0]],
        "y": [1.0, 2.0],
        "rows": 2,
        "cols": 2,
    }
    return {
        "repository": {"pipeline_id": "portable-methods", "catalog_count": 1},
        "dataset": {
            "execution_dataset": dataset,
            "execution_dataset_sha256": consumer._stable_hash(dataset),
            "execution_dataset_csv_sha256": "a" * 64,
            "io_package_summary_sha256": "b" * 64,
        },
    }


def _runtime(surface: str, predictions: list[float]) -> dict[str, object]:
    return {
        "surface": surface,
        "status": "passed",
        "name": "portable_methods_pipeline",
        "rows": 2,
        "cols": 2,
        "split": {
            "kind": "all",
            "train_count": 2,
            "test_count": 2,
            "trainIndices": [0, 1],
            "testIndices": [0, 1],
        },
        "preprocessing": [],
        "variants": [
            {
                "n_components": 1,
                "rmse": 0.25,
                "prediction_count": len(predictions),
                "predictions": predictions,
            }
        ],
        "selected": {
            "n_components": 1,
            "rmse": 0.25,
            "prediction_count": len(predictions),
            "predictions": predictions,
        },
        "target_count": len(predictions),
        "targets": [1.0, 2.0],
    }


class RepositoryDescriptorConsumerTests(unittest.TestCase):
    def test_consume_records_passed_runtime_execution_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            artifacts_dir = Path(tmp)
            shutil.copyfile(FIXTURE, artifacts_dir / consumer.PIPELINE_ARTIFACT)
            (artifacts_dir / consumer.RESOLUTION_ARTIFACT).write_text(
                json.dumps(_resolution()),
                encoding="utf-8",
            )

            with (
                mock.patch.object(
                    consumer,
                    "_load_javascript_wasm",
                    return_value={
                        "surface": "bindings/wasm",
                        "status": "passed",
                        "name": "portable_methods_pipeline",
                        "random_state": 42,
                        "classes": [
                            "nirs4all.operators.splitters.KennardStoneSplitter",
                            "nirs4all.operators.transforms.StandardNormalVariate",
                            "nirs4all.operators.transforms.SavitzkyGolay",
                            "sklearn.cross_decomposition.PLSRegression",
                        ],
                        "pipeline": json.loads(FIXTURE.read_text(encoding="utf-8")),
                    },
                ),
                mock.patch.object(
                    consumer,
                    "_run_python_execution",
                    return_value=_runtime("bindings/python", [1.25, 2.25]),
                ),
                mock.patch.object(
                    consumer,
                    "_run_r_execution",
                    return_value=_runtime("bindings/r", [1.25, 2.25]),
                ),
                mock.patch.object(
                    consumer,
                    "_run_javascript_wasm_execution",
                    return_value=_runtime("bindings/wasm", [1.25, 2.25]),
                ),
            ):
                result = consumer.consume(artifacts_dir)

        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["execution"]["status"], "passed")
        self.assertEqual(result["execution"]["dataset"]["kind"], "provider_materialized_csv_nirs_matrix")
        self.assertEqual(result["execution"]["dataset"]["source_csv_sha256"], "a" * 64)
        self.assertEqual(result["execution"]["comparison"]["status"], "passed")
        self.assertEqual(
            sorted(result["execution"]["comparisons"]),
            ["python_vs_r", "python_vs_wasm", "r_vs_wasm"],
        )
        self.assertEqual(
            [item["status"] for item in result["execution"]["runtime_results"]],
            ["passed", "passed", "passed"],
        )
        self.assertEqual(
            [item["surface"] for item in result["execution"]["runtime_results"]],
            ["bindings/python", "bindings/r", "bindings/wasm"],
        )
        self.assertNotIn("known_followups", result)

    def test_strict_prediction_comparison_fails_on_prediction_drift(self) -> None:
        comparison = consumer._strict_prediction_comparison(
            _runtime("bindings/python", [1.0, 2.0]),
            _runtime("bindings/wasm", [1.0, 2.0 + consumer.EXECUTION_TOLERANCE * 20]),
        )

        self.assertEqual(comparison["status"], "failed")
        self.assertGreater(comparison["prediction_abs_max"], consumer.EXECUTION_TOLERANCE)

    def test_runtime_execution_requires_python_r_and_wasm_evidence(self) -> None:
        with (
            mock.patch.object(
                consumer,
                "_run_python_execution",
                return_value=_runtime("bindings/python", [1.25, 2.25]),
            ),
            mock.patch.object(
                consumer,
                "_run_r_execution",
                return_value=_runtime("bindings/r", [1.25, 2.25]),
            ),
            mock.patch.object(
                consumer,
                "_run_javascript_wasm_execution",
                side_effect=RuntimeError("missing methods wasm build"),
            ),
        ):
            with self.assertRaisesRegex(AssertionError, "required runtime surface"):
                consumer._runtime_execution(FIXTURE, _resolution())

    def test_runtime_execution_requires_provider_execution_dataset(self) -> None:
        with self.assertRaisesRegex(AssertionError, "provider execution dataset"):
            consumer._runtime_execution(FIXTURE, {"repository": {"pipeline_id": "portable-methods"}})


if __name__ == "__main__":
    unittest.main()
