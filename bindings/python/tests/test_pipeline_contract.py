import unittest
from pathlib import Path

import nirs4all_lite as n4lite


FIXTURE_DIR = Path(__file__).resolve().parents[3] / "tests" / "parity" / "fixtures"


class PipelineContractTests(unittest.TestCase):
    def test_all_shared_json_and_yaml_fixtures_normalize_to_same_pipeline(self) -> None:
        fixture_names = sorted(path.stem for path in FIXTURE_DIR.glob("portable_*.json"))

        self.assertGreaterEqual(len(fixture_names), 4)
        for name in fixture_names:
            with self.subTest(name=name):
                json_pipeline = n4lite.load_pipeline_definition(FIXTURE_DIR / f"{name}.json")
                yaml_pipeline = n4lite.load_pipeline_definition(FIXTURE_DIR / f"{name}.yaml")

                self.assertEqual(json_pipeline.as_dict(), yaml_pipeline.as_dict())
                self.assertGreater(len(n4lite.portable_class_names(json_pipeline)), 0)

    def test_json_and_yaml_fixtures_normalize_to_same_pipeline(self) -> None:
        json_pipeline = n4lite.load_pipeline_definition(FIXTURE_DIR / "portable_methods_pipeline.json")
        yaml_pipeline = n4lite.load_pipeline_definition(FIXTURE_DIR / "portable_methods_pipeline.yaml")

        self.assertEqual(json_pipeline.as_dict(), yaml_pipeline.as_dict())
        self.assertEqual(json_pipeline.random_state, 42)
        self.assertEqual(
            n4lite.portable_class_names(json_pipeline),
            [
                "nirs4all.operators.splitters.KennardStoneSplitter",
                "nirs4all.operators.transforms.StandardNormalVariate",
                "nirs4all.operators.transforms.SavitzkyGolay",
                "sklearn.cross_decomposition.PLSRegression",
            ],
        )

    def test_pls_sweep_uses_nirs4all_range_syntax(self) -> None:
        definition = n4lite.load_pipeline_definition(FIXTURE_DIR / "portable_methods_pipeline.json")
        sweep = definition.pipeline[-1]

        self.assertEqual(sweep["param"], "n_components")
        self.assertEqual(sweep["_range_"], [2, 11, 2])
        self.assertEqual(sweep["model"]["class"], "sklearn.cross_decomposition.PLSRegression")

    def test_steps_alias_and_direct_list_match_nirs4all_loader_surface(self) -> None:
        definition = n4lite.load_pipeline_definition(FIXTURE_DIR / "portable_methods_pipeline.json")

        from_steps = n4lite.load_pipeline_definition({"steps": definition.pipeline})
        from_list = n4lite.load_pipeline_definition(definition.pipeline)

        self.assertEqual(from_steps.pipeline, definition.pipeline)
        self.assertEqual(from_list.pipeline, definition.pipeline)
        self.assertEqual(from_steps.name, "pipeline")
        self.assertEqual(from_list.name, "pipeline")

    def test_unsupported_operator_is_rejected(self) -> None:
        with self.assertRaisesRegex(ValueError, "outside the current nirs4all-lite portable subset"):
            n4lite.load_pipeline_definition(
                {
                    "pipeline": [
                        {"class": "sklearn.ensemble.RandomForestRegressor"},
                    ]
                }
            )


if __name__ == "__main__":
    unittest.main()
