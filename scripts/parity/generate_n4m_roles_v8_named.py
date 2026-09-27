#!/usr/bin/env python3
"""Generate the Python-trained v8 envelope with the additive fields.

``tests/parity/fixtures/n4m_roles_v8_python_named.json`` holds a
``nirs4all.n4m.trained_pipeline.v8`` envelope fitted on named columns, so it
carries ``feature_names`` and a kernel PLS state exported with the training-row
opt-in (``contains_training_rows: true``), plus the data and the predictions.
The data are those of the Methods shared fixture
(``n4m_role_pipeline_methods.json``). Run with the Methods Python binding and
libn4m selected (``PYTHONPATH``, ``N4M_LIB_PATH``).
"""

from __future__ import annotations

import json
from pathlib import Path

import pandas as pd
from nirs4all_core import N4mRolePipeline

FIXTURES = Path(__file__).resolve().parents[2] / "tests/parity/fixtures"
RECIPE = {
    "pipeline": [
        {"class": "n4m:filters.y_outlier", "params": {"threshold": 2.0}},
        "n4m:preprocessing.scatter.snv",
        {"class": "n4m:models.pls.kernel", "params": {"n_components": 3, "kernel": "linear"}},
    ]
}


def main() -> None:
    source = json.loads((FIXTURES / "n4m_role_pipeline_methods.json").read_text(encoding="utf-8"))
    names = source["feature_names"]
    fitted = N4mRolePipeline.fit_recipe(RECIPE, pd.DataFrame(source["x_train"], columns=names), source["y_train"])
    document = {
        "feature_names": names,
        "x_train": source["x_train"],
        "y_train": source["y_train"],
        "x_test": source["x_test"],
        "envelope": json.loads(fitted.to_json(allow_training_rows=True)),
        "predict": fitted.predict(pd.DataFrame(source["x_test"], columns=names)).tolist(),
    }
    (FIXTURES / "n4m_roles_v8_python_named.json").write_text(json.dumps(document, indent=1) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
