import json
import unittest
from pathlib import Path

import nirs4all_core as n4core


ROOT = Path(__file__).resolve().parents[3]
FIXTURE_DIR = ROOT / "tests" / "parity" / "fixtures"
ORACLE_PATH = ROOT / "tests" / "parity" / "expected" / "portable_python_oracle.json"


def _max_abs_diff(actual, expected):
    if len(actual) != len(expected):
        raise AssertionError(f"length mismatch: {len(actual)} != {len(expected)}")
    return max((abs(float(a) - float(e)) for a, e in zip(actual, expected)), default=0.0)


class ExecutionParityTests(unittest.TestCase):
    def test_nondefault_controls_keep_the_owner_plan_and_evaluation_population(self) -> None:
        oracle = json.loads(ORACLE_PATH.read_text(encoding="utf-8"))
        dataset = {key: oracle["dataset"][key] for key in ("X", "y", "rows", "cols")}
        for label, snv_params, model_params, components in (
            ("snv_no_scale", {"with_std": False, "ddof": 3}, {"n_components": 2}, [2]),
            ("range_keeps_scale_false", {}, {"n_components": 5, "scale": False}, [1, 2, 3]),
        ):
            model = {"model": {"class": "sklearn.cross_decomposition.PLSRegression", "params": model_params}}
            if len(components) > 1:
                model.update({"param": "n_components", "_range_": [1, 3, 1]})
            recipe = {"pipeline": [{"class": "n4m.SNV", "params": snv_params}, model]}
            with self.subTest(case=label):
                try:
                    plan = n4core.parse_execution_plan(recipe)
                    actual = n4core.run_portable_pipeline(recipe, dataset)
                except ValueError as error:
                    paths = (".params.with_std", ".params.ddof") if label == "snv_no_scale" else (".params.scale",)
                    self.assertTrue(any(path in str(error) for path in paths), str(error))
                    continue
                self.assertEqual(plan["nComponents"], components)
                self.assertEqual(actual["preprocessing"], plan["preprocessing"])
                self.assertEqual([variant["n_components"] for variant in actual["variants"]], components)
                self.assertEqual(actual["evaluation"], {"scope": "training", "independent_test": False})
                self.assertEqual(actual["targets"], dataset["y"])
                self.assertEqual(actual["split"]["trainIndices"], list(range(dataset["rows"])))
                self.assertEqual(actual["split"]["testIndices"], actual["split"]["trainIndices"])
                self.assertIn(actual["selected"], actual["variants"])
                self.assertTrue(actual["owner"]["decision"])

    def test_python_binding_execution_matches_full_nirs4all_oracle(self) -> None:
        oracle = json.loads(ORACLE_PATH.read_text(encoding="utf-8"))
        dataset = {
            "X": oracle["dataset"]["X"],
            "y": oracle["dataset"]["y"],
            "rows": oracle["dataset"]["rows"],
            "cols": oracle["dataset"]["cols"],
        }
        tolerances = oracle["metadata"]["tolerances"]

        self.assertGreaterEqual(len(oracle["cases"]), 4)
        for expected in oracle["cases"]:
            with self.subTest(case=expected["name"]):
                fixture = FIXTURE_DIR / f"{expected['name']}.json"
                actual = n4core.run_portable_pipeline(fixture, dataset)
                definition = n4core.load_pipeline_definition(fixture).as_dict()
                if actual["split"]["kind"] != "all":
                    splitter = definition["pipeline"].pop(0)
                    definition["pipeline"].insert(len(definition["pipeline"]) - 1, splitter)
                    reordered = n4core.run_portable_pipeline(definition, dataset)
                    for key in ("split", "preprocessing", "variants", "selected", "targets", "evaluation", "identities"):
                        self.assertEqual(reordered[key], actual[key], key)
                self.assertEqual(actual["evaluation"], {
                    "scope": "training" if actual["split"]["kind"] == "all" else "selection_validation",
                    "independent_test": False,
                })

                self.assertEqual(actual["split"], expected["split"])
                identities = actual["identities"]
                self.assertEqual(identities["kind"], "positional")
                ids = identities["sample_ids"]
                self.assertEqual(identities["training_sample_ids"], [ids[i] for i in actual["split"]["trainIndices"]])
                self.assertEqual(identities["evaluation_sample_ids"], [ids[i] for i in actual["split"]["testIndices"]])
                self.assertEqual(actual["profile"], "portable_linear_regression.v1")
                self.assertEqual(actual["owner_profile"], "pipeline_steps.v1")
                self.assertEqual(actual["mode"], "training" if actual["split"]["kind"] == "all" else "single_holdout")
                self.assertEqual(
                    actual["owner"]["profile"],
                    "training_bank.v1" if actual["split"]["kind"] == "all" else "single_resampled_holdout_bank.v1",
                )
                self.assertTrue(actual["owner"]["decision"])
                self.assertIn(actual["selected"], actual["variants"])
                self.assertLessEqual(
                    _max_abs_diff(actual["targets"], expected["targets"]),
                    tolerances["targets_abs"],
                )
                self.assertEqual(len(actual["variants"]), len(expected["variants"]))
                for actual_variant, expected_variant in zip(actual["variants"], expected["variants"]):
                    self.assertEqual(actual_variant["n_components"], expected_variant["n_components"])
                    self.assertLessEqual(
                        abs(actual_variant["rmse"] - expected_variant["rmse"]),
                        tolerances["rmse_abs"],
                        msg=(
                            f"{expected['name']} n_components="
                            f"{expected_variant['n_components']} RMSE diff"
                        ),
                    )
                    self.assertLessEqual(
                        _max_abs_diff(actual_variant["predictions"], expected_variant["predictions"]),
                        tolerances["predictions_abs"],
                        msg=(
                            f"{expected['name']} n_components="
                            f"{expected_variant['n_components']} prediction diff"
                        ),
                    )
                self.assertEqual(
                    actual["selected"]["n_components"],
                    expected["selected"]["n_components"],
                )


if __name__ == "__main__":
    unittest.main()
