#!/usr/bin/env python3
"""Validate the multisource stacking replay artifacts produced by full Python nirs4all."""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path
from typing import Any

SCENARIO_ID = "e2e-multisource-branching-stacking-replay"


def _write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True, allow_nan=False) + "\n", encoding="utf-8")


def _load_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def _finite(value: Any, label: str) -> float:
    number = float(value)
    if not math.isfinite(number):
        raise AssertionError(f"{label} must be finite, got {value!r}")
    return number


def _required_artifacts(artifacts_dir: Path) -> tuple[Path, Path]:
    replay_path = artifacts_dir / "stacking-replay.n4a.json"
    ledger_path = artifacts_dir / "oof-ledger.json"
    missing = [path.name for path in (replay_path, ledger_path) if not path.exists()]
    if missing:
        raise FileNotFoundError(
            "Missing multisource stacking artifact(s): "
            + ", ".join(missing)
            + ". Run `python3.11 -m pytest tests/e2e/test_multisource_stacking_replay.py --artifacts-dir=<dir>` first."
        )
    return replay_path, ledger_path


def _native_dir(replay: dict[str, Any], artifacts_dir: Path) -> Path:
    raw = replay.get("dagml_native", {}).get("native_results_dir")
    if isinstance(raw, str) and raw:
        candidate = Path(raw)
        if candidate.is_dir():
            return candidate
    native_root = artifacts_dir / "native-results"
    candidates = sorted(path for path in native_root.iterdir() if path.is_dir()) if native_root.is_dir() else []
    if len(candidates) == 1:
        return candidates[0]
    raise FileNotFoundError(f"Unable to resolve native results directory from {raw!r} or {native_root}")


def _merge_stack_score(score_set: dict[str, Any], *, partition: str, fold_id: str | None) -> float:
    matches = [
        report
        for report in score_set.get("reports", [])
        if report.get("producer_node") == "merge:stack"
        and report.get("partition") == partition
        and report.get("fold_id") == fold_id
        and isinstance(report.get("metrics"), dict)
        and "rmse" in report["metrics"]
    ]
    if len(matches) != 1:
        raise AssertionError(f"expected one merge:stack report for partition={partition!r} fold_id={fold_id!r}, got {len(matches)}")
    return _finite(matches[0]["metrics"]["rmse"], f"merge:stack {partition}/{fold_id} rmse")


def _prediction_table_audit(native_dir: Path, replay: dict[str, Any]) -> dict[str, Any]:
    try:
        import pyarrow.parquet as pq
    except ModuleNotFoundError as exc:
        raise AssertionError("pyarrow is required to audit native predictions.parquet") from exc

    table = pq.read_table(native_dir / "predictions.parquet")
    rows = table.to_pylist()
    required_columns = {
        "model_name",
        "partition",
        "fold_id",
        "sample_indices",
        "y_true",
        "y_pred",
        "arrays_present",
        "metric",
        "target_width",
    }
    columns = set(table.column_names)
    missing_columns = sorted(required_columns - columns)
    if missing_columns:
        raise AssertionError(f"native predictions.parquet missing column(s): {missing_columns}")

    expected_rows = int(replay["dagml_native"]["num_predictions"])
    if len(rows) != expected_rows:
        raise AssertionError(f"native predictions.parquet row count mismatch: {len(rows)} != {expected_rows}")

    meta_rows = [row for row in rows if row.get("model_name") == "MetaModel_Ridge"]
    if len(meta_rows) != expected_rows:
        raise AssertionError(f"expected all native prediction rows to be MetaModel_Ridge, got {len(meta_rows)} of {expected_rows}")
    if any(row.get("metric") != "rmse" for row in meta_rows):
        raise AssertionError("native prediction table carries a non-rmse MetaModel row")
    if any(int(row.get("target_width", -1)) != 1 for row in meta_rows):
        raise AssertionError("native prediction table carries an unexpected target width")

    array_rows = [row for row in meta_rows if row.get("arrays_present")]
    return {
        "rows": len(rows),
        "meta_model_rows": len(meta_rows),
        "array_rows": len(array_rows),
        "array_payload_scope": "absent_in_current_native_prediction_table",
        "partitions": sorted({str(row.get("partition")) for row in meta_rows}),
        "fold_ids": sorted({str(row.get("fold_id")) for row in meta_rows}),
        "schema_columns": table.column_names,
    }


