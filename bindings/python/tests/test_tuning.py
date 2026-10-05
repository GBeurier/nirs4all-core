"""Real native generator and optimizer checkpoint tests for the public facade."""
from __future__ import annotations

import json
import os
import struct
import tempfile
import unittest
import zipfile
from pathlib import Path

from nirs4all_core._tuning import generate, load_tuning, tune


@unittest.skipUnless(os.environ.get("NIRS4ALL_CORE_CLI"), "requires the native Core CLI")
class GenerationTest(unittest.TestCase):
    def test_native_constraints_shapes_and_seeded_subset(self) -> None:
        choices = {"shape": [[2, 3], [4, 5]], "selection": ["one", "two"]}
        constraints = {"exclude": [[{"dimension": "shape", "label": "choice:1"},
                                    {"dimension": "selection", "label": "choice:1"}]]}
        all_variants = generate(choices, constraints=constraints, seed=17)
        self.assertEqual(len(all_variants), 3)
        sampled = generate(choices, strategy="random", count=2, constraints=constraints, seed=17)
        self.assertEqual(sampled, generate(choices, strategy="random", count=2, constraints=constraints, seed=17))
        self.assertTrue(all(variant in all_variants for variant in sampled))
        self.assertEqual(len({variant["variant_id"] for variant in sampled}), 2)
        with self.assertRaises(RuntimeError):
            generate(choices, strategy="random", count=4, constraints=constraints)


@unittest.skipUnless(os.environ.get("NIRS4ALL_TUNING_DATASET") and os.environ.get("N4M_LIBRARY_PATH"),
                     "requires a native dense dataset and Methods runtime")
class TuningTest(unittest.TestCase):
    def setUp(self) -> None:
        self.data = json.loads(Path(os.environ["NIRS4ALL_TUNING_DATASET"]).read_text())
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.path = Path(self.temporary.name)

    def test_cold_resume_preserves_completed_fits_and_matches_continuous_study(self) -> None:
        first = tune(self.data, trials=2, seed=91, archive=self.path / "first.n4a", run_id="run:tuning:test:first")
        resumed = first.resume(self.data, trials=4, archive=self.path / "resumed.n4a", run_id="run:tuning:test:resume")
        continuous = tune(self.data, trials=4, seed=91, archive=self.path / "continuous.n4a", run_id="run:tuning:test:continuous")
        old = first.trials()
        new = resumed.trials()
        self.assertEqual(new[:2], old)
        self.assertEqual(len(new), 4)
        self.assertEqual([(trial["trial"]["parameters"], trial["trial"]["score"]) for trial in new],
                         [(trial["trial"]["parameters"], trial["trial"]["score"]) for trial in continuous.trials()])
        candidates = resumed.outcome["training_outcome"]["methods_hpo_resume_state"]["candidates"]
        old_fits = [line for candidate in candidates[:2] for line in candidate["lineage"] if line["phase"] == "FIT_CV"]
        new_fits = [line for candidate in candidates[2:] for line in candidate["lineage"] if line["phase"] == "FIT_CV"]
        self.assertEqual(len(old_fits), 4)
        self.assertEqual(len(new_fits), 4)
        self.assertEqual({line["run_id"] for line in old_fits}, {"run:tuning:test:first"})
        self.assertEqual({line["run_id"] for line in new_fits}, {"run:tuning:test:resume"})
        rejected = self.path / "rejected.n4a"
        with self.assertRaises(RuntimeError):
            first.resume(self.data, trials=4, seed=92, archive=rejected)
        self.assertFalse(rejected.exists())
        with self.assertRaises(RuntimeError):
            resumed.resume(self.data, trials=1, archive=rejected)
        self.assertFalse(rejected.exists())
        changed = json.loads(json.dumps(self.data))
        changed["dataset"]["sources"][0]["array"]["values"][0][0] += 1.0
        with self.assertRaises(RuntimeError):
            first.resume(changed, trials=4, archive=rejected)
        self.assertFalse(rejected.exists())

    def test_export_cold_reload_native_prediction_and_metadata_tampering(self) -> None:
        model = tune(self.data, trials=2, archive=self.path / "model.n4a")
        exported = model.export(self.path / "export")
        reopened = load_tuning(exported)
        x = self.data["dataset"]["sources"][0]["array"]["values"][:3]
        predicted = reopened.predict(x, sample_ids=["new:0", "new:1", "new:2"])
        self.assertTrue(all(line["phase"] == "PREDICT" for line in predicted["lineage"]))
        self.assertEqual(predicted["outputs"][0]["predictions"][0]["sample_ids"], ["new:0", "new:1", "new:2"])
        fitted = model.outcome["training_outcome"]["outputs"][0]["predictions"][0]
        values = dict(zip(fitted["sample_ids"], fitted["values"], strict=True))
        expected = [values[id] for id in self.data["dataset"]["sample_ids"][:3]]
        for actual, reference in zip(predicted["outputs"][0]["predictions"][0]["values"], expected, strict=True):
            self.assertAlmostEqual(actual[0], reference[0], places=6)
        # The DatasetPackage training bridge declares canonical float32 storage.
        canonical_x = [[struct.unpack("f", struct.pack("f", value))[0] for value in row] for row in x]
        self.assertEqual(reopened.predict(canonical_x)["outputs"][0]["predictions"][0]["values"], expected)
        returned = reopened.trials()
        returned[0]["trial"]["parameters"] = {}
        self.assertNotEqual(reopened.trials()[0]["trial"]["parameters"], {})
        record = json.loads((exported / "tuning.json").read_text())
        record["config"]["seed"] += 1
        (exported / "tuning.json").write_text(json.dumps(record))
        with self.assertRaisesRegex(ValueError, "disagrees"):
            load_tuning(exported)
        altered = self.path / "altered.n4a"
        with zipfile.ZipFile(model.archive) as source, zipfile.ZipFile(altered, "w", compression=zipfile.ZIP_STORED) as target:
            changed = False
            for member in source.infolist():
                payload = source.read(member.filename)
                if not changed and member.filename.startswith("artifacts/"):
                    payload = payload[:-1] + bytes([payload[-1] ^ 1])
                    changed = True
                target.writestr(member, payload)
        self.assertTrue(changed)
        from nirs4all_core._workflow import predict
        with self.assertRaises(RuntimeError):
            predict(altered, x)

    def test_nontrain_rows_are_refused_before_fit_or_archive_publication(self) -> None:
        for partition in ("test", "predict"):
            with self.subTest(partition=partition):
                data = json.loads(json.dumps(self.data))
                data["dataset"]["partitions"] = {"dtype": "<U7", "shape": [12],
                                                     "values": ["train"] * 8 + [partition] * 4}
                archive = self.path / f"{partition}.n4a"
                with self.assertRaisesRegex(RuntimeError, "only train partition"):
                    tune(data, trials=1, archive=archive)
                self.assertFalse(archive.exists())


if __name__ == "__main__":
    unittest.main()
