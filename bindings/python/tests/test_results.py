"""Native result persistence and cold query across the Core facade."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import shutil
import tempfile
import unittest
from copy import deepcopy
from pathlib import Path
from unittest.mock import patch

from nirs4all_core._results import open_experiment, save_experiment


@unittest.skipUnless(importlib.util.find_spec("dag_ml") and importlib.util.find_spec("nirs4all_core._native"),
                     "optional DAG-ML binding and installed Core native extension required")
class ExperimentTest(unittest.TestCase):
    def setUp(self) -> None:
        import dag_ml

        self._temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self._temporary.cleanup)
        self.root = Path(self._temporary.name)
        self.native = self.root / "native"
        self.native.mkdir()
        scores = {
            "schema_version": 2,
            "plan_id": "plan:experiment",
            "selection_metric": "rmse",
            "reports": [
                {"variant_id": "variant:one", "partition": "validation", "fold_id": "fold:1", "level": "sample", "metrics": {"rmse": 0.4, "mae": 1.0, "bias": -0.0}, "row_count": 2},
                {"variant_id": "variant:two", "partition": "validation", "fold_id": "fold:1", "level": "sample", "metrics": {"rmse": 0.2}, "row_count": 2},
            ],
        }
        canonical = json.dumps(scores, sort_keys=True, separators=(",", ":"))
        manifest = {
            "schema_version": 2,
            "engine": "dag-ml",
            "run_id": "run:experiment",
            "selected_variant_id": "variant:two",
            "score_set_hash": hashlib.sha256(canonical.encode()).hexdigest(),
            "files": {"score_set": "score_set.json", "predictions": "predictions.parquet"},
        }
        rows = [self._row("variant:one", [1.4, 2.4]), self._row("variant:two", [1.2, 2.2])]
        dag_ml.write_native_results_v2(self.native, manifest, scores, rows)

    @staticmethod
    def _row(variant: str, predictions: list[float]) -> dict:
        return {
            "dataset": "fixture", "config_name": variant, "variant_id": variant,
            "model_name": "MethodsN4MM", "partition": "validation", "fold_id": "fold:1",
            "refit_context": "", "sample_indices": [3, 8], "sample_ids": ["s:3", "s:8"],
            "y_true": [1.0, 2.0], "y_pred": predictions, "y_proba": [],
            "y_true_shape": [2], "y_pred_shape": [2], "y_proba_shape": [],
            "weights": [], "arrays_present": True, "val_score": None,
            "test_score": None, "train_score": None, "scores": {}, "metric": "rmse",
            "task_type": "regression", "target_width": 1, "target_names": ["y"],
        }

    def test_reopen_preserves_native_scores_and_sample_identities(self) -> None:
        experiment = save_experiment(
            self.native, self.root / "saved", run_id="run:experiment", winner_variant_id="variant:two",
        )
        reopened = open_experiment(experiment.path)
        self.assertEqual(reopened.summary()["variant_ids"], ["variant:one", "variant:two"])
        self.assertEqual(reopened.winner_variant_id, "variant:two")
        self.assertEqual([report["metrics"]["rmse"] for report in reopened.compare()], [0.4, 0.2])
        self.assertEqual(reopened.predictions(variant_id="variant:two")[0]["sample_ids"], ["s:3", "s:8"])
        self.assertEqual(reopened.predictions(variant_id="variant:two")[0]["y_pred"], [1.2, 2.2])
        self.assertEqual(reopened.compare(partition="test"), [])

    def test_tampering_and_false_winner_refused(self) -> None:
        with self.assertRaisesRegex(ValueError, "absent"):
            save_experiment(self.native, self.root / "false", run_id="run:experiment", winner_variant_id="variant:other")
        save_experiment(self.native, self.root / "saved", run_id="run:experiment", winner_variant_id="variant:two")
        with (self.root / "saved" / "results" / "score_set.json").open("ab") as output:
            output.write(b" ")
        with self.assertRaisesRegex(ValueError, "hash mismatch"):
            open_experiment(self.root / "saved")

    def test_symlinked_prediction_member_refused(self) -> None:
        save_experiment(self.native, self.root / "saved", run_id="run:experiment", winner_variant_id="variant:two")
        member = self.root / "saved" / "results" / "predictions.parquet"
        member.unlink()
        member.symlink_to(self.native / "predictions.parquet")
        with self.assertRaisesRegex(ValueError, "regular file"):
            open_experiment(self.root / "saved")

    def test_queries_are_independent_and_export_keeps_native_scopes(self) -> None:
        saved = save_experiment(self.native, self.root / "saved", run_id="run:experiment", winner_variant_id="variant:two")
        scores = saved.compare()
        scores[0]["metrics"]["rmse"] = 999.0
        rows = saved.predictions()
        rows[0]["sample_ids"][0] = "changed"
        rows[0]["y_pred"][0] = 999.0
        self.assertEqual(saved.compare()[0]["metrics"]["rmse"], 0.4)
        self.assertEqual(saved.predictions()[0]["sample_ids"], ["s:3", "s:8"])
        target = saved.export(self.root / "scores.json", kind="scores", variant_id="variant:two")
        exported = json.loads(target.read_text())
        self.assertEqual(exported[0]["fold_id"], "fold:1")
        self.assertEqual(exported[0]["metrics"], {"rmse": 0.2})
        with self.assertRaises(FileExistsError):
            saved.export(target, kind="scores")
        with self.assertRaises(KeyError):
            saved.predictions(variant_id="variant:missing")

    def test_manifest_must_name_the_native_winner(self) -> None:
        manifest = json.loads((self.native / "manifest.json").read_text())
        del manifest["selected_variant_id"]
        (self.native / "manifest.json").write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, "manifest selected_variant_id"):
            save_experiment(self.native, self.root / "missing-winner", run_id="run:experiment", winner_variant_id="variant:two")

    def test_manifest_must_name_the_native_run_on_save_and_reopen(self) -> None:
        saved = save_experiment(self.native, self.root / "saved", run_id="run:experiment", winner_variant_id="variant:two")
        manifest = json.loads((self.native / "manifest.json").read_text())
        del manifest["run_id"]
        (self.native / "manifest.json").write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, "manifest run_id"):
            save_experiment(self.native, self.root / "missing-run", run_id="run:experiment", winner_variant_id="variant:two")
        self.assertFalse((self.root / "missing-run").exists())
        member = saved.path / "results" / "manifest.json"
        member.write_text(json.dumps(manifest))
        view_path = saved.path / "result_view.json"
        view = json.loads(view_path.read_text())
        view["manifest"] = manifest
        view_path.write_text(json.dumps(view))
        index_path = saved.path / "experiment.json"
        index = json.loads(index_path.read_text())
        index["results"]["manifest.json"] = hashlib.sha256(member.read_bytes()).hexdigest()
        index["result_view_sha256"] = hashlib.sha256(view_path.read_bytes()).hexdigest()
        index_path.write_text(json.dumps(index))
        with self.assertRaisesRegex(ValueError, "manifest run_id"):
            open_experiment(saved.path)

    @unittest.skipUnless(os.environ.get("NIRS4ALL_NATIVE_RESULTS_FIXTURE") and os.environ.get("NIRS4ALL_MODEL_FIXTURE"),
                         "requires real native workflow result and selected Archive V2")
    def test_real_model_closure_and_mismatched_training_refusal(self) -> None:
        native = Path(os.environ["NIRS4ALL_NATIVE_RESULTS_FIXTURE"])
        archive = Path(os.environ["NIRS4ALL_MODEL_FIXTURE"])
        manifest = json.loads((native / "manifest.json").read_text())
        saved = save_experiment(native, self.root / "model", run_id=manifest["run_id"],
                                winner_variant_id=manifest["selected_variant_id"], model_archive=archive)
        self.assertIsNotNone(open_experiment(saved.path).model_archive)
        altered = self.root / "altered"
        shutil.copytree(native, altered)
        manifest["training_outcome_fingerprint"] = "f" * 64
        (altered / "manifest.json").write_text(json.dumps(manifest))
        rejected = self.root / "rejected"
        with self.assertRaisesRegex(ValueError, "matching training outcome"):
            save_experiment(altered, rejected, run_id=manifest["run_id"],
                            winner_variant_id=manifest["selected_variant_id"], model_archive=archive)
        self.assertFalse(rejected.exists())

        # Valid native scores with unchanged run/winner fingerprints must still
        # agree with the bound model, rather than trusting provenance strings.
        import dag_ml
        view = dag_ml.read_native_results_v2(native)
        view["score_set"]["reports"][0]["metrics"]["rmse"] += 1.0
        canonical = json.dumps(view["score_set"], sort_keys=True, separators=(",", ":"))
        view["manifest"]["score_set_hash"] = hashlib.sha256(canonical.encode()).hexdigest()
        other_scores = self.root / "other_scores"
        other_scores.mkdir()
        dag_ml.write_native_results_v2(other_scores, view["manifest"], view["score_set"], view["predictions"])
        with self.assertRaisesRegex(ValueError, "scores differ"):
            save_experiment(other_scores, self.root / "wrong_scores", run_id=manifest["run_id"],
                            winner_variant_id=manifest["selected_variant_id"], model_archive=archive)
        self.assertFalse((self.root / "wrong_scores").exists())

    @unittest.skipUnless(os.environ.get("NIRS4ALL_NATIVE_RESULTS_FIXTURE") and os.environ.get("NIRS4ALL_MODEL_FIXTURE"),
                         "requires native workflow result and selected Archive V2")
    def test_rehashed_prediction_forgery_is_refused_on_save_and_open(self) -> None:
        import dag_ml
        native = Path(os.environ["NIRS4ALL_NATIVE_RESULTS_FIXTURE"])
        archive = Path(os.environ["NIRS4ALL_MODEL_FIXTURE"])
        original = dag_ml.read_native_results_v2(native)
        manifest = original["manifest"]
        for field in ("y_pred", "y_true", "sample_ids", "sample_indices", "target_names", "refit_context"):
            with self.subTest(field=field):
                view = deepcopy(original)
                row = next(row for row in view["predictions"] if row["y_true"])
                if field in ("y_pred", "y_true"):
                    row[field][0] += 1000.0
                elif field == "sample_ids" or field == "sample_indices":
                    row[field][0], row[field][1] = row[field][1], row[field][0]
                elif field == "target_names":
                    row[field][0] = "forged-target"
                else:
                    row[field] = "forged-context"
                source = self.root / field
                source.mkdir()
                dag_ml.write_native_results_v2(source, manifest, view["score_set"], view["predictions"])
                rejected = self.root / f"{field}-model"
                with self.assertRaisesRegex(ValueError, "predictions differ"):
                    save_experiment(source, rejected, run_id=manifest["run_id"],
                                    winner_variant_id=manifest["selected_variant_id"], model_archive=archive)
                self.assertFalse(rejected.exists())
                # A model-free native store proves internal consistency only.
                plain = save_experiment(source, self.root / f"{field}-plain", run_id=manifest["run_id"],
                                        winner_variant_id=manifest["selected_variant_id"])
                self.assertEqual(plain.summary()["validation_level"], "native_results")
                shutil.copyfile(archive, plain.path / "model.n4a")
                index = json.loads((plain.path / "experiment.json").read_text())
                index["model_archive"] = {"path": "model.n4a", "sha256": hashlib.sha256(archive.read_bytes()).hexdigest()}
                (plain.path / "experiment.json").write_text(json.dumps(index))
                with self.assertRaisesRegex(ValueError, "predictions differ"):
                    open_experiment(plain.path)

    @unittest.skipUnless(os.environ.get("NIRS4ALL_NATIVE_RESULTS_FIXTURE") and os.environ.get("NIRS4ALL_MODEL_FIXTURE"),
                         "requires native model closure")
    def test_changed_copy_is_verified_before_publication(self) -> None:
        import dag_ml
        native = Path(os.environ["NIRS4ALL_NATIVE_RESULTS_FIXTURE"])
        archive = Path(os.environ["NIRS4ALL_MODEL_FIXTURE"])
        forged = dag_ml.read_native_results_v2(native)
        forged["predictions"][0]["y_pred"][0] += 1000.0
        modified = self.root / "modified-copy"
        modified.mkdir()
        dag_ml.write_native_results_v2(modified, forged["manifest"], forged["score_set"], forged["predictions"])
        original_copy = shutil.copyfile
        def changing_copy(source, destination, *args, **kwargs):
            if Path(source).name == "predictions.parquet":
                source = modified / "predictions.parquet"
            return original_copy(source, destination, *args, **kwargs)
        target = self.root / "changed-during-copy"
        with patch("nirs4all_core._results.shutil.copyfile", side_effect=changing_copy), self.assertRaisesRegex(ValueError, "predictions differ"):
            save_experiment(native, target, run_id=forged["manifest"]["run_id"],
                            winner_variant_id=forged["manifest"]["selected_variant_id"], model_archive=archive)
        self.assertFalse(target.exists())
        self.assertFalse(list(self.root.glob(f".{target.name}.*")))


if __name__ == "__main__":
    unittest.main()
