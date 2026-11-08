"""Document retention at the public loader boundary; no model fitting."""

import json
import tempfile
import unittest
from copy import deepcopy
from pathlib import Path

import nirs4all_core as n4core


class PipelineDocumentTests(unittest.TestCase):
    def test_root_controls_comments_and_params_survive_the_display_view(self):
        recipe = {
            "steps": [
                {"_comment": "Keep this source position."},
                {"class": "n4m.SNV", "params": {"with_std": False, "ddof": 3}},
                {
                    "model": {
                        "class": "sklearn.cross_decomposition.PLSRegression",
                        "params": {"scale": False, "n_components": 5},
                    },
                    "param": "n_components",
                    "_range_": [1, 3, 1],
                },
            ],
            "target": {"name": "response", "grain": "sample"},
            "selection": {"metric": "rmse"},
            "provenance": {"source": "example.v1.2"},
            "_comment": "This is a readable document, not an execution grant.",
        }
        original = deepcopy(recipe)
        definition = n4core.load_pipeline_definition(recipe)
        self.assertEqual(definition.as_dict(), original)
        self.assertNotIn("_comment", definition.pipeline[0])
        self.assertEqual(len(definition.pipeline), 2)
        recipe["target"]["name"] = "changed caller value"
        detached = definition.as_dict()
        detached["steps"][1]["params"]["with_std"] = True
        self.assertEqual(definition.as_dict(), original)

    def test_direct_list_keeps_comments_without_synthesizing_an_envelope(self):
        recipe = [{"_comment": "Keep me."}, {"class": "n4m.SNV"}]
        definition = n4core.load_pipeline_definition(recipe)
        self.assertEqual(definition.as_dict(), recipe)
        self.assertEqual(definition.pipeline, [{"class": "n4m.SNV"}])

    def test_exact_json_and_yaml_text_survive_inline_and_file_loading(self):
        json_text = '{"pipeline": [{"class": "n4m.SNV"}], "search": {"seed": 17}}\n'
        yaml_text = (
            "# Retain the original YAML comment.\n"
            "pipeline:\n"
            "  - class: n4m.SNV\n"
            "target:\n"
            "  name: response\n"
            "  grain: sample\n"
        )
        for suffix, text, control in ((".json", json_text, "search"), (".yaml", yaml_text, "target")):
            with self.subTest(suffix=suffix), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / ("recipe" + suffix)
                path.write_text(text, encoding="utf-8")
                inline = n4core.load_pipeline_definition(text)
                file_definition = n4core.load_pipeline_definition(path)
                self.assertEqual(inline.source_text, text)
                self.assertEqual(file_definition.source_text, text)
                self.assertEqual(inline.as_dict(), file_definition.as_dict())
                self.assertIn(control, inline.as_dict())
                if suffix == ".json":
                    self.assertEqual(inline.as_dict(), json.loads(text))

    def test_manual_definition_keeps_the_existing_serialized_shape(self):
        definition = n4core.PipelineDefinition("manual", "", 17, [{"class": "n4m.SNV"}])
        self.assertEqual(
            definition.as_dict(),
            {"name": "manual", "description": "", "random_state": 17, "pipeline": [{"class": "n4m.SNV"}]},
        )


if __name__ == "__main__":
    unittest.main()
