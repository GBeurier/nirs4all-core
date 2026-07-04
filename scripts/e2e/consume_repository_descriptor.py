#!/usr/bin/env python3
"""Consume a repository-served pipeline through nirs4all-core bindings."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any


PIPELINE_ARTIFACT = "repository-pipeline.n4a.json"
RESOLUTION_ARTIFACT = "provider-resolution.json"
OUTPUT_ARTIFACT = "cross-language-consumption.json"
EXECUTION_TOLERANCE = 1e-10


def _core_root() -> Path:
    return Path(__file__).resolve().parents[2]


def _workspace_root() -> Path:
    return _core_root().parent


def _read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def _write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=True, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def _ensure_python_paths() -> None:
    src = _core_root() / "bindings" / "python" / "src"
    methods_src = _workspace_root() / "nirs4all-methods" / "bindings" / "python" / "src"
    for path in (src, methods_src):
        src_text = str(path)
        if path.is_dir() and src_text not in sys.path:
            sys.path.insert(0, src_text)


def _stable_hash(payload: Any) -> str:
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":"), allow_nan=False).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def _load_python(pipeline_path: Path) -> dict[str, Any]:
    _ensure_python_paths()

    import nirs4all_lite as n4a

    definition = n4a.load_pipeline_definition(pipeline_path)
    return {
        "surface": "bindings/python",
        "status": "passed",
        "name": definition.name,
        "random_state": definition.random_state,
        "classes": n4a.portable_class_names(definition),
        "pipeline": definition.as_dict(),
    }


def _find_node() -> Path | None:
    candidates = [
        shutil.which("node"),
        "/mnt/c/Program Files/nodejs/node.exe",
    ]
    nvm_root = Path.home() / ".nvm" / "versions" / "node"
    if nvm_root.is_dir():
        candidates.extend(str(path) for path in sorted(nvm_root.glob("*/bin/node"), reverse=True))
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return Path(candidate)
    return None


def _node_arg(path: Path, node: Path) -> str:
    if node.suffix.lower() != ".exe":
        return str(path)
    try:
        proc = subprocess.run(
            ["wslpath", "-w", str(path)],
            text=True,
            capture_output=True,
            check=True,
        )
    except (FileNotFoundError, subprocess.CalledProcessError):
        return str(path)
    return proc.stdout.strip()


def _load_javascript_wasm(pipeline_path: Path) -> dict[str, Any]:
    node = _find_node()
    if node is None:
        raise RuntimeError("Node.js runtime not found; cannot validate the JavaScript/WASM binding surface")

    module_path = _core_root() / "bindings" / "wasm" / "src" / "index.js"
    code = """
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const modulePath = process.argv[1];
const pipelinePath = process.argv[2];
const nirs4all = await import(pathToFileURL(modulePath).href);
const source = readFileSync(pipelinePath, 'utf8');
const definition = nirs4all.loadPipelineDefinition(source);
console.log(JSON.stringify({
  surface: 'bindings/wasm',
  status: 'passed',
  node: process.version,
  name: definition.name,
  random_state: definition.random_state ?? null,
  classes: nirs4all.portableClassNames(definition),
  pipeline: definition,
}));
"""
    proc = subprocess.run(
        [
            str(node),
            "--input-type=module",
            "-e",
            code,
            _node_arg(module_path, node),
            _node_arg(pipeline_path, node),
        ],
        text=True,
        capture_output=True,
        check=False,
        timeout=30,
    )
    if proc.returncode != 0:
        raise RuntimeError(proc.stderr.strip() or f"node exited with {proc.returncode}")
    return _read_json_from_text(proc.stdout)


def _read_json_from_text(text: str) -> dict[str, Any]:
    stripped = text.strip()
    if not stripped:
        raise RuntimeError("JavaScript/WASM loader emitted no JSON")
    return json.loads(stripped.splitlines()[-1])


def _deterministic_noise(row: int, col: int) -> float:
    state = ((row + 1) * 73856093) ^ ((col + 1) * 19349663)
    state &= 0xFFFFFFFF
    state = (1664525 * state + 1013904223) & 0xFFFFFFFF
    return state / 4294967295 - 0.5


def _execution_dataset(rows: int = 40, cols: int = 28) -> dict[str, Any]:
    x: list[list[float]] = []
    y: list[float] = []
    for row_index in range(rows):
        phase = row_index / 5
        row: list[float] = []
        target = 0.0
        for col_index in range(cols):
            wavelength = 900 + col_index * 8
            value = (
                0.6 * math.sin(phase + col_index / 7)
                + 0.25 * math.cos(row_index / 6 - col_index / 11)
                + 0.002 * wavelength
                + ((row_index % 4) - 1.5) * 0.03
                + 0.12 * _deterministic_noise(row_index, col_index)
                + 0.03 * math.sin(((row_index + 1) * (col_index + 2)) / 13)
            )
            row.append(value)
            target += value * (0.04 if col_index < cols / 2 else -0.025) + 0.01 * _deterministic_noise(col_index, row_index)
        x.append(row)
        y.append(target + 0.2 * math.sin(row_index / 3) + row_index * 0.015)
    return {"X": x, "y": y, "rows": rows, "cols": cols}


def _as_float_list(values: Any) -> list[float]:
    return [float(value) for value in list(values or [])]


def _as_int_list(values: Any) -> list[int]:
    return [int(value) for value in list(values or [])]


def _max_abs_diff(actual: list[Any], expected: list[Any]) -> float:
    if len(actual) != len(expected):
        raise AssertionError(f"length mismatch: {len(actual)} != {len(expected)}")
    return max((abs(float(a) - float(e)) for a, e in zip(actual, expected)), default=0.0)


def _variant_evidence(variant: dict[str, Any]) -> dict[str, Any]:
    predictions = _as_float_list(variant.get("predictions"))
    return {
        "n_components": int(variant["n_components"]),
        "rmse": float(variant["rmse"]),
        "prediction_count": len(predictions),
        "predictions": predictions,
    }


def _execution_evidence(surface: str, actual: dict[str, Any], extra: dict[str, Any] | None = None) -> dict[str, Any]:
    split = actual.get("split") or {}
    selected = _variant_evidence(actual["selected"])
    evidence = {
        "surface": surface,
        "status": "passed",
        "name": actual.get("name"),
        "rows": int(actual["rows"]),
        "cols": int(actual["cols"]),
        "split": {
            "kind": split.get("kind"),
            "train_count": len(split.get("trainIndices") or []),
            "test_count": len(split.get("testIndices") or []),
            "trainIndices": _as_int_list(split.get("trainIndices")),
            "testIndices": _as_int_list(split.get("testIndices")),
        },
        "preprocessing": actual.get("preprocessing") or [],
        "variants": [_variant_evidence(variant) for variant in list(actual.get("variants") or [])],
        "selected": selected,
        "target_count": len(actual.get("targets") or []),
        "targets": _as_float_list(actual.get("targets")),
    }
    predict_roundtrip = actual.get("predict_roundtrip")
    if isinstance(predict_roundtrip, dict):
        held_out = _as_float_list(predict_roundtrip.get("held_out_predictions"))
        evidence["predict_roundtrip"] = {
            "rows": int(predict_roundtrip.get("rows", 0)),
            "cols": int(predict_roundtrip.get("cols", 0)),
            "held_out_prediction_count": len(held_out),
            "held_out_predictions": held_out,
            "selected_prediction_abs_max": _max_abs_diff(held_out, selected["predictions"]) if held_out else None,
        }
    if extra:
        evidence.update(extra)
    return evidence


def _run_python_execution(pipeline_path: Path, dataset: dict[str, Any]) -> dict[str, Any]:
    _ensure_python_paths()

    import nirs4all_lite as n4a

    actual = n4a.run_portable_pipeline(pipeline_path, dataset)
    return _execution_evidence("bindings/python", actual)


def _methods_js_index() -> tuple[Path | None, str]:
    if os.environ.get("NIRS4ALL_METHODS_JS_DIST"):
        dist = Path(os.environ["NIRS4ALL_METHODS_JS_DIST"])
        source = "NIRS4ALL_METHODS_JS_DIST"
    elif os.environ.get("NIRS4ALL_METHODS_ROOT"):
        dist = Path(os.environ["NIRS4ALL_METHODS_ROOT"]) / "bindings" / "js" / "dist"
        source = "NIRS4ALL_METHODS_ROOT"
    else:
        dist = _workspace_root() / "nirs4all-methods" / "bindings" / "js" / "dist"
        source = "default sibling nirs4all-methods"

    required = ("index.js", "n4m.js", "n4m.wasm")
    missing = [name for name in required if not (dist / name).exists()]
    if missing:
        return None, f"local nirs4all-methods JS/WASM build is unavailable from {source}: {dist}; missing {', '.join(missing)}"
    return dist / "index.js", source


def _run_javascript_wasm_execution(pipeline_path: Path, dataset: dict[str, Any]) -> dict[str, Any]:
    node = _find_node()
    if node is None:
        raise RuntimeError("Node.js runtime not found; cannot execute the JavaScript/WASM binding surface")
    methods_index, methods_source = _methods_js_index()
    if methods_index is None:
        raise RuntimeError(methods_source)

    module_path = _core_root() / "bindings" / "wasm" / "src" / "index.js"
    code = """
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const modulePath = process.argv[1];
const methodsPath = process.argv[2];
const pipelinePath = process.argv[3];
const dataset = JSON.parse(process.argv[4]);
const nirs4all = await import(pathToFileURL(modulePath).href);
const methods = await import(pathToFileURL(methodsPath).href);
if (typeof methods.loadModule === 'function') {
  await methods.loadModule();
}
const source = readFileSync(pipelinePath, 'utf8');
const actual = await nirs4all.runPortablePipeline(source, {
  X: Float64Array.from(dataset.X.flat()),
  y: Float64Array.from(dataset.y),
  rows: dataset.rows,
  cols: dataset.cols,
}, { methods });
const predicted = await nirs4all.predictPortablePipeline(actual, {
  X: Float64Array.from(dataset.X.flat()),
  rows: dataset.rows,
  cols: dataset.cols,
}, { methods });
actual.predict_roundtrip = {
  rows: predicted.rows,
  cols: predicted.cols,
  held_out_predictions: actual.split.testIndices.map((index) => predicted.data[index]),
};
console.log(JSON.stringify({
  node: process.version,
  actual,
}));
"""
    proc = subprocess.run(
        [
            str(node),
            "--input-type=module",
            "-e",
            code,
            _node_arg(module_path, node),
            _node_arg(methods_index, node),
            _node_arg(pipeline_path, node),
            json.dumps(dataset, ensure_ascii=True, sort_keys=True),
        ],
        text=True,
        capture_output=True,
        check=False,
        timeout=60,
    )
    if proc.returncode != 0:
        raise RuntimeError(proc.stderr.strip() or f"node exited with {proc.returncode}")
    payload = _read_json_from_text(proc.stdout)
    return _execution_evidence("bindings/wasm", payload["actual"], {"node": payload.get("node")})


def _strict_prediction_comparison(left: dict[str, Any], right: dict[str, Any]) -> dict[str, Any]:
    split_match = {
        "kind": left["split"].get("kind") == right["split"].get("kind"),
        "train_indices": left["split"].get("trainIndices") == right["split"].get("trainIndices"),
        "test_indices": left["split"].get("testIndices") == right["split"].get("testIndices"),
    }
    preprocessing_match = left.get("preprocessing") == right.get("preprocessing")
    selected_n_components_match = int(left["selected"]["n_components"]) == int(right["selected"]["n_components"])
    targets_abs_max = _max_abs_diff(left["targets"], right["targets"])

    left_variants = list(left.get("variants") or [])
    right_variants = list(right.get("variants") or [])
    if len(left_variants) != len(right_variants):
        raise AssertionError(f"execution variant count mismatch: {len(left_variants)} != {len(right_variants)}")

    prediction_abs_max = 0.0
    rmse_abs_max = 0.0
    variants = []
    for left_variant, right_variant in zip(left_variants, right_variants):
        n_components_match = int(left_variant["n_components"]) == int(right_variant["n_components"])
        prediction_abs = _max_abs_diff(left_variant["predictions"], right_variant["predictions"])
        rmse_abs = abs(float(left_variant["rmse"]) - float(right_variant["rmse"]))
        prediction_abs_max = max(prediction_abs_max, prediction_abs)
        rmse_abs_max = max(rmse_abs_max, rmse_abs)
        variants.append(
            {
                "n_components": int(left_variant["n_components"]),
                "n_components_match": n_components_match,
                "prediction_abs_max": prediction_abs,
                "rmse_abs": rmse_abs,
            }
        )

    predict_roundtrip_abs_max = None
    right_roundtrip = right.get("predict_roundtrip") or {}
    if right_roundtrip.get("held_out_predictions"):
        predict_roundtrip_abs_max = _max_abs_diff(right_roundtrip["held_out_predictions"], right["selected"]["predictions"])

    passed = (
        all(split_match.values())
        and preprocessing_match
        and selected_n_components_match
        and targets_abs_max <= EXECUTION_TOLERANCE
        and prediction_abs_max <= EXECUTION_TOLERANCE
        and rmse_abs_max <= EXECUTION_TOLERANCE
        and all(item["n_components_match"] for item in variants)
        and (predict_roundtrip_abs_max is None or predict_roundtrip_abs_max <= EXECUTION_TOLERANCE)
    )
    return {
        "status": "passed" if passed else "failed",
        "surfaces": [left["surface"], right["surface"]],
        "tolerance": EXECUTION_TOLERANCE,
        "split_match": split_match,
        "preprocessing_match": preprocessing_match,
        "selected_n_components_match": selected_n_components_match,
        "targets_abs_max": targets_abs_max,
        "prediction_abs_max": prediction_abs_max,
        "rmse_abs_max": rmse_abs_max,
        "predict_roundtrip_abs_max": predict_roundtrip_abs_max,
        "variants": variants,
    }


def _runtime_execution(pipeline_path: Path) -> dict[str, Any]:
    dataset = _execution_dataset()
    runtime_results = []
    runtime_errors = []
    for surface, runner in (
        ("bindings/python", _run_python_execution),
        ("bindings/wasm", _run_javascript_wasm_execution),
    ):
        try:
            runtime_results.append(runner(pipeline_path, dataset))
        except Exception as exc:
            runtime_errors.append({"surface": surface, "reason": str(exc)})

    if runtime_errors:
        raise AssertionError(f"repository pipeline execution failed on required runtime surface(s): {runtime_errors}")

    python_result = next((result for result in runtime_results if result["surface"] == "bindings/python"), None)
    wasm_result = next((result for result in runtime_results if result["surface"] == "bindings/wasm"), None)
    if python_result is None or wasm_result is None:
        raise AssertionError("repository pipeline execution requires both Python and JavaScript/WASM runtime evidence")

    comparison = _strict_prediction_comparison(python_result, wasm_result)
    if comparison["status"] != "passed":
        raise AssertionError(f"Python and JavaScript/WASM execution diverged: {comparison}")

    return {
        "status": "passed",
        "dataset": {
            "kind": "deterministic_synthetic_nirs_matrix",
            "rows": dataset["rows"],
            "cols": dataset["cols"],
            "target_count": len(dataset["y"]),
            "sha256": _stable_hash(dataset),
        },
        "runtime_results": runtime_results,
        "comparison": comparison,
    }


def consume(artifacts_dir: Path) -> dict[str, Any]:
    pipeline_path = artifacts_dir / PIPELINE_ARTIFACT
    resolution_path = artifacts_dir / RESOLUTION_ARTIFACT
    if not pipeline_path.is_file():
        raise FileNotFoundError(f"missing repository pipeline artifact: {pipeline_path}")
    if not resolution_path.is_file():
        raise FileNotFoundError(f"missing provider resolution artifact: {resolution_path}")

    resolution = _read_json(resolution_path)
    python = _load_python(pipeline_path)
    javascript_wasm = _load_javascript_wasm(pipeline_path)
    execution = _runtime_execution(pipeline_path)

    parity = {
        "classes_match": python["classes"] == javascript_wasm["classes"],
        "random_state_match": python["random_state"] == javascript_wasm["random_state"],
        "name_match": python["name"] == javascript_wasm["name"],
    }
    if not all(parity.values()):
        raise AssertionError(f"Python and JavaScript/WASM repository consumption diverged: {parity}")

    return {
        "schema_version": "n4a.e2e.repository-consumption/v1",
        "status": "passed",
        "pipeline_id": resolution["repository"]["pipeline_id"],
        "repository_index_count": resolution["repository"]["catalog_count"],
        "source_artifacts": {
            "provider_resolution": str(resolution_path),
            "repository_pipeline": str(pipeline_path),
        },
        "python": python,
        "javascript_wasm": javascript_wasm,
        "parity": parity,
        "execution": execution,
        "known_followups": [
            {
                "surface": "r",
                "coverage": "covered_by_separate_core_methods_gate",
                "reason": (
                    "R remains covered by separate core/methods gates; the current n4m R binding ABI "
                    "does not expose the preprocessing/splitter functions needed for this repository pipeline."
                ),
            }
        ],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts-dir", type=Path, required=True)
    args = parser.parse_args(argv)

    artifacts_dir = args.artifacts_dir.expanduser().resolve()
    result = consume(artifacts_dir)
    _write_json(artifacts_dir / OUTPUT_ARTIFACT, result)
    print(json.dumps(result["parity"], ensure_ascii=True, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
