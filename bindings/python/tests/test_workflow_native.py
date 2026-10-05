"""Native workflow qualification; enabled with explicit installed CLI/library."""
import copy
import math
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from nirs4all_core._workflow import Workflow, export, load, predict, retrain, run


def fixture():
    x = [[math.sin((r + 1) * (c + 1) * 0.13) + r * 0.11 + c * 0.2 for c in range(7)] for r in range(12)]
    ids = [f"s{i}" for i in range(12)]
    return {"schema": "nirs4all.dataset.v1", "schema_version": 1, "origin_ids": ids, "fold_ids": [None] * 12,
            "dataset": {"schema": "nirs4all.multimodal-dataset", "schema_version": 1, "name": "workflow-fixture", "sample_ids": ids,
                        "sources": [{"name": "spectra", "sample_ids": ids, "representation_id": "signal_1d", "axes": ["sample", "wavelength"],
                                     "feature_names": None, "axis_units": {}, "axis_coordinates": {}, "array": {"dtype": "float64", "shape": [12, 7], "values": x}}],
                        "y": {"dtype": "float64", "shape": [12], "values": [2 * row[0] - row[2] + i * 0.13 for i, row in enumerate(x)]},
                        "groups": None, "partitions": {"dtype": "<U5", "shape": [12], "values": ["train"] * 12}}}


@unittest.skipUnless(os.environ.get("NIRS4ALL_CORE_CLI") and os.environ.get("N4M_LIBRARY_PATH"), "explicit native workflow runtime required")
class NativeWorkflowTest(unittest.TestCase):
    def test_candidates_export_relocated_fresh_replay_and_retrain(self):
        data = fixture()
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            workdir = root / "source"
            workdir.mkdir()
            workflow = run(data, components=[1, 2], archive=workdir / "model.n4a", run_id="run:python:workflow:test")
            outcome = workflow.outcome["training_outcome"]
            self.assertEqual(len(outcome["effective_plan"]["variants"]), 2)
            self.assertEqual(len(outcome["variant_oof_averages"]), 1)
            self.assertEqual(len(outcome["oof_averages"][0]["predictions"]["unit_ids"]), 12)
            self.assertEqual(len(outcome["execution_bundle"]["refit_artifacts"]), 1)
            exported = export(workflow, root / "exported")
            import json
            manifest = json.loads((exported / "workflow.json").read_text())
            self.assertEqual(set(manifest), {"schema", "config", "training_outcome_fingerprint"})
            self.assertNotIn(str(workdir), (exported / "workflow.json").read_text())
            exported = exported.rename(root / "relocated")
            shutil.rmtree(workdir)
            loaded = load(exported)
            choices = loaded.config
            choices["components"][0] = 3
            loaded.outcome["config"]["components"][0] = 4
            self.assertEqual(loaded.config["components"], [1, 2])
            with self.assertRaises(TypeError):
                loaded._config["components"][0] = 5
            x = data["dataset"]["sources"][0]["array"]["values"][:2]
            replay = predict(loaded, x, sample_ids=["fresh1", "fresh2"])
            self.assertEqual(replay["outputs"][0]["predictions"][0]["sample_ids"], ["fresh1", "fresh2"])
            self.assertTrue(all(math.isfinite(v) for row in replay["outputs"][0]["predictions"][0]["values"] for v in row))
            code = "from nirs4all_core import load,predict; import json,sys; print(json.dumps(predict(load(sys.argv[1]),json.loads(sys.argv[2]),sample_ids=['fresh1','fresh2'])['outputs'][0]['predictions'][0]['values']))"
            completed = subprocess.run([sys.executable, "-c", code, str(exported), json.dumps(x)], capture_output=True, text=True, check=False)
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(json.loads(completed.stdout), replay["outputs"][0]["predictions"][0]["values"])
            metadata = exported / "workflow.json"
            saved = metadata.read_text()
            tampered = json.loads(saved)
            tampered["config"]["components"] = [3, 4]
            metadata.write_text(json.dumps(tampered))
            with self.assertRaisesRegex(ValueError, "differs"):
                load(exported)
            tampered = json.loads(saved)
            tampered["training_outcome_fingerprint"] = "0" * 64
            metadata.write_text(json.dumps(tampered))
            with self.assertRaisesRegex(ValueError, "differs"):
                load(exported)
            metadata.write_text(saved)
            fresh = retrain(loaded, data, archive=root / "fresh.n4a", run_id="run:python:workflow:retrain")
            self.assertEqual(fresh.config["components"], [1, 2])
            self.assertNotEqual(fresh.outcome["training_outcome"]["outcome_fingerprint"], outcome["outcome_fingerprint"])
            grouped = copy.deepcopy(data)
            grouped["dataset"]["groups"] = {"dtype": "<U5", "shape": [12], "values": [f"g{i // 2}" for i in range(12)]}
            with self.assertRaisesRegex(RuntimeError, "independent samples"):
                run(grouped, archive=root / "refused-groups.n4a")
            self.assertFalse((root / "refused-groups.n4a").exists())
            declared = copy.deepcopy(data)
            declared["fold_ids"] = [f"fold{i % 2}" for i in range(12)]
            with self.assertRaisesRegex(RuntimeError, "declared folds"):
                run(declared, archive=root / "refused-folds.n4a")
            self.assertFalse((root / "refused-folds.n4a").exists())


class WorkflowExportRollbackTest(unittest.TestCase):
    def test_copy_failure_removes_only_reserved_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source.n4a"
            source.write_bytes(b"copy transport fixture")
            model = Workflow(source, {"training_outcome": {"outcome_fingerprint": "f" * 64}}, {})
            target = root / "export"
            def fail_copy(_source, output):
                output.write(b"partial")
                raise OSError("injected copy failure")
            with patch("nirs4all_core._workflow.shutil.copyfileobj", side_effect=fail_copy), self.assertRaisesRegex(OSError, "copy failure"):
                export(model, target)
            self.assertFalse(target.exists())
            def fail_with_foreign_file(_source, output):
                output.write(b"partial")
                (target / "foreign.txt").write_text("other owner", encoding="utf-8")
                raise OSError("injected copy failure")
            with patch("nirs4all_core._workflow.shutil.copyfileobj", side_effect=fail_with_foreign_file), self.assertRaisesRegex(OSError, "copy failure"):
                export(model, target)
            self.assertEqual((target / "foreign.txt").read_text(), "other owner")
            self.assertFalse((target / "model.n4a").exists())
            self.assertFalse((target / "workflow.json").exists())
            with self.assertRaises(FileExistsError):
                export(model, target)
            self.assertEqual((target / "foreign.txt").read_text(), "other owner")


if __name__ == "__main__":
    unittest.main()
