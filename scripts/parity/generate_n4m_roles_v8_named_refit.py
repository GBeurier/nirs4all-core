#!/usr/bin/env python3
"""Record a fresh Methods 1.3.9 fit, leaving the historical trained envelope intact.

Run with owner Python binding 1.3.9 and N4M_LIB_PATH naming the actual library.
This fixture is separate because fresh kernel fitting changed after the older
writer, while replay of its stored model still agrees at 1e-12.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path

import n4m
import pandas as pd
from nirs4all_core import N4mRolePipeline

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "tests/parity/fixtures/n4m_roles_v8_python_named.json"
OUTPUT = ROOT / "tests/parity/expected/n4m_roles_v8_named_refit_methods139.json"


def main() -> None:
    version = n4m.version()
    abi = list(n4m.abi_version())
    if not version.startswith("1.3.9+") or abi != [2, 20, 0]:
        raise RuntimeError(f"Expected owner 1.3.9 ABI 2.20.0, got {version} / {abi}")
    library = Path(os.environ["N4M_LIB_PATH"])
    source = json.loads(SOURCE.read_text())
    names = source["feature_names"]
    fitted = N4mRolePipeline.fit_recipe(
        source["envelope"]["recipe"],
        pd.DataFrame(source["x_train"], columns=names), source["y_train"],
    )
    predicted = fitted.predict(pd.DataFrame(source["x_test"], columns=names)).tolist()
    result = {
        "schema": "nirs4all.methods.fresh-refit-parity.v1",
        "input_fixture": SOURCE.name,
        "input_sha256": hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
        "methods_version": version,
        "methods_abi": abi,
        "native_library_sha256": hashlib.sha256(library.read_bytes()).hexdigest(),
        "methods_source_commit": "a286ec65631203e7bab459f1080260eb51116671",
        "historical_replay_tolerance": 1e-12,
        "fresh_refit_tolerance": 1e-9,
        "predict": predicted,
    }
    OUTPUT.write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
