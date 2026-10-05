"""Actual native archive calibration, independent replay and frozen audits."""
import copy
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from nirs4all_core._conformal import (
    _uncertainty_input,
    calibrate,
    conformal_metrics,
    export_calibrated,
    load_calibrated,
    predict_calibrated,
)
from nirs4all_core._robustness import robustness
from nirs4all_core._workflow import run
from test_workflow_native import fixture


def cohort(prefix):
    data = fixture()
    ids = [prefix + value for value in data["dataset"]["sample_ids"]]
    data["origin_ids"] = ids
    data["dataset"]["sample_ids"] = ids
    data["dataset"]["sources"][0]["sample_ids"] = ids
    return data


class DataFrameTransportTest(unittest.TestCase):
    def test_dataframe_uses_explicit_positional_identities(self):
        class NumericFrame:
            columns = ("a", "b")

            def to_numpy(self):
                return [[1.0, 2.0], [3.0, 4.0]]

            def to_dict(self):
                raise AssertionError("a feature DataFrame is not an IO Dataset record")

        self.assertEqual(
            _uncertainty_input(NumericFrame(), ["a", "b"], None),
            {"x": [[1.0, 2.0], [3.0, 4.0]], "sample_ids": ["a", "b"]},
        )
        with self.assertRaisesRegex(ValueError, "requires sample_ids"):
            _uncertainty_input(NumericFrame(), None, None)


