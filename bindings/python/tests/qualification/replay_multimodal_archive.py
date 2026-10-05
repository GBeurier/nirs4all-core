"""Qualify public cold replay against the shared native U07 archive.

Run without the full SDK on PYTHONPATH; only Core, IO, DAG/Data and Methods are needed.
"""
from __future__ import annotations

import argparse
import copy
import json
from pathlib import Path
from unittest.mock import patch

import numpy as np
from nirs4all_core import predict_multimodal_archive


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    directory = parser.parse_args().directory
    current = json.loads((directory / "predict.json").read_text())
    expected = json.loads((directory / "expected.json").read_text())
    reversed_rows = copy.deepcopy(current)
    for source in reversed_rows["dataset"]["sources"]:
        source["sample_ids"].reverse()
        source["array"]["values"].reverse()
        source["presence_mask"]["values"].reverse()
    with patch("n4m.MultimodalPipeline.fit", side_effect=AssertionError("FIT forbidden during cold replay")):
        result = predict_multimodal_archive(directory / "u07-native.n4a", reversed_rows)
        assert result["sample_ids"] == expected["sample_ids"]
        assert result["target_names"] == expected["target_names"]
        np.testing.assert_allclose(result["values"], expected["values"], rtol=1e-8, atol=1e-8)
        assert [event["operation"] for event in result["audit"]] == ["hydrate", "PREDICT", "dispose", "release"]
        assert result["training_performed"] is False
        logical_path = directory / "logical-host-dataset.json"
        if logical_path.exists():
            logical = predict_multimodal_archive(directory / "u07-native.n4a", logical_path)
            np.testing.assert_allclose(logical["values"], expected["values"], rtol=1e-8, atol=1e-8)
            assert logical["sample_ids"] == expected["sample_ids"]
            assert [event["operation"] for event in logical["audit"]] == ["hydrate", "PREDICT", "dispose", "release"]
            (directory / "python-logical-archive-evidence.json").write_text(json.dumps(logical, indent=2, allow_nan=False))
        incompatible = copy.deepcopy(current)
        incompatible["dataset"]["sources"][0]["axis_units"]["wavelength"] = "cm-1"
        try:
            predict_multimodal_archive(directory / "u07-native.n4a", incompatible)
        except ValueError as error:
            assert "schema differs" in str(error)
        else:
            raise AssertionError("Incompatible wavelength units accepted")
    (directory / "python-core-archive-evidence.json").write_text(json.dumps(result, indent=2, allow_nan=False))
    print("PASS Python public unchanged native archive, independent cohort, no FIT, alignment and schema checks")


if __name__ == "__main__":
    main()
