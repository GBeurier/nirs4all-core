"""Transport assertions and real no-fit owner admission; no numerical oracle."""

import ast
import json
import unittest
from copy import deepcopy
from pathlib import Path
from unittest.mock import patch

import nirs4all_core as n4core
from nirs4all_core import _execution


class PortableOwnerTransportTests(unittest.TestCase):
    def test_plan_forwards_original_controls_without_a_dataset_or_library(self):
        recipe = {
            "steps": [{"_comment": "Keep the source position."}, {"class": "n4m.SNV"}],
            "target": {"name": "response"},
            "search": {"seed": 17},
        }
        definition = n4core.load_pipeline_definition(recipe)
        response = {"owner_refusal_or_plan": "transport witness"}
        with patch.object(_execution, "_invoke", return_value=response) as invoke:
            self.assertIs(n4core.parse_execution_plan(definition), response)
        invoke.assert_called_once_with(None, "portable-pipeline-plan", {"schema_version": 1, "definition": recipe})

    def test_run_forwards_all_dataset_facts_and_preserves_the_owner_winner(self):
        recipe = {"pipeline": [{"model": {"class": "n4m.PLSRegression"}}]}
        dataset = {
            "X": [[1, 2], [3, 4]],
            "y": [[5], [6]],
            "sample_ids": ["sample:b", "sample:a"],
            "axis": {"values": [900, 910], "unit": "nm"},
            "masks": {"valid": [[True, False], [True, True]]},
            "group_ids": ["group:1", "group:2"],
            "target": {"name": "response", "grain": "sample"},
        }
        expected = deepcopy(dataset)
        expected["y"] = [5, 6]
        # This mock deliberately supplies a winner inconsistent with host min.
        # It witnesses transport only; the real owner decides eligibility.
        response = {"variants": [{"rmse": 0}, {"rmse": 7}], "selected": {"rmse": 7}}
        with patch.object(_execution, "_invoke", side_effect=[{}, response]) as invoke:
            with patch.object(_execution, "_library", return_value="/shared/libn4m.so"):
                actual = n4core.run_portable_pipeline(recipe, dataset)
        self.assertIs(actual, response)
        self.assertEqual(dataset["y"], [[5], [6]])
        self.assertEqual(invoke.call_args.args, (None, "portable-pipeline-run", {
            "schema_version": 1, "definition": recipe, "dataset": expected,
        }))
        self.assertEqual(invoke.call_args.kwargs, {"methods_library": "/shared/libn4m.so"})

    def test_record_is_forwarded_without_constructing_features_or_identities(self):
        record = {"schema": "nirs4all.dataset.v1", "dataset": {"sources": [], "sample_ids": ["declared:1"]}}
        with patch.object(_execution, "_invoke", side_effect=[{}, {}]) as invoke:
            with patch.object(_execution, "_library", return_value="/shared/libn4m.so"):
                n4core.run_portable_pipeline([{"model": {"class": "n4m.PLS"}}], record)
        self.assertEqual(invoke.call_args.args[2]["dataset"], record)

    def test_public_array_dataclass_omits_unset_shape_without_inventing_ids(self):
        class Array:
            def __init__(self, values):
                self.values = values

            def tolist(self):
                return self.values

        data = n4core.PortableDataset(Array([[1, 2]]), Array([[3]]))
        with patch.object(_execution, "_invoke", side_effect=[{}, {}]) as invoke:
            with patch.object(_execution, "_library", return_value="/shared/libn4m.so"):
                n4core.run_portable_pipeline([{"model": {"class": "n4m.PLS"}}], data)
        self.assertEqual(invoke.call_args.args[2]["dataset"], {"X": [[1, 2]], "y": [3]})

    def test_native_refusal_preserves_the_path_and_never_triggers_another_route(self):
        message = "$.pipeline[0].params.unsupported_control: unsupported portable field"
        with patch.object(_execution, "_invoke", side_effect=RuntimeError(message)) as invoke:
            with patch.object(_execution, "_library") as library:
                with self.assertRaisesRegex(ValueError, r"params\.unsupported_control"):
                    n4core.run_portable_pipeline([{"class": "n4m.SNV"}], {})
                library.assert_not_called()
        self.assertEqual(invoke.call_count, 1)

    def test_execution_adapter_has_no_host_scientific_operations(self):
        tree = ast.parse(Path(_execution.__file__).read_text(encoding="utf-8"))
        forbidden = {"fit", "transform", "predict", "mean", "sqrt", "min", "argmin", "_compute_split", "_make_transformer"}
        calls = {
            node.func.attr if isinstance(node.func, ast.Attribute) else node.func.id
            for node in ast.walk(tree)
            if isinstance(node, ast.Call) and isinstance(node.func, (ast.Name, ast.Attribute))
        }
        self.assertFalse(calls & forbidden)
        imports = [node.module for node in ast.walk(tree) if isinstance(node, ast.ImportFrom)]
        self.assertFalse(any(name and name.startswith(("n4m", "pls4all", "numpy")) for name in imports))


