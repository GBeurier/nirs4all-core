"""Public JSON subprocess bridge to the SDK-owned SQLite/Parquet workspace.

R/MATLAB hosts invoke this module with argv, never interpreter code. Each
command validates an immutable workspace snapshot and closes all SDK sessions.
Python/Core plus the full nirs4all SDK are required for this optional profile.
"""
from __future__ import annotations

import argparse
import json
from datetime import date, datetime
from pathlib import Path
from typing import Any

from ._workspace import import_workspace, open_workspace, save_workspace


def execute_workspace_command(request: dict[str, Any]) -> Any:
    """Execute one bounded command against the real SDK storage/session API."""
    if not isinstance(request, dict) or request.get("schema") != "nirs4all.workspace-command.v1":
        raise ValueError("Unsupported workspace command schema")
    operation = request.get("operation")
    fields = {"open": {"path"}, "query": {"path", "run_id"}, "predict": {"path", "run_id", "X", "sample_ids"},
              "export": {"path", "destination"}, "import": {"archive", "destination"}, "save": {"experiments", "destination"}}
    if operation not in fields or set(request) != {"schema", "operation", *fields[operation]}:
        raise ValueError("Invalid workspace command fields")
    if operation == "import":
        with import_workspace(request["archive"], request["destination"]) as workspace:
            return {"path": str(workspace.path.resolve()), "runs": workspace.runs()}
    if operation == "save":
        if not isinstance(request["experiments"], list):
            raise ValueError("Workspace experiments must be an array")
        with save_workspace(request["experiments"], request["destination"]) as workspace:
            return {"path": str(workspace.path.resolve()), "runs": workspace.runs()}
    with open_workspace(request["path"]) as workspace:
        if operation == "open":
            return {"path": str(workspace.path.resolve()), "runs": workspace.runs()}
        if operation == "query":
            return workspace.query_predictions(request["run_id"])
        if operation == "export":
            return {"archive": str(workspace.export(request["destination"]))}
        session = workspace.session(request["run_id"], engine="sdk")
        result = session.predict(request["X"], sample_ids=request["sample_ids"])
        # These IDs come from the SDK replay result, rather than being copied
        # from the caller around an unverified predictor result.
        if result.metadata.get("sample_ids") != request["sample_ids"]:
            raise ValueError("SDK prediction sample identity differs from requested cohort")
        return {"y_pred": result.y_pred, "shape": list(result.y_pred.shape),
                "sample_ids": result.metadata["sample_ids"], "metadata": result.metadata,
                "model_name": result.model_name,
                "intervals": {str(coverage): {"lower": block.lower, "upper": block.upper}
                              for coverage, block in result.intervals.items()}}


def _json_default(value: Any) -> Any:
    if isinstance(value, (datetime, date)):
        return value.isoformat()
    if hasattr(value, "tolist"):
        return value.tolist()
    raise TypeError(f"Unsupported workspace result value: {type(value).__name__}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    options = parser.parse_args()
    try:
        if options.input.stat().st_size > 16 * 1024 * 1024:
            raise ValueError("Workspace command exceeds the JSON request budget")
        request = json.loads(options.input.read_text(encoding="utf-8"))
        result = {"ok": True, "result": execute_workspace_command(request)}
        status = 0
    except Exception as error:  # noqa: BLE001 - subprocess boundary returns structured errors.
        result = {"ok": False, "error": str(error), "error_type": type(error).__name__}
        status = 2
    payload = json.dumps(result, default=_json_default, ensure_ascii=False, allow_nan=False)
    with options.output.open("x", encoding="utf-8") as stream:
        stream.write(payload)
    return status


if __name__ == "__main__":
    raise SystemExit(main())
