"""Generic n4m role recipes and the cross-language trained envelope v8.

The negative envelope cases (recipe/state mismatch, empty pipeline, feature
permutation, training rows without opt-in, multi-target routing) replay the
Methods shared fixture ``n4m_role_pipeline_methods.json`` and are identical in
the Python, JS/WASM and Rust suites, as are the label-table, ``n_features``,
NUL column-name, ragged-shape and attested-recipe mutations.
"""

import base64
import copy
import hashlib
import json
import os
import re
import tempfile
import unittest
from pathlib import Path

import nirs4all_core as n4core

ROOT = Path(__file__).resolve().parents[3]
FIXTURES = ROOT / "tests/parity/fixtures"
PYTHON_TRAINED = json.loads((FIXTURES / "n4m_roles_v8_python_trained.json").read_text(encoding="utf-8"))
PYTHON_NAMED = json.loads((FIXTURES / "n4m_roles_v8_python_named.json").read_text(encoding="utf-8"))
METHODS = json.loads((FIXTURES / "n4m_role_pipeline_methods.json").read_text(encoding="utf-8"))
R_TRAINED = (FIXTURES / "n4m_roles_v8_r_trained.json").read_text(encoding="utf-8")
R_ORACLE = json.loads((ROOT / "tests/parity/expected/n4m_roles_v8_r_trained_oracle.json").read_text(encoding="utf-8"))
METHODS_CASES = {case["name"]: case for case in METHODS["cases"]}


def _max_abs_diff(actual, expected) -> float:
    if len(actual) != len(expected):
        raise AssertionError(f"length mismatch: {len(actual)} != {len(expected)}")
    return max(abs(float(a) - float(e)) for a, e in zip(actual, expected))


def _envelope(steps, states, feature_names=None, class_names=None) -> str:
    """A v8 envelope of Methods fixture states.

    A state is ``{method_id, n4me_base64, contains_training_rows}`` or bare
    base64 bytes, then labelled with its recipe step.
    """

    stateful = [step["class"][len("n4m:"):] for step in steps if not step["class"].startswith("n4m:filters.")]
    entries = []
    for method_id, state in zip(stateful, states):
        state = state if isinstance(state, dict) else {"method_id": method_id, "n4me_base64": state}
        payload = base64.b64decode(state["n4me_base64"])
        entries.append({**state, "sha256": hashlib.sha256(payload).hexdigest()})
    if class_names is not None:
        entries[-1]["class_names"] = class_names
    document = {
        "schema": n4core.N4M_TRAINED_PIPELINE_SCHEMA,
        "recipe": {"pipeline": steps},
        "n_features": len(METHODS["x_train"][0]),
        "states": entries,
    }
    if feature_names is not None:
        document["feature_names"] = feature_names
    return json.dumps(document)


def _without_bytes(document: dict) -> dict:
    """The envelope without its N4ME bytes (they record the writing ABI)."""

    document = copy.deepcopy(document)
    for state in document["states"]:
        del state["n4me_base64"], state["sha256"]
    return document


