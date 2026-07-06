#!/usr/bin/env python3
"""Prepare a real dataset/provider/io payload for the R pipeline-save E2E."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from enum import Enum
from pathlib import Path
from typing import Any

import numpy as np


DEFAULT_DATASET_ID = "malaria_anopheles_gambiae_sporozoite_nir"
DEFAULT_SOURCE = "X"
DEFAULT_MAX_ROWS = 120
DEFAULT_MAX_COLS = 96


def _repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def _workspace_root() -> Path:
    return Path.cwd().parent if Path.cwd().name == "nirs4all-core" else _repo_root().parent


def _prepend_sibling_srcs(workspace_root: Path) -> None:
    for repo in ("nirs4all-providers", "nirs4all-datasets", "nirs4all-io"):
        src = workspace_root / repo / "src"
        if src.is_dir():
            sys.path.insert(0, str(src))


def _json_default(value: Any) -> Any:
    if isinstance(value, np.generic):
        return value.item()
    if isinstance(value, np.ndarray):
        return value.tolist()
    if isinstance(value, Enum):
        return value.value
    if isinstance(value, Path):
        return str(value)
    raise TypeError(f"Object of type {type(value).__name__} is not JSON serializable")


def _canonical_json(data: Any) -> str:
    return json.dumps(data, ensure_ascii=True, sort_keys=True, separators=(",", ":"), default=_json_default)


def _sha256_json(data: Any) -> str:
    return hashlib.sha256(_canonical_json(data).encode("utf-8")).hexdigest()


def _write_json(path: Path, data: Any) -> None:
    path.write_text(json.dumps(data, ensure_ascii=True, indent=2, sort_keys=True, default=_json_default) + "\n", encoding="utf-8")


def _select_indices(size: int, limit: int) -> list[int]:
    if size <= 0:
        raise ValueError("cannot select from an empty axis")
    if size <= limit:
        return list(range(size))
    selected: list[int] = []
    seen: set[int] = set()
    for raw in np.linspace(0, size - 1, num=limit, dtype=int).tolist():
        if raw not in seen:
            selected.append(raw)
            seen.add(raw)
    candidate = 0
    while len(selected) < limit:
        if candidate not in seen:
            selected.append(candidate)
            seen.add(candidate)
        candidate += 1
    return sorted(selected)


def _payload_shape(payload: dict[str, Any]) -> list[int] | None:
    shape = payload.get("shape")
    if isinstance(shape, list) and all(isinstance(item, int) for item in shape):
        return shape
    return None


def prepare(
    *,
    out_dir: Path,
    dataset_id: str,
    source: str,
    max_rows: int,
    max_cols: int,
    workspace_root: Path,
) -> dict[str, Any]:
    _prepend_sibling_srcs(workspace_root)

    import nirs4all_io as nio
    from nirs4all_providers import DatasetProvider

    datasets_root = workspace_root / "nirs4all-datasets"
    pipeline_path = _repo_root() / "bindings" / "r" / "inst" / "extdata" / "portable_methods_pipeline.json"
    if not pipeline_path.is_file():
        raise FileNotFoundError(f"pipeline fixture not found: {pipeline_path}")

    provider = DatasetProvider(root=str(datasets_root))
    dataset = provider.get_dataset(dataset_id)
    card = provider.card(dataset_id)
    io_spec = dataset.to_io_spec(source=source, split=None)
    package = provider.to_dataset_package(io_spec, name=dataset_id)
    package_summary = provider.describe_dataset_package(package)
    assembled = nio.load(io_spec, target="assembled")

    block_name = "train" if "train" in assembled.blocks else next(iter(assembled.blocks))
    block = assembled.blocks[block_name]
    if not block.X:
        raise ValueError(f"assembled block {block_name!r} has no feature matrix")
    if block.y is None:
        raise ValueError(f"assembled block {block_name!r} has no target matrix")

    X = np.asarray(block.X[0], dtype=np.float64)
    y_matrix = np.asarray(block.y, dtype=np.float64)
    if y_matrix.ndim == 1:
        y_matrix = y_matrix.reshape(-1, 1)
    y = y_matrix[:, 0]

    finite_rows = np.isfinite(X).all(axis=1) & np.isfinite(y)
    X = X[finite_rows]
    y = y[finite_rows]
    if X.shape[0] < 20:
        raise ValueError(f"not enough finite rows after IO assembly: {X.shape[0]}")

    row_indices = _select_indices(X.shape[0], max_rows)
    col_indices = _select_indices(X.shape[1], max_cols)
    X_selected = X[np.ix_(row_indices, col_indices)]
    y_selected = y[row_indices]
    headers = list(block.feature_headers[0]) if block.feature_headers else [str(i) for i in range(X.shape[1])]
    selected_headers = [headers[i] for i in col_indices]

    dataset_payload = {
        "X": np.round(X_selected, 12).tolist(),
        "y": np.round(y_selected, 12).tolist(),
        "rows": int(X_selected.shape[0]),
        "cols": int(X_selected.shape[1]),
        "target": block.y_headers[0] if block.y_headers else "y0",
        "feature_headers": selected_headers,
    }
    package_entries = package_summary.get("manifest", {}).get("entries", []) if isinstance(package_summary, dict) else []
    feature_entry = next((entry for entry in package_entries if entry.get("role") == "features"), {})
    target_entry = next((entry for entry in package_entries if entry.get("role") == "targets"), {})
    selected_values_preserved = bool(np.array_equal(X_selected, X[np.ix_(row_indices, col_indices)]))

    payload = {
        "schema_version": "n4a.e2e.r_dataset_io_pipeline/v2",
        "status": "prepared",
        "source": {
            "dataset_id": dataset_id,
            "dataset_title": (card or {}).get("title") or (card or {}).get("name"),
            "dataset_tier": getattr(dataset.tier, "value", str(dataset.tier)),
            "source": source,
            "pipeline": str(pipeline_path),
        },
        "provider_contract": {
            "provider": "nirs4all-providers.DatasetProvider",
            "backing": "nirs4all-datasets",
            "io_bridge": "nirs4all-io.to_dataset_package",
            "dataset_provider_version": provider.version(),
            "capabilities": provider.capabilities().__dict__,
        },
        "io": {
            "assembled_block": block_name,
            "io_spec_sha256": _sha256_json(io_spec),
            "package_manifest_root": package_summary.get("manifest", {}).get("root") if isinstance(package_summary, dict) else None,
            "feature_payload_shape": _payload_shape(feature_entry),
            "target_payload_shape": _payload_shape(target_entry),
            "feature_payload_hash": feature_entry.get("content_hash"),
            "target_payload_hash": target_entry.get("content_hash"),
            "audits": package_summary.get("audits", []) if isinstance(package_summary, dict) else [],
        },
        "dataset": dataset_payload,
        "io_reshape": {
            "from": {
                "rows": int(X.shape[0]),
                "cols": int(X.shape[1]),
                "representation": "nirs4all-io AssembledDataset feature matrix",
            },
            "to": {
                "rows": int(X_selected.shape[0]),
                "cols": int(X_selected.shape[1]),
                "representation": "R list-of-row-vectors portable dataset",
            },
            "row_indices": row_indices,
            "col_indices": col_indices,
            "selected_values_preserved": selected_values_preserved,
            "dataset_sha256": _sha256_json(dataset_payload),
        },
    }
    if not selected_values_preserved:
        raise AssertionError("IO reshape changed selected spectral values")

    out_dir.mkdir(parents=True, exist_ok=True)
    _write_json(out_dir / "dataset-card.json", card or {})
    _write_json(out_dir / "io-spec.n4a.json", io_spec)
    _write_json(out_dir / "dataset-package-summary.json", package_summary)
    _write_json(out_dir / "reshaped-dataset.json", payload)
    return payload


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True, help="Output artifact directory.")
    parser.add_argument("--dataset-id", default=DEFAULT_DATASET_ID)
    parser.add_argument("--source", default=DEFAULT_SOURCE)
    parser.add_argument("--max-rows", type=int, default=DEFAULT_MAX_ROWS)
    parser.add_argument("--max-cols", type=int, default=DEFAULT_MAX_COLS)
    parser.add_argument("--workspace-root", type=Path, default=_workspace_root())
    args = parser.parse_args(argv)

    payload = prepare(
        out_dir=args.out,
        dataset_id=args.dataset_id,
        source=args.source,
        max_rows=args.max_rows,
        max_cols=args.max_cols,
        workspace_root=args.workspace_root.resolve(),
    )
    print(
        json.dumps(
            {
                "status": payload["status"],
                "dataset_id": payload["source"]["dataset_id"],
                "rows": payload["dataset"]["rows"],
                "cols": payload["dataset"]["cols"],
                "dataset_sha256": payload["io_reshape"]["dataset_sha256"],
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
