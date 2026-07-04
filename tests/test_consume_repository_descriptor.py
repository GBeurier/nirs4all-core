import json
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from scripts.e2e import consume_repository_descriptor as consumer


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests" / "parity" / "fixtures" / "portable_methods_pipeline.json"


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
                json.dumps({"repository": {"pipeline_id": "portable-methods", "catalog_count": 1}}),
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
                    "_run_javascript_wasm_execution",
                    return_value=_runtime("bindings/wasm", [1.25, 2.25]),
                ),
            ):
                result = consumer.consume(artifacts_dir)

        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["execution"]["status"], "passed")
        self.assertEqual(result["execution"]["comparison"]["status"], "passed")
        self.assertEqual(
            [item["status"] for item in result["execution"]["runtime_results"]],
            ["passed", "passed"],
        )

    def test_strict_prediction_comparison_fails_on_prediction_drift(self) -> None:
        comparison = consumer._strict_prediction_comparison(
            _runtime("bindings/python", [1.0, 2.0]),
            _runtime("bindings/wasm", [1.0, 2.0 + consumer.EXECUTION_TOLERANCE * 20]),
        )

        self.assertEqual(comparison["status"], "failed")
        self.assertGreater(comparison["prediction_abs_max"], consumer.EXECUTION_TOLERANCE)

    def test_runtime_execution_requires_python_and_wasm_evidence(self) -> None:
        with (
            mock.patch.object(
                consumer,
                "_run_python_execution",
                return_value=_runtime("bindings/python", [1.25, 2.25]),
            ),
            mock.patch.object(
                consumer,
                "_run_javascript_wasm_execution",
                side_effect=RuntimeError("missing methods wasm build"),
            ),
        ):
            with self.assertRaisesRegex(AssertionError, "required runtime surface"):
                consumer._runtime_execution(FIXTURE)


if __name__ == "__main__":
    unittest.main()
