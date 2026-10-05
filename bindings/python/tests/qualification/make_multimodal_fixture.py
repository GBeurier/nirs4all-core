"""Generate the shared synthetic U07 native qualification inputs and archive.

Provenance: the SDK's maintained examples/user/02_data_handling/U07_multimodal.py.
No measured/private data or saved training workspace is a fixture dependency.
Requires the local source SDK and native DAG/Core/IO/Methods bindings on PYTHONPATH.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path

import numpy as np
from nirs4all.pipeline.dagml.methods_multimodal import recipe_from_estimator
from nirs4all_core import MultimodalPredictor
from nirs4all_io.public_dataset import Dataset
from sklearn.model_selection import GroupKFold

import nirs4all


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sdk-source", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    source = args.sdk_source / "examples/user/02_data_handling/U07_multimodal.py"
    spec = importlib.util.spec_from_file_location("public_u07", source)
    if spec is None or spec.loader is None:
        raise ValueError("Canonical U07 example cannot be loaded")
    example = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(example)
    cohort = example.make_cohort(17)
    train = cohort.take([s for s, p in zip(cohort.sample_ids, cohort.partitions, strict=True) if p == "train"])
    prediction = example.make_cohort(29, prediction=True)
    folds: list[str | None] = [None] * len(train)
    for fold, (_, validation) in enumerate(GroupKFold(3).split(np.zeros((len(train), 1)), train.y, train.groups)):
        for index in validation:
            folds[index] = str(fold)
    training = Dataset(train, origin_ids=train.groups.tolist(), fold_ids=folds)
    current = Dataset(prediction, origin_ids=prediction.groups.tolist())
    pipeline = example.make_pipeline(backend="methods")
    recipe = recipe_from_estimator(pipeline[-1]["model"])
    for name, value in [("train", training.to_dict()), ("predict", current.to_dict()), ("recipe", recipe)]:
        (args.output / f"{name}.json").write_text(json.dumps(value, ensure_ascii=False, allow_nan=False))
    with MultimodalPredictor.fit(recipe, training) as model:
        model.export(args.output / "predictor.json")
        expected = model.predict(current)
    (args.output / "expected.json").write_text(json.dumps(expected, allow_nan=False))
    pipeline[-1].pop("_grid_")
    result = nirs4all.run(pipeline, cohort, engine="dag-ml", workspace_path=args.output / "training-workspace",
                          save_charts=False, verbose=0, random_state=17)
    result.export(args.output / "u07-native.n4a")
    print(f"Generated native U07: {len(train)} training rows, {len(prediction)} independent prediction rows")


if __name__ == "__main__":
    main()
