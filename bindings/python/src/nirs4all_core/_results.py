"""Persist and query a native DAG result with an optional Core model archive.

The experiment directory is an index over two distinct native products: a
dag-ml results V2 directory and, when available, a Core Archive V2 model.
Neither a checkpoint nor a legacy SDK workspace is interpreted here.
"""

from __future__ import annotations

import hashlib
import json
import shutil
import tempfile
from copy import deepcopy
from dataclasses import dataclass
from pathlib import Path
from typing import Any

_SCHEMA = "nirs4all.experiment.v1"
_RESULT_FILES = ("manifest.json", "score_set.json", "predictions.parquet")
_INDEX_FILE = "experiment.json"
_VIEW_FILE = "result_view.json"
_MODEL_FILE = "model.n4a"


def _native_view(run_dir: Path) -> dict[str, Any]:
    try:
        from dag_ml import read_native_results_v2
    except ImportError as error:
        raise RuntimeError("Reading experiments requires the dag-ml native Python binding") from error
    view = read_native_results_v2(run_dir)
    if not isinstance(view, dict) or not all(key in view for key in ("manifest", "score_set", "predictions")):
        raise RuntimeError("dag-ml returned an invalid native results view")
    return view


def _digest(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _regular_file(path: Path) -> None:
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"experiment member must be a regular file: {path.name}")


def _validate_id(value: str, label: str) -> str:
    if not isinstance(value, str) or not value or len(value) > 256 or any(ord(char) < 33 or ord(char) == 127 for char in value):
        raise ValueError(f"{label} must be a nonempty printable identifier")
    return value


def _variant_ids(view: dict[str, Any]) -> set[str]:
    rows = view["predictions"]
    reports = view["score_set"].get("reports", [])
    if not isinstance(rows, list) or not isinstance(reports, list):
        raise TypeError("native results prediction rows and score reports must be lists")
    return {
        value for value in [
            *(row.get("variant_id") for row in rows if isinstance(row, dict)),
            *(report.get("variant_id") for report in reports if isinstance(report, dict)),
        ] if isinstance(value, str) and value
    }


def _check_winner(view: dict[str, Any], winner_variant_id: str) -> None:
    _validate_id(winner_variant_id, "winner_variant_id")
    variants = _variant_ids(view)
    if winner_variant_id not in variants:
        raise ValueError(f"winner_variant_id {winner_variant_id!r} is absent from native results")
    recorded = view["manifest"].get("selected_variant_id")
    _validate_id(recorded, "manifest selected_variant_id")
    if recorded != winner_variant_id:
        raise ValueError("winner_variant_id disagrees with native results manifest")


def _check_archive(path: Path, view: dict[str, Any], run_id: str, winner_variant_id: str) -> None:
    from dag_ml import (
        PortablePredictorPackage,
        TrainingOutcome,
        validate_archive_v2_portable_payloads,
    )

    from ._archive import read_archive_v2_payloads

    _regular_file(path)
    inventory = read_archive_v2_payloads(path)
    members = inventory["members"]
    package_bytes = members.get("dagml/portable_predictor_package.json")
    outcome_bytes = members.get("dagml/training_outcome.json")
    if package_bytes is None or outcome_bytes is None:
        raise ValueError("Core archive lacks native predictor or training outcome")
    # DAG-ML, rather than this facade, validates the signed package/outcome semantics.
    package_contract = PortablePredictorPackage(package_bytes.decode("utf-8"))
    package = package_contract.to_dict()
    outcome = TrainingOutcome(outcome_bytes.decode("utf-8")).to_dict()
    # Keep the exact package JSON at the semantic gate. Python float
    # reserialization can change the archive's native transport closure.
    validate_archive_v2_portable_payloads(inventory["manifest"], package_contract, members)
    if outcome.get("run_id") != run_id or outcome.get("selected_variant_id") != winner_variant_id:
        raise ValueError("Core archive training outcome disagrees with experiment run or winner")
    if package.get("training_outcome", {}).get("outcome_fingerprint") != outcome.get("outcome_fingerprint"):
        raise ValueError("Core archive package and training outcome disagree")
    if package.get("execution_bundle", {}).get("selected_variant_id") != winner_variant_id:
        raise ValueError("Core archive package does not contain experiment winner")
    if view["score_set"] != outcome.get("score_set"):
        raise ValueError("native result scores differ from the model training outcome")
    manifest = view["manifest"]
    if manifest.get("training_outcome_fingerprint") != outcome.get("outcome_fingerprint"):
        raise ValueError("native results lack a matching training outcome fingerprint for model closure")
    from . import _native

    _native.validate_training_predictions_json(
        outcome_bytes.decode("utf-8"),
        json.dumps(view["predictions"], ensure_ascii=False, allow_nan=False),
    )