def _validate(artifacts_dir: Path) -> dict[str, Any]:
    replay_path, ledger_path = _required_artifacts(artifacts_dir)
    replay = _load_json(replay_path)
    ledger = _load_json(ledger_path)

    if replay.get("scenario_id") != SCENARIO_ID or ledger.get("scenario_id") != SCENARIO_ID:
        raise AssertionError("scenario_id mismatch in replay artifacts")
    if replay.get("status") != "python_oracle_and_native_ready":
        raise AssertionError(f"unexpected replay status {replay.get('status')!r}")

    tolerance = _finite(replay["parity"]["score_tolerance"], "score_tolerance")
    cv_delta = _finite(replay["parity"]["cv_best_score_abs"], "cv_best_score_abs")
    best_delta = _finite(replay["parity"]["best_rmse_abs"], "best_rmse_abs")
    if cv_delta > tolerance or best_delta > tolerance:
        raise AssertionError(f"replay parity deltas exceed tolerance: cv={cv_delta} best={best_delta} tol={tolerance}")

    oracle_scores = ledger["scores"]
    replay_scores = replay["oracle_scores"]
    for key in ("cv_best_score", "best_rmse"):
        delta = abs(_finite(oracle_scores[key], f"ledger {key}") - _finite(replay_scores[key], f"replay {key}"))
        if delta > 1e-12:
            raise AssertionError(f"oracle score mismatch for {key}: {delta}")

    native_dir = _native_dir(replay, artifacts_dir)
    required_native = [native_dir / "manifest.json", native_dir / "score_set.json", native_dir / "predictions.parquet"]
    missing_native = [path.name for path in required_native if not path.exists()]
    if missing_native:
        raise FileNotFoundError(f"native result directory is incomplete: {missing_native}")

    native_manifest = _load_json(native_dir / "manifest.json")
    score_set = _load_json(native_dir / "score_set.json")

    checks = {
        "native_engine": native_manifest.get("engine") == "dag-ml",
        "native_metric": native_manifest.get("metric") == "rmse",
        "native_model": "MetaModel_Ridge" in set(native_manifest.get("model_names", [])),
        "native_num_predictions": int(native_manifest.get("num_predictions", -1)) == int(replay["dagml_native"]["num_predictions"]),
        "native_prediction_file": (native_dir / native_manifest.get("files", {}).get("predictions", "")).exists(),
        "native_score_file": (native_dir / native_manifest.get("files", {}).get("score_set", "")).exists(),
    }
    if not all(checks.values()):
        raise AssertionError(f"native manifest checks failed: {checks}")

    score_cv = _merge_stack_score(score_set, partition="validation", fold_id="avg")
    score_best = _merge_stack_score(score_set, partition="test", fold_id=None)
    cv_score_delta = abs(score_cv - _finite(oracle_scores["cv_best_score"], "oracle cv_best_score"))
    best_score_delta = abs(score_best - _finite(oracle_scores["best_rmse"], "oracle best_rmse"))
    if cv_score_delta > tolerance or best_score_delta > tolerance:
        raise AssertionError(
            f"native score_set deltas exceed tolerance: cv={cv_score_delta} best={best_score_delta} tol={tolerance}"
        )
    prediction_table = _prediction_table_audit(native_dir, replay)

    return {
        "scenario_id": SCENARIO_ID,
        "status": "passed",
        "artifacts_dir": str(artifacts_dir),
        "replay": replay_path.name,
        "ledger": ledger_path.name,
        "native_results_dir": str(native_dir),
        "checks": checks,
        "score_set_parity": {
            "cv_best_score_abs": cv_score_delta,
            "best_rmse_abs": best_score_delta,
            "tolerance": tolerance,
        },
        "prediction_table": prediction_table,
        "decisions": [
            "nirs4all-core validates the native dag-ml replay score_set and audits predictions.parquet schema/coverage instead of reimplementing the Python stacking runner.",
            "Current native MetaModel rows do not persist per-sample arrays, so this scenario no longer claims native vector parity.",
            "The richer by_source stacking legacy case remains fallback-only and is recorded as a known boundary in the replay manifest.",
        ],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts-dir", type=Path, required=True)
    args = parser.parse_args(argv)

    artifacts_dir = args.artifacts_dir.resolve()
    output_path = artifacts_dir / "native-replay.json"
    try:
        evidence = _validate(artifacts_dir)
    except Exception as exc:
        _write_json(output_path, {"scenario_id": SCENARIO_ID, "status": "failed", "reason": str(exc)})
        print(str(exc), file=sys.stderr)
        return 1

    _write_json(output_path, evidence)
    print(json.dumps({"scenario_id": SCENARIO_ID, "status": "passed"}, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