class PortableOwnerAdmissionTests(unittest.TestCase):
    """These assertions need the actual native plan command; missing it fails."""

    def test_b02_four_controls_are_distinct_or_refused_by_parameter_path(self):
        admitted = []
        for with_std in (True, False):
            for scale in (True, False):
                recipe = {"pipeline": [
                    {"class": "nirs4all.operators.transforms.SNV", "params": {"with_std": with_std}},
                    {"model": {"class": "sklearn.cross_decomposition.PLSRegression", "params": {"n_components": 2, "scale": scale}}},
                ]}
                with self.subTest(with_std=with_std, scale=scale):
                    try:
                        plan = n4core.parse_execution_plan(recipe)
                    except ValueError as error:
                        paths = ([] if with_std else [".params.with_std"]) + ([] if scale else [".params.scale"])
                        self.assertTrue(any(path in str(error) for path in paths), str(error))
                        continue
                    self.assertEqual(plan["mode"], "training")
                    self.assertEqual(plan["recipe"][0]["params"]["with_std"], with_std)
                    model = plan["recipe"][-1]["params"]
                    self.assertEqual(model["scale_x"], scale)
                    self.assertEqual(model["scale_y"], scale)
                    self.assertEqual(model["solver"], "simpls")
                    admitted.append(json.dumps(plan["recipe"], sort_keys=True))
        self.assertEqual(len(admitted), len(set(admitted)))

    def test_range_keeps_scale_false_and_snv_no_scale(self):
        # Minimal inputs from the frozen B02 cases with these names; the
        # historical corpus and its per-language expectations stay untouched.
        recipe = {"pipeline": [
            {"class": "n4m.SNV", "params": {"with_std": False, "ddof": 3}},
            {"model": {"class": "sklearn.cross_decomposition.PLSRegression", "params": {"n_components": 5, "scale": False}},
             "param": "n_components", "_range_": [1, 3, 1]},
        ]}
        try:
            plan = n4core.parse_execution_plan(recipe)
        except ValueError as error:
            self.assertTrue(any(path in str(error) for path in (".params.with_std", ".params.ddof", ".params.scale")), str(error))
            return
        self.assertEqual(plan["nComponents"], [1, 2, 3])
        self.assertEqual(plan["recipe"][0]["params"]["ddof"], 3)
        self.assertFalse(plan["recipe"][0]["params"]["with_std"])
        self.assertFalse(plan["recipe"][-1]["params"]["scale_x"])
        self.assertFalse(plan["recipe"][-1]["params"]["scale_y"])

    def test_unknown_parameter_is_refused_before_dataset_conversion(self):
        class MustNotSerialize:
            def tolist(self):
                raise AssertionError("An invalid recipe reached dataset conversion.")

        recipe = {"pipeline": [
            {"class": "n4m.SNV", "params": {"unsupported_control": True}},
            {"model": {"class": "n4m.PLS"}},
        ]}
        with self.assertRaisesRegex(ValueError, r"params\.unsupported_control"):
            n4core.run_portable_pipeline(recipe, {"X": MustNotSerialize(), "y": [1]})


if __name__ == "__main__":
    unittest.main()
