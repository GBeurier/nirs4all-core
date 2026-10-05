"""Portable calibration facades over the authenticated native archive cycle."""
from __future__ import annotations

import json
import os
import shutil
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from uuid import uuid4

from ._dataset import dataset
from ._workflow import Workflow, _invoke


def _library(value: str | Path | None) -> str:
    library = value or os.environ.get("N4M_LIBRARY_PATH") or os.environ.get("N4M_LIB_PATH")
    if library is None:
        raise ValueError("methods_library_path or N4M_LIBRARY_PATH is required")
    return str(Path(library).resolve())


def _uncertainty_input(x: Any, sample_ids: Any, source_id: str | None) -> dict[str, Any]:
    # A numeric DataFrame is the positional matrix interface, not an IO record.
    if hasattr(x, "to_numpy") and hasattr(x, "columns"):
        x = x.to_numpy()
    if hasattr(x, "to_dict") or isinstance(x, (dict, str, Path)):
        if sample_ids is not None:
            raise ValueError("Structured uncertainty uses the Dataset sample IDs; omit sample_ids")
        record = {"dataset": dataset(x).to_dict()}
        if source_id is not None:
            record["source_id"] = source_id
        return record
    if sample_ids is None:
        raise ValueError("Positional uncertainty requires sample_ids")
    if source_id is not None:
        raise ValueError("source_id requires a structured Dataset")
    return {"x": x.tolist() if hasattr(x, "tolist") else x, "sample_ids": list(sample_ids)}


@dataclass(frozen=True)
class CalibratedWorkflow:
    """A frozen native winner and its identity-bound split calibrator."""
    archive: Path
    calibration: dict[str, Any]

    def predict(self, x: Any, **kwargs: Any) -> dict[str, Any]:
        return predict_calibrated(self, x, **kwargs)

    def export(self, path: str | Path) -> Path:
        return export_calibrated(self, path)


def calibrate(model: Workflow | str | Path, data: Any, *, coverages: Any = (0.9,),
              small_sample_policy: str = "error", source_id: str = "spectra",
              archive: str | Path | None = None, methods_library_path: str | Path | None = None,
              cli: str | Path | None = None) -> CalibratedWorkflow:
    """Replay a held-out labelled dataset and attach native split calibration."""
    original = model.archive if isinstance(model, Workflow) else Path(model)
    target = Path(archive).resolve() if archive is not None else Path(tempfile.mkdtemp(prefix="nirs4all-calibrate-")) / "calibrated.n4a"
    record = _invoke(cli, "conformal-calibrate", dataset(data).to_dict(),
                     archive=Path(original).resolve(), destination=target, source_id=source_id,
                     methods_library=_library(methods_library_path), run_id="run:calibrate:" + uuid4().hex,
                     coverages=json.dumps(list(coverages), allow_nan=False),
                     small_sample_policy=json.dumps(small_sample_policy))
    return CalibratedWorkflow(target, record["calibration"])


def predict_calibrated(model: CalibratedWorkflow | str | Path, x: Any, *, sample_ids: Any = None,
                       source_id: str | None = None,
                       methods_library_path: str | Path | None = None,
                       cli: str | Path | None = None) -> dict[str, Any]:
    """Replay a schema-checked Dataset or positional matrix with explicit IDs.

    Positional inputs cannot verify undeclared axes, columns, units or groups;
    the native replay records this limit before sealing its diagnostics.
    """
    archive = model.archive if isinstance(model, CalibratedWorkflow) else Path(model)
    return _invoke(cli, "conformal-predict", _uncertainty_input(x, sample_ids, source_id),
                   archive=Path(archive).resolve(), methods_library=_library(methods_library_path),
                   run_id="run:calibrated:predict:" + uuid4().hex)


def conformal_metrics(model: CalibratedWorkflow, prediction: dict[str, Any], y: Any, *,
                      sample_ids: Any, cli: str | Path | None = None) -> dict[str, Any]:
    """Evaluate keyed truth against the native intervals and persisted policy."""
    values = y.tolist() if hasattr(y, "tolist") else list(y)
    if values and not isinstance(values[0], (list, tuple)):
        values = [[value] for value in values]
    return _invoke(cli, "conformal-metrics", {"calibration": model.calibration,
                   "intervals": prediction["interval_block"],
                   "truth": {"sample_ids": list(sample_ids), "values": values}})


def export_calibrated(model: CalibratedWorkflow, path: str | Path) -> Path:
    """Copy the complete validated archive, including native calibration state."""
    target = Path(path)
    with model.archive.open("rb") as source:
        with target.open("xb") as output:
            identity = os.fstat(output.fileno())
            try:
                shutil.copyfileobj(source, output)
            except BaseException:
                output.close()
                try:
                    current = target.lstat()
                    if (current.st_dev, current.st_ino) == (identity.st_dev, identity.st_ino):
                        target.unlink()
                except OSError:
                    pass
                raise
    return target


def load_calibrated(path: str | Path, *, cli: str | Path | None = None) -> CalibratedWorkflow:
    """Validate native archive closure and reopen its embedded calibrator."""
    archive = Path(path).resolve()
    record = _invoke(cli, "conformal-load", None, archive=archive)
    return CalibratedWorkflow(archive, record["calibration"])