@dataclass(frozen=True)
class Experiment:
    """A reopened native result; score values remain native-owned."""

    path: Path
    index: dict[str, Any]
    view: dict[str, Any]

    @property
    def winner_variant_id(self) -> str:
        return self.index["winner_variant_id"]

    @property
    def model_archive(self) -> Path | None:
        return self.path / _MODEL_FILE if self.index["model_archive"] is not None else None

    def summary(self) -> dict[str, Any]:
        """Return identity and scope; no score is recomputed or aggregated."""
        return {
            "schema": _SCHEMA,
            "run_id": self.index["run_id"],
            "winner_variant_id": self.winner_variant_id,
            "variant_ids": sorted(_variant_ids(self.view)),
            "score_report_count": len(self.view["score_set"]["reports"]),
            "prediction_row_count": len(self.view["predictions"]),
            "selection_metric": self.view["score_set"].get("selection_metric"),
            "model_archive": str(self.model_archive) if self.model_archive else None,
            "validation_level": "native_results_and_model_prediction_closure" if self.model_archive else "native_results",
            "unattested_display_fields": ["dataset", "task_type"],
        }

    def compare(self, *, variant_id: str | None = None, partition: str | None = None) -> list[dict[str, Any]]:
        """Expose each native score report with its own fold and partition."""
        if variant_id is not None and variant_id not in _variant_ids(self.view):
            raise KeyError(f"unknown variant_id {variant_id!r}")
        return [
            {**deepcopy(report), "is_winner": report.get("variant_id") == self.winner_variant_id}
            for report in self.view["score_set"]["reports"]
            if (variant_id is None or report.get("variant_id") == variant_id)
            and (partition is None or report.get("partition") == partition)
        ]

    def predictions(self, *, variant_id: str | None = None, partition: str | None = None, fold_id: str | None = None) -> list[dict[str, Any]]:
        """Extract native prediction rows, including sample identity and shape."""
        if variant_id is not None and variant_id not in _variant_ids(self.view):
            raise KeyError(f"unknown variant_id {variant_id!r}")
        return [
            deepcopy(row) for row in self.view["predictions"]
            if (variant_id is None or row["variant_id"] == variant_id)
            and (partition is None or row["partition"] == partition)
            and (fold_id is None or row["fold_id"] == fold_id)
        ]

    def export(self, destination: str | Path, *, kind: str = "predictions", variant_id: str | None = None,
               partition: str | None = None, fold_id: str | None = None) -> Path:
        """Export a queried native JSON projection without changing its scores."""
        if kind == "predictions":
            payload = self.predictions(variant_id=variant_id, partition=partition, fold_id=fold_id)
        elif kind == "scores":
            if fold_id is not None:
                raise ValueError("score export filters use variant_id and partition")
            payload = self.compare(variant_id=variant_id, partition=partition)
        else:
            raise ValueError("kind must be 'predictions' or 'scores'")
        target = Path(destination)
        with target.open("x", encoding="utf-8") as output:
            json.dump(payload, output, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False)
        return target

    def predict_methods_matrix(
        self,
        sample_ids: list[str],
        x: list[list[float]],
        expected_target_names: list[str],
        *,
        methods_library_path: str | Path,
        methods_library_sha256: str,
        request_id: str,
        outcome_id: str,
        run_id: str,
    ) -> dict[str, Any]:
        """Reload the selected native Methods model through Core's closed replay."""
        archive = self.model_archive
        if archive is None:
            raise ValueError("experiment has no portable model archive")
        from ._archive import predict_methods_archive_v2_matrix

        return predict_methods_archive_v2_matrix(
            archive, sample_ids, x, expected_target_names,
            methods_library_path=methods_library_path,
            methods_library_sha256=methods_library_sha256,
            request_id=request_id, outcome_id=outcome_id, run_id=run_id,
        )


