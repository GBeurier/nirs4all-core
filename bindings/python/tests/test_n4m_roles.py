"""Generic n4m role recipes and the cross-language trained envelope v8."""

import copy
import json
import os
import tempfile
import unittest
from pathlib import Path

import nirs4all_core as n4core

ROOT = Path(__file__).resolve().parents[3]
PYTHON_TRAINED = json.loads((ROOT / "tests/parity/fixtures/n4m_roles_v8_python_trained.json").read_text(encoding="utf-8"))
R_TRAINED = (ROOT / "tests/parity/fixtures/n4m_roles_v8_r_trained.json").read_text(encoding="utf-8")
R_ORACLE = json.loads((ROOT / "tests/parity/expected/n4m_roles_v8_r_trained_oracle.json").read_text(encoding="utf-8"))


def _max_abs_diff(actual, expected) -> float:
    if len(actual) != len(expected):
        raise AssertionError(f"length mismatch: {len(actual)} != {len(expected)}")
    return max(abs(float(a) - float(e)) for a, e in zip(actual, expected))


class N4mRoleRecipeTests(unittest.TestCase):
    def setUp(self) -> None:
        try:
            import numpy  # noqa: F401
            from n4m.roles import method_class  # noqa: F401
        except ImportError as exc:
            if os.environ.get("NIRS4ALL_CORE_REQUIRE_METHODS_PARITY") == "1":
                raise
            raise unittest.SkipTest("nirs4all-methods Python roles (ABI 2.13) are not available") from exc

    def test_tokens_resolve_through_the_methods_manifest(self) -> None:
        recipe = PYTHON_TRAINED["regression"]["envelope"]["recipe"]

        self.assertEqual(
            n4core.portable_class_names(recipe),
            ["n4m:filters.y_outlier", "n4m:preprocessing.scatter.snv", "n4m:filters.variance", "n4m:models.pls.cppls"],
        )
        self.assertEqual(n4core.load_pipeline_definition(recipe).pipeline, recipe["pipeline"])
        with self.assertRaisesRegex(ValueError, r"portable subset: n4m:not\.a\.method$"):
            n4core.load_pipeline_definition({"pipeline": ["n4m:not.a.method"]})
        with self.assertRaisesRegex(ValueError, "N4mRolePipeline"):
            n4core.parse_execution_plan(recipe)

        capabilities = {item["token"]: item for item in n4core.n4m_role_capabilities()}
        self.assertEqual(capabilities["n4m:models.pls.cppls"]["roles"], ("regressor",))
        self.assertIn("n_components", capabilities["n4m:models.pls.cppls"]["parameters"])
        self.assertNotIn("n4m:splitters.kennard_stone", capabilities)

    def test_replays_python_and_r_trained_envelopes(self) -> None:
        x_test = PYTHON_TRAINED["x_test"]
        regression = n4core.N4mRolePipeline.from_json(json.dumps(PYTHON_TRAINED["regression"]["envelope"]))
        self.assertLessEqual(_max_abs_diff(regression.predict(x_test).ravel(), PYTHON_TRAINED["regression"]["predict"]), 1e-12)
        self.assertEqual(json.loads(regression.to_json()), PYTHON_TRAINED["regression"]["envelope"])

        classification = n4core.N4mRolePipeline.from_json(json.dumps(PYTHON_TRAINED["classification"]["envelope"]))
        self.assertEqual(classification.predict(x_test).tolist(), PYTHON_TRAINED["classification"]["predict"])

        from_r = n4core.N4mRolePipeline.from_json(R_TRAINED)
        self.assertLessEqual(_max_abs_diff(from_r.predict(R_ORACLE["x_test"]).ravel(), R_ORACLE["predict"]), 1e-12)

    def test_fitted_envelopes_round_trip(self) -> None:
        x_train, x_test = PYTHON_TRAINED["x_train"], PYTHON_TRAINED["x_test"]
        for name in ("regression", "classification"):
            with self.subTest(name), tempfile.TemporaryDirectory() as directory:
                case = PYTHON_TRAINED[name]
                fitted = n4core.N4mRolePipeline.fit_recipe(case["envelope"]["recipe"], x_train, case["y_train"])
                path = Path(directory) / "trained.json"
                fitted.to_json(path)
                self.assertEqual(json.loads(path.read_text(encoding="utf-8")), case["envelope"])
                replayed = n4core.N4mRolePipeline.from_json(path)
                self.assertEqual(replayed.predict(x_test).tolist(), fitted.predict(x_test).tolist())
                self.assertEqual(fitted.retrain(x_train, case["y_train"]).predict(x_test).tolist(), fitted.predict(x_test).tolist())

    def test_refuses_tampered_or_mismatched_states(self) -> None:
        envelope = PYTHON_TRAINED["regression"]["envelope"]
        cases = {
            "unsupported trained n4m pipeline envelope": {**envelope, "schema": "nirs4all.n4m.trained_pipeline.v7"},
            "states do not match": {**envelope, "states": envelope["states"][1:]},
        }
        tampered = copy.deepcopy(envelope)
        tampered["states"][0]["sha256"] = "0" * 64
        cases["fails its checksum"] = tampered
        swapped = copy.deepcopy(envelope)
        swapped["recipe"]["pipeline"][1] = "n4m:preprocessing.scatter.msc"
        cases["does not match its recipe step"] = swapped
        for message, document in cases.items():
            with self.subTest(message), self.assertRaisesRegex(ValueError, message):
                n4core.N4mRolePipeline.from_json(json.dumps(document))


if __name__ == "__main__":
    unittest.main()