class N4mRoleRecipeTests(unittest.TestCase):
    def setUp(self) -> None:
        try:
            import numpy  # noqa: F401
            import pandas  # noqa: F401
            from n4m.roles import RolePipeline  # noqa: F401
        except ImportError as exc:
            if os.environ.get("NIRS4ALL_CORE_REQUIRE_METHODS_PARITY") == "1":
                raise
            raise unittest.SkipTest("nirs4all-methods Python role pipelines (ABI 2.14) are not available") from exc

    def frame(self, rows, names=None):
        import pandas as pd

        return pd.DataFrame(rows, columns=names or METHODS["feature_names"])

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

    def test_replays_envelopes_written_before_the_additive_fields(self) -> None:
        x_test = PYTHON_TRAINED["x_test"]
        envelope = PYTHON_TRAINED["regression"]["envelope"]
        regression = n4core.N4mRolePipeline.from_json(json.dumps(envelope))
        self.assertLessEqual(_max_abs_diff(regression.predict(x_test).ravel(), PYTHON_TRAINED["regression"]["predict"]), 1e-12)
        self.assertIsNone(regression.feature_names)
        rewritten = json.loads(regression.to_json())
        self.assertNotIn("feature_names", rewritten)
        self.assertEqual([state.pop("contains_training_rows") for state in rewritten["states"]], [False, False, False])
        self.assertEqual(_without_bytes(rewritten), _without_bytes(envelope))

        classification = n4core.N4mRolePipeline.from_json(json.dumps(PYTHON_TRAINED["classification"]["envelope"]))
        self.assertEqual(classification.predict(x_test).tolist(), PYTHON_TRAINED["classification"]["predict"])

        from_r = n4core.N4mRolePipeline.from_json(R_TRAINED)
        self.assertLessEqual(_max_abs_diff(from_r.predict(R_ORACLE["x_test"]).ravel(), R_ORACLE["predict"]), 1e-12)

    def test_fitted_envelopes_round_trip(self) -> None:
        x_train, x_test = PYTHON_TRAINED["x_train"], PYTHON_TRAINED["x_test"]
        for name, tolerance in (("regression", 1e-9), ("classification", 0)):
            with self.subTest(name), tempfile.TemporaryDirectory() as directory:
                case = PYTHON_TRAINED[name]
                fitted = n4core.N4mRolePipeline.fit_recipe(case["envelope"]["recipe"], x_train, case["y_train"])
                path = Path(directory) / "trained.json"
                fitted.to_json(path)
                document = json.loads(path.read_text(encoding="utf-8"))
                for state in document["states"]:
                    self.assertIs(state.pop("contains_training_rows"), False)
                self.assertEqual(_without_bytes(document), _without_bytes(case["envelope"]))
                live = fitted.predict(x_test).tolist()
                if tolerance:
                    self.assertLessEqual(_max_abs_diff(live, case["predict"]), tolerance)
                else:
                    self.assertEqual(live, case["predict"])
                replayed = n4core.N4mRolePipeline.from_json(path)
                self.assertEqual(replayed.predict(x_test).tolist(), live)
                self.assertEqual(fitted.retrain(x_train, case["y_train"]).predict(x_test).tolist(), live)

    def test_named_envelope_keeps_column_identity_and_training_rows(self) -> None:
        names = PYTHON_NAMED["feature_names"]
        replayed = n4core.N4mRolePipeline.from_json(json.dumps(PYTHON_NAMED["envelope"]))
        self.assertEqual(replayed.feature_names, names)
        predicted = replayed.predict(self.frame(PYTHON_NAMED["x_test"], names))
        self.assertLessEqual(_max_abs_diff(predicted, PYTHON_NAMED["predict"]), 1e-12)
        with self.assertRaisesRegex(Exception, "retains training rows"):
            replayed.to_json()
        rewritten = json.loads(replayed.to_json(allow_training_rows=True))
        self.assertEqual(rewritten["feature_names"], names)
        self.assertEqual(_without_bytes(rewritten), _without_bytes(PYTHON_NAMED["envelope"]))

        refit = n4core.N4mRolePipeline.fit_recipe(
            PYTHON_NAMED["envelope"]["recipe"], self.frame(PYTHON_NAMED["x_train"], names), PYTHON_NAMED["y_train"])
        self.assertEqual(refit.feature_names, names)
        self.assertLessEqual(_max_abs_diff(refit.predict(self.frame(PYTHON_NAMED["x_test"], names)), PYTHON_NAMED["predict"]), 1e-9)

    def test_replays_the_methods_shared_pipelines(self) -> None:
        names = METHODS["feature_names"]
        x_test = self.frame(METHODS["x_test"])
        case = METHODS["regression"]
        regression = n4core.N4mRolePipeline.from_json(_envelope(case["steps"], case["states"], names))
        self.assertLessEqual(_max_abs_diff(regression.predict(x_test), case["predict"]), 1e-9)
        case = METHODS["classification"]
        classification = n4core.N4mRolePipeline.from_json(_envelope(case["steps"], case["states"], names, case["class_names"]))
        self.assertEqual(classification.predict(x_test).tolist(), case["predict"])
        self.assertEqual(json.loads(classification.to_json())["states"][-1]["class_names"], case["class_names"])

    def test_multi_target_y_reaches_supervised_transformers(self) -> None:
        case = METHODS_CASES["multi_target_supervised_transformer"]
        fitted = n4core.N4mRolePipeline.fit_recipe({"pipeline": case["steps"]}, METHODS["x_train"], METHODS["y2_train"])
        predicted = fitted.predict(METHODS["x_test"])
        self.assertEqual(predicted.shape, (len(METHODS["x_test"]), 2))
        self.assertLessEqual(_max_abs_diff(predicted.ravel(), [v for row in case["predict"] for v in row]), 1e-9)
        replayed = n4core.N4mRolePipeline.from_json(fitted.to_json())
        self.assertEqual(replayed.predict(METHODS["x_test"]).tolist(), predicted.tolist())

    def test_refuses_invalid_recipes(self) -> None:
        for name in ("empty_recipe", "wrong_role_order", "missing_terminal"):
            case = METHODS_CASES[name]
            with self.subTest(name), self.assertRaisesRegex(Exception, re.escape(case["message"])):
                n4core.N4mRolePipeline.fit_recipe({"pipeline": case["steps"]}, METHODS["x_train"], METHODS["y_train"])
        with self.assertRaisesRegex(Exception, "at least one step"):
            n4core.N4mRolePipeline.from_json(_envelope([], []))
        with self.assertRaisesRegex(ValueError, "n4m:<method id> steps only"):
            n4core.N4mRolePipeline.fit_recipe({"pipeline": ["sklearn.cross_decomposition.PLSRegression"]}, METHODS["x_train"], METHODS["y_train"])

    def test_refuses_states_that_contradict_the_recipe(self) -> None:
        for name in ("recipe_param_differs_from_state", "method_mismatch", "state_count_mismatch"):
            case = METHODS_CASES[name]
            with self.subTest(name), self.assertRaisesRegex(Exception, re.escape(case["message"])):
                n4core.N4mRolePipeline.from_json(_envelope(case["steps"], case["states"]))
        case = METHODS["regression"]
        contradicting = copy.deepcopy(case["states"])
        contradicting[0]["contains_training_rows"] = True
        with self.assertRaisesRegex(ValueError, "contains_training_rows"):
            n4core.N4mRolePipeline.from_json(_envelope(case["steps"], contradicting))

    def test_refuses_permuted_or_missing_columns(self) -> None:
        case = METHODS["regression"]
        pipeline = n4core.N4mRolePipeline.from_json(_envelope(case["steps"], case["states"], METHODS["feature_names"]))
        permuted = METHODS_CASES["feature_name_permutation"]
        with self.assertRaisesRegex(Exception, re.escape(permuted["message"])):
            pipeline.predict(self.frame(METHODS["x_test"], permuted["feature_names"]))
        with self.assertRaisesRegex(Exception, re.escape(METHODS_CASES["width_mismatch"]["message"])):
            pipeline.predict([row[:-1] for row in METHODS["x_test"]])

        fitted = n4core.N4mRolePipeline.fit_recipe({"pipeline": case["steps"]}, self.frame(METHODS["x_train"]), METHODS["y_train"])
        replayed = n4core.N4mRolePipeline.from_json(fitted.to_json())
        self.assertEqual(replayed.feature_names, METHODS["feature_names"])
        with self.assertRaisesRegex(Exception, "reordered"):
            replayed.predict(self.frame(METHODS["x_test"], permuted["feature_names"]))

    def test_training_rows_need_the_export_opt_in(self) -> None:
        case = METHODS_CASES["training_rows_without_opt_in"]
        fitted = n4core.N4mRolePipeline.fit_recipe({"pipeline": case["steps"]}, METHODS["x_train"], METHODS[case["y"]])
        with self.assertRaisesRegex(Exception, re.escape(case["message"])):
            fitted.to_json()
        document = json.loads(fitted.to_json(allow_training_rows=True))
        self.assertEqual([state["contains_training_rows"] for state in document["states"]], [False, True])
        replayed = n4core.N4mRolePipeline.from_json(json.dumps(document))
        self.assertEqual(replayed.predict(METHODS["x_test"]).tolist(), fitted.predict(METHODS["x_test"]).tolist())

    def test_refuses_tampered_or_foreign_envelopes(self) -> None:
        envelope = PYTHON_TRAINED["regression"]["envelope"]
        with self.assertRaisesRegex(ValueError, "unsupported trained n4m pipeline envelope"):
            n4core.N4mRolePipeline.from_json(json.dumps({**envelope, "schema": "nirs4all.n4m.trained_pipeline.v7"}))
        tampered = copy.deepcopy(envelope)
        tampered["states"][0]["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "fails its checksum"):
            n4core.N4mRolePipeline.from_json(json.dumps(tampered))
        relabelled = copy.deepcopy(envelope)
        relabelled["states"][0]["method_id"] = "preprocessing.scatter.msc"
        with self.assertRaisesRegex(ValueError, "does not match its recipe step"):
            n4core.N4mRolePipeline.from_json(json.dumps(relabelled))
        with self.assertRaisesRegex(ValueError, "n_features"):
            n4core.N4mRolePipeline.from_json(json.dumps({**envelope, "n_features": 3}))

    def test_refuses_label_tables_that_contradict_the_states(self) -> None:
        source = PYTHON_TRAINED["classification"]["envelope"]
        x_test = PYTHON_TRAINED["x_test"]
        expected = PYTHON_TRAINED["classification"]["predict"]
        for class_names, message in (
            ([], "class_names must be a non-empty list of labels"),
            ("high", "class_names must be a non-empty list of labels"),
            (None, "class_names must be a non-empty list of labels"),
            (["only"], "class id 1 has no entry in class_names (1 labels)"),
            (["same", "same"], "class label 'same' is duplicated"),
            ([1.5, 1.5], "class label 1.5 is duplicated"),
            (["high", None], "class label None is not a string or a finite number"),
            ([True, False], "class label True is not a string or a finite number"),
            (["high", float("nan")], "class label nan is not a string or a finite number"),
            (["high", float("inf")], "class label inf is not a string or a finite number"),
        ):
            with self.subTest(class_names), self.assertRaisesRegex(ValueError, re.escape(message)):
                n4core.N4mRolePipeline.from_json(_with_class_names(source, class_names))

        # Index = class id: a longer table keeps the slots of labels a filter
        # removed, and finite numbers (even beside strings) are labels too.
        longer = n4core.N4mRolePipeline.from_json(_with_class_names(source, ["a", "b", "c"]))
        self.assertEqual(len(longer.predict(x_test)), len(x_test))
        numeric = n4core.N4mRolePipeline.from_json(_with_class_names(source, [0.5, 1.5]))
        self.assertEqual(numeric.predict(x_test).tolist(), [0.5 if label == "high" else 1.5 for label in expected])
        self.assertEqual(json.loads(numeric.to_json())["states"][-1]["class_names"], [0.5, 1.5])
        mixed = n4core.N4mRolePipeline.from_json(_with_class_names(source, ["high", 2]))
        self.assertEqual(mixed.predict(x_test).tolist(), [label if label == "high" else 2 for label in expected])

        # States fitted on class ids 10 and 20 have no entry in a two-label table.
        ids = [10 if label == "high" else 20 for label in PYTHON_TRAINED["classification"]["y_train"]]
        by_ids = json.loads(n4core.N4mRolePipeline.fit_recipe(source["recipe"], PYTHON_TRAINED["x_train"], ids).to_json())
        self.assertNotIn("class_names", by_ids["states"][-1])
        with self.assertRaisesRegex(ValueError, re.escape("class id 10 has no entry in class_names (2 labels)")):
            n4core.N4mRolePipeline.from_json(_with_class_names(by_ids, ["high", "low"]))
        with self.assertRaisesRegex(ValueError, "class_names label the classes of a final classifier"):
            n4core.N4mRolePipeline.from_json(_with_class_names(PYTHON_TRAINED["regression"]["envelope"], ["high", "low"]))

    def test_refuses_missing_or_non_finite_labels_at_fit(self) -> None:
        recipe = PYTHON_TRAINED["classification"]["envelope"]["recipe"]
        x_train = PYTHON_TRAINED["x_train"]
        labels = [0.5 if label == "high" else 1.5 for label in PYTHON_TRAINED["classification"]["y_train"]]
        fitted = n4core.N4mRolePipeline.fit_recipe(recipe, x_train, labels)
        self.assertEqual(json.loads(fitted.to_json())["states"][-1]["class_names"], [0.5, 1.5])
        for bad, shown in ((float("nan"), "nan"), (float("inf"), "inf")):
            with self.subTest(shown), self.assertRaisesRegex(ValueError, f"class label {shown} is not a string or a finite number"):
                n4core.N4mRolePipeline.fit_recipe(recipe, x_train, [bad, *labels[1:]])
        with self.assertRaisesRegex(ValueError, "class label False is not a string or a finite number"):
            n4core.N4mRolePipeline.fit_recipe(recipe, x_train, [label == 0.5 for label in labels])

    def test_envelope_widths_are_positive_json_integers(self) -> None:
        source = PYTHON_TRAINED["regression"]["envelope"]
        for value in (24.9, 24.0, "24", True, 0, -24, None):
            with self.subTest(value), self.assertRaisesRegex(ValueError, re.escape(f"n_features must be a positive JSON integer, got {value!r}")):
                n4core.N4mRolePipeline.from_json(json.dumps({**source, "n_features": value}))
        missing = {key: value for key, value in source.items() if key != "n_features"}
        with self.assertRaisesRegex(ValueError, "n_features must be a positive JSON integer, got None"):
            n4core.N4mRolePipeline.from_json(json.dumps(missing))

    def test_refuses_column_names_with_nul(self) -> None:
        case = METHODS["regression"]
        names = list(METHODS["feature_names"])
        names[3] = "nm1006\0"
        with self.assertRaisesRegex(ValueError, "contains a NUL character"):
            n4core.N4mRolePipeline.from_json(_envelope(case["steps"], case["states"], names))
        with self.assertRaisesRegex(ValueError, "contains a NUL character"):
            n4core.N4mRolePipeline.fit_recipe({"pipeline": case["steps"]}, self.frame(METHODS["x_train"], names), METHODS["y_train"])
        pipeline = n4core.N4mRolePipeline.from_json(_envelope(case["steps"], case["states"], METHODS["feature_names"]))
        with self.assertRaisesRegex(ValueError, "contains a NUL character"):
            pipeline.predict(self.frame(METHODS["x_test"], names))

    def test_refuses_ragged_or_contradictory_targets(self) -> None:
        recipe = {"pipeline": [{"class": "n4m:models.regularized.ridge", "params": {"alpha": 1}}]}
        x_train, y = METHODS["x_train"], METHODS["y2_train"]
        n4core.N4mRolePipeline.fit_recipe(recipe, x_train, y)
        ragged = copy.deepcopy(y)
        ragged[1], ragged[2] = [10], [20, 21, 22]
        for name, target in (("ragged", ragged), ("short", y[1:]), ("long", [*y, y[0]]), ("no column", [[] for _ in y])):
            with self.subTest(name), self.assertRaises(ValueError):
                n4core.N4mRolePipeline.fit_recipe(recipe, x_train, target)
        with self.assertRaises(ValueError):
            n4core.N4mRolePipeline.fit_recipe(recipe, [[*row, 123] if i == 0 else row for i, row in enumerate(x_train)], y)

    def test_exports_the_recipe_the_states_attest(self) -> None:
        recipe = copy.deepcopy(PYTHON_TRAINED["regression"]["envelope"]["recipe"])
        x_train, x_test, y_train = PYTHON_TRAINED["x_train"], PYTHON_TRAINED["x_test"], PYTHON_TRAINED["regression"]["y_train"]
        fitted = n4core.N4mRolePipeline.fit_recipe(recipe, x_train, y_train)
        recipe["pipeline"][-1]["params"]["n_components"] = 1
        fitted.recipe["pipeline"][-1]["params"]["n_components"] = 1
        text = fitted.to_json()
        self.assertEqual(json.loads(text)["recipe"], PYTHON_TRAINED["regression"]["envelope"]["recipe"])
        replayed = n4core.N4mRolePipeline.from_json(text)
        self.assertEqual(replayed.predict(x_test).tolist(), fitted.predict(x_test).tolist())
        self.assertEqual(fitted.retrain(x_train, y_train).predict(x_test).tolist(), fitted.predict(x_test).tolist())


def _with_class_names(source: dict, class_names) -> str:
    """``source`` with the label table of its last state replaced."""

    document = copy.deepcopy(source)
    document["states"][-1]["class_names"] = class_names
    return json.dumps(document)


if __name__ == "__main__":
    unittest.main()