def save_experiment(
    native_results_dir: str | Path,
    destination: str | Path,
    *,
    run_id: str,
    winner_variant_id: str,
    model_archive: str | Path | None = None,
) -> Experiment:
    """Copy a native result and optional verified Core Archive V2 atomically."""
    _validate_id(run_id, "run_id")
    source = Path(native_results_dir)
    view = _native_view(source)
    _check_winner(view, winner_variant_id)
    recorded_run_id = view["manifest"].get("run_id")
    _validate_id(recorded_run_id, "manifest run_id")
    if recorded_run_id != run_id:
        raise ValueError("run_id disagrees with native results manifest")
    for name in _RESULT_FILES:
        _regular_file(source / name)
    archive = Path(model_archive) if model_archive is not None else None
    if archive is not None:
        _check_archive(archive, view, run_id, winner_variant_id)

    target = Path(destination)
    if target.exists() or target.is_symlink():
        raise FileExistsError(f"experiment destination already exists: {target}")
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(tempfile.mkdtemp(prefix=f".{target.name}.", dir=target.parent))
    try:
        result_dir = temporary / "results"
        result_dir.mkdir()
        for name in _RESULT_FILES:
            shutil.copyfile(source / name, result_dir / name)
        model_hash = None
        if archive is not None:
            shutil.copyfile(archive, temporary / _MODEL_FILE)
            model_hash = _digest(temporary / _MODEL_FILE)
        index = {
            "schema": _SCHEMA,
            "run_id": run_id,
            "winner_variant_id": winner_variant_id,
            "results": {name: _digest(result_dir / name) for name in _RESULT_FILES},
            "result_view_sha256": "",
            "model_archive": {"path": _MODEL_FILE, "sha256": model_hash} if model_hash else None,
        }
        verified_view = _native_view(result_dir)
        (temporary / _VIEW_FILE).write_text(
            json.dumps(verified_view, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False),
            encoding="utf-8",
        )
        index["result_view_sha256"] = _digest(temporary / _VIEW_FILE)
        (temporary / _INDEX_FILE).write_text(json.dumps(index, sort_keys=True, separators=(",", ":"), allow_nan=False), encoding="utf-8")
        open_experiment(temporary)
        from . import _native

        _native.publish_directory_noreplace(str(temporary), str(target))
    except BaseException:
        shutil.rmtree(temporary)
        raise
    return open_experiment(target)


def open_experiment(path: str | Path) -> Experiment:
    """Reopen a closed experiment after file hashes and native validation."""
    directory = Path(path)
    _regular_file(directory / _INDEX_FILE)
    index = json.loads((directory / _INDEX_FILE).read_text(encoding="utf-8"))
    if not isinstance(index, dict) or set(index) != {"schema", "run_id", "winner_variant_id", "results", "result_view_sha256", "model_archive"} or index["schema"] != _SCHEMA:
        raise ValueError("unsupported experiment index")
    _validate_id(index["run_id"], "run_id")
    _validate_id(index["winner_variant_id"], "winner_variant_id")
    if not isinstance(index["results"], dict) or set(index["results"]) != set(_RESULT_FILES):
        raise ValueError("experiment results inventory is invalid")
    for name in _RESULT_FILES:
        member = directory / "results" / name
        _regular_file(member)
        if _digest(member) != index["results"][name]:
            raise ValueError(f"experiment result hash mismatch: {name}")
    _regular_file(directory / _VIEW_FILE)
    if _digest(directory / _VIEW_FILE) != index["result_view_sha256"]:
        raise ValueError("experiment result view hash mismatch")
    model = index["model_archive"]
    if model is not None:
        if not isinstance(model, dict) or set(model) != {"path", "sha256"} or model["path"] != _MODEL_FILE:
            raise ValueError("experiment model archive reference is invalid")
        member = directory / _MODEL_FILE
        _regular_file(member)
        if _digest(member) != model["sha256"]:
            raise ValueError("experiment model archive hash mismatch")
    view = _native_view(directory / "results")
    projection = json.loads((directory / _VIEW_FILE).read_text(encoding="utf-8"))
    if projection != view:
        raise ValueError("experiment result view disagrees with native results")
    _check_winner(view, index["winner_variant_id"])
    recorded_run_id = view["manifest"].get("run_id")
    _validate_id(recorded_run_id, "manifest run_id")
    if recorded_run_id != index["run_id"]:
        raise ValueError("experiment run_id disagrees with native results manifest")
    if model is not None:
        _check_archive(directory / _MODEL_FILE, view, index["run_id"], index["winner_variant_id"])
    return Experiment(directory, index, view)
