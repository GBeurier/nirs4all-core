"""Portable recipe admission and execution through the native Core dispatcher."""

from __future__ import annotations

import os
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from ._native_pipeline import _library
from ._pipeline import PipelineDefinition, load_pipeline_definition
from ._workflow import _invoke


@dataclass(frozen=True)
class PortableDataset:
    """Dense matrix dataset accepted by the portable runner."""

    X: Any
    y: Any
    rows: int | None = None
    cols: int | None = None


def _owner_call(operation: str, request: dict[str, Any], **flags: Any) -> dict[str, Any]:
    try:
        return _invoke(None, operation, request, **flags)
    except RuntimeError as exc:
        if str(exc).startswith("Native workflows require the nirs4all-core-archive CLI"):
            raise ImportError(
                "Portable execution requires the Core native dispatcher or the "
                "nirs4all-core-archive CLI; set NIRS4ALL_CORE_CLI."
            ) from exc
        raise ValueError(str(exc)) from exc


def _definition(source: str | Path | dict[str, Any] | list[Any] | PipelineDefinition) -> PipelineDefinition:
    return source if isinstance(source, PipelineDefinition) else load_pipeline_definition(source)


def parse_execution_plan(
    source: str | Path | dict[str, Any] | list[Any] | PipelineDefinition,
) -> dict[str, Any]:
    """Return the owner's no-fit admission, retaining complete recipe parameters."""
    return _owner_call(
        "portable-pipeline-plan",
        {"schema_version": 1, "definition": _definition(source).as_dict()},
    )


def _json_value(value: Any) -> Any:
    """Marshal host arrays without changing their numeric values or metadata."""
    if hasattr(value, "tolist"):
        return _json_value(value.tolist())
    if isinstance(value, Mapping):
        return {key: _json_value(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_json_value(item) for item in value]
    return value


def _dataset_value(dataset: PortableDataset | Mapping[str, Any]) -> dict[str, Any]:
    if isinstance(dataset, PortableDataset):
        raw = {"X": dataset.X, "y": dataset.y}
        if dataset.rows is not None:
            raw["rows"] = dataset.rows
        if dataset.cols is not None:
            raw["cols"] = dataset.cols
    elif isinstance(dataset, Mapping):
        raw = dict(dataset)
    else:
        raise TypeError("Portable execution requires a PortableDataset or mapping.")
    value = _json_value(raw)
    # A dense column target is the same single target on the JSON boundary.
    # IO records and all their declared facts are forwarded intact.
    if "X" in value and "y" in value:
        target = value["y"]
        if isinstance(target, list) and target and all(isinstance(row, list) and len(row) == 1 for row in target):
            value["y"] = [row[0] for row in target]
    return value


def run_portable_pipeline(
    source: str | Path | dict[str, Any] | list[Any] | PipelineDefinition,
    dataset: PortableDataset | Mapping[str, Any],
) -> dict[str, Any]:
    """Delegate splitting, training, scoring and selection to the native owner.

    A missing native runtime is an error. No host execution or deployment refit
    substitutes for the declared training or single-holdout evaluation profile.
    """
    definition = _definition(source)
    # Keep recipe refusals ahead of dataset conversion and library discovery.
    parse_execution_plan(definition)
    try:
        library = _library(
            os.environ.get("N4M_LIBRARY_PATH") or os.environ.get("N4M_LIB_PATH") or os.environ.get("PLS4ALL_LIB_PATH")
        )
    except ValueError as exc:
        raise ImportError("Portable execution requires a Methods library; set N4M_LIBRARY_PATH.") from exc
    return _owner_call(
        "portable-pipeline-run",
        {"schema_version": 1, "definition": definition.as_dict(), "dataset": _dataset_value(dataset)},
        methods_library=library,
    )