@unittest.skipUnless(os.environ.get("NIRS4ALL_CORE_CLI") and os.environ.get("N4M_LIBRARY_PATH"), "explicit native runtime required")
class NativeConformalTest(unittest.TestCase):
    def test_calibrate_independent_intervals_metrics_cold_reload_and_frozen_robustness(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            train = fixture()
            workflow = run(train, archive=root / "model.n4a")
            calibration_data = cohort("cal.")
            calibrated = calibrate(workflow, calibration_data, coverages=[0.8, 0.9], archive=root / "calibrated.n4a")
            self.assertEqual(calibrated.calibration["sample_ids"], sorted(calibration_data["dataset"]["sample_ids"]))
            independent = cohort("test.")
            x = independent["dataset"]["sources"][0]["array"]["values"]
            y = independent["dataset"]["y"]["values"]
            ids = independent["dataset"]["sample_ids"]
            prediction = predict_calibrated(calibrated, x, sample_ids=ids)
            self.assertEqual(prediction["sample_ids"], ids)
            self.assertEqual(len(prediction["interval_block"]["intervals"]), 2)
            metrics = conformal_metrics(calibrated, prediction, y, sample_ids=ids)
            self.assertEqual(len(metrics["coverages"]), 2)
            with self.assertRaisesRegex(RuntimeError, "identities"):
                conformal_metrics(calibrated, prediction, y, sample_ids=list(reversed(ids)))
            export_calibrated(calibrated, root / "relocated.n4a")
            (root / "calibrated.n4a").unlink()
            (root / "model.n4a").unlink()
            loaded = load_calibrated(root / "relocated.n4a")
            replay = predict_calibrated(loaded, x, sample_ids=ids)
            self.assertEqual(replay["interval_block"]["intervals"], prediction["interval_block"]["intervals"])
            structured = copy.deepcopy(independent)
            structured["dataset"]["y"] = None
            structured["dataset"]["partitions"] = {"dtype": "<U7", "shape": [len(ids)], "values": ["predict"] * len(ids)}
            matched = predict_calibrated(loaded, structured)
            self.assertEqual(matched["point_prediction"]["values"], replay["point_prediction"]["values"])
            singleton = predict_calibrated(loaded, x[:1], sample_ids=ids[:1])
            self.assertEqual(singleton["sample_ids"], ids[:1])
            self.assertEqual(len(conformal_metrics(loaded, singleton, y[:1], sample_ids=ids[:1])["coverages"]), 2)
            for field in ("axis_units", "feature_names", "groups", "origin_ids", "independent_unit_ids", "repetition_ids"):
                invalid = copy.deepcopy(structured)
                if field == "axis_units":
                    invalid["dataset"]["sources"][0][field] = {"wavelength": "nm"}
                elif field == "feature_names":
                    invalid["dataset"]["sources"][0][field] = ["b", "a", "c", "d", "e", "f", "g"]
                elif field == "groups":
                    invalid["dataset"][field] = {"dtype": "<U64", "shape": [len(ids)], "values": ids}
                elif field == "origin_ids":
                    invalid[field] = ["s0"] * len(ids)
                else:
                    invalid["dataset"][field] = ids
                    if field == "repetition_ids":
                        invalid["dataset"]["independent_unit_ids"] = ids
                with self.assertRaisesRegex(RuntimeError, "schema differs|independent physical"):
                    predict_calibrated(loaded, invalid)
                with self.assertRaisesRegex(RuntimeError, "schema differs|independent physical"):
                    robustness(loaded, invalid, y)
            code = "import json,sys;from nirs4all_core._conformal import load_calibrated,predict_calibrated;print(json.dumps(predict_calibrated(load_calibrated(sys.argv[1]),json.loads(sys.argv[2]),sample_ids=json.loads(sys.argv[3]))['interval_block']['intervals']))"
            cold = subprocess.run([sys.executable, "-c", code, str(root / "relocated.n4a"), json.dumps(x), json.dumps(ids)], capture_output=True, text=True, check=False)
            self.assertEqual(cold.returncode, 0, cold.stderr)
            self.assertEqual(json.loads(cold.stdout), prediction["interval_block"]["intervals"])
            for invalid_ids in (train["dataset"]["sample_ids"], calibration_data["dataset"]["sample_ids"]):
                with self.assertRaisesRegex(RuntimeError, "overlaps"):
                    predict_calibrated(loaded, x, sample_ids=invalid_ids)
            report = robustness(loaded, x, y, sample_ids=ids)
            structured_report = robustness(loaded, structured, y)
            self.assertEqual(structured_report["scenarios"][0]["point_predictions"], report["scenarios"][0]["point_predictions"])
            singleton_report = robustness(loaded, x[:1], y[:1], sample_ids=ids[:1])
            self.assertEqual(len(singleton_report["scenarios"][0]["point_predictions"]), 1)
            repeated = robustness(loaded, x, y, sample_ids=ids)
            self.assertEqual(report["mode"], "clean_frozen")
            self.assertTrue(report["audit_only"])
            self.assertEqual(report["scenarios"][0]["point_predictions"], replay["point_prediction"]["values"])
            self.assertNotEqual(report["scenarios"][0]["point_predictions"], report["scenarios"][1]["point_predictions"])
            self.assertEqual(report["scenarios"][1]["point_predictions"], repeated["scenarios"][1]["point_predictions"])
            with self.assertRaisesRegex(RuntimeError, "supported frozen"):
                robustness(loaded, x, y, sample_ids=ids, scenarios=[{"id": "x", "kind": "observed", "severity": 1.0, "seed": 0}])

    def test_calibration_overlap_schema_and_small_sample_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workflow = run(fixture(), archive=root / "model.n4a")
            with self.assertRaisesRegex(RuntimeError, "overlaps"):
                calibrate(workflow, fixture(), coverages=[0.8], archive=root / "overlap.n4a")
            self.assertFalse((root / "overlap.n4a").exists())
            wrong = cohort("cal.")
            wrong["dataset"]["sources"][0]["axis_units"] = {"wavelength": "nm"}
            with self.assertRaisesRegex(RuntimeError, "schema differs"):
                calibrate(workflow, wrong, coverages=[0.8], archive=root / "schema.n4a")
            aliases = cohort("cal.")
            aliases["origin_ids"] = ["s0"] * 12
            aliases["dataset"]["groups"] = {"dtype": "<U64", "shape": [12], "values": aliases["dataset"]["sample_ids"]}
            with self.assertRaisesRegex(RuntimeError, "independent physical"):
                calibrate(workflow, aliases, coverages=[0.8], archive=root / "aliases.n4a")
            self.assertFalse((root / "aliases.n4a").exists())
            with self.assertRaisesRegex(RuntimeError, "rank|sample|statistic|finite"):
                calibrate(workflow, cohort("cal."), coverages=[0.999], archive=root / "too-small.n4a")
            unbounded = calibrate(workflow, cohort("cal."), coverages=[0.999], small_sample_policy="unbounded", archive=root / "unbounded.n4a")
            prediction = predict_calibrated(unbounded, fixture()["dataset"]["sources"][0]["array"]["values"][:2], sample_ids=["fresh.a", "fresh.b"])
            self.assertIn("unbounded", json.dumps(prediction["interval_block"]))


if __name__ == "__main__":
    unittest.main()
