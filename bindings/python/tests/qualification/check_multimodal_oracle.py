"""Compare portable U07 predictions with independently fitted sklearn operators."""
from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path

import numpy as np
from nirs4all_io.public_dataset import Dataset
from sklearn.base import clone


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sdk-source", required=True, type=Path)
    parser.add_argument("--directory", required=True, type=Path)
    args = parser.parse_args()
    source = args.sdk_source / "examples/user/02_data_handling/U07_multimodal.py"
    spec = importlib.util.spec_from_file_location("public_u07_oracle", source)
    if spec is None or spec.loader is None:
        raise ValueError("Canonical U07 example cannot be loaded")
    example = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(example)
    model = example.make_pipeline(backend="methods")[-1]["model"]
    training = Dataset.from_dict(json.loads((args.directory / "train.json").read_text())).multimodal
    current = Dataset.from_dict(json.loads((args.directory / "predict.json").read_text())).multimodal
    fit_blocks, predict_blocks = [], []
    for name, transformer in model.transformers.items():
        fitted = clone(transformer).fit(training.sources[name].values, training.y)
        weight = (model.source_weights or {}).get(name, 1.0)
        fit_blocks.append(np.asarray(fitted.transform(training.sources[name].values), dtype=float) * weight)
        predict_blocks.append(np.asarray(fitted.transform(current.sources[name].values), dtype=float) * weight)
    ridge = clone(model.model).fit(np.concatenate(fit_blocks, axis=1), training.y)
    oracle = ridge.predict(np.concatenate(predict_blocks, axis=1)).reshape(-1, 1)
    actual = np.asarray(json.loads((args.directory / "expected.json").read_text())["values"])
    np.testing.assert_allclose(actual, oracle, rtol=2e-7, atol=2e-7)
    evidence = {"oracle": "independent sklearn encoders/fusion/Ridge", "source": str(source),
                "max_absolute_difference": float(np.max(np.abs(actual - oracle))), "predictions": oracle.tolist()}
    (args.directory / "oracle-evidence.json").write_text(json.dumps(evidence, indent=2, allow_nan=False))
    print("PASS independent sklearn U07 oracle", evidence["max_absolute_difference"])


if __name__ == "__main__":
    main()
